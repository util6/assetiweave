use super::*;

pub(crate) async fn import_web_record_sessions_sqlx(
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
        });
    }

    let now = Utc::now().to_rfc3339();
    let sync_run_id = stable_id("web-record-sync", &[&source.id, &now]);
    {
        let mut tx = pool.begin().await.map_err(StoreError::external)?;
        clear_legacy_conversation_records_for_source_sqlx_tx(&mut tx, tenant_id, &source.id)
            .await?;
        tx.commit().await.map_err(StoreError::external)?;
    }

    let mut warning_count = 0usize;
    let mut skipped_session_count = 0usize;
    let mut changed_session_count = 0usize;
    for batch in sessions.chunks(CONVERSATION_IMPORT_BATCH_SIZE) {
        let mut tx = pool.begin().await.map_err(StoreError::external)?;
        for normalized in batch {
            let session = web_record_session_from_normalized(source, normalized, &now);
            let change_kind =
                if web_record_session_exists_sqlx_tx(&mut tx, tenant_id, &session.id).await? {
                    "updated"
                } else {
                    "new"
                };
            if web_record_session_is_unchanged_sqlx_tx(&mut tx, tenant_id, &session, normalized)
                .await?
            {
                skipped_session_count += 1;
                continue;
            }
            let translation_state =
                load_web_record_part_translation_state_sqlx_tx(&mut tx, tenant_id, &session.id)
                    .await
                    .map_err(StoreError::external)?;
            delete_web_record_session_sqlx_tx(&mut tx, tenant_id, &session.id).await?;
            insert_web_record_session_sqlx_tx(&mut tx, tenant_id, &session).await?;

            let mut stored_turns = Vec::new();
            for turn in &normalized.turns {
                if turn.user_text.trim().is_empty() {
                    warning_count += 1;
                    continue;
                }
                let stored_turn = web_record_turn_from_normalized(&session.id, turn, &now);
                insert_web_record_turn_sqlx_tx(&mut tx, tenant_id, &stored_turn).await?;
                insert_web_record_parts_sqlx_tx(
                    &mut tx,
                    tenant_id,
                    &stored_turn.id,
                    &turn.parts,
                    &translation_state,
                )
                .await?;
                stored_turns.push(stored_turn);
            }
            insert_web_record_questions_sqlx_tx(
                &mut tx,
                tenant_id,
                &session.id,
                &stored_turns,
                &now,
            )
            .await?;
            insert_conversation_sync_delta_sqlx_tx(
                &mut tx,
                tenant_id,
                &sync_run_id,
                "web",
                &session.id,
                change_kind,
                &now,
            )
            .await
            .map_err(StoreError::external)?;
            changed_session_count += 1;
        }
        tx.commit().await.map_err(StoreError::external)?;
    }

    let mut tx = pool.begin().await.map_err(StoreError::external)?;
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
    })
}

pub(crate) async fn web_record_session_is_unchanged_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session: &ConversationSession,
    normalized: &NormalizedConversationSession,
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
        && row.missing == 0
        && session_turns_match_normalized_sqlx_tx(tx, tenant_id, &session.id, normalized).await?)
}

pub(crate) async fn web_record_session_exists_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<bool> {
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM web_record_sessions WHERE tenant_id = ?1 AND id = ?2)",
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(exists != 0)
}

pub(crate) async fn session_turns_match_normalized_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    normalized: &NormalizedConversationSession,
) -> StoreResult<bool> {
    let rows = sqlx::query_as::<_, ExistingWebRecordTurnRow>(
        r#"
        SELECT external_id, fingerprint, missing
        FROM web_record_turns
        WHERE tenant_id = ?1 AND session_id = ?2
        ORDER BY turn_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    if rows.len() != normalized.turns.len() {
        return Ok(false);
    }
    for (row, turn) in rows.iter().zip(&normalized.turns) {
        if row.external_id != turn.external_id
            || row.fingerprint != conversation_turn_fingerprint(turn)
            || row.missing != 0
        {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(crate) async fn insert_web_record_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session: &ConversationSession,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        INSERT INTO web_record_sessions (
            tenant_id, id, source_id, adapter_id, external_id, title, started_at, updated_at,
            source_locator, source_fingerprint, missing, created_at, imported_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
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

pub(crate) async fn insert_web_record_turn_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    turn: &ConversationTurn,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        INSERT INTO web_record_turns (
            tenant_id, id, session_id, external_id, turn_index, user_text, title, started_at,
            ended_at, fingerprint, missing, imported_at, model
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
    )
    .bind(tenant_id)
    .bind(&turn.id)
    .bind(&turn.session_id)
    .bind(&turn.external_id)
    .bind(turn.turn_index)
    .bind(&turn.user_text)
    .bind(&turn.title)
    .bind(&turn.started_at)
    .bind(&turn.ended_at)
    .bind(&turn.fingerprint)
    .bind(if turn.missing { 1_i64 } else { 0_i64 })
    .bind(&turn.imported_at)
    .bind(&turn.model)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

pub(crate) async fn insert_sync_run_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    run: &ConversationSyncRun,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_sync_runs (
            tenant_id, id, source_id, adapter_id, status, started_at, finished_at,
            session_count, turn_count, warning_count, error_message
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        "#,
    )
    .bind(tenant_id)
    .bind(&run.id)
    .bind(&run.source_id)
    .bind(&run.adapter_id)
    .bind(encode_enum_app(run.status)?)
    .bind(&run.started_at)
    .bind(&run.finished_at)
    .bind(run.session_count)
    .bind(run.turn_count)
    .bind(run.warning_count)
    .bind(&run.error_message)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}
