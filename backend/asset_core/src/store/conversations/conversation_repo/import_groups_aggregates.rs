use super::*;

pub(super) async fn rebuild_session_question_aggregates_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    now: &str,
) -> StoreResult<()> {
    let question_ids = question_ids_for_session_sqlx_tx(tx, tenant_id, session_id).await?;
    for question_id in question_ids {
        rebuild_question_aggregate_sqlx_tx(tx, tenant_id, &question_id, now).await?;
    }
    Ok(())
}

pub(super) async fn rebuild_question_aggregate_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
    now: &str,
) -> StoreResult<()> {
    // 问题只剩标题这类派生信息。正文检索由零件级索引独立维护，既不再按分组聚合，
    // 也就不需要在写入端判断正文属于答复、代码还是命令。
    let turns = load_question_turns_sqlx_tx(tx, tenant_id, question_id).await?;
    let question_text = turns
        .iter()
        .map(|turn| turn.user_text.clone())
        .collect::<Vec<_>>()
        .join("\n\n");
    let title = first_line(&question_text);

    sqlx::query(
        "UPDATE conversation_questions SET title = COALESCE(NULLIF(title, ''), ?1), updated_at = ?2 WHERE tenant_id = ?3 AND id = ?4",
    )
    .bind(&title)
    .bind(now)
    .bind(tenant_id)
    .bind(question_id)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

pub(super) async fn insert_sync_run_sqlx_tx(
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
    .bind(encode_enum(run.status)?)
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

pub async fn insert_conversation_sync_delta_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    sync_run_id: &str,
    record_kind: &str,
    session_id: &str,
    change_kind: &str,
    observed_at: &str,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_sync_deltas (
            tenant_id, sync_run_id, record_kind, session_id, change_kind, observed_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(tenant_id)
    .bind(sync_run_id)
    .bind(record_kind)
    .bind(session_id)
    .bind(change_kind)
    .bind(observed_at)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

pub(super) async fn load_question_turns_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
) -> StoreResult<Vec<ConversationTurn>> {
    let rows = sqlx::query(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at, t.model
        FROM conversation_question_turns qt
        JOIN conversation_turns t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1 AND qt.question_id = ?2
        ORDER BY qt.turn_order ASC, t.turn_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    rows.iter().map(map_sqlx_conversation_turn).collect()
}

pub(super) async fn load_conversation_question_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
) -> StoreResult<Option<ConversationQuestion>> {
    sqlx::query(
        r#"
        SELECT id, session_id, title, created_at, updated_at
        FROM conversation_questions
        WHERE tenant_id = ?1 AND id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::external)?
    .as_ref()
    .map(map_sqlx_conversation_question)
    .transpose()
}

pub(super) async fn question_ids_for_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<Vec<String>> {
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT q.id
        FROM conversation_questions q
        WHERE q.tenant_id = ?1 AND q.session_id = ?2
        ORDER BY COALESCE((SELECT MIN(t.turn_index)
                          FROM conversation_question_turns qt
                          JOIN conversation_turns t
                            ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
                          WHERE qt.tenant_id = q.tenant_id AND qt.question_id = q.id),
                         9223372036854775807),
                 q.created_at ASC,
                 q.id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)
}

pub(super) async fn renumber_questions_for_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<()> {
    let _ = (tx, tenant_id, session_id);
    Ok(())
}
