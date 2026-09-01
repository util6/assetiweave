use crate::backend::models::{
    ProjectMemory, ProjectMemoryJob, ProjectMemoryJobStatus, ProjectMemorySource,
    ProjectMemoryVersion, ProjectMemoryVersionStatus, SessionMemory,
};
use crate::backend::runtime::{AppError, AppResult};
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

pub(crate) const PROJECT_MEMORY_CONTRACT_VERSION: &str = "project-memory.v1";
pub(crate) const PROJECT_MEMORY_PROMPT_VERSION: &str = "project-memory-prompt.v1";
pub(crate) const PROJECT_MEMORY_JOB_LEASE: Duration = Duration::minutes(2);

#[derive(Debug, Clone)]
pub(crate) struct ProjectMemoryInputSet {
    pub(crate) memories: Vec<SessionMemory>,
    pub(crate) fingerprint: String,
    pub(crate) watermark: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectMemoryPersistInput {
    pub(crate) tenant_id: String,
    pub(crate) project_id: String,
    pub(crate) project_path: String,
    pub(crate) input_fingerprint: String,
    pub(crate) source_watermark: i64,
    pub(crate) content_markdown: String,
    pub(crate) raw_output_json: String,
    pub(crate) document_path: String,
    pub(crate) ownership_token: String,
    pub(crate) sources: Vec<ProjectMemorySource>,
}

pub(crate) async fn load_project_memory_inputs_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    project_path: &str,
) -> AppResult<ProjectMemoryInputSet> {
    let memories = super::session_memory_repo::list_session_memories_for_project_sqlx(
        pool,
        tenant_id,
        project_path,
    )
    .await?;
    Ok(input_set_from_memories(memories))
}

pub(crate) fn input_set_from_memories(mut memories: Vec<SessionMemory>) -> ProjectMemoryInputSet {
    memories.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.source_revision.cmp(&right.source_revision))
    });
    let watermark = memories
        .iter()
        .map(|memory| memory.source_revision)
        .max()
        .unwrap_or(0);
    let mut hasher = Sha256::new();
    for memory in &memories {
        for value in [
            memory.id.as_str(),
            memory.session_id.as_str(),
            memory.source_id.as_str(),
            &memory.source_revision.to_string(),
            memory.source_fingerprint.as_str(),
            memory.contract_version.as_str(),
            memory.prompt_version.as_str(),
        ] {
            hasher.update(value.as_bytes());
            hasher.update([0]);
        }
    }
    ProjectMemoryInputSet {
        memories,
        fingerprint: format!("{:x}", hasher.finalize()),
        watermark,
    }
}

pub(crate) fn project_memory_id(tenant_id: &str, project_path: &str) -> String {
    format!(
        "project-memory-{}",
        digest(&format!("{tenant_id}\0{project_path}"))
    )
}

pub(crate) fn project_memory_job_id(tenant_id: &str, project_path: &str) -> String {
    format!(
        "project-memory-job-{}",
        digest(&format!("{tenant_id}\0{project_path}"))
    )
}

