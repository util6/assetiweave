use chrono::Utc;
use sqlx::{pool::PoolConnection, Row, Sqlite, SqlitePool};

use crate::backend::{
    models::{
        MemoryRecallSession, MemoryRecallSessionStatus, MemoryRecallStructuredOutput,
        MemoryRecallTurn, MemoryRecallTurnStatus,
    },
    runtime::{AppError, AppResult},
};

pub(crate) async fn create_memory_recall_session_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session: &MemoryRecallSession,
) -> AppResult<()> {
    let scope_json = serde_json::to_string(&session.scope).map_err(AppError::external)?;
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
    .map_err(AppError::external)?;
    Ok(())
}

pub(crate) async fn load_memory_recall_session_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
) -> AppResult<Option<MemoryRecallSession>> {
    let Some(row) = sqlx::query(
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
    .map_err(AppError::external)?
    else {
        return Ok(None);
    };

    let turns = load_memory_recall_turns_sqlx(pool, tenant_id, session_id).await?;
    Ok(Some(MemoryRecallSession {
        id: row.try_get("id").map_err(AppError::external)?,
        status: decode_session_status(
            &row.try_get::<String, _>("status")
                .map_err(AppError::external)?,
        )?,
        scope: serde_json::from_str(
            &row.try_get::<String, _>("scope_json")
                .map_err(AppError::external)?,
        )
        .map_err(AppError::external)?,
        execution_context_key: row
            .try_get("execution_context_key")
            .map_err(AppError::external)?,
        agent_id: row.try_get("agent_id").map_err(AppError::external)?,
        model: row.try_get("model").map_err(AppError::external)?,
        turn_count: row.try_get("turn_count").map_err(AppError::external)?,
        active_turn_id: row.try_get("active_turn_id").map_err(AppError::external)?,
        last_error: row.try_get("last_error").map_err(AppError::external)?,
        created_at: row.try_get("created_at").map_err(AppError::external)?,
        updated_at: row.try_get("updated_at").map_err(AppError::external)?,
        turns,
    }))
}

pub(crate) async fn load_memory_recall_turn_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    turn_id: &str,
) -> AppResult<Option<MemoryRecallTurn>> {
    let Some(row) = sqlx::query(
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
    .map_err(AppError::external)?
    else {
        return Ok(None);
    };
    Ok(Some(map_turn(&row)?))
}

pub(crate) async fn load_memory_recall_turns_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
) -> AppResult<Vec<MemoryRecallTurn>> {
    let rows = sqlx::query(
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
    .map_err(AppError::external)?;
    rows.iter().map(map_turn).collect()
}

pub(crate) async fn list_memory_recall_turns_for_recovery_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<(String, MemoryRecallTurnStatus)>> {
    let rows = sqlx::query(
        "SELECT id, status FROM memory_recall_turns WHERE tenant_id = ?1 AND status IN ('queued', 'running') ORDER BY created_at, id",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    rows.into_iter()
        .map(|row| {
            let id = row.try_get("id").map_err(AppError::external)?;
            let status = decode_turn_status(
                &row.try_get::<String, _>("status")
                    .map_err(AppError::external)?,
            )?;
            Ok((id, status))
        })
        .collect()
}

pub(crate) async fn create_memory_recall_turn_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    turn: &MemoryRecallTurn,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let session = sqlx::query(
        "SELECT status, turn_count, active_turn_id FROM memory_recall_sessions WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(&turn.session_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::external)?
    .ok_or_else(|| AppError::NotFound(format!("Recall session not found: {}", turn.session_id)))?;
    let status: String = session.try_get("status").map_err(AppError::external)?;
    if status != MemoryRecallSessionStatus::Active.as_str() {
        return Err(AppError::Conflict(format!(
            "Recall session is not active: {status}"
        )));
    }
    let active_turn_id: Option<String> = session
        .try_get("active_turn_id")
        .map_err(AppError::external)?;
    if active_turn_id.is_some() {
        return Err(AppError::Conflict(
            "Recall session already has an active turn".to_string(),
        ));
    }
    let expected_sequence: i64 = session.try_get("turn_count").map_err(AppError::external)?;
    if turn.sequence != expected_sequence {
        return Err(AppError::Conflict(
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
    .map_err(AppError::external)?;
    sqlx::query(
        "UPDATE memory_recall_sessions SET turn_count = turn_count + 1, active_turn_id = ?1, updated_at = ?2 WHERE tenant_id = ?3 AND id = ?4",
    )
    .bind(&turn.id)
    .bind(&now)
    .bind(tenant_id)
    .bind(&turn.session_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)
}

pub(crate) async fn mark_memory_recall_turn_running_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    turn_id: &str,
) -> AppResult<()> {
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
) -> AppResult<()> {
    let output_json = serde_json::to_string(output).map_err(AppError::external)?;
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let session_id: String = sqlx::query_scalar(
        "SELECT session_id FROM memory_recall_turns WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(turn_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::external)?
    .ok_or_else(|| AppError::NotFound(format!("Recall turn not found: {turn_id}")))?;
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
    .map_err(AppError::external)?;
    if result.rows_affected() != 1 {
        return Err(AppError::Conflict(
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
    .map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)
}

pub(crate) async fn fail_memory_recall_turn_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    turn_id: &str,
    status: MemoryRecallTurnStatus,
    error: &str,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    // Cancellation competes with the provider task's status update. A deferred
    // SQLite transaction can keep a read snapshot and then fail with
    // SQLITE_BUSY_SNAPSHOT when it tries to promote that snapshot to a writer.
    // Acquire the write reservation before reading anything in this transaction
    // so cancellation is serialized and remains the winner or a no-op.
    let mut connection = begin_immediate(pool).await?;
    let session_id: Option<String> = sqlx::query_scalar(
        "SELECT session_id FROM memory_recall_turns WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(turn_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(AppError::external)?;
    let Some(session_id) = session_id else {
        let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
        return Err(AppError::NotFound(format!(
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
    .map_err(AppError::external)?;
    if result.rows_affected() == 0 {
        sqlx::query("COMMIT")
            .execute(&mut *connection)
            .await
            .map_err(AppError::external)?;
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
    .map_err(AppError::external)?;
    sqlx::query("COMMIT")
        .execute(&mut *connection)
        .await
        .map(|_| ())
        .map_err(AppError::external)
}

async fn begin_immediate(pool: &SqlitePool) -> AppResult<PoolConnection<Sqlite>> {
    let mut connection = pool.acquire().await.map_err(AppError::external)?;
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *connection)
        .await
        .map_err(AppError::external)?;
    Ok(connection)
}

pub(crate) async fn retry_memory_recall_turn_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    turn_id: &str,
) -> AppResult<bool> {
    let result = sqlx::query(
        "UPDATE memory_recall_turns SET status = 'queued', structured_output_json = NULL, last_error = NULL, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3 AND status IN ('failed', 'resume_unavailable')",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(tenant_id)
    .bind(turn_id)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
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
    .map_err(AppError::external)?;
    Ok(true)
}

async fn update_turn_status_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    turn_id: &str,
    status: MemoryRecallTurnStatus,
    error: Option<&str>,
) -> AppResult<()> {
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
    .map_err(AppError::external)?;
    if result.rows_affected() != 1 {
        return Err(AppError::Conflict(
            "Recall turn is no longer queued".to_string(),
        ));
    }
    Ok(())
}

fn map_turn(row: &sqlx::sqlite::SqliteRow) -> AppResult<MemoryRecallTurn> {
    let structured_output = row
        .try_get::<Option<String>, _>("structured_output_json")?
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(AppError::external)?;
    Ok(MemoryRecallTurn {
        id: row.try_get("id").map_err(AppError::external)?,
        session_id: row.try_get("session_id").map_err(AppError::external)?,
        sequence: row.try_get("sequence").map_err(AppError::external)?,
        conversation_session_id: row
            .try_get("conversation_session_id")
            .map_err(AppError::external)?,
        conversation_turn_id: row
            .try_get("conversation_turn_id")
            .map_err(AppError::external)?,
        status: decode_turn_status(
            &row.try_get::<String, _>("status")
                .map_err(AppError::external)?,
        )?,
        user_text: row.try_get("user_text").map_err(AppError::external)?,
        structured_output,
        last_error: row.try_get("last_error").map_err(AppError::external)?,
        created_at: row.try_get("created_at").map_err(AppError::external)?,
        updated_at: row.try_get("updated_at").map_err(AppError::external)?,
    })
}

fn decode_session_status(value: &str) -> AppResult<MemoryRecallSessionStatus> {
    match value {
        "active" => Ok(MemoryRecallSessionStatus::Active),
        "completed" => Ok(MemoryRecallSessionStatus::Completed),
        "failed" => Ok(MemoryRecallSessionStatus::Failed),
        "cancelled" => Ok(MemoryRecallSessionStatus::Cancelled),
        "resume_unavailable" => Ok(MemoryRecallSessionStatus::ResumeUnavailable),
        other => Err(AppError::External(format!(
            "invalid Recall session status: {other}"
        ))),
    }
}

fn decode_turn_status(value: &str) -> AppResult<MemoryRecallTurnStatus> {
    match value {
        "queued" => Ok(MemoryRecallTurnStatus::Queued),
        "running" => Ok(MemoryRecallTurnStatus::Running),
        "completed" => Ok(MemoryRecallTurnStatus::Completed),
        "failed" => Ok(MemoryRecallTurnStatus::Failed),
        "cancelled" => Ok(MemoryRecallTurnStatus::Cancelled),
        "resume_unavailable" => Ok(MemoryRecallTurnStatus::ResumeUnavailable),
        other => Err(AppError::External(format!(
            "invalid Recall turn status: {other}"
        ))),
    }
}
