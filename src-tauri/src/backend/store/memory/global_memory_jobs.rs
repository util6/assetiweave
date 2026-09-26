use crate::backend::domain::{GlobalMemoryJob, GlobalMemoryJobStatus};
use crate::backend::store::{StoreError, StoreResult};
use chrono::{DateTime, Duration, Utc};
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::global_memory_repo::{
    global_input_set_from_projects, global_memory_id, GlobalMemoryCandidateProjectRow,
    GLOBAL_MEMORY_CANDIDATE_PROJECTS_QUERY,
};

pub(crate) const GLOBAL_MEMORY_JOB_LEASE: Duration = Duration::minutes(2);
pub(crate) const MAX_GLOBAL_MEMORY_JOB_RETRIES: i64 = 5;

#[derive(Debug, sqlx::FromRow)]
struct GlobalJobStatusAndFingerprintRow {
    status: String,
    input_fingerprint: String,
}

#[derive(Debug, sqlx::FromRow)]
struct GlobalMemoryJobRow {
    tenant_id: String,
    id: String,
    target_watermark: i64,
    input_fingerprint: String,
    status: String,
    attempt_count: i64,
    retry_count: i64,
    retry_at: Option<String>,
    last_error: Option<String>,
    ownership_token: Option<String>,
    lease_expires_at: Option<String>,
    heartbeat_at: Option<String>,
    started_at: Option<String>,
    finished_at: Option<String>,
    created_at: String,
    updated_at: String,
}

/// A Project Memory success dirties the single tenant Global Memory job in the
/// same transaction, so a process crash cannot acknowledge a project update
/// without leaving a durable global rebuild target.
pub(crate) async fn enqueue_global_memory_job_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    now: &str,
) -> StoreResult<Option<String>> {
    let rows = sqlx::query_as::<_, GlobalMemoryCandidateProjectRow>(
        GLOBAL_MEMORY_CANDIDATE_PROJECTS_QUERY,
    )
    .bind(tenant_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::Db)?;
    if rows.is_empty() {
        return Ok(None);
    }
    let projects = rows.into_iter().map(Into::into).collect();
    let inputs = global_input_set_from_projects(projects);
    let memory_id = global_memory_id(tenant_id);
    sqlx::query(
        "INSERT OR IGNORE INTO global_memories (tenant_id, id, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
    )
    .bind(tenant_id)
    .bind(&memory_id)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::Db)?;
    let current = sqlx::query_as::<_, GlobalJobStatusAndFingerprintRow>(
        "SELECT status, input_fingerprint FROM global_memory_jobs WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::Db)?;
    if let Some(row) = current {
        if matches!(row.status.as_str(), "queued" | "running")
            && row.input_fingerprint == inputs.fingerprint
        {
            return Ok(Some(memory_id));
        }
        if row.status == "succeeded" && row.input_fingerprint == inputs.fingerprint {
            return Ok(None);
        }
    }
    sqlx::query(
        "INSERT INTO global_memory_jobs (tenant_id, id, target_watermark, input_fingerprint, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'queued', ?5, ?5) ON CONFLICT (tenant_id) DO UPDATE SET target_watermark = excluded.target_watermark, input_fingerprint = excluded.input_fingerprint, status = CASE WHEN global_memory_jobs.status = 'running' THEN 'running' ELSE 'queued' END, retry_at = NULL, last_error = NULL, finished_at = NULL, updated_at = excluded.updated_at",
    )
    .bind(tenant_id)
    .bind(&memory_id)
    .bind(inputs.watermark)
    .bind(&inputs.fingerprint)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::Db)?;
    Ok(Some(memory_id))
}

pub(crate) async fn list_global_memory_job_ids_for_scheduler_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    now: &str,
) -> StoreResult<Vec<String>> {
    sqlx::query_scalar(
        "SELECT id FROM global_memory_jobs WHERE tenant_id = ?1 AND (status = 'queued' OR (status = 'failed' AND retry_at IS NOT NULL AND retry_at <= ?2)) ORDER BY updated_at ASC, id ASC LIMIT 1",
    )
    .bind(tenant_id)
    .bind(now)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Db)
}

pub(crate) async fn load_global_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
) -> StoreResult<Option<GlobalMemoryJob>> {
    let row = sqlx::query_as::<_, GlobalMemoryJobRow>(
        "SELECT tenant_id, id, target_watermark, input_fingerprint, status, attempt_count, retry_count, retry_at, last_error, ownership_token, lease_expires_at, heartbeat_at, started_at, finished_at, created_at, updated_at FROM global_memory_jobs WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::Db)?;
    row.map(map_job).transpose()
}

pub(crate) async fn claim_global_memory_job_with_lease_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    now: &str,
    ownership_token: &str,
) -> StoreResult<Option<GlobalMemoryJob>> {
    let lease_expires_at = lease_expiry(now);
    let result = sqlx::query(
        "UPDATE global_memory_jobs SET status = 'running', ownership_token = ?1, lease_expires_at = ?2, heartbeat_at = ?3, started_at = COALESCE(started_at, ?3), attempt_count = attempt_count + 1, updated_at = ?3 WHERE tenant_id = ?4 AND id = ?5 AND (status = 'queued' OR (status = 'failed' AND (retry_at IS NULL OR retry_at <= ?3)) OR (status = 'running' AND (lease_expires_at IS NULL OR lease_expires_at <= ?3)))",
    )
    .bind(ownership_token)
    .bind(lease_expires_at)
    .bind(now)
    .bind(tenant_id)
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    load_global_memory_job_sqlx(pool, tenant_id, job_id).await
}

