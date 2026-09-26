use super::*;

pub(crate) async fn load_session_memory_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    memory_id: &str,
) -> StoreResult<Option<SessionMemory>> {
    let row = sqlx::query_as::<_, SessionMemoryRow>(
        "SELECT tenant_id, id, session_id, source_id, source_revision, source_fingerprint, contract_version, prompt_version, status, project_path, summary, goal, result, decisions_json, verification_json, blockers_json, follow_up_json, topics_json, generated_at, created_at, updated_at, recipe_id, recipe_content_hash, work_order_json FROM session_memories WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(memory_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::Db)?;
    row.map(|r| r.try_into_memory()).transpose()
}

pub(crate) async fn list_session_memories_for_project_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    project_path: &str,
) -> StoreResult<Vec<SessionMemory>> {
    let rows = sqlx::query_as::<_, SessionMemoryRow>(
        "SELECT m.tenant_id, m.id, m.session_id, m.source_id, m.source_revision, m.source_fingerprint, m.contract_version, m.prompt_version, m.status, m.project_path, m.summary, m.goal, m.result, m.decisions_json, m.verification_json, m.blockers_json, m.follow_up_json, m.topics_json, m.generated_at, m.created_at, m.updated_at, m.recipe_id, m.recipe_content_hash, m.work_order_json FROM session_memories m WHERE m.tenant_id = ?1 AND m.project_path = ?2 AND m.status = 'active' AND NOT EXISTS (SELECT 1 FROM session_memories newer WHERE newer.tenant_id = m.tenant_id AND newer.session_id = m.session_id AND newer.status = 'active' AND (newer.source_revision > m.source_revision OR (newer.source_revision = m.source_revision AND newer.id > m.id))) AND (NOT EXISTS (SELECT 1 FROM conversation_sessions c WHERE c.tenant_id = m.tenant_id AND c.id = m.session_id) OR EXISTS (SELECT 1 FROM conversation_sessions c WHERE c.tenant_id = m.tenant_id AND c.id = m.session_id AND c.source_id = m.source_id AND c.missing = 0 AND EXISTS (SELECT 1 FROM conversation_sources source WHERE source.tenant_id = c.tenant_id AND source.id = c.source_id AND source.enabled = 1) AND (c.source_fingerprint IS NULL OR c.source_fingerprint = m.source_fingerprint))) ORDER BY m.id ASC",
    )
    .bind(tenant_id)
    .bind(project_path)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Db)?;
    rows.into_iter().map(|r| r.try_into_memory()).collect()
}

/// Load the latest active Phase-1 memory for each requested Session. The
/// result is used to freeze Recent Snapshot evidence; the worker must not
/// substitute a later SQLite read into an already queued Work Order.
pub(crate) async fn list_active_session_memories_for_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_ids: &[String],
) -> StoreResult<BTreeMap<String, SessionMemory>> {
    if session_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT m.tenant_id, m.id, m.session_id, m.source_id, m.source_revision, m.source_fingerprint, m.contract_version, m.prompt_version, m.status, m.project_path, m.summary, m.goal, m.result, m.decisions_json, m.verification_json, m.blockers_json, m.follow_up_json, m.topics_json, m.generated_at, m.created_at, m.updated_at, m.recipe_id, m.recipe_content_hash, m.work_order_json FROM session_memories m WHERE m.tenant_id = ",
    );
    query.push_bind(tenant_id);
    query.push(" AND m.status = 'active' AND m.session_id IN (");
    {
        let mut separated = query.separated(", ");
        for session_id in session_ids {
            separated.push_bind(session_id);
        }
    }
    query.push(") AND NOT EXISTS (SELECT 1 FROM session_memories newer WHERE newer.tenant_id = m.tenant_id AND newer.session_id = m.session_id AND newer.status = 'active' AND (newer.source_revision > m.source_revision OR (newer.source_revision = m.source_revision AND newer.id > m.id))) ORDER BY m.session_id ASC, m.source_revision DESC, m.id DESC");

    let rows = query
        .build_query_as::<SessionMemoryRow>()
        .fetch_all(pool)
        .await
        .map_err(StoreError::Db)?;
    let mut memories = BTreeMap::new();
    for row in rows {
        let memory = row.try_into_memory()?;
        memories.insert(memory.session_id.clone(), memory);
    }
    Ok(memories)
}

