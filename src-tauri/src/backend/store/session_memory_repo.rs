use crate::backend::models::{
    RecentMemoryEvent, RecentMemoryEventCategory, SessionMemory, SessionMemoryJob,
    SessionMemoryJobStatus, SessionMemorySourceReference, SessionMemoryStatus,
};
use crate::backend::runtime::{AppError, AppResult};
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, QueryBuilder, Sqlite, SqlitePool, Transaction};
use std::collections::BTreeMap;
use std::collections::HashSet;

pub(crate) const SESSION_MEMORY_CONTRACT_VERSION: &str = "session-memory.v1";
pub(crate) const SESSION_MEMORY_PROMPT_VERSION: &str = "session-memory-prompt.v1";
pub(crate) const SESSION_MEMORY_JOB_LEASE: Duration = Duration::minutes(2);
const SESSION_IDLE_DELAY: Duration = Duration::minutes(30);

#[derive(Debug, Clone)]
pub(crate) struct SessionMemoryJobCandidate {
    pub(crate) session_id: String,
    pub(crate) source_id: String,
    pub(crate) source_revision: i64,
    pub(crate) source_fingerprint: String,
    pub(crate) not_before: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionMemoryPersistInput {
    pub(crate) memory_id: String,
    pub(crate) tenant_id: String,
    pub(crate) session_id: String,
    pub(crate) source_id: String,
    pub(crate) source_revision: i64,
    pub(crate) source_fingerprint: String,
    pub(crate) contract_version: String,
    pub(crate) prompt_version: String,
    pub(crate) project_path: Option<String>,
    pub(crate) summary: String,
    pub(crate) goal: String,
    pub(crate) result: String,
    pub(crate) decisions_json: String,
    pub(crate) verification_json: String,
    pub(crate) blockers_json: String,
    pub(crate) follow_up_json: String,
    pub(crate) topics_json: String,
    pub(crate) raw_output_json: String,
    pub(crate) generated_at: String,
    pub(crate) ownership_token: String,
    pub(crate) references: Vec<SessionMemoryReferenceInput>,
    pub(crate) events: Vec<RecentMemoryEventInput>,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionMemoryReferenceInput {
    pub(crate) source_id: String,
    pub(crate) session_id: String,
    pub(crate) question_id: Option<String>,
    pub(crate) turn_id: Option<String>,
    pub(crate) part_id: Option<String>,
    pub(crate) node_id: Option<String>,
    pub(crate) node_order: Option<usize>,
    pub(crate) reference_key: String,
    pub(crate) source_revision: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct RecentMemoryEventInput {
    pub(crate) category: RecentMemoryEventCategory,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) occurred_at: String,
    pub(crate) source_reference_id: Option<String>,
    pub(crate) fingerprint: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionMemorySourceCandidate {
    pub(crate) id: String,
    pub(crate) source_id: String,
    pub(crate) source_fingerprint: Option<String>,
    pub(crate) updated_at: Option<String>,
}

pub(crate) async fn enqueue_session_memory_jobs_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
    sync_run_id: &str,
    source_revision: i64,
    source_event_id: &str,
    changed_session_ids: Option<&[String]>,
    excluded_project_root: &str,
    now: &str,
) -> AppResult<usize> {
    let policy = load_memory_generation_policy_sqlx(pool).await?;
    if !policy.enabled || policy.excluded_source_ids.contains(source_id) {
        return Ok(0);
    }
    let session_ids = match changed_session_ids {
        Some(ids) => ids.to_vec(),
        None => sqlx::query_scalar::<_, String>(
            "SELECT session_id FROM conversation_sync_deltas WHERE tenant_id = ?1 AND sync_run_id = ?2 AND record_kind = 'session' ORDER BY session_id ASC",
        )
        .bind(tenant_id)
        .bind(sync_run_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::Db)?,
    };
    let candidates = load_session_candidates_sqlx(
        pool,
        tenant_id,
        source_id,
        &session_ids,
        excluded_project_root,
    )
    .await?
    .into_iter()
    .filter(|candidate| !policy.excluded_session_ids.contains(&candidate.id))
    .collect::<Vec<_>>();
    let mut inserted = 0usize;
    for candidate in candidates {
        inserted += insert_job_sqlx(
            pool,
            tenant_id,
            &candidate,
            source_revision,
            source_event_id,
            sync_run_id,
            now,
        )
        .await?;
    }
    Ok(inserted)
}

pub(crate) async fn backfill_session_memory_jobs_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    excluded_project_root: &str,
    now: &str,
) -> AppResult<usize> {
    let policy = load_memory_generation_policy_sqlx(pool).await?;
    if !policy.enabled {
        return Ok(0);
    }
    let source_revision = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT source_revision FROM conversation_search_index_state WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?
    .flatten()
    .unwrap_or(0);
    let sources = sqlx::query_as::<_, SessionMemorySourceCandidateRow>(
        "SELECT id, source_id, source_fingerprint, updated_at FROM conversation_sessions WHERE tenant_id = ?1 AND missing = 0 AND (?2 = '' OR project_path IS NULL OR (project_path <> ?2 AND instr(project_path, ?2 || '/') <> 1)) ORDER BY id ASC",
    )
    .bind(tenant_id)
    .bind(excluded_project_root)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)?;
    let mut inserted = 0usize;
    for row in sources {
        let candidate: SessionMemorySourceCandidate = row.into();
        if policy.excluded_session_ids.contains(&candidate.id)
            || policy.excluded_source_ids.contains(&candidate.source_id)
        {
            continue;
        }
        inserted += insert_job_sqlx(
            pool,
            tenant_id,
            &candidate,
            source_revision,
            "backfill:session-memory",
            "backfill:session-memory",
            now,
        )
        .await?;
    }
    Ok(inserted)
}

struct MemoryGenerationPolicy {
    enabled: bool,
    excluded_session_ids: HashSet<String>,
    excluded_source_ids: HashSet<String>,
}

