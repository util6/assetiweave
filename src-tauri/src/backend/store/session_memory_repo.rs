use crate::backend::models::{
    RecentMemoryEvent, RecentMemoryEventCategory, SessionMemory, SessionMemoryJob,
    SessionMemoryJobStatus, SessionMemorySourceReference, SessionMemoryStatus,
};
use crate::backend::runtime::{AppError, AppResult};
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool, Transaction};
use std::collections::BTreeMap;

pub(crate) const SESSION_MEMORY_CONTRACT_VERSION: &str = "session-memory.v1";
pub(crate) const SESSION_MEMORY_PROMPT_VERSION: &str = "session-memory-prompt.v1";
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
    now: &str,
) -> AppResult<usize> {
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
    let candidates = load_session_candidates_sqlx(pool, tenant_id, source_id, &session_ids).await?;
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
    now: &str,
) -> AppResult<usize> {
    let source_revision = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT source_revision FROM conversation_search_index_state WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?
    .flatten()
    .unwrap_or(0);
    let sources = sqlx::query(
        "SELECT id, source_id, source_fingerprint, updated_at FROM conversation_sessions WHERE tenant_id = ?1 AND missing = 0 ORDER BY id ASC",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)?;
    let mut inserted = 0usize;
    for row in sources {
        let candidate = candidate_from_row(&row)?;
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

async fn load_session_candidates_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
    session_ids: &[String],
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
    query.push(" AND missing = 0 AND id IN (");
    {
        let mut separated = query.separated(", ");
        for id in session_ids {
            separated.push_bind(id);
        }
    }
    query.push(") ORDER BY id ASC");
    let rows = query.build().fetch_all(pool).await.map_err(AppError::Db)?;
    rows.iter()
        .map(|row| {
            let mut candidate = candidate_from_row(row)?;
            candidate.source_id = source_id.to_string();
            Ok(candidate)
        })
        .collect()
}

fn candidate_from_row(row: &sqlx::sqlite::SqliteRow) -> AppResult<SessionMemorySourceCandidate> {
    Ok(SessionMemorySourceCandidate {
        id: row.try_get(0).map_err(AppError::external)?,
        source_id: row.try_get(1).map_err(AppError::external)?,
        source_fingerprint: row.try_get(2).map_err(AppError::external)?,
        updated_at: row.try_get(3).map_err(AppError::external)?,
    })
}

fn candidate_with_not_before(
    candidate: &SessionMemorySourceCandidate,
    source_revision: i64,
    now: &str,
) -> SessionMemoryJobCandidate {
    let not_before = candidate
        .updated_at
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| (value.with_timezone(&Utc) + SESSION_IDLE_DELAY).to_rfc3339())
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
    let row = sqlx::query(
        "SELECT tenant_id, id, session_id, source_id, source_revision, source_fingerprint, contract_version, prompt_version, source_event_id, source_sync_run_id, status, not_before, attempt_count, last_error, started_at, finished_at, created_at, updated_at FROM session_memory_jobs WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;
    row.as_ref().map(map_job).transpose()
}

pub(crate) async fn claim_session_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    now: &str,
    ready_override: bool,
) -> AppResult<Option<SessionMemoryJob>> {
    let result = sqlx::query(
        r#"
        UPDATE session_memory_jobs
        SET status = 'running', attempt_count = attempt_count + 1,
            started_at = ?1, updated_at = ?1
        WHERE tenant_id = ?2 AND id = ?3 AND status = 'queued'
          AND (?4 = 1 OR not_before <= ?1)
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
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    load_session_memory_job_sqlx(pool, tenant_id, job_id).await
}

pub(crate) async fn mark_session_memory_job_failed_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    error_code: &str,
    now: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE session_memory_jobs SET status = 'failed', last_error = ?1, finished_at = ?2, updated_at = ?2 WHERE tenant_id = ?3 AND id = ?4 AND status = 'running'",
    )
    .bind(error_code)
    .bind(now)
    .bind(tenant_id)
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}

