use super::project_memory_jobs::project_memory_job_id;
pub(crate) use super::project_memory_jobs::*;
use crate::backend::domain::{
    ProjectMemory, ProjectMemorySource, ProjectMemoryVersion, ProjectMemoryVersionStatus,
    SessionMemory,
};
use crate::backend::store::{StoreError, StoreResult};
use sha2::{Digest, Sha256};
use sqlx::{SqlitePool, Transaction};

pub(crate) const PROJECT_MEMORY_CONTRACT_VERSION: &str = "project-memory.v1";
pub(crate) const PROJECT_MEMORY_PROMPT_VERSION: &str = "project-memory-prompt.v1";

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
) -> StoreResult<ProjectMemoryInputSet> {
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

#[derive(Debug, sqlx::FromRow)]
struct ProjectMemorySourceRow {
    session_memory_id: String,
    source_revision: i64,
    sort_order: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct ProjectMemoryEntityRow {
    tenant_id: String,
    id: String,
    project_path: String,
    last_successful_version_id: Option<String>,
    last_successful_at: Option<String>,
    last_successful_watermark: i64,
    last_successful_input_fingerprint: Option<String>,
    document_path: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, sqlx::FromRow)]
struct ProjectMemoryVersionEntityRow {
    tenant_id: String,
    id: String,
    project_id: String,
    version_number: i64,
    status: String,
    input_fingerprint: String,
    source_watermark: i64,
    content_markdown: Option<String>,
    raw_output_json: Option<String>,
    error_message: Option<String>,
    created_at: String,
    updated_at: String,
}

pub(crate) async fn next_project_memory_version_number_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    project_id: &str,
) -> StoreResult<i64> {
    sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM project_memory_versions WHERE tenant_id = ?1 AND project_id = ?2",
    )
    .bind(tenant_id)
    .bind(project_id)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Db)
}

pub(crate) async fn persist_project_memory_success_sqlx(
    pool: &SqlitePool,
    input: &ProjectMemoryPersistInput,
    now: &str,
) -> StoreResult<ProjectMemoryVersion> {
    let mut tx = pool.begin().await.map_err(StoreError::Db)?;
    let job_id = project_memory_job_id(&input.tenant_id, &input.project_path);
    let current_fingerprint: String = sqlx::query_scalar(
        "SELECT input_fingerprint FROM project_memory_jobs WHERE tenant_id = ?1 AND id = ?2 AND status = 'running' AND ownership_token = ?3",
    )
    .bind(&input.tenant_id)
    .bind(&job_id)
    .bind(&input.ownership_token)
    .fetch_optional(&mut *tx)
    .await
    .map_err(StoreError::Db)?
    .ok_or_else(|| StoreError::Conflict("Project Memory job lease is no longer owned".to_string()))?;
    let version_number: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM project_memory_versions WHERE tenant_id = ?1 AND project_id = ?2",
    )
    .bind(&input.tenant_id)
    .bind(&input.project_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(StoreError::Db)?;
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
    .map_err(StoreError::Db)?;
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
        .map_err(StoreError::Db)?;
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
    .map_err(StoreError::Db)?;
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
    .map_err(StoreError::Db)?;
    tx.commit().await.map_err(StoreError::Db)?;
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
) -> StoreResult<Option<ProjectMemory>> {
    let row = sqlx::query_as::<_, ProjectMemoryEntityRow>(
        "SELECT tenant_id, id, project_path, last_successful_version_id, last_successful_at, last_successful_watermark, last_successful_input_fingerprint, document_path, created_at, updated_at FROM project_memories WHERE tenant_id = ?1 AND project_path = ?2",
    )
    .bind(tenant_id)
    .bind(project_path)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::Db)?;
    row.map(map_project).transpose()
}

pub(crate) async fn list_project_paths_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT project_path FROM project_memories WHERE tenant_id = ?1 ORDER BY project_path ASC",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(rows.into_iter().map(|(path,)| path).collect())
}