/// Marks the project dirty from inside the Session Memory commit transaction.
/// At most one mutable job exists per project; a newer input replaces the
/// queued target or leaves a running job with a successor target to consume.
pub(crate) async fn enqueue_project_memory_job_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    project_path: &str,
    now: &str,
) -> AppResult<Option<String>> {
    let rows = sqlx::query(
        "SELECT id, session_id, source_id, source_revision, source_fingerprint, contract_version, prompt_version FROM session_memories WHERE tenant_id = ?1 AND project_path = ?2 AND status = 'active' ORDER BY id ASC",
    )
    .bind(tenant_id)
    .bind(project_path)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::Db)?;
    if rows.is_empty() {
        return Ok(None);
    }
    let mut hasher = Sha256::new();
    let mut watermark = 0i64;
    for row in &rows {
        let id: String = row.try_get(0).map_err(AppError::external)?;
        let session_id: String = row.try_get(1).map_err(AppError::external)?;
        let source_id: String = row.try_get(2).map_err(AppError::external)?;
        let source_revision: i64 = row.try_get(3).map_err(AppError::external)?;
        let source_fingerprint: String = row.try_get(4).map_err(AppError::external)?;
        let contract_version: String = row.try_get(5).map_err(AppError::external)?;
        let prompt_version: String = row.try_get(6).map_err(AppError::external)?;
        watermark = watermark.max(source_revision);
        for value in [
            id,
            session_id,
            source_id,
            source_revision.to_string(),
            source_fingerprint,
            contract_version,
            prompt_version,
        ] {
            hasher.update(value.as_bytes());
            hasher.update([0]);
        }
    }
    let input_fingerprint = format!("{:x}", hasher.finalize());
    let project_id = project_memory_id(tenant_id, project_path);
    let job_id = project_memory_job_id(tenant_id, project_path);
    sqlx::query(
        "INSERT OR IGNORE INTO project_memories (tenant_id, id, project_path, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
    )
    .bind(tenant_id)
    .bind(&project_id)
    .bind(project_path)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(AppError::Db)?;

    let current = sqlx::query(
        "SELECT status, input_fingerprint FROM project_memory_jobs WHERE tenant_id = ?1 AND project_path = ?2",
    )
    .bind(tenant_id)
    .bind(project_path)
    .fetch_optional(&mut **tx)
    .await
    .map_err(AppError::Db)?;
    if current.as_ref().is_some_and(|row| {
        let status: Result<String, _> = row.try_get(0);
        let fingerprint: Result<String, _> = row.try_get(1);
        status
            .as_deref()
            .is_ok_and(|value| matches!(value, "running" | "queued"))
            && fingerprint
                .as_deref()
                .is_ok_and(|value| value == input_fingerprint)
    }) {
        return Ok(Some(job_id));
    }
    if current.as_ref().is_some_and(|row| {
        let status: Result<String, _> = row.try_get(0);
        let fingerprint: Result<String, _> = row.try_get(1);
        status.as_deref().is_ok_and(|value| value == "succeeded")
            && fingerprint
                .as_deref()
                .is_ok_and(|value| value == input_fingerprint)
    }) {
        return Ok(None);
    }

    sqlx::query(
        "INSERT INTO project_memory_jobs (tenant_id, id, project_id, project_path, target_watermark, input_fingerprint, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'queued', ?7, ?7) ON CONFLICT (tenant_id, project_path) DO UPDATE SET target_watermark = excluded.target_watermark, input_fingerprint = excluded.input_fingerprint, status = CASE WHEN project_memory_jobs.status = 'running' THEN 'running' ELSE 'queued' END, retry_at = NULL, last_error = NULL, finished_at = NULL, updated_at = excluded.updated_at",
    )
    .bind(tenant_id)
    .bind(&job_id)
    .bind(&project_id)
    .bind(project_path)
    .bind(watermark)
    .bind(&input_fingerprint)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(AppError::Db)?;
    Ok(Some(job_id))
}

pub(crate) async fn list_project_memory_job_ids_for_scheduler_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    now: &str,
    limit: i64,
) -> AppResult<Vec<String>> {
    sqlx::query_scalar(
        "SELECT id FROM project_memory_jobs WHERE tenant_id = ?1 AND ((status = 'queued') OR (status = 'failed' AND retry_at IS NOT NULL AND retry_at <= ?2)) ORDER BY updated_at ASC, id ASC LIMIT ?3",
    )
    .bind(tenant_id)
    .bind(now)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}

