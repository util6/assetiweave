use crate::backend::models::{
    GlobalMemory, GlobalMemoryJob, GlobalMemoryJobStatus, GlobalMemorySource, GlobalMemoryVersion,
    GlobalMemoryVersionStatus,
};
use crate::backend::runtime::{AppError, AppResult};
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

pub(crate) const GLOBAL_MEMORY_CONTRACT_VERSION: &str = "global-memory.v1";
pub(crate) const GLOBAL_MEMORY_PROMPT_VERSION: &str = "global-memory-prompt.v1";
pub(crate) const GLOBAL_MEMORY_JOB_LEASE: Duration = Duration::minutes(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GlobalMemoryProjectInput {
    pub(crate) project_id: String,
    pub(crate) project_path: String,
    pub(crate) project_version_id: String,
    pub(crate) project_version_number: i64,
    pub(crate) project_watermark: i64,
    pub(crate) project_input_fingerprint: String,
    pub(crate) memory_markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GlobalMemoryInputSet {
    pub(crate) projects: Vec<GlobalMemoryProjectInput>,
    pub(crate) fingerprint: String,
    pub(crate) watermark: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct GlobalMemoryPersistInput {
    pub(crate) tenant_id: String,
    pub(crate) input_fingerprint: String,
    pub(crate) source_watermark: i64,
    pub(crate) summary_markdown: String,
    pub(crate) memory_markdown: String,
    pub(crate) raw_output_json: String,
    pub(crate) summary_document_path: String,
    pub(crate) memory_document_path: String,
    pub(crate) ownership_token: String,
    pub(crate) sources: Vec<GlobalMemorySource>,
}

pub(crate) async fn load_global_memory_inputs_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<GlobalMemoryInputSet> {
    let rows = sqlx::query(
        "SELECT pm.id, pm.project_path, v.id, v.version_number, v.source_watermark, v.input_fingerprint, v.content_markdown FROM project_memories pm JOIN project_memory_versions v ON v.tenant_id = pm.tenant_id AND v.id = pm.last_successful_version_id WHERE pm.tenant_id = ?1 AND v.status = 'succeeded' AND NOT EXISTS (SELECT 1 FROM project_memory_sources source WHERE source.tenant_id = v.tenant_id AND source.version_id = v.id AND NOT EXISTS (SELECT 1 FROM session_memories m WHERE m.tenant_id = source.tenant_id AND m.id = source.session_memory_id AND m.status = 'active' AND m.project_path = pm.project_path AND m.source_revision = source.source_revision AND NOT EXISTS (SELECT 1 FROM session_memories newer WHERE newer.tenant_id = m.tenant_id AND newer.session_id = m.session_id AND newer.status = 'active' AND (newer.source_revision > m.source_revision OR (newer.source_revision = m.source_revision AND newer.id > m.id))) AND (NOT EXISTS (SELECT 1 FROM conversation_sessions c WHERE c.tenant_id = m.tenant_id AND c.id = m.session_id) OR EXISTS (SELECT 1 FROM conversation_sessions c WHERE c.tenant_id = m.tenant_id AND c.id = m.session_id AND c.source_id = m.source_id AND c.missing = 0 AND EXISTS (SELECT 1 FROM conversation_sources source_record WHERE source_record.tenant_id = c.tenant_id AND source_record.id = c.source_id AND source_record.enabled = 1) AND (c.source_fingerprint IS NULL OR c.source_fingerprint = m.source_fingerprint))))) ORDER BY pm.project_path ASC, pm.id ASC",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)?;
    let mut projects = Vec::with_capacity(rows.len());
    for row in rows {
        projects.push(GlobalMemoryProjectInput {
            project_id: row.try_get(0).map_err(AppError::external)?,
            project_path: row.try_get(1).map_err(AppError::external)?,
            project_version_id: row.try_get(2).map_err(AppError::external)?,
            project_version_number: row.try_get(3).map_err(AppError::external)?,
            project_watermark: row.try_get(4).map_err(AppError::external)?,
            project_input_fingerprint: row.try_get(5).map_err(AppError::external)?,
            memory_markdown: row.try_get(6).map_err(AppError::external)?,
        });
    }
    Ok(global_input_set_from_projects(projects))
}

pub(crate) fn global_input_set_from_projects(
    mut projects: Vec<GlobalMemoryProjectInput>,
) -> GlobalMemoryInputSet {
    projects.sort_by(|left, right| {
        left.project_path
            .cmp(&right.project_path)
            .then_with(|| left.project_id.cmp(&right.project_id))
            .then_with(|| left.project_version_id.cmp(&right.project_version_id))
    });
    let watermark = projects
        .iter()
        .map(|project| project.project_watermark)
        .max()
        .unwrap_or(0);
    let mut hasher = Sha256::new();
    for project in &projects {
        for value in [
            project.project_id.as_str(),
            project.project_path.as_str(),
            project.project_version_id.as_str(),
            project.project_version_number.to_string().as_str(),
            project.project_watermark.to_string().as_str(),
            project.project_input_fingerprint.as_str(),
        ] {
            hasher.update(value.as_bytes());
            hasher.update([0]);
        }
    }
    hasher.update(GLOBAL_MEMORY_CONTRACT_VERSION.as_bytes());
    hasher.update([0]);
    hasher.update(GLOBAL_MEMORY_PROMPT_VERSION.as_bytes());
    GlobalMemoryInputSet {
        projects,
        fingerprint: format!("{:x}", hasher.finalize()),
        watermark,
    }
}

pub(crate) fn global_memory_id(tenant_id: &str) -> String {
    format!("global-memory-{}", digest(tenant_id))
}

/// A Project Memory success dirties the single tenant Global Memory job in the
/// same transaction, so a process crash cannot acknowledge a project update
/// without leaving a durable global rebuild target.
pub(crate) async fn enqueue_global_memory_job_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    now: &str,
) -> AppResult<Option<String>> {
    let rows = sqlx::query(
        "SELECT pm.id, pm.project_path, v.id, v.version_number, v.source_watermark, v.input_fingerprint, v.content_markdown FROM project_memories pm JOIN project_memory_versions v ON v.tenant_id = pm.tenant_id AND v.id = pm.last_successful_version_id WHERE pm.tenant_id = ?1 AND v.status = 'succeeded' AND NOT EXISTS (SELECT 1 FROM project_memory_sources source WHERE source.tenant_id = v.tenant_id AND source.version_id = v.id AND NOT EXISTS (SELECT 1 FROM session_memories m WHERE m.tenant_id = source.tenant_id AND m.id = source.session_memory_id AND m.status = 'active' AND m.project_path = pm.project_path AND m.source_revision = source.source_revision AND NOT EXISTS (SELECT 1 FROM session_memories newer WHERE newer.tenant_id = m.tenant_id AND newer.session_id = m.session_id AND newer.status = 'active' AND (newer.source_revision > m.source_revision OR (newer.source_revision = m.source_revision AND newer.id > m.id))) AND (NOT EXISTS (SELECT 1 FROM conversation_sessions c WHERE c.tenant_id = m.tenant_id AND c.id = m.session_id) OR EXISTS (SELECT 1 FROM conversation_sessions c WHERE c.tenant_id = m.tenant_id AND c.id = m.session_id AND c.source_id = m.source_id AND c.missing = 0 AND EXISTS (SELECT 1 FROM conversation_sources source_record WHERE source_record.tenant_id = c.tenant_id AND source_record.id = c.source_id AND source_record.enabled = 1) AND (c.source_fingerprint IS NULL OR c.source_fingerprint = m.source_fingerprint))))) ORDER BY pm.project_path ASC, pm.id ASC",
    )
    .bind(tenant_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::Db)?;
    if rows.is_empty() {
        return Ok(None);
    }
    let projects = rows
        .into_iter()
        .map(|row| {
            Ok(GlobalMemoryProjectInput {
                project_id: row.try_get(0).map_err(AppError::external)?,
                project_path: row.try_get(1).map_err(AppError::external)?,
                project_version_id: row.try_get(2).map_err(AppError::external)?,
                project_version_number: row.try_get(3).map_err(AppError::external)?,
                project_watermark: row.try_get(4).map_err(AppError::external)?,
                project_input_fingerprint: row.try_get(5).map_err(AppError::external)?,
                memory_markdown: row.try_get(6).map_err(AppError::external)?,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
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
    .map_err(AppError::Db)?;
    let current = sqlx::query(
        "SELECT status, input_fingerprint FROM global_memory_jobs WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(AppError::Db)?;
    if current.as_ref().is_some_and(|row| {
        let status: Result<String, _> = row.try_get(0);
        let fingerprint: Result<String, _> = row.try_get(1);
        status
            .as_deref()
            .is_ok_and(|value| matches!(value, "queued" | "running"))
            && fingerprint
                .as_deref()
                .is_ok_and(|value| value == inputs.fingerprint)
    }) {
        return Ok(Some(memory_id));
    }
    if current.as_ref().is_some_and(|row| {
        let status: Result<String, _> = row.try_get(0);
        let fingerprint: Result<String, _> = row.try_get(1);
        status.as_deref().is_ok_and(|value| value == "succeeded")
            && fingerprint
                .as_deref()
                .is_ok_and(|value| value == inputs.fingerprint)
    }) {
        return Ok(None);
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
    .map_err(AppError::Db)?;
    Ok(Some(memory_id))
}

pub(crate) async fn list_global_memory_job_ids_for_scheduler_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    now: &str,
) -> AppResult<Vec<String>> {
    sqlx::query_scalar(
        "SELECT id FROM global_memory_jobs WHERE tenant_id = ?1 AND (status = 'queued' OR (status = 'failed' AND retry_at IS NOT NULL AND retry_at <= ?2)) ORDER BY updated_at ASC, id ASC LIMIT 1",
    )
    .bind(tenant_id)
    .bind(now)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}

pub(crate) async fn load_global_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
) -> AppResult<Option<GlobalMemoryJob>> {
    let row = sqlx::query(
        "SELECT tenant_id, id, target_watermark, input_fingerprint, status, attempt_count, retry_count, retry_at, last_error, ownership_token, lease_expires_at, heartbeat_at, started_at, finished_at, created_at, updated_at FROM global_memory_jobs WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;
    row.as_ref().map(map_job).transpose()
}

pub(crate) async fn claim_global_memory_job_with_lease_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    now: &str,
    ownership_token: &str,
) -> AppResult<Option<GlobalMemoryJob>> {
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
    .map_err(AppError::Db)?;
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
) -> AppResult<bool> {
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
    .map_err(AppError::Db)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn recover_expired_global_memory_leases_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    now: &str,
) -> AppResult<u64> {
    let result = sqlx::query(
        "UPDATE global_memory_jobs SET status = 'queued', ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, retry_count = retry_count + 1, retry_at = ?1, last_error = 'lease_expired', updated_at = ?1 WHERE tenant_id = ?2 AND status = 'running' AND (lease_expires_at IS NULL OR lease_expires_at <= ?1)",
    )
    .bind(now)
    .bind(tenant_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(result.rows_affected())
}

pub(crate) async fn mark_global_memory_job_failed_with_lease_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    ownership_token: &str,
    error_message: &str,
    now: &str,
) -> AppResult<bool> {
    let retry_count: Option<i64> = sqlx::query_scalar(
        "SELECT retry_count FROM global_memory_jobs WHERE tenant_id = ?1 AND id = ?2 AND status = 'running' AND ownership_token = ?3",
    )
    .bind(tenant_id)
    .bind(job_id)
    .bind(ownership_token)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;
    let Some(retry_count) = retry_count else {
        return Ok(false);
    };
    let delay = 5_i64.saturating_mul(2_i64.saturating_pow(retry_count.min(6) as u32));
    let retry_at = (parse_utc(now) + Duration::seconds(delay)).to_rfc3339();
    let result = sqlx::query(
        "UPDATE global_memory_jobs SET status = 'failed', last_error = ?1, finished_at = ?2, updated_at = ?2, retry_count = retry_count + 1, retry_at = ?3, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL WHERE tenant_id = ?4 AND id = ?5 AND status = 'running' AND ownership_token = ?6",
    )
    .bind(error_message)
    .bind(now)
    .bind(retry_at)
    .bind(tenant_id)
    .bind(job_id)
    .bind(ownership_token)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn cancel_global_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    now: &str,
) -> AppResult<bool> {
    let result = sqlx::query(
        "UPDATE global_memory_jobs SET status = 'canceled', last_error = 'canceled', finished_at = ?1, updated_at = ?1, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL WHERE tenant_id = ?2 AND id = ?3 AND status IN ('queued', 'running', 'failed')",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn next_global_memory_version_number_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<i64> {
    sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM global_memory_versions WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::Db)
}

pub(crate) async fn persist_global_memory_success_sqlx(
    pool: &SqlitePool,
    input: &GlobalMemoryPersistInput,
    now: &str,
) -> AppResult<GlobalMemoryVersion> {
    let mut tx = pool.begin().await.map_err(AppError::Db)?;
    let job_id = global_memory_id(&input.tenant_id);
    let current_fingerprint: String = sqlx::query_scalar(
        "SELECT input_fingerprint FROM global_memory_jobs WHERE tenant_id = ?1 AND id = ?2 AND status = 'running' AND ownership_token = ?3",
    )
    .bind(&input.tenant_id)
    .bind(&job_id)
    .bind(&input.ownership_token)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Db)?
    .ok_or_else(|| AppError::Conflict("Global Memory job lease is no longer owned".to_string()))?;
    let version_number: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM global_memory_versions WHERE tenant_id = ?1",
    )
    .bind(&input.tenant_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::Db)?;
    let version_id = format!(
        "global-memory-version-{}",
        digest(&format!(
            "{}\0{}\0{}",
            input.tenant_id, input.input_fingerprint, version_number
        ))
    );
    sqlx::query(
        "INSERT INTO global_memory_versions (tenant_id, id, version_number, status, input_fingerprint, source_watermark, summary_markdown, memory_markdown, raw_output_json, created_at, updated_at) VALUES (?1, ?2, ?3, 'succeeded', ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
    )
    .bind(&input.tenant_id)
    .bind(&version_id)
    .bind(version_number)
    .bind(&input.input_fingerprint)
    .bind(input.source_watermark)
    .bind(&input.summary_markdown)
    .bind(&input.memory_markdown)
    .bind(&input.raw_output_json)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Db)?;
    for source in &input.sources {
        sqlx::query(
            "INSERT INTO global_memory_sources (tenant_id, version_id, project_id, project_path, project_version_id, project_watermark, sort_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(&input.tenant_id)
        .bind(&version_id)
        .bind(&source.project_id)
        .bind(&source.project_path)
        .bind(&source.project_version_id)
        .bind(source.project_watermark)
        .bind(source.sort_order)
        .execute(&mut *tx)
        .await
        .map_err(AppError::Db)?;
    }
    let successor = current_fingerprint != input.input_fingerprint;
    let memory_id = global_memory_id(&input.tenant_id);
    sqlx::query(
        "UPDATE global_memories SET last_successful_version_id = ?1, last_successful_at = ?2, last_successful_watermark = ?3, last_successful_input_fingerprint = ?4, summary_document_path = ?5, memory_document_path = ?6, updated_at = ?2 WHERE tenant_id = ?7 AND id = ?8",
    )
    .bind(&version_id)
    .bind(now)
    .bind(input.source_watermark)
    .bind(&input.input_fingerprint)
    .bind(&input.summary_document_path)
    .bind(&input.memory_document_path)
    .bind(&input.tenant_id)
    .bind(&memory_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Db)?;
    sqlx::query(
        "UPDATE global_memory_jobs SET status = CASE WHEN ?1 THEN 'queued' ELSE 'succeeded' END, finished_at = CASE WHEN ?1 THEN NULL ELSE ?2 END, last_error = NULL, retry_at = NULL, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, updated_at = ?2 WHERE tenant_id = ?3 AND id = ?4 AND status = 'running' AND ownership_token = ?5",
    )
    .bind(successor)
    .bind(now)
    .bind(&input.tenant_id)
    .bind(&memory_id)
    .bind(&input.ownership_token)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Db)?;
    tx.commit().await.map_err(AppError::Db)?;
    Ok(GlobalMemoryVersion {
        tenant_id: input.tenant_id.clone(),
        id: version_id,
        version_number,
        status: GlobalMemoryVersionStatus::Succeeded,
        input_fingerprint: input.input_fingerprint.clone(),
        source_watermark: input.source_watermark,
        summary_markdown: Some(input.summary_markdown.clone()),
        memory_markdown: Some(input.memory_markdown.clone()),
        raw_output_json: Some(input.raw_output_json.clone()),
        error_message: None,
        created_at: now.to_string(),
        updated_at: now.to_string(),
    })
}

pub(crate) async fn load_global_memory_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Option<GlobalMemory>> {
    let row = sqlx::query(
        "SELECT tenant_id, id, last_successful_version_id, last_successful_at, last_successful_watermark, last_successful_input_fingerprint, summary_document_path, memory_document_path, created_at, updated_at FROM global_memories WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;
    row.as_ref().map(map_global).transpose()
}

pub(crate) async fn load_global_memory_latest_version_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Option<GlobalMemoryVersion>> {
    let row = sqlx::query(
        "SELECT tenant_id, id, version_number, status, input_fingerprint, source_watermark, summary_markdown, memory_markdown, raw_output_json, error_message, created_at, updated_at FROM global_memory_versions WHERE tenant_id = ?1 AND status = 'succeeded' ORDER BY version_number DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let version = map_version(&row)?;
    let sources = load_global_memory_sources_sqlx(pool, tenant_id, &version.id).await?;
    for source in sources {
        let current_project = super::project_memory_repo::load_project_memory_latest_version_sqlx(
            pool,
            tenant_id,
            &source.project_id,
        )
        .await?;
        if current_project.as_ref().map(|value| value.id.as_str())
            != Some(source.project_version_id.as_str())
        {
            return Ok(None);
        }
    }
    Ok(Some(version))
}

pub(crate) async fn load_global_memory_sources_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    version_id: &str,
) -> AppResult<Vec<GlobalMemorySource>> {
    let rows = sqlx::query(
        "SELECT project_id, project_path, project_version_id, project_watermark, sort_order FROM global_memory_sources WHERE tenant_id = ?1 AND version_id = ?2 ORDER BY sort_order ASC, project_id ASC",
    )
    .bind(tenant_id)
    .bind(version_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)?;
    rows.iter()
        .map(|row| {
            Ok(GlobalMemorySource {
                project_id: row.try_get(0).map_err(AppError::external)?,
                project_path: row.try_get(1).map_err(AppError::external)?,
                project_version_id: row.try_get(2).map_err(AppError::external)?,
                project_watermark: row.try_get(3).map_err(AppError::external)?,
                sort_order: row.try_get(4).map_err(AppError::external)?,
            })
        })
        .collect()
}

fn map_job(row: &sqlx::sqlite::SqliteRow) -> AppResult<GlobalMemoryJob> {
    Ok(GlobalMemoryJob {
        tenant_id: row.try_get(0).map_err(AppError::external)?,
        id: row.try_get(1).map_err(AppError::external)?,
        target_watermark: row.try_get(2).map_err(AppError::external)?,
        input_fingerprint: row.try_get(3).map_err(AppError::external)?,
        status: parse_job_status(row.try_get(4).map_err(AppError::external)?)?,
        attempt_count: row.try_get(5).map_err(AppError::external)?,
        retry_count: row.try_get(6).map_err(AppError::external)?,
        retry_at: row.try_get(7).map_err(AppError::external)?,
        last_error: row.try_get(8).map_err(AppError::external)?,
        ownership_token: row.try_get(9).map_err(AppError::external)?,
        lease_expires_at: row.try_get(10).map_err(AppError::external)?,
        heartbeat_at: row.try_get(11).map_err(AppError::external)?,
        started_at: row.try_get(12).map_err(AppError::external)?,
        finished_at: row.try_get(13).map_err(AppError::external)?,
        created_at: row.try_get(14).map_err(AppError::external)?,
        updated_at: row.try_get(15).map_err(AppError::external)?,
    })
}

fn map_global(row: &sqlx::sqlite::SqliteRow) -> AppResult<GlobalMemory> {
    Ok(GlobalMemory {
        tenant_id: row.try_get(0).map_err(AppError::external)?,
        id: row.try_get(1).map_err(AppError::external)?,
        last_successful_version_id: row.try_get(2).map_err(AppError::external)?,
        last_successful_at: row.try_get(3).map_err(AppError::external)?,
        last_successful_watermark: row.try_get(4).map_err(AppError::external)?,
        last_successful_input_fingerprint: row.try_get(5).map_err(AppError::external)?,
        summary_document_path: row.try_get(6).map_err(AppError::external)?,
        memory_document_path: row.try_get(7).map_err(AppError::external)?,
        created_at: row.try_get(8).map_err(AppError::external)?,
        updated_at: row.try_get(9).map_err(AppError::external)?,
    })
}

fn map_version(row: &sqlx::sqlite::SqliteRow) -> AppResult<GlobalMemoryVersion> {
    Ok(GlobalMemoryVersion {
        tenant_id: row.try_get(0).map_err(AppError::external)?,
        id: row.try_get(1).map_err(AppError::external)?,
        version_number: row.try_get(2).map_err(AppError::external)?,
        status: parse_version_status(row.try_get(3).map_err(AppError::external)?)?,
        input_fingerprint: row.try_get(4).map_err(AppError::external)?,
        source_watermark: row.try_get(5).map_err(AppError::external)?,
        summary_markdown: row.try_get(6).map_err(AppError::external)?,
        memory_markdown: row.try_get(7).map_err(AppError::external)?,
        raw_output_json: row.try_get(8).map_err(AppError::external)?,
        error_message: row.try_get(9).map_err(AppError::external)?,
        created_at: row.try_get(10).map_err(AppError::external)?,
        updated_at: row.try_get(11).map_err(AppError::external)?,
    })
}

fn parse_job_status(value: String) -> AppResult<GlobalMemoryJobStatus> {
    match value.as_str() {
        "queued" => Ok(GlobalMemoryJobStatus::Queued),
        "running" => Ok(GlobalMemoryJobStatus::Running),
        "succeeded" => Ok(GlobalMemoryJobStatus::Succeeded),
        "failed" => Ok(GlobalMemoryJobStatus::Failed),
        "canceled" => Ok(GlobalMemoryJobStatus::Canceled),
        _ => Err(AppError::External(format!(
            "unknown Global Memory job status: {value}"
        ))),
    }
}

fn parse_version_status(value: String) -> AppResult<GlobalMemoryVersionStatus> {
    match value.as_str() {
        "running" => Ok(GlobalMemoryVersionStatus::Running),
        "succeeded" => Ok(GlobalMemoryVersionStatus::Succeeded),
        "failed" => Ok(GlobalMemoryVersionStatus::Failed),
        "invalid" => Ok(GlobalMemoryVersionStatus::Invalid),
        _ => Err(AppError::External(format!(
            "unknown Global Memory version status: {value}"
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

fn digest(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(id: &str, path: &str, version: i64) -> GlobalMemoryProjectInput {
        GlobalMemoryProjectInput {
            project_id: id.into(),
            project_path: path.into(),
            project_version_id: format!("version-{id}"),
            project_version_number: 1,
            project_watermark: version,
            project_input_fingerprint: format!("fingerprint-{id}"),
            memory_markdown: format!("# {id}"),
        }
    }

    #[test]
    fn global_input_fingerprint_is_order_independent_and_watermarked() {
        let left =
            global_input_set_from_projects(vec![project("b", "/b", 7), project("a", "/a", 3)]);
        let right =
            global_input_set_from_projects(vec![project("a", "/a", 3), project("b", "/b", 7)]);
        assert_eq!(left.fingerprint, right.fingerprint);
        assert_eq!(left.watermark, 7);
    }

    #[test]
    fn global_ids_are_tenant_scoped() {
        assert_ne!(global_memory_id("tenant-a"), global_memory_id("tenant-b"));
    }
}
