use super::*;

pub(crate) async fn load_session_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
) -> StoreResult<Option<SessionMemoryJob>> {
    let row = sqlx::query_as::<_, SessionMemoryJobRow>(
        "SELECT tenant_id, id, session_id, source_id, source_revision, source_fingerprint, contract_version, prompt_version, source_event_id, source_sync_run_id, status, not_before, attempt_count, last_error, started_at, finished_at, created_at, updated_at, ownership_token, lease_expires_at, heartbeat_at, retry_count, retry_at, watermark, recipe_id, recipe_revision, recipe_content_hash, budget_policy_version, work_order_json FROM session_memory_jobs WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::Db)?;
    row.map(|r| r.try_into_job()).transpose()
}

pub(crate) async fn claim_session_memory_job_with_lease_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    now: &str,
    ready_override: bool,
    ownership_token: &str,
    lease_duration: Duration,
) -> StoreResult<Option<SessionMemoryJob>> {
    let lease_expires_at = lease_expires_at(now, lease_duration)?;
    let result = sqlx::query(
        r#"
        UPDATE session_memory_jobs
        SET status = 'running', attempt_count = attempt_count + 1,
            started_at = COALESCE(started_at, ?1), updated_at = ?1,
            ownership_token = ?5, lease_expires_at = ?6, heartbeat_at = ?1
        WHERE tenant_id = ?2 AND id = ?3
          AND ((status = 'queued' AND (?4 = 1 OR not_before <= ?1))
               OR (status = 'failed' AND retry_at IS NOT NULL AND retry_at <= ?1))
          AND NOT EXISTS (
              SELECT 1 FROM session_memory_jobs running
              WHERE running.tenant_id = session_memory_jobs.tenant_id
                AND running.session_id = session_memory_jobs.session_id
                AND running.status = 'running'
                AND running.id <> session_memory_jobs.id
          )
        "#,
    )
    .bind(now)
    .bind(tenant_id)
    .bind(job_id)
    .bind(i64::from(ready_override))
    .bind(ownership_token)
    .bind(&lease_expires_at)
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    load_session_memory_job_sqlx(pool, tenant_id, job_id).await
}

pub(crate) async fn heartbeat_session_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    ownership_token: &str,
    now: &str,
    lease_duration: Duration,
) -> StoreResult<bool> {
    let lease_expires_at = lease_expires_at(now, lease_duration)?;
    let result = sqlx::query(
        "UPDATE session_memory_jobs SET heartbeat_at = ?1, lease_expires_at = ?2, updated_at = ?1 WHERE tenant_id = ?3 AND id = ?4 AND status = 'running' AND ownership_token = ?5 AND lease_expires_at > ?1",
    )
    .bind(now)
    .bind(lease_expires_at)
    .bind(tenant_id)
    .bind(job_id)
    .bind(ownership_token)
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn recover_expired_session_memory_leases_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    now: &str,
) -> StoreResult<usize> {
    let result = sqlx::query(
        "UPDATE session_memory_jobs SET status = CASE WHEN retry_count + 1 >= ?3 THEN 'failed' ELSE 'queued' END, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, retry_count = retry_count + 1, retry_at = CASE WHEN retry_count + 1 >= ?3 THEN NULL ELSE ?1 END, finished_at = CASE WHEN retry_count + 1 >= ?3 THEN ?1 ELSE NULL END, last_error = 'lease_expired', updated_at = ?1 WHERE tenant_id = ?2 AND status = 'running' AND (lease_expires_at IS NULL OR lease_expires_at <= ?1)",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(MAX_SESSION_MEMORY_JOB_RETRIES)
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(result.rows_affected() as usize)
}

pub(crate) const MAX_SESSION_MEMORY_JOB_RETRIES: i64 = 5;

pub(crate) async fn mark_session_memory_job_failed_with_lease_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    ownership_token: &str,
    error_code: &str,
    now: &str,
    retryable: bool,
) -> StoreResult<bool> {
    let retry_count = sqlx::query_scalar::<_, i64>(
        "SELECT retry_count FROM session_memory_jobs WHERE tenant_id = ?1 AND id = ?2 AND status = 'running' AND ownership_token = ?3",
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
    let retry_at = if retryable && next_retry_count < MAX_SESSION_MEMORY_JOB_RETRIES {
        Some(next_retry_at(now, next_retry_count)?)
    } else {
        None
    };
    let result = sqlx::query(
        "UPDATE session_memory_jobs SET status = 'failed', last_error = ?1, finished_at = ?2, updated_at = ?2, retry_count = ?3, retry_at = ?4, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL WHERE tenant_id = ?5 AND id = ?6 AND status = 'running' AND ownership_token = ?7",
    )
    .bind(error_code)
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

pub(crate) async fn cancel_session_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    now: &str,
) -> StoreResult<bool> {
    let result = sqlx::query(
        "UPDATE session_memory_jobs SET status = 'canceled', last_error = 'canceled', finished_at = ?1, updated_at = ?1, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL WHERE tenant_id = ?2 AND id = ?3 AND status IN ('queued', 'running', 'failed')",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn retry_session_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
) -> StoreResult<bool> {
    restart_session_memory_job_sqlx(pool, tenant_id, job_id, &Utc::now().to_rfc3339()).await
}

pub(crate) async fn restart_session_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    now: &str,
) -> StoreResult<bool> {
    let result = sqlx::query(
        "UPDATE session_memory_jobs SET status = 'queued', retry_count = 0, retry_at = NULL, last_error = NULL, not_before = ?1, started_at = NULL, finished_at = NULL, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3 AND status IN ('failed', 'canceled')",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn list_session_memory_job_ids_for_scheduler_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    now: &str,
    limit: i64,
) -> StoreResult<Vec<String>> {
    sqlx::query_scalar(
        "SELECT id FROM session_memory_jobs WHERE tenant_id = ?1 AND (status = 'queued' OR (status = 'failed' AND retry_at IS NOT NULL AND retry_at <= ?2)) ORDER BY CASE WHEN status = 'failed' OR not_before <= ?2 THEN 0 ELSE 1 END, not_before DESC, created_at DESC, id ASC LIMIT ?3",
    )
    .bind(tenant_id)
    .bind(now)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Db)
}

pub(crate) fn lease_expires_at(now: &str, lease_duration: Duration) -> StoreResult<String> {
    DateTime::parse_from_rfc3339(now)
        .map(|value| (value.with_timezone(&Utc) + lease_duration).to_rfc3339())
        .map_err(|_| StoreError::Validation("invalid Session Memory lease timestamp".to_string()))
}

pub(crate) fn next_retry_at(now: &str, retry_count: i64) -> StoreResult<String> {
    let delay_seconds = match retry_count.min(8) {
        1 => 5,
        2 => 15,
        3 => 30,
        4 => 60,
        5 => 120,
        _ => 300,
    };
    DateTime::parse_from_rfc3339(now)
        .map(|value| (value.with_timezone(&Utc) + Duration::seconds(delay_seconds)).to_rfc3339())
        .map_err(|_| StoreError::Validation("invalid Session Memory retry timestamp".to_string()))
}