pub(crate) async fn heartbeat_global_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    ownership_token: &str,
    now: &str,
) -> StoreResult<bool> {
    let result = sqlx::query(
        "UPDATE global_memory_jobs SET heartbeat_at = ?1, lease_expires_at = ?2, updated_at = ?1 WHERE tenant_id = ?3 AND id = ?4 AND status = 'running' AND ownership_token = ?5 AND lease_expires_at > ?1",
    )
    .bind(now)
    .bind(lease_expiry(now))
    .bind(tenant_id)
    .bind(job_id)
    .bind(ownership_token)
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn recover_expired_global_memory_leases_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    now: &str,
) -> StoreResult<u64> {
    let result = sqlx::query(
        "UPDATE global_memory_jobs SET status = CASE WHEN retry_count + 1 >= ?3 THEN 'failed' ELSE 'queued' END, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, retry_count = retry_count + 1, retry_at = CASE WHEN retry_count + 1 >= ?3 THEN NULL ELSE ?1 END, finished_at = CASE WHEN retry_count + 1 >= ?3 THEN ?1 ELSE NULL END, last_error = 'lease_expired', updated_at = ?1 WHERE tenant_id = ?2 AND status = 'running' AND (lease_expires_at IS NULL OR lease_expires_at <= ?1)",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(MAX_GLOBAL_MEMORY_JOB_RETRIES)
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(result.rows_affected())
}

pub(crate) async fn mark_global_memory_job_failed_with_lease_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    ownership_token: &str,
    error_message: &str,
    now: &str,
    retryable: bool,
) -> StoreResult<bool> {
    let retry_count: Option<i64> = sqlx::query_scalar(
        "SELECT retry_count FROM global_memory_jobs WHERE tenant_id = ?1 AND id = ?2 AND status = 'running' AND ownership_token = ?3",
    )
    .bind(tenant_id)
    .bind(job_id)
    .bind(ownership_token)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::Db)?;
    let Some(retry_count) = retry_count else {
        return Ok(false);
    };
    let next_retry_count = retry_count + 1;
    let retry_at = if retryable && next_retry_count < MAX_GLOBAL_MEMORY_JOB_RETRIES {
        let delay = 5_i64.saturating_mul(2_i64.saturating_pow(next_retry_count.min(6) as u32));
        Some((parse_utc(now) + Duration::seconds(delay)).to_rfc3339())
    } else {
        None
    };
    let result = sqlx::query(
        "UPDATE global_memory_jobs SET status = 'failed', last_error = ?1, finished_at = ?2, updated_at = ?2, retry_count = ?3, retry_at = ?4, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL WHERE tenant_id = ?5 AND id = ?6 AND status = 'running' AND ownership_token = ?7",
    )
    .bind(error_message)
    .bind(now)
    .bind(next_retry_count)
    .bind(retry_at)
    .bind(tenant_id)
    .bind(job_id)
    .bind(ownership_token)
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn cancel_global_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    now: &str,
) -> StoreResult<bool> {
    let result = sqlx::query(
        "UPDATE global_memory_jobs SET status = 'canceled', last_error = 'canceled', finished_at = ?1, updated_at = ?1, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL WHERE tenant_id = ?2 AND id = ?3 AND status IN ('queued', 'running', 'failed')",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn retry_global_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
) -> StoreResult<bool> {
    let result = sqlx::query(
        "UPDATE global_memory_jobs SET status = 'queued', last_error = NULL, retry_at = NULL, finished_at = NULL, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3 AND status = 'failed'",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(tenant_id)
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(result.rows_affected() == 1)
}

fn map_job(row: GlobalMemoryJobRow) -> StoreResult<GlobalMemoryJob> {
    Ok(GlobalMemoryJob {
        tenant_id: row.tenant_id,
        id: row.id,
        target_watermark: row.target_watermark,
        input_fingerprint: row.input_fingerprint,
        status: parse_job_status(row.status)?,
        attempt_count: row.attempt_count,
        retry_count: row.retry_count,
        retry_at: row.retry_at,
        last_error: row.last_error,
        ownership_token: row.ownership_token,
        lease_expires_at: row.lease_expires_at,
        heartbeat_at: row.heartbeat_at,
        started_at: row.started_at,
        finished_at: row.finished_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn parse_job_status(value: String) -> StoreResult<GlobalMemoryJobStatus> {
    match value.as_str() {
        "queued" => Ok(GlobalMemoryJobStatus::Queued),
        "running" => Ok(GlobalMemoryJobStatus::Running),
        "succeeded" => Ok(GlobalMemoryJobStatus::Succeeded),
        "failed" => Ok(GlobalMemoryJobStatus::Failed),
        "canceled" => Ok(GlobalMemoryJobStatus::Canceled),
        _ => Err(StoreError::External(format!(
            "unknown Global Memory job status: {value}"
        ))),
    }
}

fn lease_expiry(now: &str) -> String {
    (parse_utc(now) + GLOBAL_MEMORY_JOB_LEASE).to_rfc3339()
}

fn parse_utc(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}