async fn load_memory_generation_policy_sqlx(
    pool: &SqlitePool,
) -> AppResult<MemoryGenerationPolicy> {
    let settings = super::settings_repo::load_app_settings_sqlx(pool)
        .await?
        .map(|(_, value)| value)
        .unwrap_or_default();
    let memory = settings
        .get("memory")
        .and_then(serde_json::Value::as_object);
    let enabled = memory
        .and_then(|memory| memory.get("generationEnabled"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let values = |key: &str| {
        memory
            .and_then(|memory| memory.get(key))
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_string)
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default()
    };
    Ok(MemoryGenerationPolicy {
        enabled,
        excluded_session_ids: values("excludedSessionIds"),
        excluded_source_ids: values("excludedSourceIds"),
    })
}

async fn load_session_candidates_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
    session_ids: &[String],
    excluded_project_root: &str,
) -> AppResult<Vec<SessionMemorySourceCandidate>> {
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT id, source_id, source_fingerprint, updated_at FROM conversation_sessions WHERE tenant_id = ",
    );
    query.push_bind(tenant_id);
    query.push(" AND source_id = ");
    query.push_bind(source_id);
    query.push(" AND missing = 0 AND (");
    query.push_bind(excluded_project_root);
    query.push(" = '' OR project_path IS NULL OR (project_path <> ");
    query.push_bind(excluded_project_root);
    query.push(" AND instr(project_path, ");
    query.push_bind(excluded_project_root);
    query.push(" || '/') <> 1)) AND id IN (");
    {
        let mut separated = query.separated(", ");
        for id in session_ids {
            separated.push_bind(id);
        }
    }
    query.push(") ORDER BY id ASC");
    let rows = query
        .build_query_as::<SessionMemorySourceCandidateRow>()
        .fetch_all(pool)
        .await
        .map_err(AppError::Db)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let mut candidate: SessionMemorySourceCandidate = row.into();
            candidate.source_id = source_id.to_string();
            candidate
        })
        .collect())
}

fn candidate_with_not_before(
    candidate: &SessionMemorySourceCandidate,
    source_revision: i64,
    now: &str,
) -> SessionMemoryJobCandidate {
    let not_before = candidate
        .updated_at
        .as_deref()
        .and_then(crate::backend::models::parse_conversation_timestamp)
        .map(|value| (value + SESSION_IDLE_DELAY).to_rfc3339())
        .unwrap_or_else(|| now.to_string());
    SessionMemoryJobCandidate {
        session_id: candidate.id.clone(),
        source_id: candidate.source_id.clone(),
        source_revision,
        source_fingerprint: candidate
            .source_fingerprint
            .clone()
            .unwrap_or_else(|| fallback_fingerprint(&candidate.id, source_revision)),
        not_before,
    }
}

fn fallback_fingerprint(session_id: &str, source_revision: i64) -> String {
    digest(&format!("{session_id}\0{source_revision}"))
}

fn digest(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

async fn insert_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_candidate: &SessionMemorySourceCandidate,
    source_revision: i64,
    source_event_id: &str,
    sync_run_id: &str,
    now: &str,
) -> AppResult<usize> {
    let candidate = candidate_with_not_before(source_candidate, source_revision, now);
    insert_job_candidate_sqlx(
        pool,
        tenant_id,
        &candidate,
        source_event_id,
        sync_run_id,
        now,
    )
    .await
}

