use sqlx::SqlitePool;

use crate::backend::{
    domain::{MemoryRecallSession, MemoryRecallSessionStatus},
    store::{StoreError, StoreResult},
};

use super::turns::load_memory_recall_turns_sqlx;

#[derive(Debug, sqlx::FromRow)]
pub(super) struct MemoryRecallSessionRow {
    pub(super) id: String,
    pub(super) status: String,
    pub(super) scope_json: String,
    pub(super) execution_context_key: String,
    pub(super) agent_id: String,
    pub(super) model: Option<String>,
    pub(super) turn_count: i64,
    pub(super) active_turn_id: Option<String>,
    pub(super) last_error: Option<String>,
    pub(super) created_at: String,
    pub(super) updated_at: String,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct SessionGateRow {
    pub(super) status: String,
    pub(super) turn_count: i64,
    pub(super) active_turn_id: Option<String>,
}

pub(crate) async fn create_memory_recall_session_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session: &MemoryRecallSession,
) -> StoreResult<()> {
    let scope_json = serde_json::to_string(&session.scope).map_err(StoreError::external)?;
    sqlx::query(
        r#"
        INSERT INTO memory_recall_sessions (
            tenant_id, id, status, scope_json, execution_context_key, agent_id, model,
            turn_count, active_turn_id, last_error, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
        "#,
    )
    .bind(tenant_id)
    .bind(&session.id)
    .bind(session.status.as_str())
    .bind(scope_json)
    .bind(&session.execution_context_key)
    .bind(&session.agent_id)
    .bind(&session.model)
    .bind(session.turn_count)
    .bind(&session.active_turn_id)
    .bind(&session.last_error)
    .bind(&session.created_at)
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

pub(crate) async fn load_memory_recall_session_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<Option<MemoryRecallSession>> {
    let Some(row) = sqlx::query_as::<_, MemoryRecallSessionRow>(
        r#"
        SELECT id, status, scope_json, execution_context_key, agent_id, model,
               turn_count, active_turn_id, last_error, created_at, updated_at
        FROM memory_recall_sessions
        WHERE tenant_id = ?1 AND id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?
    else {
        return Ok(None);
    };

    let turns = load_memory_recall_turns_sqlx(pool, tenant_id, session_id).await?;
    let status = decode_session_status(&row.status)?;
    let scope = serde_json::from_str(&row.scope_json).map_err(StoreError::external)?;
    Ok(Some(MemoryRecallSession {
        id: row.id,
        status,
        scope,
        execution_context_key: row.execution_context_key,
        agent_id: row.agent_id,
        model: row.model,
        turn_count: row.turn_count,
        active_turn_id: row.active_turn_id,
        last_error: row.last_error,
        created_at: row.created_at,
        updated_at: row.updated_at,
        turns,
    }))
}

pub(super) fn decode_session_status(value: &str) -> StoreResult<MemoryRecallSessionStatus> {
    match value {
        "active" => Ok(MemoryRecallSessionStatus::Active),
        "completed" => Ok(MemoryRecallSessionStatus::Completed),
        "failed" => Ok(MemoryRecallSessionStatus::Failed),
        "cancelled" => Ok(MemoryRecallSessionStatus::Cancelled),
        "resume_unavailable" => Ok(MemoryRecallSessionStatus::ResumeUnavailable),
        other => Err(StoreError::External(format!(
            "invalid Recall session status: {other}"
        ))),
    }
}
