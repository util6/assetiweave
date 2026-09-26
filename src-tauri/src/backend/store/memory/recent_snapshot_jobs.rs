use crate::backend::store::{StoreError, StoreResult};
use sqlx::{Row, SqlitePool};

pub(crate) const MAX_RECENT_MEMORY_JOB_RETRIES: i64 = 5;
pub(crate) const RECENT_MEMORY_JOB_LEASE_SECONDS: i64 = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecentMemoryJob {
    pub(crate) tenant_id: String,
    pub(crate) id: String,
    pub(crate) status: String,
    pub(crate) ownership_token: Option<String>,
    pub(crate) lease_expires_at: Option<String>,
    pub(crate) heartbeat_at: Option<String>,
    pub(crate) attempt_count: i64,
    pub(crate) retry_count: i64,
    pub(crate) retry_at: Option<String>,
    pub(crate) target_watermark_utc: String,
    pub(crate) window_hours: i64,
    pub(crate) target_fingerprint: String,
    pub(crate) content_fingerprint: String,
    pub(crate) work_order_json: String,
    pub(crate) last_error_code: Option<String>,
    pub(crate) last_error_message: Option<String>,
    pub(crate) started_at: Option<String>,
    pub(crate) finished_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

pub(crate) async fn enqueue_recent_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    target_watermark_utc: &str,
    window_hours: i64,
    target_fingerprint: &str,
    content_fingerprint: &str,
    work_order_json: &str,
    now: &str,
) -> StoreResult<String> {
    sqlx::query(
        "INSERT OR IGNORE INTO recent_memory_jobs (\
            tenant_id, id, status, target_watermark_utc, window_hours, target_fingerprint, \
            content_fingerprint, work_order_json, created_at, updated_at\
         ) VALUES (?1, ?2, 'queued', ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
    )
    .bind(tenant_id)
    .bind(job_id)
    .bind(target_watermark_utc)
    .bind(window_hours)
    .bind(target_fingerprint)
    .bind(content_fingerprint)
    .bind(work_order_json)
    .bind(now)
    .execute(pool)
    .await
    .map_err(StoreError::external)?;

    sqlx::query_scalar(
        "SELECT id FROM recent_memory_jobs WHERE tenant_id = ?1 AND target_watermark_utc = ?2 \
         AND window_hours = ?3 AND target_fingerprint = ?4 AND content_fingerprint = ?5 \
         ORDER BY created_at ASC, id ASC LIMIT 1",
    )
    .bind(tenant_id)
    .bind(target_watermark_utc)
    .bind(window_hours)
    .bind(target_fingerprint)
    .bind(content_fingerprint)
    .fetch_one(pool)
    .await
    .map_err(StoreError::external)
}

