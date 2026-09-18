use crate::backend::runtime::{AppError, AppResult};
use sqlx::{Row, SqlitePool};

pub(crate) const MAX_MEMORY_V2_MAINTENANCE_RETRIES: i64 = 5;
pub(crate) const MEMORY_V2_MAINTENANCE_LEASE_SECONDS: i64 = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryV2MaintenanceJob {
    pub(crate) tenant_id: String,
    pub(crate) id: String,
    pub(crate) purpose: String,
    pub(crate) project_key: Option<String>,
    pub(crate) project_path: Option<String>,
    pub(crate) status: String,
    pub(crate) ownership_token: Option<String>,
    pub(crate) lease_expires_at: Option<String>,
    pub(crate) heartbeat_at: Option<String>,
    pub(crate) retry_count: i64,
    pub(crate) retry_at: Option<String>,
    pub(crate) input_fingerprint: String,
    pub(crate) work_order_json: String,
    pub(crate) last_error_code: Option<String>,
    pub(crate) last_error_message: Option<String>,
    pub(crate) started_at: Option<String>,
    pub(crate) finished_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

pub(crate) async fn enqueue_memory_v2_maintenance_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    purpose: &str,
    project_key: Option<&str>,
    project_path: Option<&str>,
    input_fingerprint: &str,
    work_order_json: &str,
    now: &str,
) -> AppResult<String> {
    if !matches!(purpose, "project_consolidation" | "global_consolidation") {
        return Err(AppError::Validation(
            "invalid memory v2 maintenance purpose".to_string(),
        ));
    }

    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let existing = sqlx::query(
        "SELECT id, status FROM memory_v2_maintenance_jobs
         WHERE tenant_id = ?1 AND purpose = ?2
           AND COALESCE(project_key, '') = COALESCE(?3, '')
         LIMIT 1",
    )
    .bind(tenant_id)
    .bind(purpose)
    .bind(project_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::external)?;

    if let Some(row) = existing {
        let existing_id: String = row.get("id");
        let existing_status: String = row.get("status");
        if existing_status == "running" {
            tx.commit().await.map_err(AppError::external)?;
            return Ok(existing_id);
        }

        sqlx::query(
            "UPDATE memory_v2_maintenance_jobs SET
                status = 'queued', project_path = ?1, input_fingerprint = ?2,
                work_order_json = ?3, ownership_token = NULL, lease_expires_at = NULL,
                heartbeat_at = NULL, retry_count = 0, retry_at = NULL,
                last_error_code = NULL, last_error_message = NULL,
                started_at = NULL, finished_at = NULL, updated_at = ?4
             WHERE tenant_id = ?5 AND id = ?6",
        )
        .bind(project_path)
        .bind(input_fingerprint)
        .bind(work_order_json)
        .bind(now)
        .bind(tenant_id)
        .bind(&existing_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
        tx.commit().await.map_err(AppError::external)?;
        return Ok(existing_id);
    }

    sqlx::query(
        "INSERT INTO memory_v2_maintenance_jobs (
            tenant_id, id, purpose, project_key, project_path, status,
            input_fingerprint, work_order_json, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, 'queued', ?6, ?7, ?8, ?8)",
    )
    .bind(tenant_id)
    .bind(job_id)
    .bind(purpose)
    .bind(project_key)
    .bind(project_path)
    .bind(input_fingerprint)
    .bind(work_order_json)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)?;
    Ok(job_id.to_string())
}