pub(crate) async fn load_project_memory_latest_version_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    project_id: &str,
) -> StoreResult<Option<ProjectMemoryVersion>> {
    let row = sqlx::query_as::<_, ProjectMemoryVersionEntityRow>(
        "SELECT v.tenant_id, v.id, v.project_id, v.version_number, v.status, v.input_fingerprint, v.source_watermark, v.content_markdown, v.raw_output_json, v.error_message, v.created_at, v.updated_at FROM project_memory_versions v JOIN project_memories p ON p.tenant_id = v.tenant_id AND p.id = v.project_id WHERE v.tenant_id = ?1 AND v.project_id = ?2 AND v.status = 'succeeded' AND NOT EXISTS (SELECT 1 FROM project_memory_sources source WHERE source.tenant_id = v.tenant_id AND source.version_id = v.id AND NOT EXISTS (SELECT 1 FROM session_memories m WHERE m.tenant_id = source.tenant_id AND m.id = source.session_memory_id AND m.status = 'active' AND m.project_path = p.project_path AND m.source_revision = source.source_revision AND NOT EXISTS (SELECT 1 FROM session_memories newer WHERE newer.tenant_id = m.tenant_id AND newer.session_id = m.session_id AND newer.status = 'active' AND (newer.source_revision > m.source_revision OR (newer.source_revision = m.source_revision AND newer.id > m.id))) AND (NOT EXISTS (SELECT 1 FROM conversation_sessions c WHERE c.tenant_id = m.tenant_id AND c.id = m.session_id) OR EXISTS (SELECT 1 FROM conversation_sessions c WHERE c.tenant_id = m.tenant_id AND c.id = m.session_id AND c.source_id = m.source_id AND c.missing = 0 AND EXISTS (SELECT 1 FROM conversation_sources source_record WHERE source_record.tenant_id = c.tenant_id AND source_record.id = c.source_id AND source_record.enabled = 1) AND (c.source_fingerprint IS NULL OR c.source_fingerprint = m.source_fingerprint))))) ORDER BY v.version_number DESC LIMIT 1",
    )
    .bind(tenant_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::Db)?;
    row.map(map_version).transpose()
}

pub(crate) async fn load_project_memory_sources_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    version_id: &str,
) -> StoreResult<Vec<ProjectMemorySource>> {
    let rows = sqlx::query_as::<_, ProjectMemorySourceRow>(
        "SELECT session_memory_id, source_revision, sort_order FROM project_memory_sources WHERE tenant_id = ?1 AND version_id = ?2 ORDER BY sort_order ASC, session_memory_id ASC",
    )
    .bind(tenant_id)
    .bind(version_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(rows
        .into_iter()
        .map(|row| ProjectMemorySource {
            session_memory_id: row.session_memory_id,
            source_revision: row.source_revision,
            sort_order: row.sort_order,
        })
        .collect())
}

fn map_project(row: ProjectMemoryEntityRow) -> StoreResult<ProjectMemory> {
    Ok(ProjectMemory {
        tenant_id: row.tenant_id,
        id: row.id,
        project_path: row.project_path,
        last_successful_version_id: row.last_successful_version_id,
        last_successful_at: row.last_successful_at,
        last_successful_watermark: row.last_successful_watermark,
        last_successful_input_fingerprint: row.last_successful_input_fingerprint,
        document_path: row.document_path,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn map_version(row: ProjectMemoryVersionEntityRow) -> StoreResult<ProjectMemoryVersion> {
    Ok(ProjectMemoryVersion {
        tenant_id: row.tenant_id,
        id: row.id,
        project_id: row.project_id,
        version_number: row.version_number,
        status: parse_version_status(row.status)?,
        input_fingerprint: row.input_fingerprint,
        source_watermark: row.source_watermark,
        content_markdown: row.content_markdown,
        raw_output_json: row.raw_output_json,
        error_message: row.error_message,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn parse_version_status(value: String) -> StoreResult<ProjectMemoryVersionStatus> {
    match value.as_str() {
        "running" => Ok(ProjectMemoryVersionStatus::Running),
        "succeeded" => Ok(ProjectMemoryVersionStatus::Succeeded),
        "failed" => Ok(ProjectMemoryVersionStatus::Failed),
        "invalid" => Ok(ProjectMemoryVersionStatus::Invalid),
        _ => Err(StoreError::External(format!(
            "unknown Project Memory version status: {value}"
        ))),
    }
}

pub(crate) fn digest(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
#[path = "project_memory_repo_tests.rs"]
mod tests;