async fn insert_job_candidate_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    candidate: &SessionMemoryJobCandidate,
    source_event_id: &str,
    sync_run_id: &str,
    now: &str,
) -> AppResult<usize> {
    sqlx::query(
        "UPDATE session_memories SET status = 'invalid', updated_at = ?1 WHERE tenant_id = ?2 AND session_id = ?3 AND status = 'active' AND (source_revision < ?4 OR source_fingerprint <> ?5 OR contract_version <> ?6 OR prompt_version <> ?7)",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(&candidate.session_id)
    .bind(candidate.source_revision)
    .bind(&candidate.source_fingerprint)
    .bind(SESSION_MEMORY_CONTRACT_VERSION)
    .bind(SESSION_MEMORY_PROMPT_VERSION)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    sqlx::query(
        "UPDATE session_memory_jobs SET status = 'skipped', last_error = 'superseded_by_newer_watermark', finished_at = ?1, updated_at = ?1, retry_at = NULL, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL WHERE tenant_id = ?2 AND session_id = ?3 AND source_revision < ?4 AND status IN ('queued', 'failed')",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(&candidate.session_id)
    .bind(candidate.source_revision)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    let id = format!(
        "session-memory-job-{}",
        digest(&format!(
            "{tenant_id}\0{}\0{}\0{}\0{}\0{}\0{}",
            candidate.session_id,
            candidate.source_id,
            candidate.source_revision,
            candidate.source_fingerprint,
            SESSION_MEMORY_CONTRACT_VERSION,
            SESSION_MEMORY_PROMPT_VERSION
        ))
    );
    let result = sqlx::query(
        r#"
        INSERT OR IGNORE INTO session_memory_jobs (
            tenant_id, id, session_id, source_id, source_revision,
            source_fingerprint, contract_version, prompt_version,
            source_event_id, source_sync_run_id, status, not_before,
            attempt_count, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'queued', ?11, 0, ?12, ?12)
        "#,
    )
    .bind(tenant_id)
    .bind(id)
    .bind(&candidate.session_id)
    .bind(&candidate.source_id)
    .bind(candidate.source_revision)
    .bind(&candidate.source_fingerprint)
    .bind(SESSION_MEMORY_CONTRACT_VERSION)
    .bind(SESSION_MEMORY_PROMPT_VERSION)
    .bind(source_event_id)
    .bind(sync_run_id)
    .bind(&candidate.not_before)
    .bind(now)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(result.rows_affected() as usize)
}

pub(crate) async fn load_session_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
) -> AppResult<Option<SessionMemoryJob>> {
    let row = sqlx::query_as::<_, SessionMemoryJobRow>(
        "SELECT tenant_id, id, session_id, source_id, source_revision, source_fingerprint, contract_version, prompt_version, source_event_id, source_sync_run_id, status, not_before, attempt_count, last_error, started_at, finished_at, created_at, updated_at, ownership_token, lease_expires_at, heartbeat_at, retry_count, retry_at, watermark FROM session_memory_jobs WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;
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
) -> AppResult<Option<SessionMemoryJob>> {
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
    .map_err(AppError::Db)?;
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
) -> AppResult<bool> {
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
    .map_err(AppError::Db)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn recover_expired_session_memory_leases_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    now: &str,
) -> AppResult<usize> {
    let result = sqlx::query(
        "UPDATE session_memory_jobs SET status = 'queued', ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, retry_count = retry_count + 1, retry_at = ?1, last_error = 'lease_expired', updated_at = ?1 WHERE tenant_id = ?2 AND status = 'running' AND (lease_expires_at IS NULL OR lease_expires_at <= ?1)",
    )
    .bind(now)
    .bind(tenant_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(result.rows_affected() as usize)
}

pub(crate) async fn mark_session_memory_job_failed_with_lease_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    ownership_token: &str,
    error_code: &str,
    now: &str,
) -> AppResult<bool> {
    let retry_count = sqlx::query_scalar::<_, i64>(
        "SELECT retry_count FROM session_memory_jobs WHERE tenant_id = ?1 AND id = ?2 AND status = 'running' AND ownership_token = ?3",
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
    let retry_at = next_retry_at(now, retry_count + 1)?;
    let result = sqlx::query(
        "UPDATE session_memory_jobs SET status = 'failed', last_error = ?1, finished_at = ?2, updated_at = ?2, retry_count = retry_count + 1, retry_at = ?3, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL WHERE tenant_id = ?4 AND id = ?5 AND status = 'running' AND ownership_token = ?6",
    )
    .bind(error_code)
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

pub(crate) async fn cancel_session_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    now: &str,
) -> AppResult<bool> {
    let result = sqlx::query(
        "UPDATE session_memory_jobs SET status = 'canceled', last_error = 'canceled', finished_at = ?1, updated_at = ?1, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL WHERE tenant_id = ?2 AND id = ?3 AND status IN ('queued', 'running', 'failed')",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn retry_session_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
) -> AppResult<bool> {
    let result = sqlx::query(
        "UPDATE session_memory_jobs SET status = 'queued', last_error = NULL, retry_at = NULL, finished_at = NULL, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3 AND status IN ('failed', 'canceled')",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(tenant_id)
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn list_due_session_memory_job_ids_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    now: &str,
    limit: i64,
) -> AppResult<Vec<String>> {
    sqlx::query_scalar(
        "SELECT id FROM session_memory_jobs WHERE tenant_id = ?1 AND ((status = 'queued' AND not_before <= ?2) OR (status = 'failed' AND retry_at IS NOT NULL AND retry_at <= ?2)) ORDER BY created_at ASC, id ASC LIMIT ?3",
    )
    .bind(tenant_id)
    .bind(now)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}

pub(crate) async fn list_session_memory_job_ids_for_scheduler_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    now: &str,
    limit: i64,
) -> AppResult<Vec<String>> {
    sqlx::query_scalar(
        "SELECT id FROM session_memory_jobs WHERE tenant_id = ?1 AND (status = 'queued' OR (status = 'failed' AND retry_at IS NOT NULL AND retry_at <= ?2)) ORDER BY CASE WHEN status = 'failed' OR not_before <= ?2 THEN 0 ELSE 1 END, not_before DESC, created_at DESC, id ASC LIMIT ?3",
    )
    .bind(tenant_id)
    .bind(now)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}

fn lease_expires_at(now: &str, lease_duration: Duration) -> AppResult<String> {
    DateTime::parse_from_rfc3339(now)
        .map(|value| (value.with_timezone(&Utc) + lease_duration).to_rfc3339())
        .map_err(|_| AppError::Validation("invalid Session Memory lease timestamp".to_string()))
}

fn next_retry_at(now: &str, retry_count: i64) -> AppResult<String> {
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
        .map_err(|_| AppError::Validation("invalid Session Memory retry timestamp".to_string()))
}

pub(crate) async fn persist_session_memory_sqlx(
    pool: &SqlitePool,
    input: &SessionMemoryPersistInput,
) -> AppResult<()> {
    let mut tx = pool.begin().await.map_err(AppError::Db)?;
    let claimed = sqlx::query(
        "UPDATE session_memory_jobs SET status = 'succeeded', finished_at = ?1, updated_at = ?1, last_error = NULL, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, retry_at = NULL, watermark = source_revision WHERE tenant_id = ?2 AND id = (SELECT id FROM session_memory_jobs WHERE tenant_id = ?2 AND session_id = ?3 AND source_revision = ?4 AND source_fingerprint = ?5 AND contract_version = ?6 AND prompt_version = ?7 AND status = 'running' AND ownership_token = ?8 LIMIT 1) AND status = 'running' AND ownership_token = ?8",
    )
    .bind(&input.generated_at)
    .bind(&input.tenant_id)
    .bind(&input.session_id)
    .bind(input.source_revision)
    .bind(&input.source_fingerprint)
    .bind(&input.contract_version)
    .bind(&input.prompt_version)
    .bind(&input.ownership_token)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Db)?;
    if claimed.rows_affected() != 1 {
        return Err(AppError::Conflict(
            "Session Memory job lease is no longer owned".to_string(),
        ));
    }
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO session_memories (
            tenant_id, id, session_id, source_id, source_revision,
            source_fingerprint, contract_version, prompt_version, status,
            project_path, summary, goal, result, decisions_json,
            verification_json, blockers_json, follow_up_json, topics_json,
            raw_output_json, generated_at, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'active', ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?19, ?19)
        "#,
    )
    .bind(&input.tenant_id)
    .bind(&input.memory_id)
    .bind(&input.session_id)
    .bind(&input.source_id)
    .bind(input.source_revision)
    .bind(&input.source_fingerprint)
    .bind(&input.contract_version)
    .bind(&input.prompt_version)
    .bind(&input.project_path)
    .bind(&input.summary)
    .bind(&input.goal)
    .bind(&input.result)
    .bind(&input.decisions_json)
    .bind(&input.verification_json)
    .bind(&input.blockers_json)
    .bind(&input.follow_up_json)
    .bind(&input.topics_json)
    .bind(&input.raw_output_json)
    .bind(&input.generated_at)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Db)?;
    let memory_id = input.memory_id.clone();
    for (index, reference) in input.references.iter().enumerate() {
        let reference_id = format!(
            "session-memory-ref-{}",
            digest(&format!("{}\0{}", memory_id, reference.reference_key))
        );
        insert_source_reference_sqlx(&mut tx, input, reference, &reference_id, &memory_id)
            .await
            .map_err(|error| AppError::Domain {
                code: "session_memory_reference_persist_failed".to_string(),
                message: format!("source reference {index} could not be stored: {error}"),
                retryable: false,
                details: None,
            })?;
    }
    for event in &input.events {
        let event_id = format!(
            "recent-event-{}",
            digest(&format!("{}\0{}", memory_id, event.fingerprint))
        );
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO recent_memory_events (
                tenant_id, id, memory_id, session_id, category, title, summary,
                occurred_at, source_reference_id, fingerprint, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
        )
        .bind(&input.tenant_id)
        .bind(event_id)
        .bind(&memory_id)
        .bind(&input.session_id)
        .bind(event.category.as_str())
        .bind(&event.title)
        .bind(&event.summary)
        .bind(&event.occurred_at)
        .bind(&event.source_reference_id)
        .bind(&event.fingerprint)
        .bind(&input.generated_at)
        .execute(&mut *tx)
        .await
        .map_err(AppError::Db)?;
    }
    if let Some(project_path) = input.project_path.as_deref() {
        super::project_memory_repo::enqueue_project_memory_job_tx(
            &mut tx,
            &input.tenant_id,
            project_path,
            &input.generated_at,
        )
        .await?;
    }
    tx.commit().await.map_err(AppError::Db)
}

async fn insert_source_reference_sqlx(
    tx: &mut Transaction<'_, Sqlite>,
    input: &SessionMemoryPersistInput,
    reference: &SessionMemoryReferenceInput,
    reference_id: &str,
    memory_id: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO session_memory_source_references (
            tenant_id, id, memory_id, source_id, session_id, record_kind,
            question_id, turn_id, part_id, node_id, node_order,
            reference_key, source_revision, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, 'session', ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
    )
    .bind(&input.tenant_id)
    .bind(reference_id)
    .bind(memory_id)
    .bind(&reference.source_id)
    .bind(&reference.session_id)
    .bind(&reference.question_id)
    .bind(&reference.turn_id)
    .bind(&reference.part_id)
    .bind(&reference.node_id)
    .bind(reference.node_order.map(|value| value as i64))
    .bind(&reference.reference_key)
    .bind(reference.source_revision)
    .bind(&input.generated_at)
    .execute(&mut **tx)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}

pub(crate) async fn load_session_memory_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    memory_id: &str,
) -> AppResult<Option<SessionMemory>> {
    let row = sqlx::query_as::<_, SessionMemoryRow>(
        "SELECT tenant_id, id, session_id, source_id, source_revision, source_fingerprint, contract_version, prompt_version, status, project_path, summary, goal, result, decisions_json, verification_json, blockers_json, follow_up_json, topics_json, generated_at, created_at, updated_at FROM session_memories WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(memory_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;
    row.map(|r| r.try_into_memory()).transpose()
}

pub(crate) async fn list_session_memories_for_project_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    project_path: &str,
) -> AppResult<Vec<SessionMemory>> {
    let rows = sqlx::query_as::<_, SessionMemoryRow>(
        "SELECT m.tenant_id, m.id, m.session_id, m.source_id, m.source_revision, m.source_fingerprint, m.contract_version, m.prompt_version, m.status, m.project_path, m.summary, m.goal, m.result, m.decisions_json, m.verification_json, m.blockers_json, m.follow_up_json, m.topics_json, m.generated_at, m.created_at, m.updated_at FROM session_memories m WHERE m.tenant_id = ?1 AND m.project_path = ?2 AND m.status = 'active' AND NOT EXISTS (SELECT 1 FROM session_memories newer WHERE newer.tenant_id = m.tenant_id AND newer.session_id = m.session_id AND newer.status = 'active' AND (newer.source_revision > m.source_revision OR (newer.source_revision = m.source_revision AND newer.id > m.id))) AND (NOT EXISTS (SELECT 1 FROM conversation_sessions c WHERE c.tenant_id = m.tenant_id AND c.id = m.session_id) OR EXISTS (SELECT 1 FROM conversation_sessions c WHERE c.tenant_id = m.tenant_id AND c.id = m.session_id AND c.source_id = m.source_id AND c.missing = 0 AND EXISTS (SELECT 1 FROM conversation_sources source WHERE source.tenant_id = c.tenant_id AND source.id = c.source_id AND source.enabled = 1) AND (c.source_fingerprint IS NULL OR c.source_fingerprint = m.source_fingerprint))) ORDER BY m.id ASC",
    )
    .bind(tenant_id)
    .bind(project_path)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)?;
    rows.into_iter().map(|r| r.try_into_memory()).collect()
}