/// Load canonical content locators belonging to the latest active Session
/// Memory. Recent Snapshot reference validation uses these locators instead
/// of inferring user ownership from a free-form reference key.
pub(crate) async fn list_session_memory_source_references_for_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_ids: &[String],
) -> StoreResult<BTreeMap<String, Vec<SessionMemorySourceReference>>> {
    if session_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT r.tenant_id, r.id, r.memory_id, r.source_id, r.session_id, r.question_id, r.turn_id, r.part_id, r.node_id, r.node_order, r.reference_key, r.source_revision, r.created_at FROM session_memory_source_references r JOIN session_memories m ON m.tenant_id = r.tenant_id AND m.id = r.memory_id WHERE r.tenant_id = ",
    );
    query.push_bind(tenant_id);
    query.push(" AND m.status = 'active' AND r.session_id IN (");
    {
        let mut separated = query.separated(", ");
        for session_id in session_ids {
            separated.push_bind(session_id);
        }
    }
    query.push(") AND NOT EXISTS (SELECT 1 FROM session_memories newer WHERE newer.tenant_id = m.tenant_id AND newer.session_id = m.session_id AND newer.status = 'active' AND (newer.source_revision > m.source_revision OR (newer.source_revision = m.source_revision AND newer.id > m.id))) ORDER BY r.session_id ASC, CASE WHEN r.question_id IS NOT NULL OR r.turn_id IS NOT NULL OR r.node_id IS NOT NULL THEN 0 ELSE 1 END ASC, r.node_order ASC, r.id ASC");

    let rows = query
        .build_query_as::<SessionMemorySourceReferenceRow>()
        .fetch_all(pool)
        .await
        .map_err(StoreError::Db)?;
    let mut references = BTreeMap::new();
    for row in rows {
        let reference = row.into_reference();
        references
            .entry(reference.session_id.clone())
            .or_insert_with(Vec::new)
            .push(reference);
    }
    Ok(references)
}

pub(crate) async fn load_session_memory_for_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job: &SessionMemoryJob,
) -> StoreResult<Option<SessionMemory>> {
    let memory_id = format!(
        "session-memory-{}",
        digest(&format!(
            "{}\0{}\0{}",
            job.tenant_id, job.id, job.source_revision
        ))
    );
    load_session_memory_sqlx(pool, tenant_id, &memory_id).await
}

pub(crate) async fn load_recent_memory_event_target_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    event_id: &str,
) -> StoreResult<Option<crate::backend::domain::memory::RecentMemoryEventTarget>> {
    let row = sqlx::query_as::<_, RecentMemoryEventTargetRow>(
        "SELECT e.session_id, r.question_id, r.turn_id, r.node_id FROM recent_memory_events e JOIN session_memories m ON m.tenant_id = e.tenant_id AND m.id = e.memory_id LEFT JOIN session_memory_source_references r ON r.tenant_id = e.tenant_id AND r.id = e.source_reference_id WHERE e.tenant_id = ?1 AND e.id = ?2 AND m.status = 'active' AND NOT EXISTS (SELECT 1 FROM session_memories newer WHERE newer.tenant_id = m.tenant_id AND newer.session_id = m.session_id AND newer.status = 'active' AND (newer.source_revision > m.source_revision OR (newer.source_revision = m.source_revision AND newer.id > m.id))) AND EXISTS (SELECT 1 FROM conversation_sessions c JOIN conversation_sources source ON source.tenant_id = c.tenant_id AND source.id = c.source_id AND source.enabled = 1 WHERE c.tenant_id = e.tenant_id AND c.id = e.session_id AND c.missing = 0)",
    )
    .bind(tenant_id)
    .bind(event_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(row.map(Into::into))
}

pub(crate) async fn list_recent_memory_events_for_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_ids: &[String],
    cutoff: &str,
    now: &str,
) -> StoreResult<BTreeMap<String, Vec<RecentMemoryEvent>>> {
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
        .map_err(StoreError::Db)?;
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

#[cfg(test)]
pub(crate) async fn count_session_memory_rows_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    table: &str,
) -> StoreResult<i64> {
    let query = match table {
        "jobs" => "SELECT COUNT(*) FROM session_memory_jobs WHERE tenant_id = ?1",
        "memories" => "SELECT COUNT(*) FROM session_memories WHERE tenant_id = ?1",
        "events" => "SELECT COUNT(*) FROM recent_memory_events WHERE tenant_id = ?1",
        "references" => {
            "SELECT COUNT(*) FROM session_memory_source_references WHERE tenant_id = ?1"
        }
        _ => {
            return Err(StoreError::Validation(
                "unknown Session Memory row kind".to_string(),
            ))
        }
    };
    sqlx::query_scalar(query)
        .bind(tenant_id)
        .fetch_one(pool)
        .await
        .map_err(StoreError::Db)
}