pub(crate) async fn load_project_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
) -> AppResult<Option<ProjectMemoryJob>> {
    let row = sqlx::query(
        "SELECT tenant_id, id, project_id, project_path, target_watermark, input_fingerprint, status, attempt_count, retry_count, retry_at, last_error, ownership_token, lease_expires_at, heartbeat_at, started_at, finished_at, created_at, updated_at FROM project_memory_jobs WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;
    row.as_ref().map(map_job).transpose()
}

pub(crate) async fn claim_project_memory_job_with_lease_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    now: &str,
    ownership_token: &str,
) -> AppResult<Option<ProjectMemoryJob>> {
    let lease_expires_at = (DateTime::parse_from_rfc3339(now)
        .map(|value| value.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
        + PROJECT_MEMORY_JOB_LEASE)
        .to_rfc3339();
    let result = sqlx::query(
        "UPDATE project_memory_jobs SET status = 'running', ownership_token = ?1, lease_expires_at = ?2, heartbeat_at = ?3, started_at = COALESCE(started_at, ?3), attempt_count = attempt_count + 1, updated_at = ?3 WHERE tenant_id = ?4 AND id = ?5 AND (status = 'queued' OR (status = 'failed' AND (retry_at IS NULL OR retry_at <= ?3)) OR (status = 'running' AND (lease_expires_at IS NULL OR lease_expires_at <= ?3)))",
    )
    .bind(ownership_token)
    .bind(&lease_expires_at)
    .bind(now)
    .bind(tenant_id)
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    load_project_memory_job_sqlx(pool, tenant_id, job_id).await
}

pub(crate) async fn heartbeat_project_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    ownership_token: &str,
    now: &str,
) -> AppResult<bool> {
    let lease_expires_at = (DateTime::parse_from_rfc3339(now)
        .map(|value| value.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
        + PROJECT_MEMORY_JOB_LEASE)
        .to_rfc3339();
    let result = sqlx::query(
        "UPDATE project_memory_jobs SET heartbeat_at = ?1, lease_expires_at = ?2, updated_at = ?1 WHERE tenant_id = ?3 AND id = ?4 AND status = 'running' AND ownership_token = ?5 AND lease_expires_at > ?1",
    )
    .bind(now)
    .bind(lease_expires_at)
    .bind(tenant_id)
    .bind(job_id)
    .bind(ownership_token)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn recover_expired_project_memory_leases_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    now: &str,
) -> AppResult<u64> {
    let result = sqlx::query(
        "UPDATE project_memory_jobs SET status = 'queued', ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, retry_count = retry_count + 1, retry_at = ?1, last_error = 'lease_expired', updated_at = ?1 WHERE tenant_id = ?2 AND status = 'running' AND (lease_expires_at IS NULL OR lease_expires_at <= ?1)",
    )
    .bind(now)
    .bind(tenant_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(result.rows_affected())
}

pub(crate) async fn mark_project_memory_job_failed_with_lease_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    ownership_token: &str,
    error_message: &str,
    now: &str,
) -> AppResult<bool> {
    let retry_count: Option<i64> = sqlx::query_scalar(
        "SELECT retry_count FROM project_memory_jobs WHERE tenant_id = ?1 AND id = ?2 AND status = 'running' AND ownership_token = ?3",
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
    let retry_at = (DateTime::parse_from_rfc3339(now)
        .map(|value| value.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
        + Duration::seconds(delay))
    .to_rfc3339();
    let result = sqlx::query(
        "UPDATE project_memory_jobs SET status = 'failed', last_error = ?1, finished_at = ?2, updated_at = ?2, retry_count = retry_count + 1, retry_at = ?3, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL WHERE tenant_id = ?4 AND id = ?5 AND status = 'running' AND ownership_token = ?6",
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

pub(crate) async fn cancel_project_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    now: &str,
) -> AppResult<bool> {
    let result = sqlx::query(
        "UPDATE project_memory_jobs SET status = 'canceled', last_error = 'canceled', finished_at = ?1, updated_at = ?1, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL WHERE tenant_id = ?2 AND id = ?3 AND status IN ('queued', 'running', 'failed')",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn next_project_memory_version_number_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    project_id: &str,
) -> AppResult<i64> {
    sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM project_memory_versions WHERE tenant_id = ?1 AND project_id = ?2",
    )
    .bind(tenant_id)
    .bind(project_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::Db)
}

pub(crate) async fn persist_project_memory_success_sqlx(
    pool: &SqlitePool,
    input: &ProjectMemoryPersistInput,
    now: &str,
) -> AppResult<ProjectMemoryVersion> {
    let mut tx = pool.begin().await.map_err(AppError::Db)?;
    let job_id = project_memory_job_id(&input.tenant_id, &input.project_path);
    let current_fingerprint: String = sqlx::query_scalar(
        "SELECT input_fingerprint FROM project_memory_jobs WHERE tenant_id = ?1 AND id = ?2 AND status = 'running' AND ownership_token = ?3",
    )
    .bind(&input.tenant_id)
    .bind(&job_id)
    .bind(&input.ownership_token)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Db)?
    .ok_or_else(|| AppError::Conflict("Project Memory job lease is no longer owned".to_string()))?;
    let version_number: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM project_memory_versions WHERE tenant_id = ?1 AND project_id = ?2",
    )
    .bind(&input.tenant_id)
    .bind(&input.project_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::Db)?;
    let version_id = format!(
        "project-memory-version-{}",
        digest(&format!(
            "{}\0{}\0{}",
            input.project_id, input.input_fingerprint, version_number
        ))
    );
    sqlx::query(
        "INSERT INTO project_memory_versions (tenant_id, id, project_id, version_number, status, input_fingerprint, source_watermark, content_markdown, raw_output_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'succeeded', ?5, ?6, ?7, ?8, ?9, ?9)",
    )
    .bind(&input.tenant_id)
    .bind(&version_id)
    .bind(&input.project_id)
    .bind(version_number)
    .bind(&input.input_fingerprint)
    .bind(input.source_watermark)
    .bind(&input.content_markdown)
    .bind(&input.raw_output_json)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Db)?;
    for source in &input.sources {
        sqlx::query(
            "INSERT INTO project_memory_sources (tenant_id, version_id, session_memory_id, source_revision, sort_order) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(&input.tenant_id)
        .bind(&version_id)
        .bind(&source.session_memory_id)
        .bind(source.source_revision)
        .bind(source.sort_order)
        .execute(&mut *tx)
        .await
        .map_err(AppError::Db)?;
    }
    let successor = current_fingerprint != input.input_fingerprint;
    sqlx::query(
        "UPDATE project_memories SET last_successful_version_id = ?1, last_successful_at = ?2, last_successful_watermark = ?3, last_successful_input_fingerprint = ?4, document_path = ?5, updated_at = ?2 WHERE tenant_id = ?6 AND id = ?7",
    )
    .bind(&version_id)
    .bind(now)
    .bind(input.source_watermark)
    .bind(&input.input_fingerprint)
    .bind(&input.document_path)
    .bind(&input.tenant_id)
    .bind(&input.project_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Db)?;
    sqlx::query(
        "UPDATE project_memory_jobs SET status = CASE WHEN ?1 THEN 'queued' ELSE 'succeeded' END, finished_at = CASE WHEN ?1 THEN NULL ELSE ?2 END, last_error = NULL, retry_at = NULL, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, updated_at = ?2 WHERE tenant_id = ?3 AND id = ?4 AND status = 'running' AND ownership_token = ?5",
    )
    .bind(successor)
    .bind(now)
    .bind(&input.tenant_id)
    .bind(&job_id)
    .bind(&input.ownership_token)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Db)?;
    super::global_memory_repo::enqueue_global_memory_job_tx(&mut tx, &input.tenant_id, now).await?;
    tx.commit().await.map_err(AppError::Db)?;
    Ok(ProjectMemoryVersion {
        tenant_id: input.tenant_id.clone(),
        id: version_id,
        project_id: input.project_id.clone(),
        version_number,
        status: ProjectMemoryVersionStatus::Succeeded,
        input_fingerprint: input.input_fingerprint.clone(),
        source_watermark: input.source_watermark,
        content_markdown: Some(input.content_markdown.clone()),
        raw_output_json: Some(input.raw_output_json.clone()),
        error_message: None,
        created_at: now.to_string(),
        updated_at: now.to_string(),
    })
}

pub(crate) async fn load_project_memory_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    project_path: &str,
) -> AppResult<Option<ProjectMemory>> {
    let row = sqlx::query(
        "SELECT tenant_id, id, project_path, last_successful_version_id, last_successful_at, last_successful_watermark, last_successful_input_fingerprint, document_path, created_at, updated_at FROM project_memories WHERE tenant_id = ?1 AND project_path = ?2",
    )
    .bind(tenant_id)
    .bind(project_path)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;
    row.as_ref().map(map_project).transpose()
}

