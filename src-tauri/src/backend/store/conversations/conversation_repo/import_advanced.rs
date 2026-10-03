use super::*;

pub(crate) async fn import_conversation_sessions_advanced_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &ConversationSource,
    sessions: &[NormalizedConversationSession],
    discovered_external_ids: Option<&BTreeSet<String>>,
    descriptor_versions: Option<&BTreeMap<String, String>>,
    mut session_failures: Vec<crate::backend::domain::SessionSyncFailure>,
    session_warnings: Vec<crate::backend::domain::SessionSyncWarning>,
    adapter_content_hash: Option<&str>,
    card_contract_version: Option<u32>,
    payload_policy_version: u32,
    dry_run: bool,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
    on_progress: &mut impl FnMut(usize, usize),
) -> StoreResult<ConversationImportResult> {
    ensure_sync_import_active(cancellation)?;
    on_progress(0, sessions.len());
    ensure_sync_import_active(cancellation)?;
    let turn_count = sessions.iter().map(|session| session.turns.len()).sum();
    let initial_failed_count = session_failures.len();
    if dry_run {
        on_progress(sessions.len(), sessions.len());
        ensure_sync_import_active(cancellation)?;
        let status = if !session_failures.is_empty() {
            if !sessions.is_empty() {
                ConversationSyncStatus::PartialSuccess
            } else {
                ConversationSyncStatus::Failed
            }
        } else {
            ConversationSyncStatus::Completed
        };
        return Ok(ConversationImportResult {
            source_id: source.id.clone(),
            adapter_id: source.adapter_id.clone(),
            dry_run: true,
            sync_run_id: None,
            session_count: sessions.len() + initial_failed_count,
            skipped_session_count: 0,
            changed_session_count: 0,
            failed_session_count: initial_failed_count,
            turn_count,
            warning_count: session_warnings.len(),
            warnings: session_warnings.iter().map(|w| w.message.clone()).collect(),
            session_failures,
            session_warnings,
            status,
        });
    }

    audit_invalid_conversation_question_turns_sqlx(pool, tenant_id).await?;

    let now = Utc::now().to_rfc3339();
    let sync_run_id = stable_id("conversation-sync", &[&source.id, &now]);
    let mut skipped_session_count = 0usize;
    let mut changed_session_count = 0usize;

    // 先记录读取阶段失败的会话到 observation 表 (dirty = 1, presence = present)
    for failure in &session_failures {
        let observed_version = descriptor_versions.and_then(|map| {
            map.get(failure.session_external_id.as_str())
                .map(|s| s.as_str())
        });
        let _ = record_conversation_session_failure_sqlx(
            pool,
            tenant_id,
            &source.id,
            "session",
            &failure.session_external_id,
            observed_version,
            &failure.error_code,
            &failure.error_message,
            &failure.stage,
            failure.retryable,
        )
        .await;
    }

    let incoming_session_ids = discovered_external_ids
        .map(|external_ids| {
            external_ids
                .iter()
                .map(|external_id| stable_id("conversation-session", &[&source.id, external_id]))
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_else(|| {
            let mut ids = sessions
                .iter()
                .map(|session| {
                    stable_id("conversation-session", &[&source.id, &session.external_id])
                })
                .collect::<BTreeSet<_>>();
            for failure in &session_failures {
                ids.insert(stable_id(
                    "conversation-session",
                    &[&source.id, &failure.session_external_id],
                ));
            }
            ids
        });

    let mut completed_session_count = 0;
    let mut all_changed_session_ids = Vec::new();
    const SESSION_IMPORT_BATCH_SIZE: usize = 50;

    for chunk in sessions.chunks(SESSION_IMPORT_BATCH_SIZE) {
        ensure_sync_import_active(cancellation)?;

        // 优先尝试以批量事务导入当前批次，减少 SQLite 写锁争抢与频繁磁盘 fsync
        let mut batch_success = false;
        if let Ok(mut batch_tx) = pool.begin().await {
            let mut chunk_changed = Vec::new();
            let mut chunk_skipped = 0usize;
            let mut chunk_ok = true;

            for normalized in chunk {
                let session = conversation_session_from_normalized(source, normalized, &now);
                match import_single_session_sqlx_tx(
                    &mut batch_tx,
                    tenant_id,
                    source,
                    normalized,
                    &session,
                    &now,
                    &sync_run_id,
                    adapter_content_hash,
                    card_contract_version,
                    payload_policy_version,
                )
                .await
                {
                    Ok(Some(changed_id)) => {
                        chunk_changed.push(changed_id);
                    }
                    Ok(None) => {
                        chunk_skipped += 1;
                    }
                    Err(_) => {
                        chunk_ok = false;
                        break;
                    }
                }
            }

            if chunk_ok && batch_tx.commit().await.is_ok() {
                batch_success = true;
                changed_session_count += chunk_changed.len();
                skipped_session_count += chunk_skipped;
                all_changed_session_ids.extend(chunk_changed);
                completed_session_count += chunk.len();
                on_progress(completed_session_count, sessions.len());
                tokio::task::yield_now().await;
            }
        }

        // 若批量事务失败（如遇到异常数据），平滑回滚并降级为逐条事务处理，确保精确定位故障并隔离错误
        if !batch_success {
            for normalized in chunk {
                ensure_sync_import_active(cancellation)?;
                let session = conversation_session_from_normalized(source, normalized, &now);
                let mut tx = match pool.begin().await {
                    Ok(tx) => tx,
                    Err(err) => {
                        let sanitized = sanitize_store_sync_error_message(&err.to_string());
                        session_failures.push(crate::backend::domain::SessionSyncFailure {
                            session_external_id: normalized.external_id.clone(),
                            stage: "storage".to_string(),
                            error_code: "transaction_begin_failed".to_string(),
                            error_message: sanitized.clone(),
                            retryable: true,
                        });
                        let _ = record_conversation_session_failure_sqlx(
                            pool,
                            tenant_id,
                            &source.id,
                            "session",
                            &normalized.external_id,
                            session.source_fingerprint.as_deref(),
                            "transaction_begin_failed",
                            &sanitized,
                            "storage",
                            true,
                        )
                        .await;
                        completed_session_count += 1;
                        on_progress(completed_session_count, sessions.len());
                        continue;
                    }
                };

                let session_res = import_single_session_sqlx_tx(
                    &mut tx,
                    tenant_id,
                    source,
                    normalized,
                    &session,
                    &now,
                    &sync_run_id,
                    adapter_content_hash,
                    card_contract_version,
                    payload_policy_version,
                )
                .await;

                match session_res {
                    Ok(changed_opt) => {
                        if let Err(err) = tx.commit().await {
                            let sanitized = sanitize_store_sync_error_message(&err.to_string());
                            session_failures.push(crate::backend::domain::SessionSyncFailure {
                                session_external_id: normalized.external_id.clone(),
                                stage: "storage".to_string(),
                                error_code: "transaction_commit_failed".to_string(),
                                error_message: sanitized.clone(),
                                retryable: true,
                            });
                        } else {
                            match changed_opt {
                                Some(changed_id) => {
                                    changed_session_count += 1;
                                    all_changed_session_ids.push(changed_id);
                                }
                                None => {
                                    skipped_session_count += 1;
                                }
                            }
                        }
                    }
                    Err(err) => {
                        let err_str = err.to_string();
                        let sanitized = sanitize_store_sync_error_message(&err_str);
                        session_failures.push(crate::backend::domain::SessionSyncFailure {
                            session_external_id: session.external_id.clone(),
                            stage: "storage".to_string(),
                            error_code: "storage_error".to_string(),
                            error_message: sanitized.clone(),
                            retryable: true,
                        });
                        let _ = record_conversation_session_failure_sqlx(
                            pool,
                            tenant_id,
                            &source.id,
                            "session",
                            &session.external_id,
                            session.source_fingerprint.as_deref(),
                            "storage_error",
                            &sanitized,
                            "storage",
                            true,
                        )
                        .await;
                    }
                }
                completed_session_count += 1;
                on_progress(completed_session_count, sessions.len());
            }
        }
    }

    let is_cancelled = cancellation.is_some_and(tokio_util::sync::CancellationToken::is_cancelled);

    // Missing 对账（仅在非主动取消时进行，防止取消时误将未处理会话当成 missing）
    let missing_or_restored_session_ids = if !is_cancelled {
        let mut tx = pool.begin().await.map_err(StoreError::external)?;
        let missing_ids = mark_missing_conversation_sessions_sqlx_tx(
            &mut tx,
            tenant_id,
            &source.id,
            &incoming_session_ids,
            &sync_run_id,
            &now,
        )
        .await?;
        tx.commit().await.map_err(StoreError::external)?;
        missing_ids
    } else {
        Vec::new()
    };

    let total_failed = session_failures.len();
    let status = if is_cancelled {
        ConversationSyncStatus::Cancelled
    } else if total_failed > 0 {
        if changed_session_count > 0 || skipped_session_count > 0 {
            ConversationSyncStatus::PartialSuccess
        } else {
            ConversationSyncStatus::Failed
        }
    } else {
        ConversationSyncStatus::Completed
    };

    let status_str = match status {
        ConversationSyncStatus::Completed => "completed",
        ConversationSyncStatus::PartialSuccess => "partial_success",
        ConversationSyncStatus::Failed => "failed",
        ConversationSyncStatus::Cancelled => "cancelled",
        ConversationSyncStatus::Running => "running",
    };

    let error_summary = if total_failed > 0 {
        Some(format!("{total_failed} session(s) failed during sync"))
    } else {
        None
    };

    let mut final_tx = pool.begin().await.map_err(StoreError::external)?;
    sqlx::query(
        r#"
        UPDATE conversation_sources
        SET last_synced_at = ?1, last_sync_status = ?2, updated_at = ?1
        WHERE tenant_id = ?3 AND id = ?4
        "#,
    )
    .bind(&now)
    .bind(status_str)
    .bind(tenant_id)
    .bind(&source.id)
    .execute(&mut *final_tx)
    .await
    .map_err(StoreError::external)?;

    insert_sync_run_sqlx_tx(
        &mut final_tx,
        tenant_id,
        &ConversationSyncRun {
            id: sync_run_id.clone(),
            source_id: Some(source.id.clone()),
            adapter_id: Some(source.adapter_id.clone()),
            status,
            started_at: now.clone(),
            finished_at: Some(now.clone()),
            session_count: (sessions.len() + initial_failed_count) as i64,
            turn_count: turn_count as i64,
            warning_count: session_warnings.len() as i64,
            error_message: error_summary,
        },
    )
    .await?;

    if !all_changed_session_ids.is_empty() || !missing_or_restored_session_ids.is_empty() {
        let revision =
            super::bump_conversation_search_source_revision_sqlx_tx(&mut *final_tx, tenant_id)
                .await?;
        let mut affected = all_changed_session_ids;
        affected.extend(missing_or_restored_session_ids);
        crate::backend::store::append_outbox_event_sqlx_tx(
            &mut final_tx,
            &DomainEvent::conversation_source_committed(
                tenant_id,
                &sync_run_id,
                &source.id,
                revision,
                affected,
            ),
        )
        .await?;
    }
    final_tx.commit().await.map_err(StoreError::external)?;

    let string_warnings = session_warnings.iter().map(|w| w.message.clone()).collect();
    Ok(ConversationImportResult {
        source_id: source.id.clone(),
        adapter_id: source.adapter_id.clone(),
        dry_run: false,
        sync_run_id: Some(sync_run_id),
        session_count: sessions.len() + initial_failed_count,
        skipped_session_count,
        changed_session_count,
        failed_session_count: total_failed,
        turn_count,
        warning_count: session_warnings.len(),
        warnings: string_warnings,
        session_failures,
        session_warnings,
        status,
    })
}

fn sanitize_store_sync_error_message(message: &str) -> String {
    let mut home_paths = Vec::new();
    let home = std::env::var("HOME").ok();
    let userprofile = std::env::var("USERPROFILE").ok();
    if let Some(h) = home.as_deref() {
        home_paths.push(h);
    }
    if let Some(u) = userprofile.as_deref() {
        home_paths.push(u);
    }
    crate::backend::domain::sanitize_sync_error_message(message, &home_paths)
}

#[allow(clippy::too_many_arguments)]
async fn import_single_session_sqlx_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    tenant_id: &str,
    source: &ConversationSource,
    normalized: &NormalizedConversationSession,
    session: &ConversationSession,
    now: &str,
    sync_run_id: &str,
    adapter_content_hash: Option<&str>,
    card_contract_version: Option<u32>,
    payload_policy_version: u32,
) -> StoreResult<Option<String>> {
    let change_kind =
        if conversation_session_exists_sqlx_tx(&mut *tx, tenant_id, &session.id).await? {
            "updated"
        } else {
            "new"
        };
    if conversation_session_is_unchanged_sqlx_tx(&mut *tx, tenant_id, session, normalized).await? {
        upsert_single_session_observation_clean_sqlx_tx(
            &mut *tx,
            tenant_id,
            &source.id,
            "session",
            &session.external_id,
            session.source_fingerprint.as_deref().unwrap_or(now),
            now,
            adapter_content_hash,
            card_contract_version,
            payload_policy_version,
        )
        .await?;
        return Ok(None);
    }
    sqlx::query(
        "UPDATE session_memories SET status = 'invalid', updated_at = ?1 WHERE tenant_id = ?2 AND session_id = ?3 AND status = 'active'",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(&session.id)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    upsert_conversation_session_sqlx_tx(&mut *tx, tenant_id, session).await?;
    for turn in &normalized.turns {
        if turn.user_text.trim().is_empty() {
            continue;
        }
        let stored_turn = conversation_turn_from_normalized(&session.id, turn, now);
        upsert_conversation_turn_sqlx_tx(&mut *tx, tenant_id, &stored_turn).await?;
        replace_conversation_parts_sqlx_tx(&mut *tx, tenant_id, &stored_turn.id, &turn.parts)
            .await?;
    }
    prune_conversation_turns_sqlx_tx(&mut *tx, tenant_id, &session.id, normalized).await?;
    ensure_question_groups_for_session_sqlx_tx(&mut *tx, tenant_id, &session.id, now).await?;
    rebuild_session_question_aggregates_sqlx_tx(&mut *tx, tenant_id, &session.id, now).await?;
    insert_conversation_sync_delta_sqlx_tx(
        &mut *tx,
        tenant_id,
        sync_run_id,
        "session",
        &session.id,
        change_kind,
        now,
    )
    .await?;

    upsert_single_session_observation_clean_sqlx_tx(
        &mut *tx,
        tenant_id,
        &source.id,
        "session",
        &session.external_id,
        session.source_fingerprint.as_deref().unwrap_or(now),
        now,
        adapter_content_hash,
        card_contract_version,
        payload_policy_version,
    )
    .await?;

    Ok(Some(session.id.clone()))
}
