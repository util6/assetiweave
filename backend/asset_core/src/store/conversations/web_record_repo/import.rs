use super::*;

pub async fn import_web_record_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &ConversationSource,
    sessions: &[NormalizedConversationSession],
    dry_run: bool,
) -> StoreResult<ConversationImportResult> {
    let turn_count = sessions.iter().map(|session| session.turns.len()).sum();
    if dry_run {
        return Ok(ConversationImportResult {
            source_id: source.id.clone(),
            adapter_id: source.adapter_id.clone(),
            dry_run: true,
            sync_run_id: None,
            session_count: sessions.len(),
            skipped_session_count: 0,
            changed_session_count: 0,
            turn_count,
            warning_count: 0,
            warnings: Vec::new(),
            failed_session_count: 0,
            session_failures: Vec::new(),
            session_warnings: Vec::new(),
            status: crate::backend::domain::ConversationSyncStatus::Completed,
            succeeded_external_ids: Vec::new(),
        });
    }

    let now = Utc::now().to_rfc3339();
    let sync_run_id = stable_id("web-record-sync", &[&source.id, &now]);
    {
        let mut tx = crate::backend::store::begin_immediate(pool).await?;
        clear_legacy_conversation_records_for_source_sqlx_tx(&mut tx, tenant_id, &source.id)
            .await?;
        tx.commit().await.map_err(StoreError::external)?;
    }

    let mut warning_count = 0usize;
    let mut skipped_session_count = 0usize;
    let mut changed_session_count = 0usize;
    for batch in sessions.chunks(CONVERSATION_IMPORT_BATCH_SIZE) {
        let mut tx = crate::backend::store::begin_immediate(pool).await?;
        for normalized in batch {
            let session = web_record_session_from_normalized(source, normalized, &now);
            if web_record_session_is_unchanged_sqlx_tx(&mut tx, tenant_id, &session).await? {
                skipped_session_count += 1;
                continue;
            }
            warning_count += normalized
                .turns
                .iter()
                .filter(|turn| turn.user_text.trim().is_empty())
                .count();
            let prepared = prepare_session_snapshot(
                ConversationRecordKind::Web,
                &source.id,
                &session.id,
                &session.adapter_id,
                None,
                session.started_at.as_deref(),
                session.updated_at.as_deref(),
                &normalized.turns,
                &now,
            );
            upsert_web_record_session_sqlx_tx(&mut tx, tenant_id, &session).await?;
            replace_session_snapshot_sqlx_tx(
                &mut tx,
                tenant_id,
                ConversationRecordKind::Web,
                &session,
                &prepared,
            )
            .await?;
            changed_session_count += 1;
        }
        tx.commit().await.map_err(StoreError::external)?;
    }

    let mut tx = crate::backend::store::begin_immediate(pool).await?;
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
    tx.commit().await.map_err(StoreError::external)?;

    Ok(ConversationImportResult {
        source_id: source.id.clone(),
        adapter_id: source.adapter_id.clone(),
        dry_run: false,
        sync_run_id: Some(sync_run_id),
        session_count: sessions.len(),
        skipped_session_count,
        changed_session_count,
        turn_count,
        warning_count,
        warnings: Vec::new(),
        failed_session_count: 0,
        session_failures: Vec::new(),
        session_warnings: Vec::new(),
        status: crate::backend::domain::ConversationSyncStatus::Completed,
        succeeded_external_ids: sessions.iter().map(|s| s.external_id.clone()).collect(),
    })
}

pub async fn web_record_session_is_unchanged_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session: &ConversationSession,
) -> StoreResult<bool> {
    let Some(source_fingerprint) = session.source_fingerprint.as_deref() else {
        return Ok(false);
    };
    let Some(row) = sqlx::query_as::<_, ExistingWebRecordSessionRow>(
        r#"
        SELECT title, started_at, updated_at, source_locator, source_fingerprint, missing
        FROM web_record_sessions
        WHERE tenant_id = ?1 AND id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(&session.id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::external)?
    else {
        return Ok(false);
    };

    Ok(row.title == session.title
        && row.started_at == session.started_at
        && row.updated_at == session.updated_at
        && row.source_locator == session.source_locator
        && row.source_fingerprint.as_deref() == Some(source_fingerprint)
        && row.missing == 0)
}

pub async fn upsert_web_record_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session: &ConversationSession,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        INSERT INTO web_record_sessions (
            tenant_id, id, source_id, adapter_id, external_id, title, started_at,
            updated_at, source_locator, source_fingerprint, missing, created_at, imported_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        ON CONFLICT(tenant_id, source_id, external_id) DO UPDATE SET
            adapter_id = excluded.adapter_id,
            title = excluded.title,
            started_at = excluded.started_at,
            updated_at = excluded.updated_at,
            source_locator = excluded.source_locator,
            source_fingerprint = excluded.source_fingerprint,
            missing = 0,
            imported_at = excluded.imported_at
        "#,
    )
    .bind(tenant_id)
    .bind(&session.id)
    .bind(&session.source_id)
    .bind(&session.adapter_id)
    .bind(&session.external_id)
    .bind(&session.title)
    .bind(&session.started_at)
    .bind(&session.updated_at)
    .bind(&session.source_locator)
    .bind(&session.source_fingerprint)
    .bind(if session.missing { 1_i64 } else { 0_i64 })
    .bind(&session.created_at)
    .bind(&session.imported_at)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}