pub(crate) async fn persist_session_memory_sqlx(
    pool: &SqlitePool,
    input: &SessionMemoryPersistInput,
) -> AppResult<()> {
    let mut tx = pool.begin().await.map_err(AppError::Db)?;
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
    sqlx::query(
        "UPDATE session_memory_jobs SET status = 'succeeded', finished_at = ?1, updated_at = ?1, last_error = NULL WHERE tenant_id = ?2 AND session_id = ?3 AND source_revision = ?4 AND source_fingerprint = ?5 AND contract_version = ?6 AND prompt_version = ?7 AND status = 'running'",
    )
    .bind(&input.generated_at)
    .bind(&input.tenant_id)
    .bind(&input.session_id)
    .bind(input.source_revision)
    .bind(&input.source_fingerprint)
    .bind(&input.contract_version)
    .bind(&input.prompt_version)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Db)?;
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
    let row = sqlx::query(
        "SELECT tenant_id, id, session_id, source_id, source_revision, source_fingerprint, contract_version, prompt_version, status, project_path, summary, goal, result, decisions_json, verification_json, blockers_json, follow_up_json, topics_json, generated_at, created_at, updated_at FROM session_memories WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(memory_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;
    let Some(row) = row else { return Ok(None) };
    Ok(Some(map_memory(&row)?))
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
    let rows = sqlx::query(
        "SELECT tenant_id, id, memory_id, source_id, session_id, question_id, turn_id, part_id, node_id, node_order, reference_key, source_revision, created_at FROM session_memory_source_references WHERE tenant_id = ?1 AND memory_id = ?2 ORDER BY reference_key ASC",
    )
    .bind(tenant_id)
    .bind(memory_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)?;
    rows.iter().map(map_reference).collect()
}