pub(crate) async fn load_recent_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
) -> StoreResult<Option<RecentMemoryJob>> {
    let row = sqlx::query(
        "SELECT tenant_id, id, status, ownership_token, lease_expires_at, heartbeat_at, \
         attempt_count, retry_count, retry_at, target_watermark_utc, window_hours, \
         target_fingerprint, content_fingerprint, work_order_json, last_error_code, \
         last_error_message, started_at, finished_at, created_at, updated_at \
         FROM recent_memory_jobs WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?;

    Ok(row.map(|row| RecentMemoryJob {
        tenant_id: row.get("tenant_id"),
        id: row.get("id"),
        status: row.get("status"),
        ownership_token: row.get("ownership_token"),
        lease_expires_at: row.get("lease_expires_at"),
        heartbeat_at: row.get("heartbeat_at"),
        attempt_count: row.get("attempt_count"),
        retry_count: row.get("retry_count"),
        retry_at: row.get("retry_at"),
        target_watermark_utc: row.get("target_watermark_utc"),
        window_hours: row.get("window_hours"),
        target_fingerprint: row.get("target_fingerprint"),
        content_fingerprint: row.get("content_fingerprint"),
        work_order_json: row.get("work_order_json"),
        last_error_code: row.get("last_error_code"),
        last_error_message: row.get("last_error_message"),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }))
}

pub(crate) async fn recover_expired_recent_memory_leases_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    now: &str,
) -> StoreResult<()> {
    sqlx::query(
        "UPDATE recent_memory_jobs SET \
            status = CASE WHEN retry_count + 1 >= ?1 THEN 'failed' ELSE 'queued' END, \
            ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, \
            retry_count = retry_count + 1, \
            retry_at = CASE WHEN retry_count + 1 >= ?1 THEN NULL ELSE ?2 END, \
            last_error_code = 'LEASE_EXPIRED', last_error_message = 'recent memory job lease expired', \
            finished_at = CASE WHEN retry_count + 1 >= ?1 THEN ?2 ELSE NULL END, updated_at = ?2 \
         WHERE tenant_id = ?3 AND status = 'running' AND (lease_expires_at IS NULL OR lease_expires_at <= ?2)",
    )
    .bind(MAX_RECENT_MEMORY_JOB_RETRIES)
    .bind(now)
    .bind(tenant_id)
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

pub(crate) async fn list_recent_memory_job_ids_for_scheduler_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    now: &str,
    limit: i64,
) -> StoreResult<Vec<String>> {
    sqlx::query_scalar(
        "SELECT id FROM recent_memory_jobs \
         WHERE tenant_id = ?1 AND retry_count < ?2 AND \
         (status = 'queued' OR (status = 'failed' AND retry_at IS NOT NULL AND retry_at <= ?3)) \
         ORDER BY created_at ASC, id ASC LIMIT ?4",
    )
    .bind(tenant_id)
    .bind(MAX_RECENT_MEMORY_JOB_RETRIES)
    .bind(now)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)
}

pub(crate) async fn claim_recent_memory_job_with_lease_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    ownership_token: &str,
    now: &str,
) -> StoreResult<bool> {
    let lease_expires_at = chrono::DateTime::parse_from_rfc3339(now)
        .map(|value| {
            (value.with_timezone(&chrono::Utc)
                + chrono::Duration::seconds(RECENT_MEMORY_JOB_LEASE_SECONDS))
            .to_rfc3339()
        })
        .map_err(|_| StoreError::Validation("invalid recent memory lease timestamp".to_string()))?;
    let result = sqlx::query(
        "UPDATE recent_memory_jobs SET status = 'running', ownership_token = ?1, \
         lease_expires_at = ?2, heartbeat_at = ?3, started_at = COALESCE(started_at, ?3), \
         attempt_count = attempt_count + 1, updated_at = ?3 \
         WHERE tenant_id = ?4 AND id = ?5 AND retry_count < ?6 AND \
         (status = 'queued' OR (status = 'failed' AND retry_at IS NOT NULL AND retry_at <= ?3))",
    )
    .bind(ownership_token)
    .bind(&lease_expires_at)
    .bind(now)
    .bind(tenant_id)
    .bind(job_id)
    .bind(MAX_RECENT_MEMORY_JOB_RETRIES)
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn heartbeat_recent_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    ownership_token: &str,
    now: &str,
) -> StoreResult<bool> {
    let lease_expires_at = chrono::DateTime::parse_from_rfc3339(now)
        .map(|value| {
            (value.with_timezone(&chrono::Utc)
                + chrono::Duration::seconds(RECENT_MEMORY_JOB_LEASE_SECONDS))
            .to_rfc3339()
        })
        .map_err(|_| StoreError::Validation("invalid recent memory lease timestamp".to_string()))?;
    let result = sqlx::query(
        "UPDATE recent_memory_jobs SET heartbeat_at = ?1, lease_expires_at = ?2, updated_at = ?1 \
         WHERE tenant_id = ?3 AND id = ?4 AND status = 'running' AND ownership_token = ?5 \
         AND lease_expires_at > ?1",
    )
    .bind(now)
    .bind(&lease_expires_at)
    .bind(tenant_id)
    .bind(job_id)
    .bind(ownership_token)
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn finish_recent_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    ownership_token: &str,
    status: &str,
    error_code: Option<&str>,
    error_message: Option<&str>,
    retryable: bool,
    now: &str,
) -> StoreResult<bool> {
    let row = sqlx::query(
        "SELECT retry_count FROM recent_memory_jobs WHERE tenant_id = ?1 AND id = ?2 \
         AND status = 'running' AND ownership_token = ?3",
    )
    .bind(tenant_id)
    .bind(job_id)
    .bind(ownership_token)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?;
    let Some(row) = row else {
        return Ok(false);
    };
    let retry_count: i64 = row.get("retry_count");
    let can_retry = retryable && retry_count < MAX_RECENT_MEMORY_JOB_RETRIES;
    let retry_at = if can_retry {
        Some(
            (chrono::DateTime::parse_from_rfc3339(now)
                .map_err(|_| {
                    StoreError::Validation("invalid recent memory retry timestamp".to_string())
                })?
                .with_timezone(&chrono::Utc)
                + chrono::Duration::seconds(match retry_count {
                    0 => 5,
                    1 => 15,
                    2 => 30,
                    3 => 60,
                    _ => 120,
                }))
            .to_rfc3339(),
        )
    } else {
        None
    };
    let final_status = if can_retry { "failed" } else { status };
    let updated = sqlx::query(
        "UPDATE recent_memory_jobs SET status = ?1, last_error_code = ?2, last_error_message = ?3, \
         finished_at = CASE WHEN ?4 THEN NULL ELSE ?5 END, retry_at = ?6, ownership_token = NULL, \
         lease_expires_at = NULL, heartbeat_at = NULL, retry_count = retry_count + CASE WHEN ?4 THEN 1 ELSE 0 END, \
         updated_at = ?5 WHERE tenant_id = ?7 AND id = ?8 AND status = 'running' AND ownership_token = ?9",
    )
    .bind(final_status)
    .bind(error_code)
    .bind(error_message)
    .bind(can_retry)
    .bind(now)
    .bind(retry_at)
    .bind(tenant_id)
    .bind(job_id)
    .bind(ownership_token)
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(updated.rows_affected() == 1)
}