pub(crate) async fn load_session_memory_for_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job: &SessionMemoryJob,
) -> AppResult<Option<SessionMemory>> {
    let memory_id = format!(
        "session-memory-{}",
        digest(&format!(
            "{}\0{}\0{}",
            job.tenant_id, job.id, job.source_revision
        ))
    );
    load_session_memory_sqlx(pool, tenant_id, &memory_id).await
}

pub(crate) async fn list_session_memory_source_references_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    memory_id: &str,
) -> AppResult<Vec<SessionMemorySourceReference>> {
    let rows = sqlx::query_as::<_, SessionMemorySourceReferenceRow>(
        "SELECT tenant_id, id, memory_id, source_id, session_id, question_id, turn_id, part_id, node_id, node_order, reference_key, source_revision, created_at FROM session_memory_source_references WHERE tenant_id = ?1 AND memory_id = ?2 ORDER BY reference_key ASC",
    )
    .bind(tenant_id)
    .bind(memory_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub(crate) async fn list_recent_memory_events_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
) -> AppResult<Vec<RecentMemoryEvent>> {
    let rows = sqlx::query_as::<_, RecentMemoryEventRow>(
        "SELECT e.tenant_id, e.id, e.memory_id, e.session_id, e.category, e.title, e.summary, e.occurred_at, e.source_reference_id, e.fingerprint, e.created_at FROM recent_memory_events e JOIN session_memories m ON m.tenant_id = e.tenant_id AND m.id = e.memory_id WHERE e.tenant_id = ?1 AND e.session_id = ?2 AND m.status = 'active' AND NOT EXISTS (SELECT 1 FROM session_memories newer WHERE newer.tenant_id = m.tenant_id AND newer.session_id = m.session_id AND newer.status = 'active' AND (newer.source_revision > m.source_revision OR (newer.source_revision = m.source_revision AND newer.id > m.id))) ORDER BY e.occurred_at DESC, e.id ASC",
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)?;
    rows.into_iter().map(|r| r.try_into_event()).collect()
}

pub(crate) async fn load_recent_memory_event_target_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    event_id: &str,
) -> AppResult<Option<crate::backend::dto::RecentMemoryEventTarget>> {
    let row = sqlx::query_as::<_, RecentMemoryEventTargetRow>(
        "SELECT e.session_id, r.question_id, r.turn_id, r.node_id FROM recent_memory_events e JOIN session_memories m ON m.tenant_id = e.tenant_id AND m.id = e.memory_id LEFT JOIN session_memory_source_references r ON r.tenant_id = e.tenant_id AND r.id = e.source_reference_id WHERE e.tenant_id = ?1 AND e.id = ?2 AND m.status = 'active' AND NOT EXISTS (SELECT 1 FROM session_memories newer WHERE newer.tenant_id = m.tenant_id AND newer.session_id = m.session_id AND newer.status = 'active' AND (newer.source_revision > m.source_revision OR (newer.source_revision = m.source_revision AND newer.id > m.id))) AND EXISTS (SELECT 1 FROM conversation_sessions c JOIN conversation_sources source ON source.tenant_id = c.tenant_id AND source.id = c.source_id AND source.enabled = 1 WHERE c.tenant_id = e.tenant_id AND c.id = e.session_id AND c.missing = 0)",
    )
    .bind(tenant_id)
    .bind(event_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(row.map(Into::into))
}

pub(crate) async fn list_recent_memory_events_for_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_ids: &[String],
    cutoff: &str,
    now: &str,
) -> AppResult<BTreeMap<String, Vec<RecentMemoryEvent>>> {
    if session_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT e.tenant_id, e.id, e.memory_id, e.session_id, e.category, e.title, e.summary, e.occurred_at, e.source_reference_id, e.fingerprint, e.created_at FROM recent_memory_events e JOIN session_memories m ON m.tenant_id = e.tenant_id AND m.id = e.memory_id WHERE e.tenant_id = ",
    );
    query.push_bind(tenant_id);
    query.push(" AND m.status = 'active' AND datetime(e.occurred_at) >= datetime(");
    query.push_bind(cutoff);
    query.push(") AND datetime(e.occurred_at) <= datetime(");
    query.push_bind(now);
    query.push(") AND e.session_id IN (");
    {
        let mut separated = query.separated(", ");
        for session_id in session_ids {
            separated.push_bind(session_id);
        }
    }
    query.push(") AND NOT EXISTS (SELECT 1 FROM session_memories newer WHERE newer.tenant_id = m.tenant_id AND newer.session_id = m.session_id AND newer.status = 'active' AND (newer.source_revision > m.source_revision OR (newer.source_revision = m.source_revision AND newer.id > m.id))) ORDER BY e.session_id ASC, e.occurred_at DESC, e.id ASC");
    let rows = query
        .build_query_as::<RecentMemoryEventRow>()
        .fetch_all(pool)
        .await
        .map_err(AppError::Db)?;
    let mut events_by_session = BTreeMap::new();
    for row in rows {
        let event = row.try_into_event()?;
        events_by_session
            .entry(event.session_id.clone())
            .or_insert_with(Vec::new)
            .push(event);
    }
    Ok(events_by_session)
}