pub(crate) async fn retry_project_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
) -> AppResult<bool> {
    let result = sqlx::query(
        "UPDATE project_memory_jobs SET status = 'queued', last_error = NULL, retry_at = NULL, finished_at = NULL, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3 AND status = 'failed'",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(tenant_id)
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn load_project_memory_latest_version_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    project_id: &str,
) -> AppResult<Option<ProjectMemoryVersion>> {
    let row = sqlx::query(
        "SELECT v.tenant_id, v.id, v.project_id, v.version_number, v.status, v.input_fingerprint, v.source_watermark, v.content_markdown, v.raw_output_json, v.error_message, v.created_at, v.updated_at FROM project_memory_versions v JOIN project_memories p ON p.tenant_id = v.tenant_id AND p.id = v.project_id WHERE v.tenant_id = ?1 AND v.project_id = ?2 AND v.status = 'succeeded' AND NOT EXISTS (SELECT 1 FROM project_memory_sources source WHERE source.tenant_id = v.tenant_id AND source.version_id = v.id AND NOT EXISTS (SELECT 1 FROM session_memories m WHERE m.tenant_id = source.tenant_id AND m.id = source.session_memory_id AND m.status = 'active' AND m.project_path = p.project_path AND m.source_revision = source.source_revision AND NOT EXISTS (SELECT 1 FROM session_memories newer WHERE newer.tenant_id = m.tenant_id AND newer.session_id = m.session_id AND newer.status = 'active' AND (newer.source_revision > m.source_revision OR (newer.source_revision = m.source_revision AND newer.id > m.id))) AND (NOT EXISTS (SELECT 1 FROM conversation_sessions c WHERE c.tenant_id = m.tenant_id AND c.id = m.session_id) OR EXISTS (SELECT 1 FROM conversation_sessions c WHERE c.tenant_id = m.tenant_id AND c.id = m.session_id AND c.source_id = m.source_id AND c.missing = 0 AND EXISTS (SELECT 1 FROM conversation_sources source_record WHERE source_record.tenant_id = c.tenant_id AND source_record.id = c.source_id AND source_record.enabled = 1) AND (c.source_fingerprint IS NULL OR c.source_fingerprint = m.source_fingerprint))))) ORDER BY v.version_number DESC LIMIT 1",
    )
    .bind(tenant_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;
    row.as_ref().map(map_version).transpose()
}

pub(crate) async fn load_project_memory_sources_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    version_id: &str,
) -> AppResult<Vec<ProjectMemorySource>> {
    let rows = sqlx::query(
        "SELECT session_memory_id, source_revision, sort_order FROM project_memory_sources WHERE tenant_id = ?1 AND version_id = ?2 ORDER BY sort_order ASC, session_memory_id ASC",
    )
    .bind(tenant_id)
    .bind(version_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)?;
    rows.iter()
        .map(|row| {
            Ok(ProjectMemorySource {
                session_memory_id: row.try_get(0).map_err(AppError::external)?,
                source_revision: row.try_get(1).map_err(AppError::external)?,
                sort_order: row.try_get(2).map_err(AppError::external)?,
            })
        })
        .collect()
}

fn map_job(row: &sqlx::sqlite::SqliteRow) -> AppResult<ProjectMemoryJob> {
    Ok(ProjectMemoryJob {
        tenant_id: row.try_get(0).map_err(AppError::external)?,
        id: row.try_get(1).map_err(AppError::external)?,
        project_id: row.try_get(2).map_err(AppError::external)?,
        project_path: row.try_get(3).map_err(AppError::external)?,
        target_watermark: row.try_get(4).map_err(AppError::external)?,
        input_fingerprint: row.try_get(5).map_err(AppError::external)?,
        status: parse_job_status(row.try_get(6).map_err(AppError::external)?)?,
        attempt_count: row.try_get(7).map_err(AppError::external)?,
        retry_count: row.try_get(8).map_err(AppError::external)?,
        retry_at: row.try_get(9).map_err(AppError::external)?,
        last_error: row.try_get(10).map_err(AppError::external)?,
        ownership_token: row.try_get(11).map_err(AppError::external)?,
        lease_expires_at: row.try_get(12).map_err(AppError::external)?,
        heartbeat_at: row.try_get(13).map_err(AppError::external)?,
        started_at: row.try_get(14).map_err(AppError::external)?,
        finished_at: row.try_get(15).map_err(AppError::external)?,
        created_at: row.try_get(16).map_err(AppError::external)?,
        updated_at: row.try_get(17).map_err(AppError::external)?,
    })
}

fn map_project(row: &sqlx::sqlite::SqliteRow) -> AppResult<ProjectMemory> {
    Ok(ProjectMemory {
        tenant_id: row.try_get(0).map_err(AppError::external)?,
        id: row.try_get(1).map_err(AppError::external)?,
        project_path: row.try_get(2).map_err(AppError::external)?,
        last_successful_version_id: row.try_get(3).map_err(AppError::external)?,
        last_successful_at: row.try_get(4).map_err(AppError::external)?,
        last_successful_watermark: row.try_get(5).map_err(AppError::external)?,
        last_successful_input_fingerprint: row.try_get(6).map_err(AppError::external)?,
        document_path: row.try_get(7).map_err(AppError::external)?,
        created_at: row.try_get(8).map_err(AppError::external)?,
        updated_at: row.try_get(9).map_err(AppError::external)?,
    })
}

fn map_version(row: &sqlx::sqlite::SqliteRow) -> AppResult<ProjectMemoryVersion> {
    Ok(ProjectMemoryVersion {
        tenant_id: row.try_get(0).map_err(AppError::external)?,
        id: row.try_get(1).map_err(AppError::external)?,
        project_id: row.try_get(2).map_err(AppError::external)?,
        version_number: row.try_get(3).map_err(AppError::external)?,
        status: parse_version_status(row.try_get(4).map_err(AppError::external)?)?,
        input_fingerprint: row.try_get(5).map_err(AppError::external)?,
        source_watermark: row.try_get(6).map_err(AppError::external)?,
        content_markdown: row.try_get(7).map_err(AppError::external)?,
        raw_output_json: row.try_get(8).map_err(AppError::external)?,
        error_message: row.try_get(9).map_err(AppError::external)?,
        created_at: row.try_get(10).map_err(AppError::external)?,
        updated_at: row.try_get(11).map_err(AppError::external)?,
    })
}

fn parse_job_status(value: String) -> AppResult<ProjectMemoryJobStatus> {
    match value.as_str() {
        "queued" => Ok(ProjectMemoryJobStatus::Queued),
        "running" => Ok(ProjectMemoryJobStatus::Running),
        "succeeded" => Ok(ProjectMemoryJobStatus::Succeeded),
        "failed" => Ok(ProjectMemoryJobStatus::Failed),
        "canceled" => Ok(ProjectMemoryJobStatus::Canceled),
        _ => Err(AppError::External(format!(
            "unknown Project Memory job status: {value}"
        ))),
    }
}

fn parse_version_status(value: String) -> AppResult<ProjectMemoryVersionStatus> {
    match value.as_str() {
        "running" => Ok(ProjectMemoryVersionStatus::Running),
        "succeeded" => Ok(ProjectMemoryVersionStatus::Succeeded),
        "failed" => Ok(ProjectMemoryVersionStatus::Failed),
        "invalid" => Ok(ProjectMemoryVersionStatus::Invalid),
        _ => Err(AppError::External(format!(
            "unknown Project Memory version status: {value}"
        ))),
    }
}

fn digest(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory(id: &str, revision: i64) -> SessionMemory {
        SessionMemory {
            tenant_id: "tenant".into(),
            id: id.into(),
            session_id: format!("session-{id}"),
            source_id: "source".into(),
            source_revision: revision,
            source_fingerprint: format!("fingerprint-{id}"),
            contract_version: "session-memory.v1".into(),
            prompt_version: "prompt.v1".into(),
            status: crate::backend::models::SessionMemoryStatus::Active,
            project_path: Some("/project".into()),
            summary: format!("summary-{id}"),
            goal: String::new(),
            result: String::new(),
            decisions: vec![],
            verification: vec![],
            blockers: vec![],
            follow_up: vec![],
            topics: vec![],
            generated_at: "2026-08-31T00:00:00Z".into(),
            created_at: "2026-08-31T00:00:00Z".into(),
            updated_at: "2026-08-31T00:00:00Z".into(),
        }
    }

    #[test]
    fn project_input_fingerprint_is_order_independent_and_watermarked() {
        let left = input_set_from_memories(vec![memory("b", 7), memory("a", 3)]);
        let right = input_set_from_memories(vec![memory("a", 3), memory("b", 7)]);
        assert_eq!(left.fingerprint, right.fingerprint);
        assert_eq!(left.watermark, 7);
        assert_eq!(left.memories[0].id, "a");
    }

    #[test]
    fn project_and_job_ids_are_scope_stable() {
        assert_eq!(
            project_memory_id("tenant", "/project"),
            project_memory_id("tenant", "/project")
        );
        assert_ne!(
            project_memory_job_id("tenant", "/project"),
            project_memory_job_id("tenant", "/other")
        );
    }
}
