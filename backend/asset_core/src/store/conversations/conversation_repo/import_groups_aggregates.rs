use super::*;

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