pub(crate) async fn count_session_memory_rows_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    table: &str,
) -> AppResult<i64> {
    let query = match table {
        "jobs" => "SELECT COUNT(*) FROM session_memory_jobs WHERE tenant_id = ?1",
        "memories" => "SELECT COUNT(*) FROM session_memories WHERE tenant_id = ?1",
        "events" => "SELECT COUNT(*) FROM recent_memory_events WHERE tenant_id = ?1",
        "references" => {
            "SELECT COUNT(*) FROM session_memory_source_references WHERE tenant_id = ?1"
        }
        _ => {
            return Err(AppError::Validation(
                "unknown Session Memory row kind".to_string(),
            ))
        }
    };
    sqlx::query_scalar(query)
        .bind(tenant_id)
        .fetch_one(pool)
        .await
        .map_err(AppError::Db)
}

#[derive(Debug, FromRow)]
struct SessionMemorySourceCandidateRow {
    id: String,
    source_id: String,
    source_fingerprint: Option<String>,
    updated_at: Option<String>,
}

impl From<SessionMemorySourceCandidateRow> for SessionMemorySourceCandidate {
    fn from(row: SessionMemorySourceCandidateRow) -> Self {
        Self {
            id: row.id,
            source_id: row.source_id,
            source_fingerprint: row.source_fingerprint,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, FromRow)]
struct RecentMemoryEventTargetRow {
    session_id: String,
    question_id: Option<String>,
    turn_id: Option<String>,
    node_id: Option<String>,
}

impl From<RecentMemoryEventTargetRow> for crate::backend::dto::RecentMemoryEventTarget {
    fn from(row: RecentMemoryEventTargetRow) -> Self {
        Self {
            record_kind: "session".to_string(),
            session_id: row.session_id,
            question_id: row.question_id,
            turn_id: row.turn_id,
            block_id: row.node_id,
        }
    }
}

#[derive(Debug, FromRow)]
struct SessionMemoryJobRow {
    tenant_id: String,
    id: String,
    session_id: String,
    source_id: String,
    source_revision: i64,
    source_fingerprint: String,
    contract_version: String,
    prompt_version: String,
    source_event_id: String,
    source_sync_run_id: String,
    status: String,
    not_before: String,
    attempt_count: i64,
    last_error: Option<String>,
    started_at: Option<String>,
    finished_at: Option<String>,
    created_at: String,
    updated_at: String,
    ownership_token: Option<String>,
    lease_expires_at: Option<String>,
    heartbeat_at: Option<String>,
    retry_count: i64,
    retry_at: Option<String>,
    watermark: Option<i64>,
}

impl SessionMemoryJobRow {
    fn try_into_job(self) -> AppResult<SessionMemoryJob> {
        Ok(SessionMemoryJob {
            tenant_id: self.tenant_id,
            id: self.id,
            session_id: self.session_id,
            source_id: self.source_id,
            source_revision: self.source_revision,
            source_fingerprint: self.source_fingerprint,
            contract_version: self.contract_version,
            prompt_version: self.prompt_version,
            source_event_id: self.source_event_id,
            source_sync_run_id: self.source_sync_run_id,
            status: parse_job_status(&self.status)?,
            not_before: self.not_before,
            attempt_count: self.attempt_count,
            last_error: self.last_error,
            started_at: self.started_at,
            finished_at: self.finished_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
            ownership_token: self.ownership_token,
            lease_expires_at: self.lease_expires_at,
            heartbeat_at: self.heartbeat_at,
            retry_count: self.retry_count,
            retry_at: self.retry_at,
            watermark: self.watermark,
        })
    }
}

#[derive(Debug, FromRow)]
struct SessionMemoryRow {
    tenant_id: String,
    id: String,
    session_id: String,
    source_id: String,
    source_revision: i64,
    source_fingerprint: String,
    contract_version: String,
    prompt_version: String,
    status: String,
    project_path: Option<String>,
    summary: String,
    goal: String,
    result: String,
    decisions_json: String,
    verification_json: String,
    blockers_json: String,
    follow_up_json: String,
    topics_json: String,
    generated_at: String,
    created_at: String,
    updated_at: String,
}

impl SessionMemoryRow {
    fn try_into_memory(self) -> AppResult<SessionMemory> {
        Ok(SessionMemory {
            tenant_id: self.tenant_id,
            id: self.id,
            session_id: self.session_id,
            source_id: self.source_id,
            source_revision: self.source_revision,
            source_fingerprint: self.source_fingerprint,
            contract_version: self.contract_version,
            prompt_version: self.prompt_version,
            status: parse_memory_status(&self.status)?,
            project_path: self.project_path,
            summary: self.summary,
            goal: self.goal,
            result: self.result,
            decisions: decode_string_array(&self.decisions_json)?,
            verification: decode_string_array(&self.verification_json)?,
            blockers: decode_string_array(&self.blockers_json)?,
            follow_up: decode_string_array(&self.follow_up_json)?,
            topics: decode_string_array(&self.topics_json)?,
            generated_at: self.generated_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct SessionMemorySourceReferenceRow {
    tenant_id: String,
    id: String,
    memory_id: String,
    source_id: String,
    session_id: String,
    question_id: Option<String>,
    turn_id: Option<String>,
    part_id: Option<String>,
    node_id: Option<String>,
    node_order: Option<i64>,
    reference_key: String,
    source_revision: i64,
    created_at: String,
}

impl From<SessionMemorySourceReferenceRow> for SessionMemorySourceReference {
    fn from(row: SessionMemorySourceReferenceRow) -> Self {
        Self {
            tenant_id: row.tenant_id,
            id: row.id,
            memory_id: row.memory_id,
            source_id: row.source_id,
            session_id: row.session_id,
            question_id: row.question_id,
            turn_id: row.turn_id,
            part_id: row.part_id,
            node_id: row.node_id,
            node_order: row.node_order.map(|v| v as usize),
            reference_key: row.reference_key,
            source_revision: row.source_revision,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug, FromRow)]
struct RecentMemoryEventRow {
    tenant_id: String,
    id: String,
    memory_id: String,
    session_id: String,
    category: String,
    title: String,
    summary: String,
    occurred_at: String,
    source_reference_id: Option<String>,
    fingerprint: String,
    created_at: String,
}

impl RecentMemoryEventRow {
    fn try_into_event(self) -> AppResult<RecentMemoryEvent> {
        Ok(RecentMemoryEvent {
            tenant_id: self.tenant_id,
            id: self.id,
            memory_id: self.memory_id,
            session_id: self.session_id,
            category: RecentMemoryEventCategory::parse(&self.category)
                .ok_or_else(|| AppError::external("invalid Recent Event category"))?,
            title: self.title,
            summary: self.summary,
            occurred_at: self.occurred_at,
            source_reference_id: self.source_reference_id,
            fingerprint: self.fingerprint,
            created_at: self.created_at,
        })
    }
}

fn decode_string_array(value: &str) -> AppResult<Vec<String>> {
    serde_json::from_str(value).map_err(AppError::external)
}

fn parse_job_status(value: &str) -> AppResult<SessionMemoryJobStatus> {
    match value {
        "queued" => Ok(SessionMemoryJobStatus::Queued),
        "running" => Ok(SessionMemoryJobStatus::Running),
        "succeeded" => Ok(SessionMemoryJobStatus::Succeeded),
        "failed" => Ok(SessionMemoryJobStatus::Failed),
        "skipped" => Ok(SessionMemoryJobStatus::Skipped),
        "canceled" => Ok(SessionMemoryJobStatus::Canceled),
        _ => Err(AppError::external("invalid Session Memory job status")),
    }
}

fn parse_memory_status(value: &str) -> AppResult<SessionMemoryStatus> {
    match value {
        "active" => Ok(SessionMemoryStatus::Active),
        "invalid" => Ok(SessionMemoryStatus::Invalid),
        "failed" => Ok(SessionMemoryStatus::Failed),
        _ => Err(AppError::external("invalid Session Memory status")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_memory_identity_is_stable_and_revision_bound() {
        let first = digest(
            "tenant\0session\0source\01\0fingerprint\0session-memory.v1\0session-memory-prompt.v1",
        );
        let second = digest(
            "tenant\0session\0source\02\0fingerprint\0session-memory.v1\0session-memory-prompt.v1",
        );
        assert_ne!(first, second);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_new_source_watermark_invalidates_old_projection_and_is_idempotent() {
        let path = std::env::temp_dir().join(format!(
            "assetiweave-session-memory-invalidation-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open_initialized_async(&path)
            .await
            .expect("open invalidation fixture");
        sqlx::query(
            "INSERT INTO session_memories (tenant_id,id,session_id,source_id,source_revision,source_fingerprint,contract_version,prompt_version,status,project_path,summary,goal,result,decisions_json,verification_json,blockers_json,follow_up_json,topics_json,raw_output_json,generated_at,created_at,updated_at) VALUES ('default','memory-old','session-invalidation','source-invalidation',1,'fingerprint-old','session-memory.v1','session-memory-prompt.v1','active','/project','old summary','','','[]','[]','[]','[]','[]','{}','2026-08-31T00:00:00Z','2026-08-31T00:00:00Z','2026-08-31T00:00:00Z')",
        ).execute(database.pool()).await.expect("insert active projection");
        let old = SessionMemoryJobCandidate {
            session_id: "session-invalidation".to_string(),
            source_id: "source-invalidation".to_string(),
            source_revision: 1,
            source_fingerprint: "fingerprint-old".to_string(),
            not_before: "2026-08-31T00:00:00Z".to_string(),
        };
        let new = SessionMemoryJobCandidate {
            source_revision: 2,
            source_fingerprint: "fingerprint-new".to_string(),
            not_before: "2026-08-31T00:01:00Z".to_string(),
            ..old.clone()
        };
        assert_eq!(
            insert_job_candidate_sqlx(
                database.pool(),
                "default",
                &old,
                "event-old",
                "sync-old",
                "2026-08-31T00:00:00Z",
            )
            .await
            .expect("insert old job"),
            1
        );
        assert_eq!(
            insert_job_candidate_sqlx(
                database.pool(),
                "default",
                &new,
                "event-new",
                "sync-new",
                "2026-08-31T00:01:00Z",
            )
            .await
            .expect("insert new job"),
            1
        );
        let projection_state: (String, String) = sqlx::query_as(
            "SELECT status, source_fingerprint FROM session_memories WHERE tenant_id = 'default' AND id = 'memory-old'",
        )
        .fetch_one(database.pool())
        .await
        .expect("read invalidated projection");
        assert_eq!(
            projection_state,
            ("invalid".to_string(), "fingerprint-old".to_string())
        );
        let old_job_status: String = sqlx::query_scalar(
            "SELECT status FROM session_memory_jobs WHERE tenant_id = 'default' AND session_id = 'session-invalidation' AND source_revision = 1",
        )
        .fetch_one(database.pool())
        .await
        .expect("read superseded job");
        assert_eq!(old_job_status, "skipped");
        assert_eq!(
            insert_job_candidate_sqlx(
                database.pool(),
                "default",
                &new,
                "event-new-repeat",
                "sync-new-repeat",
                "2026-08-31T00:02:00Z",
            )
            .await
            .expect("repeat new job"),
            0
        );
        let active_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM session_memories WHERE tenant_id = 'default' AND session_id = 'session-invalidation' AND status = 'active'",
        )
        .fetch_one(database.pool())
        .await
        .expect("count active projections");
        assert_eq!(active_count, 0);
        drop(database);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn every_recent_event_category_has_a_wire_name() {
        assert_eq!(
            RecentMemoryEventCategory::ALL
                .into_iter()
                .map(RecentMemoryEventCategory::as_str)
                .collect::<Vec<_>>(),
            vec![
                "progress",
                "decision",
                "research",
                "verification",
                "blocker",
                "follow_up"
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn scheduler_prioritizes_the_most_recent_ready_session() {
        let path = std::env::temp_dir().join(format!(
            "assetiweave-session-memory-scheduler-priority-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open_initialized_async(&path)
            .await
            .expect("open scheduler priority fixture");
        let old = SessionMemoryJobCandidate {
            session_id: "old-session".to_string(),
            source_id: "source".to_string(),
            source_revision: 1,
            source_fingerprint: "old-fingerprint".to_string(),
            not_before: "2026-08-01T00:30:00Z".to_string(),
        };
        let recent = SessionMemoryJobCandidate {
            session_id: "recent-session".to_string(),
            source_fingerprint: "recent-fingerprint".to_string(),
            not_before: "2026-08-31T23:30:00Z".to_string(),
            ..old.clone()
        };
        insert_job_candidate_sqlx(
            database.pool(),
            "default",
            &old,
            "event-old",
            "sync-old",
            "2026-08-01T00:00:00Z",
        )
        .await
        .expect("insert old job");
        insert_job_candidate_sqlx(
            database.pool(),
            "default",
            &recent,
            "event-recent",
            "sync-recent",
            "2026-08-31T23:00:00Z",
        )
        .await
        .expect("insert recent job");

        let jobs = list_session_memory_job_ids_for_scheduler_sqlx(
            database.pool(),
            "default",
            "2026-09-01T00:00:00Z",
            2,
        )
        .await
        .expect("list scheduler jobs");
        let mut sessions = Vec::new();
        for job_id in &jobs {
            let job = load_session_memory_job_sqlx(database.pool(), "default", job_id)
                .await
                .expect("load scheduler job")
                .expect("scheduler job exists");
            sessions.push(job.session_id);
        }
        assert_eq!(sessions, vec!["recent-session", "old-session"]);

        drop(database);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn durable_job_lease_recovery_retry_and_cancellation_are_token_bound() {
        let path = std::env::temp_dir().join(format!(
            "assetiweave-session-memory-durable-red-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open_initialized_async(&path)
            .await
            .expect("open durable job fixture");
        sqlx::query(
            r#"
            INSERT INTO conversation_sessions (
                tenant_id, id, source_id, adapter_id, external_id, title,
                project_path, started_at, updated_at, source_locator,
                source_fingerprint, missing, created_at, imported_at
            ) VALUES (
                'default', 'durable-session', 'durable-source', 'durable-adapter',
                'durable-external', 'Durable fixture', NULL,
                '2026-08-30T00:00:00Z', '2026-08-30T00:00:00Z',
                'fixture://durable-session', 'durable-revision', 0,
                '2026-08-30T00:00:00Z', '2026-08-30T00:00:00Z'
            )
            "#,
        )
        .execute(database.pool())
        .await
        .expect("insert durable session");
        let now = "2026-08-31T00:00:00Z";
        assert_eq!(
            enqueue_session_memory_jobs_sqlx(
                database.pool(),
                "default",
                "durable-source",
                "durable-sync",
                1,
                "durable-event",
                Some(&["durable-session".to_string()]),
                "",
                now,
            )
            .await
            .expect("enqueue durable job"),
            1
        );
        let job_id: String = sqlx::query_scalar(
            "SELECT id FROM session_memory_jobs WHERE tenant_id = 'default' AND session_id = 'durable-session'",
        )
        .fetch_one(database.pool())
        .await
        .expect("load durable job");
        let first = claim_session_memory_job_with_lease_sqlx(
            database.pool(),
            "default",
            &job_id,
            now,
            false,
            "owner-a",
            Duration::seconds(30),
        )
        .await
        .expect("claim first owner")
        .expect("first owner claim");
        assert_eq!(first.ownership_token.as_deref(), Some("owner-a"));
        assert_eq!(
            heartbeat_session_memory_job_sqlx(
                database.pool(),
                "default",
                &job_id,
                "owner-old",
                "2026-08-31T00:00:10Z",
                Duration::seconds(30),
            )
            .await
            .expect("reject stale heartbeat"),
            false
        );
        assert_eq!(
            recover_expired_session_memory_leases_sqlx(
                database.pool(),
                "default",
                "2026-08-31T00:00:31Z",
            )
            .await
            .expect("recover expired lease"),
            1
        );
        let second = claim_session_memory_job_with_lease_sqlx(
            database.pool(),
            "default",
            &job_id,
            "2026-08-31T00:00:31Z",
            false,
            "owner-b",
            Duration::seconds(30),
        )
        .await
        .expect("claim recovered owner")
        .expect("recovered owner claim");
        assert_eq!(second.ownership_token.as_deref(), Some("owner-b"));
        assert_eq!(
            mark_session_memory_job_failed_with_lease_sqlx(
                database.pool(),
                "default",
                &job_id,
                "owner-a",
                "phase1_failed",
                "2026-08-31T00:00:32Z",
            )
            .await
            .expect("reject stale failure"),
            false
        );
        assert!(mark_session_memory_job_failed_with_lease_sqlx(
            database.pool(),
            "default",
            &job_id,
            "owner-b",
            "phase1_failed",
            "2026-08-31T00:00:32Z",
        )
        .await
        .expect("record retryable failure"));
        assert!(list_due_session_memory_job_ids_sqlx(
            database.pool(),
            "default",
            "2026-08-31T00:00:33Z",
            10,
        )
        .await
        .expect("list before retry")
        .is_empty());
        assert_eq!(
            list_due_session_memory_job_ids_sqlx(
                database.pool(),
                "default",
                "2026-08-31T00:00:47Z",
                10,
            )
            .await
            .expect("list after retry backoff"),
            vec![job_id.clone()]
        );
        let third = claim_session_memory_job_with_lease_sqlx(
            database.pool(),
            "default",
            &job_id,
            "2026-08-31T00:00:47Z",
            false,
            "owner-c",
            Duration::seconds(30),
        )
        .await
        .expect("claim retry owner")
        .expect("retry owner claim");
        assert_eq!(third.retry_count, 2);
        assert!(cancel_session_memory_job_sqlx(
            database.pool(),
            "default",
            &job_id,
            "2026-08-31T00:00:34Z",
        )
        .await
        .expect("cancel retry job"));
        assert!(!cancel_session_memory_job_sqlx(
            database.pool(),
            "default",
            &job_id,
            "2026-08-31T00:00:35Z",
        )
        .await
        .expect("cancel retry job idempotently"));
        let status: String = sqlx::query_scalar(
            "SELECT status FROM session_memory_jobs WHERE tenant_id = 'default' AND id = ?1",
        )
        .bind(&job_id)
        .fetch_one(database.pool())
        .await
        .expect("read cancelled job");
        assert_eq!(status, "canceled");
        assert_eq!(
            recover_expired_session_memory_leases_sqlx(
                database.pool(),
                "default",
                "2026-08-31T00:48:00Z",
            )
            .await
            .expect("do not recover cancelled job"),
            0
        );
        drop(database);
        let _ = std::fs::remove_file(&path);
    }
}