pub(crate) async fn load_memory_v2_maintenance_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
) -> AppResult<Option<MemoryV2MaintenanceJob>> {
    let row = sqlx::query(
        "SELECT tenant_id, id, purpose, project_key, project_path, status,
                ownership_token, lease_expires_at, heartbeat_at, retry_count,
                retry_at, input_fingerprint, work_order_json, last_error_code,
                last_error_message, started_at, finished_at, created_at, updated_at
         FROM memory_v2_maintenance_jobs WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;

    Ok(row.map(|row| MemoryV2MaintenanceJob {
        tenant_id: row.get("tenant_id"),
        id: row.get("id"),
        purpose: row.get("purpose"),
        project_key: row.get("project_key"),
        project_path: row.get("project_path"),
        status: row.get("status"),
        ownership_token: row.get("ownership_token"),
        lease_expires_at: row.get("lease_expires_at"),
        heartbeat_at: row.get("heartbeat_at"),
        retry_count: row.get("retry_count"),
        retry_at: row.get("retry_at"),
        input_fingerprint: row.get("input_fingerprint"),
        work_order_json: row.get("work_order_json"),
        last_error_code: row.get("last_error_code"),
        last_error_message: row.get("last_error_message"),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }))
}

pub(crate) async fn recover_expired_memory_v2_maintenance_leases_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    now: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE memory_v2_maintenance_jobs SET
            status = CASE WHEN retry_count + 1 >= ?1 THEN 'failed' ELSE 'queued' END,
            ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL,
            retry_count = retry_count + 1,
            retry_at = CASE WHEN retry_count + 1 >= ?1 THEN NULL ELSE ?2 END,
            last_error_code = 'LEASE_EXPIRED',
            last_error_message = 'memory v2 maintenance job lease expired',
            finished_at = CASE WHEN retry_count + 1 >= ?1 THEN ?2 ELSE NULL END,
            updated_at = ?2
         WHERE tenant_id = ?3 AND status = 'running'
           AND (lease_expires_at IS NULL OR lease_expires_at <= ?2)",
    )
    .bind(MAX_MEMORY_V2_MAINTENANCE_RETRIES)
    .bind(now)
    .bind(tenant_id)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    Ok(())
}

pub(crate) async fn list_memory_v2_maintenance_job_ids_for_scheduler_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    now: &str,
    limit: i64,
) -> AppResult<Vec<String>> {
    sqlx::query_scalar(
        "SELECT id FROM memory_v2_maintenance_jobs
         WHERE tenant_id = ?1 AND retry_count < ?2
           AND (status = 'queued' OR (status = 'failed' AND retry_at IS NOT NULL AND retry_at <= ?3))
         ORDER BY created_at ASC, id ASC LIMIT ?4",
    )
    .bind(tenant_id)
    .bind(MAX_MEMORY_V2_MAINTENANCE_RETRIES)
    .bind(now)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)
}

fn lease_expiry(now: &str) -> AppResult<String> {
    chrono::DateTime::parse_from_rfc3339(now)
        .map(|value| {
            (value.with_timezone(&chrono::Utc)
                + chrono::Duration::seconds(MEMORY_V2_MAINTENANCE_LEASE_SECONDS))
            .to_rfc3339()
        })
        .map_err(|_| AppError::Validation("invalid memory v2 maintenance timestamp".to_string()))
}

