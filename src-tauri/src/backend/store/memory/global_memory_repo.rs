use crate::backend::domain::{
    GlobalMemory, GlobalMemoryJob, GlobalMemoryJobStatus, GlobalMemorySource, GlobalMemoryVersion,
    GlobalMemoryVersionStatus,
};
use crate::backend::store::{StoreError, StoreResult};
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::{Sqlite, SqlitePool, Transaction};

pub(crate) const GLOBAL_MEMORY_CONTRACT_VERSION: &str = "global-memory.v1";
pub(crate) const GLOBAL_MEMORY_PROMPT_VERSION: &str = "global-memory-prompt.v1";
pub(crate) use super::global_memory_jobs::*;

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

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct GlobalMemoryCandidateProjectRow {
    project_id: String,
    project_path: String,
    project_version_id: String,
    project_version_number: i64,
    project_watermark: i64,
    project_input_fingerprint: String,
    memory_markdown: String,
}

impl From<GlobalMemoryCandidateProjectRow> for GlobalMemoryProjectInput {
    fn from(row: GlobalMemoryCandidateProjectRow) -> Self {
        Self {
            project_id: row.project_id,
            project_path: row.project_path,
            project_version_id: row.project_version_id,
            project_version_number: row.project_version_number,
            project_watermark: row.project_watermark,
            project_input_fingerprint: row.project_input_fingerprint,
            memory_markdown: row.memory_markdown,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct GlobalMemorySourceRow {
    project_id: String,
    project_path: String,
    project_version_id: String,
    project_watermark: i64,
    sort_order: i64,
}

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct GlobalMemoryRow {
    tenant_id: String,
    id: String,
    last_successful_version_id: Option<String>,
    last_successful_at: Option<String>,
    last_successful_watermark: i64,
    last_successful_input_fingerprint: Option<String>,
    summary_document_path: Option<String>,
    memory_document_path: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, sqlx::FromRow)]
struct GlobalMemoryVersionRow {
    tenant_id: String,
    id: String,
    version_number: i64,
    status: String,
    input_fingerprint: String,
    source_watermark: i64,
    summary_markdown: Option<String>,
    memory_markdown: Option<String>,
    raw_output_json: Option<String>,
    error_message: Option<String>,
    created_at: String,
    updated_at: String,
}

pub(crate) const GLOBAL_MEMORY_CANDIDATE_PROJECTS_QUERY: &str = "\
SELECT \
    pm.id AS project_id, \
    pm.project_path AS project_path, \
    v.id AS project_version_id, \
    v.version_number AS project_version_number, \
    v.source_watermark AS project_watermark, \
    v.input_fingerprint AS project_input_fingerprint, \
    v.content_markdown AS memory_markdown \
FROM project_memories pm \
JOIN project_memory_versions v \
    ON v.tenant_id = pm.tenant_id AND v.id = pm.last_successful_version_id \
WHERE pm.tenant_id = ?1 \
    AND v.status = 'succeeded' \
    AND NOT EXISTS ( \
        SELECT 1 FROM project_memory_sources source \
        WHERE source.tenant_id = v.tenant_id \
            AND source.version_id = v.id \
            AND NOT EXISTS ( \
                SELECT 1 FROM session_memories m \
                WHERE m.tenant_id = source.tenant_id \
                    AND m.id = source.session_memory_id \
                    AND m.status = 'active' \
                    AND m.project_path = pm.project_path \
                    AND m.source_revision = source.source_revision \
                    AND NOT EXISTS ( \
                        SELECT 1 FROM session_memories newer \
                        WHERE newer.tenant_id = m.tenant_id \
                            AND newer.session_id = m.session_id \
                            AND newer.status = 'active' \
                            AND (newer.source_revision > m.source_revision \
                                OR (newer.source_revision = m.source_revision AND newer.id > m.id)) \
                    ) \
                    AND ( \
                        NOT EXISTS ( \
                            SELECT 1 FROM conversation_sessions c \
                            WHERE c.tenant_id = m.tenant_id AND c.id = m.session_id \
                        ) \
                        OR EXISTS ( \
                            SELECT 1 FROM conversation_sessions c \
                            WHERE c.tenant_id = m.tenant_id \
                                AND c.id = m.session_id \
                                AND c.source_id = m.source_id \
                                AND c.missing = 0 \
                                AND EXISTS ( \
                                    SELECT 1 FROM conversation_sources source_record \
                                    WHERE source_record.tenant_id = c.tenant_id \
                                        AND source_record.id = c.source_id \
                                        AND source_record.enabled = 1 \
                                ) \
                                AND (c.source_fingerprint IS NULL OR c.source_fingerprint = m.source_fingerprint) \
                        ) \
                    ) \
            ) \
    ) \
ORDER BY pm.project_path ASC, pm.id ASC";

pub(crate) async fn load_global_memory_inputs_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<GlobalMemoryInputSet> {
    let rows = sqlx::query_as::<_, GlobalMemoryCandidateProjectRow>(
        GLOBAL_MEMORY_CANDIDATE_PROJECTS_QUERY,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Db)?;
    let projects = rows.into_iter().map(Into::into).collect();
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

pub(crate) async fn next_global_memory_version_number_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<i64> {
    sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM global_memory_versions WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Db)
}

pub(crate) async fn persist_global_memory_success_sqlx(
    pool: &SqlitePool,
    input: &GlobalMemoryPersistInput,
    now: &str,
) -> StoreResult<GlobalMemoryVersion> {
    let mut tx = pool.begin().await.map_err(StoreError::Db)?;
    let job_id = global_memory_id(&input.tenant_id);
    let current_fingerprint: String = sqlx::query_scalar(
        "SELECT input_fingerprint FROM global_memory_jobs WHERE tenant_id = ?1 AND id = ?2 AND status = 'running' AND ownership_token = ?3",
    )
    .bind(&input.tenant_id)
    .bind(&job_id)
    .bind(&input.ownership_token)
    .fetch_optional(&mut *tx)
    .await
    .map_err(StoreError::Db)?
    .ok_or_else(|| StoreError::Conflict("Global Memory job lease is no longer owned".to_string()))?;
    let version_number: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM global_memory_versions WHERE tenant_id = ?1",
    )
    .bind(&input.tenant_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(StoreError::Db)?;
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
    .map_err(StoreError::Db)?;
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
        .map_err(StoreError::Db)?;
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
    .map_err(StoreError::Db)?;
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
    .map_err(StoreError::Db)?;
    tx.commit().await.map_err(StoreError::Db)?;
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

#[allow(dead_code)]
pub(crate) async fn load_global_memory_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<Option<GlobalMemory>> {
    let row = sqlx::query_as::<_, GlobalMemoryRow>(
        "SELECT tenant_id, id, last_successful_version_id, last_successful_at, last_successful_watermark, last_successful_input_fingerprint, summary_document_path, memory_document_path, created_at, updated_at FROM global_memories WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(row.map(map_global))
}

pub(crate) async fn load_global_memory_latest_version_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<Option<GlobalMemoryVersion>> {
    let row = sqlx::query_as::<_, GlobalMemoryVersionRow>(
        "SELECT tenant_id, id, version_number, status, input_fingerprint, source_watermark, summary_markdown, memory_markdown, raw_output_json, error_message, created_at, updated_at FROM global_memory_versions WHERE tenant_id = ?1 AND status = 'succeeded' ORDER BY version_number DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::Db)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let version = map_version(row)?;
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
) -> StoreResult<Vec<GlobalMemorySource>> {
    let rows = sqlx::query_as::<_, GlobalMemorySourceRow>(
        "SELECT project_id, project_path, project_version_id, project_watermark, sort_order FROM global_memory_sources WHERE tenant_id = ?1 AND version_id = ?2 ORDER BY sort_order ASC, project_id ASC",
    )
    .bind(tenant_id)
    .bind(version_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(rows
        .into_iter()
        .map(|row| GlobalMemorySource {
            project_id: row.project_id,
            project_path: row.project_path,
            project_version_id: row.project_version_id,
            project_watermark: row.project_watermark,
            sort_order: row.sort_order,
        })
        .collect())
}

#[allow(dead_code)]
fn map_global(row: GlobalMemoryRow) -> GlobalMemory {
    GlobalMemory {
        tenant_id: row.tenant_id,
        id: row.id,
        last_successful_version_id: row.last_successful_version_id,
        last_successful_at: row.last_successful_at,
        last_successful_watermark: row.last_successful_watermark,
        last_successful_input_fingerprint: row.last_successful_input_fingerprint,
        summary_document_path: row.summary_document_path,
        memory_document_path: row.memory_document_path,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn map_version(row: GlobalMemoryVersionRow) -> StoreResult<GlobalMemoryVersion> {
    Ok(GlobalMemoryVersion {
        tenant_id: row.tenant_id,
        id: row.id,
        version_number: row.version_number,
        status: parse_version_status(row.status)?,
        input_fingerprint: row.input_fingerprint,
        source_watermark: row.source_watermark,
        summary_markdown: row.summary_markdown,
        memory_markdown: row.memory_markdown,
        raw_output_json: row.raw_output_json,
        error_message: row.error_message,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn parse_version_status(value: String) -> StoreResult<GlobalMemoryVersionStatus> {
    match value.as_str() {
        "running" => Ok(GlobalMemoryVersionStatus::Running),
        "succeeded" => Ok(GlobalMemoryVersionStatus::Succeeded),
        "failed" => Ok(GlobalMemoryVersionStatus::Failed),
        "invalid" => Ok(GlobalMemoryVersionStatus::Invalid),
        _ => Err(StoreError::External(format!(
            "unknown Global Memory version status: {value}"
        ))),
    }
}

fn digest(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
#[path = "global_memory_repo_tests.rs"]
mod tests;
