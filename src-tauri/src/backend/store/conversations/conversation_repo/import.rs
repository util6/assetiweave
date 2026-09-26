use super::*;

pub(crate) async fn import_conversation_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &ConversationSource,
    sessions: &[NormalizedConversationSession],
    dry_run: bool,
) -> StoreResult<ConversationImportResult> {
    import_conversation_sessions_with_presence_sqlx(
        pool, tenant_id, source, sessions, None, dry_run,
    )
    .await
}

pub(super) async fn import_conversation_sessions_with_presence_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &ConversationSource,
    sessions: &[NormalizedConversationSession],
    discovered_external_ids: Option<&BTreeSet<String>>,
    dry_run: bool,
) -> StoreResult<ConversationImportResult> {
    import_conversation_sessions_with_control_sqlx(
        pool,
        tenant_id,
        source,
        sessions,
        discovered_external_ids,
        dry_run,
        None,
        &mut |_, _| {},
    )
    .await
}

pub(super) fn ensure_sync_import_active(
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> StoreResult<()> {
    if cancellation.is_some_and(tokio_util::sync::CancellationToken::is_cancelled) {
        return Err(StoreError::Cancelled(
            "conversation sync cancelled".to_string(),
        ));
    }
    Ok(())
}

pub(crate) async fn import_conversation_sessions_with_control_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &ConversationSource,
    sessions: &[NormalizedConversationSession],
    discovered_external_ids: Option<&BTreeSet<String>>,
    dry_run: bool,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
    on_progress: &mut impl FnMut(usize, usize),
) -> StoreResult<ConversationImportResult> {
    ensure_sync_import_active(cancellation)?;
    on_progress(0, sessions.len());
    ensure_sync_import_active(cancellation)?;
    let turn_count = sessions.iter().map(|session| session.turns.len()).sum();
    if dry_run {
        on_progress(sessions.len(), sessions.len());
        ensure_sync_import_active(cancellation)?;
        return Ok(ConversationImportResult {
            source_id: source.id.clone(),
            adapter_id: source.adapter_id.clone(),
            dry_run: true,
            sync_run_id: None,
            session_count: sessions.len(),
            skipped_session_count: 0,
            changed_session_count: 0,
            failed_session_count: 0,
            turn_count,
            warning_count: 0,
            warnings: Vec::new(),
            session_failures: Vec::new(),
            session_warnings: Vec::new(),
            status: ConversationSyncStatus::Completed,
        });
    }

    audit_invalid_conversation_question_turns_sqlx(pool, tenant_id).await?;

    let now = Utc::now().to_rfc3339();
    let sync_run_id = stable_id("conversation-sync", &[&source.id, &now]);
    let mut warning_count = 0usize;
    let mut skipped_session_count = 0usize;
    let mut changed_session_count = 0usize;
    let warnings = Vec::new();
    let incoming_session_ids = discovered_external_ids
        .map(|external_ids| {
            external_ids
                .iter()
                .map(|external_id| stable_id("conversation-session", &[&source.id, external_id]))
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_else(|| {
            sessions
                .iter()
                .map(|session| {
                    stable_id("conversation-session", &[&source.id, &session.external_id])
                })
                .collect::<BTreeSet<_>>()
        });

    let mut completed_session_count = 0;
    for batch in sessions.chunks(CONVERSATION_IMPORT_BATCH_SIZE) {
        let mut tx = pool.begin().await.map_err(StoreError::external)?;
        let mut batch_changed_session_ids = Vec::new();
        for normalized in batch {
            ensure_sync_import_active(cancellation)?;
            let session = conversation_session_from_normalized(source, normalized, &now);
            let change_kind =
                if conversation_session_exists_sqlx_tx(&mut tx, tenant_id, &session.id).await? {
                    "updated"
                } else {
                    "new"
                };
            if conversation_session_is_unchanged_sqlx_tx(&mut tx, tenant_id, &session, normalized)
                .await?
            {
                skipped_session_count += 1;
                completed_session_count += 1;
                on_progress(completed_session_count, sessions.len());
                continue;
            }
            sqlx::query(
                "UPDATE session_memories SET status = 'invalid', updated_at = ?1 WHERE tenant_id = ?2 AND session_id = ?3 AND status = 'active'",
            )
            .bind(&now)
            .bind(tenant_id)
            .bind(&session.id)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::external)?;
            upsert_conversation_session_sqlx_tx(&mut tx, tenant_id, &session).await?;
            for turn in &normalized.turns {
                ensure_sync_import_active(cancellation)?;
                if turn.user_text.trim().is_empty() {
                    warning_count += 1;
                    continue;
                }
                let stored_turn = conversation_turn_from_normalized(&session.id, turn, &now);
                upsert_conversation_turn_sqlx_tx(&mut tx, tenant_id, &stored_turn).await?;
                replace_conversation_parts_sqlx_tx(
                    &mut tx,
                    tenant_id,
                    &stored_turn.id,
                    &turn.parts,
                )
                .await?;
            }
            prune_conversation_turns_sqlx_tx(&mut tx, tenant_id, &session.id, normalized).await?;
            ensure_question_groups_for_session_sqlx_tx(&mut tx, tenant_id, &session.id, &now)
                .await?;
            rebuild_session_question_aggregates_sqlx_tx(&mut tx, tenant_id, &session.id, &now)
                .await?;
            insert_conversation_sync_delta_sqlx_tx(
                &mut tx,
                tenant_id,
                &sync_run_id,
                "session",
                &session.id,
                change_kind,
                &now,
            )
            .await?;
            changed_session_count += 1;
            batch_changed_session_ids.push(session.id);
            completed_session_count += 1;
            on_progress(completed_session_count, sessions.len());
        }
        ensure_sync_import_active(cancellation)?;
        let revision =
            super::bump_conversation_search_source_revision_sqlx_tx(&mut *tx, tenant_id).await?;
        crate::backend::store::append_outbox_event_sqlx_tx(
            &mut tx,
            &DomainEvent::conversation_source_committed(
                tenant_id,
                &sync_run_id,
                &source.id,
                revision,
                batch_changed_session_ids,
            ),
        )
        .await?;
        ensure_sync_import_active(cancellation)?;
        tx.commit().await.map_err(StoreError::external)?;
    }

    ensure_sync_import_active(cancellation)?;
    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    let missing_or_restored_session_ids = mark_missing_conversation_sessions_sqlx_tx(
        &mut tx,
        tenant_id,
        &source.id,
        &incoming_session_ids,
        &sync_run_id,
        &now,
    )
    .await?;
    sqlx::query(
        r#"
        UPDATE conversation_sources
        SET last_synced_at = ?1, last_sync_status = 'completed', updated_at = ?1
        WHERE tenant_id = ?2 AND id = ?3
        "#,
    )
    .bind(&now)
    .bind(tenant_id)
    .bind(&source.id)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;
    insert_sync_run_sqlx_tx(
        &mut tx,
        tenant_id,
        &ConversationSyncRun {
            id: sync_run_id.clone(),
            source_id: Some(source.id.clone()),
            adapter_id: Some(source.adapter_id.clone()),
            status: ConversationSyncStatus::Completed,
            started_at: now.clone(),
            finished_at: Some(now.clone()),
            session_count: sessions.len() as i64,
            turn_count: turn_count as i64,
            warning_count: warning_count as i64,
            error_message: None,
        },
    )
    .await?;
    let revision =
        super::bump_conversation_search_source_revision_sqlx_tx(&mut *tx, tenant_id).await?;
    crate::backend::store::append_outbox_event_sqlx_tx(
        &mut tx,
        &DomainEvent::conversation_source_committed(
            tenant_id,
            &sync_run_id,
            &source.id,
            revision,
            missing_or_restored_session_ids,
        ),
    )
    .await?;
    ensure_sync_import_active(cancellation)?;
    tx.commit().await.map_err(StoreError::external)?;

    Ok(ConversationImportResult {
        source_id: source.id.clone(),
        adapter_id: source.adapter_id.clone(),
        dry_run: false,
        sync_run_id: Some(sync_run_id),
        session_count: sessions.len(),
        skipped_session_count,
        changed_session_count,
        failed_session_count: 0,
        turn_count,
        warning_count,
        warnings,
        session_failures: Vec::new(),
        session_warnings: Vec::new(),
        status: ConversationSyncStatus::Completed,
    })
}

pub(crate) use super::import_advanced::*;