pub(crate) async fn list_recent_memory_events_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
) -> AppResult<Vec<RecentMemoryEvent>> {
    let rows = sqlx::query(
        "SELECT e.tenant_id, e.id, e.memory_id, e.session_id, e.category, e.title, e.summary, e.occurred_at, e.source_reference_id, e.fingerprint, e.created_at FROM recent_memory_events e JOIN session_memories m ON m.tenant_id = e.tenant_id AND m.id = e.memory_id WHERE e.tenant_id = ?1 AND e.session_id = ?2 AND m.status = 'active' ORDER BY e.occurred_at DESC, e.id ASC",
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)?;
    rows.iter().map(map_event).collect()
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
    query.push(") ORDER BY e.session_id ASC, e.occurred_at DESC, e.id ASC");
    let rows = query.build().fetch_all(pool).await.map_err(AppError::Db)?;
    let mut events_by_session = BTreeMap::new();
    for row in &rows {
        let event = map_event(row)?;
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

fn map_job(row: &sqlx::sqlite::SqliteRow) -> AppResult<SessionMemoryJob> {
    Ok(SessionMemoryJob {
        tenant_id: row.try_get(0).map_err(AppError::external)?,
        id: row.try_get(1).map_err(AppError::external)?,
        session_id: row.try_get(2).map_err(AppError::external)?,
        source_id: row.try_get(3).map_err(AppError::external)?,
        source_revision: row.try_get(4).map_err(AppError::external)?,
        source_fingerprint: row.try_get(5).map_err(AppError::external)?,
        contract_version: row.try_get(6).map_err(AppError::external)?,
        prompt_version: row.try_get(7).map_err(AppError::external)?,
        source_event_id: row.try_get(8).map_err(AppError::external)?,
        source_sync_run_id: row.try_get(9).map_err(AppError::external)?,
        status: parse_job_status(&row.try_get::<String, _>(10).map_err(AppError::external)?)?,
        not_before: row.try_get(11).map_err(AppError::external)?,
        attempt_count: row.try_get(12).map_err(AppError::external)?,
        last_error: row.try_get(13).map_err(AppError::external)?,
        started_at: row.try_get(14).map_err(AppError::external)?,
        finished_at: row.try_get(15).map_err(AppError::external)?,
        created_at: row.try_get(16).map_err(AppError::external)?,
        updated_at: row.try_get(17).map_err(AppError::external)?,
    })
}

fn map_memory(row: &sqlx::sqlite::SqliteRow) -> AppResult<SessionMemory> {
    Ok(SessionMemory {
        tenant_id: row.try_get(0).map_err(AppError::external)?,
        id: row.try_get(1).map_err(AppError::external)?,
        session_id: row.try_get(2).map_err(AppError::external)?,
        source_id: row.try_get(3).map_err(AppError::external)?,
        source_revision: row.try_get(4).map_err(AppError::external)?,
        source_fingerprint: row.try_get(5).map_err(AppError::external)?,
        contract_version: row.try_get(6).map_err(AppError::external)?,
        prompt_version: row.try_get(7).map_err(AppError::external)?,
        status: parse_memory_status(&row.try_get::<String, _>(8).map_err(AppError::external)?)?,
        project_path: row.try_get(9).map_err(AppError::external)?,
        summary: row.try_get(10).map_err(AppError::external)?,
        goal: row.try_get(11).map_err(AppError::external)?,
        result: row.try_get(12).map_err(AppError::external)?,
        decisions: decode_string_array(&row.try_get::<String, _>(13).map_err(AppError::external)?)?,
        verification: decode_string_array(
            &row.try_get::<String, _>(14).map_err(AppError::external)?,
        )?,
        blockers: decode_string_array(&row.try_get::<String, _>(15).map_err(AppError::external)?)?,
        follow_up: decode_string_array(&row.try_get::<String, _>(16).map_err(AppError::external)?)?,
        topics: decode_string_array(&row.try_get::<String, _>(17).map_err(AppError::external)?)?,
        generated_at: row.try_get(18).map_err(AppError::external)?,
        created_at: row.try_get(19).map_err(AppError::external)?,
        updated_at: row.try_get(20).map_err(AppError::external)?,
    })
}

fn map_reference(row: &sqlx::sqlite::SqliteRow) -> AppResult<SessionMemorySourceReference> {
    Ok(SessionMemorySourceReference {
        tenant_id: row.try_get(0).map_err(AppError::external)?,
        id: row.try_get(1).map_err(AppError::external)?,
        memory_id: row.try_get(2).map_err(AppError::external)?,
        source_id: row.try_get(3).map_err(AppError::external)?,
        session_id: row.try_get(4).map_err(AppError::external)?,
        question_id: row.try_get(5).map_err(AppError::external)?,
        turn_id: row.try_get(6).map_err(AppError::external)?,
        part_id: row.try_get(7).map_err(AppError::external)?,
        node_id: row.try_get(8).map_err(AppError::external)?,
        node_order: row
            .try_get::<Option<i64>, _>(9)
            .map_err(AppError::external)?
            .map(|value| value as usize),
        reference_key: row.try_get(10).map_err(AppError::external)?,
        source_revision: row.try_get(11).map_err(AppError::external)?,
        created_at: row.try_get(12).map_err(AppError::external)?,
    })
}

fn map_event(row: &sqlx::sqlite::SqliteRow) -> AppResult<RecentMemoryEvent> {
    Ok(RecentMemoryEvent {
        tenant_id: row.try_get(0).map_err(AppError::external)?,
        id: row.try_get(1).map_err(AppError::external)?,
        memory_id: row.try_get(2).map_err(AppError::external)?,
        session_id: row.try_get(3).map_err(AppError::external)?,
        category: RecentMemoryEventCategory::parse(
            &row.try_get::<String, _>(4).map_err(AppError::external)?,
        )
        .ok_or_else(|| AppError::external("invalid Recent Event category"))?,
        title: row.try_get(5).map_err(AppError::external)?,
        summary: row.try_get(6).map_err(AppError::external)?,
        occurred_at: row.try_get(7).map_err(AppError::external)?,
        source_reference_id: row.try_get(8).map_err(AppError::external)?,
        fingerprint: row.try_get(9).map_err(AppError::external)?,
        created_at: row.try_get(10).map_err(AppError::external)?,
    })
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
}
