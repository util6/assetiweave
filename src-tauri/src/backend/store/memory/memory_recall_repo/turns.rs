use chrono::Utc;
use sqlx::{pool::PoolConnection, Sqlite, SqlitePool};

use crate::backend::{
    domain::{
        MemoryRecallSessionStatus, MemoryRecallStructuredOutput, MemoryRecallTurn,
        MemoryRecallTurnStatus,
    },
    store::{StoreError, StoreResult},
};

use super::sessions::SessionGateRow;

#[derive(Debug, sqlx::FromRow)]
pub(super) struct MemoryRecallTurnRow {
    pub(super) id: String,
    pub(super) session_id: String,
    pub(super) sequence: i64,
    pub(super) conversation_session_id: String,
    pub(super) conversation_turn_id: String,
    pub(super) status: String,
    pub(super) structured_output_json: Option<String>,
    pub(super) last_error: Option<String>,
    pub(super) created_at: String,
    pub(super) updated_at: String,
    pub(super) user_text: String,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct RecoveryTurnRow {
    pub(super) id: String,
    pub(super) status: String,
}

pub(crate) async fn load_memory_recall_turn_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    turn_id: &str,
) -> StoreResult<Option<MemoryRecallTurn>> {
    let Some(row) = sqlx::query_as::<_, MemoryRecallTurnRow>(
        r#"
        SELECT r.id, r.session_id, r.sequence, r.conversation_session_id,
               r.conversation_turn_id, r.status, r.structured_output_json,
               r.last_error, r.created_at, r.updated_at, COALESCE(t.user_text, '') AS user_text
        FROM memory_recall_turns r
        LEFT JOIN conversation_turns t
          ON t.tenant_id = r.tenant_id AND t.id = r.conversation_turn_id
        WHERE r.tenant_id = ?1 AND r.id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(turn_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?
    else {
        return Ok(None);
    };
    Ok(Some(map_turn(row)?))
}

pub(crate) async fn load_memory_recall_turns_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<Vec<MemoryRecallTurn>> {
    let rows = sqlx::query_as::<_, MemoryRecallTurnRow>(
        r#"
        SELECT r.id, r.session_id, r.sequence, r.conversation_session_id,
               r.conversation_turn_id, r.status, r.structured_output_json,
               r.last_error, r.created_at, r.updated_at, COALESCE(t.user_text, '') AS user_text
        FROM memory_recall_turns r
        LEFT JOIN conversation_turns t
          ON t.tenant_id = r.tenant_id AND t.id = r.conversation_turn_id
        WHERE r.tenant_id = ?1 AND r.session_id = ?2
        ORDER BY r.sequence ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    rows.into_iter().map(map_turn).collect()
}

pub(crate) async fn list_memory_recall_turns_for_recovery_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<Vec<(String, MemoryRecallTurnStatus)>> {
    let rows = sqlx::query_as::<_, RecoveryTurnRow>(
        "SELECT id, status FROM memory_recall_turns WHERE tenant_id = ?1 AND status IN ('queued', 'running') ORDER BY created_at, id",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    rows.into_iter()
        .map(|row| {
            let status = decode_turn_status(&row.status)?;
            Ok((row.id, status))
        })
        .collect()
}

pub(crate) async fn create_memory_recall_turn_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    turn: &MemoryRecallTurn,
) -> StoreResult<()> {
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    let session = sqlx::query_as::<_, SessionGateRow>(
        "SELECT status, turn_count, active_turn_id FROM memory_recall_sessions WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(&turn.session_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(StoreError::external)?
    .ok_or_else(|| StoreError::NotFound(format!("Recall session not found: {}", turn.session_id)))?;
    if session.status != MemoryRecallSessionStatus::Active.as_str() {
        return Err(StoreError::Conflict(format!(
            "Recall session is not active: {}",
            session.status
        )));
    }
    if session.active_turn_id.is_some() {
        return Err(StoreError::Conflict(
            "Recall session already has an active turn".to_string(),
        ));
    }
    if turn.sequence != session.turn_count {
        return Err(StoreError::Conflict(
            "Recall turn sequence is stale".to_string(),
        ));
    }
    sqlx::query(
        r#"
        INSERT INTO memory_recall_turns (
            tenant_id, id, session_id, sequence, conversation_session_id,
            conversation_turn_id, status, structured_output_json, last_error,
            created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL, ?8, ?8)
        "#,
    )
    .bind(tenant_id)
    .bind(&turn.id)
    .bind(&turn.session_id)
    .bind(turn.sequence)
    .bind(&turn.conversation_session_id)
    .bind(&turn.conversation_turn_id)
    .bind(MemoryRecallTurnStatus::Queued.as_str())
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;
    sqlx::query(
        "UPDATE memory_recall_sessions SET turn_count = turn_count + 1, active_turn_id = ?1, updated_at = ?2 WHERE tenant_id = ?3 AND id = ?4",
    )
    .bind(&turn.id)
    .bind(&now)
    .bind(tenant_id)
    .bind(&turn.session_id)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;
    tx.commit().await.map_err(StoreError::external)
}

pub(crate) async fn mark_memory_recall_turn_running_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    turn_id: &str,
) -> StoreResult<()> {
    update_turn_status_sqlx(
        pool,
        tenant_id,
        turn_id,
        MemoryRecallTurnStatus::Running,
        None,
    )
    .await
}

pub(crate) async fn complete_memory_recall_turn_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    turn_id: &str,
    output: &MemoryRecallStructuredOutput,
) -> StoreResult<()> {
    let output_json = serde_json::to_string(output).map_err(StoreError::external)?;
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    let session_id: String = sqlx::query_scalar(
        "SELECT session_id FROM memory_recall_turns WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(turn_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(StoreError::external)?
    .ok_or_else(|| StoreError::NotFound(format!("Recall turn not found: {turn_id}")))?;
    let result = sqlx::query(
        "UPDATE memory_recall_turns SET status = ?1, structured_output_json = ?2, last_error = NULL, updated_at = ?3 WHERE tenant_id = ?4 AND id = ?5 AND status IN ('queued', 'running')",
    )
    .bind(MemoryRecallTurnStatus::Completed.as_str())
    .bind(output_json)
    .bind(&now)
    .bind(tenant_id)
    .bind(turn_id)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;
    if result.rows_affected() != 1 {
        return Err(StoreError::Conflict(
            "Recall turn is no longer running".to_string(),
        ));
    }
    sqlx::query(
        "UPDATE memory_recall_sessions SET status = 'active', active_turn_id = NULL, last_error = NULL, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3 AND active_turn_id = ?4",
    )
    .bind(&now)
    .bind(tenant_id)
    .bind(&session_id)
    .bind(turn_id)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;
    tx.commit().await.map_err(StoreError::external)
}

pub(crate) async fn fail_memory_recall_turn_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    turn_id: &str,
    status: MemoryRecallTurnStatus,
    error: &str,
) -> StoreResult<()> {
    let now = Utc::now().to_rfc3339();
    let mut connection = begin_immediate(pool).await?;
    let session_id: Option<String> = sqlx::query_scalar(
        "SELECT session_id FROM memory_recall_turns WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(turn_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(StoreError::external)?;
    let Some(session_id) = session_id else {
        let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
        return Err(StoreError::NotFound(format!(
            "Recall turn not found: {turn_id}"
        )));
    };
    let result = sqlx::query(
        "UPDATE memory_recall_turns SET status = ?1, last_error = ?2, updated_at = ?3 WHERE tenant_id = ?4 AND id = ?5 AND status IN ('queued', 'running')",
    )
    .bind(status.as_str())
    .bind(error)
    .bind(&now)
    .bind(tenant_id)
    .bind(turn_id)
    .execute(&mut *connection)
    .await
    .map_err(StoreError::external)?;
    if result.rows_affected() == 0 {
        sqlx::query("COMMIT")
            .execute(&mut *connection)
            .await
            .map_err(StoreError::external)?;
        return Ok(());
    }
    sqlx::query(
        "UPDATE memory_recall_sessions SET status = 'active', active_turn_id = NULL, last_error = ?1, updated_at = ?2 WHERE tenant_id = ?3 AND id = ?4 AND active_turn_id = ?5",
    )
    .bind(error)
    .bind(&now)
    .bind(tenant_id)
    .bind(&session_id)
    .bind(turn_id)
    .execute(&mut *connection)
    .await
    .map_err(StoreError::external)?;
    sqlx::query("COMMIT")
        .execute(&mut *connection)
        .await
        .map(|_| ())
        .map_err(StoreError::external)
}

async fn begin_immediate(pool: &SqlitePool) -> StoreResult<PoolConnection<Sqlite>> {
    let mut connection = pool.acquire().await.map_err(StoreError::external)?;
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *connection)
        .await
        .map_err(StoreError::external)?;
    Ok(connection)
}

pub(crate) async fn retry_memory_recall_turn_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    turn_id: &str,
) -> StoreResult<bool> {
    let result = sqlx::query(
        "UPDATE memory_recall_turns SET status = 'queued', structured_output_json = NULL, last_error = NULL, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3 AND status IN ('failed', 'resume_unavailable')",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(tenant_id)
    .bind(turn_id)
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    if result.rows_affected() != 1 {
        return Ok(false);
    }
    sqlx::query(
        "UPDATE memory_recall_sessions SET status = 'active', active_turn_id = ?1, last_error = NULL, updated_at = ?2 WHERE tenant_id = ?3 AND id = (SELECT session_id FROM memory_recall_turns WHERE tenant_id = ?3 AND id = ?1)",
    )
    .bind(turn_id)
    .bind(Utc::now().to_rfc3339())
    .bind(tenant_id)
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(true)
}

async fn update_turn_status_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    turn_id: &str,
    status: MemoryRecallTurnStatus,
    error: Option<&str>,
) -> StoreResult<()> {
    let result = sqlx::query(
        "UPDATE memory_recall_turns SET status = ?1, last_error = ?2, updated_at = ?3 WHERE tenant_id = ?4 AND id = ?5 AND status = 'queued'",
    )
    .bind(status.as_str())
    .bind(error)
    .bind(Utc::now().to_rfc3339())
    .bind(tenant_id)
    .bind(turn_id)
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    if result.rows_affected() != 1 {
        return Err(StoreError::Conflict(
            "Recall turn is no longer queued".to_string(),
        ));
    }
    Ok(())
}

fn map_turn(row: MemoryRecallTurnRow) -> StoreResult<MemoryRecallTurn> {
    let structured_output = row
        .structured_output_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(StoreError::external)?;
    Ok(MemoryRecallTurn {
        id: row.id,
        session_id: row.session_id,
        sequence: row.sequence,
        conversation_session_id: row.conversation_session_id,
        conversation_turn_id: row.conversation_turn_id,
        status: decode_turn_status(&row.status)?,
        user_text: row.user_text,
        structured_output,
        last_error: row.last_error,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

pub(super) fn decode_turn_status(value: &str) -> StoreResult<MemoryRecallTurnStatus> {
    match value {
        "queued" => Ok(MemoryRecallTurnStatus::Queued),
        "running" => Ok(MemoryRecallTurnStatus::Running),
        "completed" => Ok(MemoryRecallTurnStatus::Completed),
        "failed" => Ok(MemoryRecallTurnStatus::Failed),
        "cancelled" => Ok(MemoryRecallTurnStatus::Cancelled),
        "resume_unavailable" => Ok(MemoryRecallTurnStatus::ResumeUnavailable),
        other => Err(StoreError::External(format!(
            "invalid Recall turn status: {other}"
        ))),
    }
}