pub(crate) async fn retry_recent_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    now: &str,
) -> StoreResult<bool> {
    let result = sqlx::query(
        "UPDATE recent_memory_jobs SET status = 'queued', retry_count = 0, retry_at = NULL, last_error_code = NULL, last_error_message = NULL, finished_at = NULL, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3 AND status = 'failed'",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(result.rows_affected() == 1)
}

/// An explicit rebuild is a new user intent, not another automatic retry.
/// It may therefore reopen any terminal job while resetting the bounded retry
/// budget. Automatic reconciliation never calls this function.
pub(crate) async fn restart_recent_memory_job_for_rebuild_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    now: &str,
) -> StoreResult<bool> {
    let result = sqlx::query(
        "UPDATE recent_memory_jobs SET status = 'queued', retry_count = 0, retry_at = NULL, last_error_code = NULL, last_error_message = NULL, started_at = NULL, finished_at = NULL, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3 AND status IN ('succeeded', 'reused', 'failed', 'canceled', 'stale')",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn record_recent_memory_attempt_task_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    task_id: &str,
    now: &str,
) -> StoreResult<()> {
    let state_id = format!("state-{tenant_id}");
    sqlx::query(
        "INSERT INTO recent_memory_state (\
            tenant_id, id, last_successful_snapshot_id, latest_attempt_task_id, \
            latest_attempt_error_code, latest_attempt_error_message, latest_attempt_error_retryable, created_at, updated_at\
         ) VALUES (?1, ?2, NULL, ?3, NULL, NULL, NULL, ?4, ?4) \
         ON CONFLICT (tenant_id) DO UPDATE SET \
            latest_attempt_task_id = excluded.latest_attempt_task_id, \
            latest_attempt_error_code = NULL, \
            latest_attempt_error_message = NULL, \
            latest_attempt_error_retryable = NULL, \
            updated_at = excluded.updated_at",
    )
    .bind(tenant_id)
    .bind(&state_id)
    .bind(task_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}