pub(crate) async fn claim_memory_v2_maintenance_job_with_lease_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    ownership_token: &str,
    now: &str,
) -> AppResult<bool> {
    let lease_expires_at = lease_expiry(now)?;
    let result = sqlx::query(
        "UPDATE memory_v2_maintenance_jobs SET status = 'running',
            ownership_token = ?1, lease_expires_at = ?2, heartbeat_at = ?3,
            started_at = COALESCE(started_at, ?3), retry_at = NULL, updated_at = ?3
         WHERE tenant_id = ?4 AND id = ?5 AND retry_count < ?6
           AND (status = 'queued' OR (status = 'failed' AND retry_at IS NOT NULL AND retry_at <= ?3))",
    )
    .bind(ownership_token)
    .bind(&lease_expires_at)
    .bind(now)
    .bind(tenant_id)
    .bind(job_id)
    .bind(MAX_MEMORY_V2_MAINTENANCE_RETRIES)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn heartbeat_memory_v2_maintenance_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    ownership_token: &str,
    now: &str,
) -> AppResult<bool> {
    let lease_expires_at = lease_expiry(now)?;
    let result = sqlx::query(
        "UPDATE memory_v2_maintenance_jobs SET heartbeat_at = ?1,
            lease_expires_at = ?2, updated_at = ?1
         WHERE tenant_id = ?3 AND id = ?4 AND status = 'running'
           AND ownership_token = ?5 AND lease_expires_at > ?1",
    )
    .bind(now)
    .bind(&lease_expires_at)
    .bind(tenant_id)
    .bind(job_id)
    .bind(ownership_token)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn finish_memory_v2_maintenance_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    ownership_token: &str,
    status: &str,
    error_code: Option<&str>,
    error_message: Option<&str>,
    retryable: bool,
    now: &str,
) -> AppResult<bool> {
    let row = sqlx::query(
        "SELECT retry_count FROM memory_v2_maintenance_jobs
         WHERE tenant_id = ?1 AND id = ?2 AND status = 'running' AND ownership_token = ?3",
    )
    .bind(tenant_id)
    .bind(job_id)
    .bind(ownership_token)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;
    let Some(row) = row else {
        return Ok(false);
    };
    let retry_count: i64 = row.get("retry_count");
    let can_retry = retryable && retry_count < MAX_MEMORY_V2_MAINTENANCE_RETRIES;
    let retry_at = if can_retry {
        let delay = match retry_count {
            0 => 5,
            1 => 15,
            2 => 30,
            3 => 60,
            _ => 120,
        };
        Some(
            (chrono::DateTime::parse_from_rfc3339(now)
                .map_err(|_| {
                    AppError::Validation("invalid memory v2 maintenance timestamp".to_string())
                })?
                .with_timezone(&chrono::Utc)
                + chrono::Duration::seconds(delay))
            .to_rfc3339(),
        )
    } else {
        None
    };
    let final_status = if can_retry { "failed" } else { status };
    let updated = sqlx::query(
        "UPDATE memory_v2_maintenance_jobs SET status = ?1,
            last_error_code = ?2, last_error_message = ?3,
            finished_at = CASE WHEN ?4 THEN NULL ELSE ?5 END,
            retry_at = ?6, ownership_token = NULL, lease_expires_at = NULL,
            heartbeat_at = NULL,
            retry_count = retry_count + CASE WHEN ?4 THEN 1 ELSE 0 END,
            updated_at = ?5
         WHERE tenant_id = ?7 AND id = ?8 AND status = 'running' AND ownership_token = ?9",
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
    .map_err(AppError::external)?;
    Ok(updated.rows_affected() == 1)
}

/// Atomically fences a successful maintenance result with the same SQLite
/// transaction that publishes L2/L3 changes. A worker whose lease expired (or
/// was replaced) cannot commit memory changes and then report success.
pub(crate) async fn complete_memory_v2_maintenance_job_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    tenant_id: &str,
    job_id: &str,
    ownership_token: &str,
    now: &str,
) -> AppResult<bool> {
    let updated = sqlx::query(
        "UPDATE memory_v2_maintenance_jobs SET status = 'succeeded',
            last_error_code = NULL, last_error_message = NULL,
            finished_at = ?1, retry_at = NULL, ownership_token = NULL,
            lease_expires_at = NULL, heartbeat_at = NULL, updated_at = ?1
         WHERE tenant_id = ?2 AND id = ?3 AND status = 'running'
           AND ownership_token = ?4 AND lease_expires_at > ?1",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(job_id)
    .bind(ownership_token)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    Ok(updated.rows_affected() == 1)
}

pub(crate) async fn retry_memory_v2_maintenance_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    now: &str,
) -> AppResult<bool> {
    let result = sqlx::query(
        "UPDATE memory_v2_maintenance_jobs SET status = 'queued', retry_at = NULL,
            last_error_code = NULL, last_error_message = NULL, finished_at = NULL,
            ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL,
            updated_at = ?1
         WHERE tenant_id = ?2 AND id = ?3 AND status IN ('failed', 'stale', 'canceled')
           AND retry_count < ?4",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(job_id)
    .bind(MAX_MEMORY_V2_MAINTENANCE_RETRIES)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    Ok(result.rows_affected() == 1)
}

#[cfg(test)]
#[path = "memory_v2_maintenance_repo_tests.rs"]
mod tests;
