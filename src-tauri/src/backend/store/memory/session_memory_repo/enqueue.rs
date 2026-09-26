use super::*;

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
) -> StoreResult<usize> {
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
        .map_err(StoreError::Db)?,
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
) -> StoreResult<usize> {
    ensure_recent_session_memory_jobs_sqlx(pool, tenant_id, excluded_project_root, now, false).await
}

/// Re-establishes the Phase-1 prerequisites for an explicit Recent Memory
/// rebuild. Unlike automatic backfill, this may reopen a matching terminal
/// failure/cancellation and starts a fresh, still-bounded retry budget.
pub(crate) async fn rebuild_recent_session_memory_jobs_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    excluded_project_root: &str,
    now: &str,
) -> StoreResult<usize> {
    ensure_recent_session_memory_jobs_sqlx(pool, tenant_id, excluded_project_root, now, true).await
}

/// Ensures Phase-1 work for the exact candidate set frozen by the Recent
/// watermark window. This deliberately accepts session identities instead of
/// deriving another rolling `now - 48h` window, which can differ from the
/// selected watermark by as much as one scheduling interval.
pub(crate) async fn ensure_session_memory_jobs_for_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_ids: &[String],
    excluded_project_root: &str,
    now: &str,
    restart_terminal: bool,
) -> StoreResult<usize> {
    if session_ids.is_empty() {
        return Ok(0);
    }
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
    .map_err(StoreError::Db)?
    .flatten()
    .unwrap_or(0);
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT id, source_id, source_fingerprint, updated_at FROM conversation_sessions WHERE tenant_id = ",
    );
    query.push_bind(tenant_id);
    query.push(" AND missing = 0 AND execution_origin = 'user' AND user_visible = 1 AND (");
    query.push_bind(excluded_project_root);
    query.push(" = '' OR project_path IS NULL OR (project_path <> ");
    query.push_bind(excluded_project_root);
    query.push(" AND instr(project_path, ");
    query.push_bind(excluded_project_root);
    query.push(" || '/') <> 1)) AND id IN (");
    {
        let mut separated = query.separated(", ");
        for session_id in session_ids {
            separated.push_bind(session_id);
        }
    }
    query.push(") ORDER BY id ASC");
    let candidates = query
        .build_query_as::<SessionMemorySourceCandidateRow>()
        .fetch_all(pool)
        .await
        .map_err(StoreError::Db)?;

    let mut prepared = 0usize;
    for row in candidates {
        let candidate: SessionMemorySourceCandidate = row.into();
        if policy.excluded_session_ids.contains(&candidate.id)
            || policy.excluded_source_ids.contains(&candidate.source_id)
        {
            continue;
        }
        let job_candidate = candidate_with_not_before(&candidate, source_revision, now);
        let inserted = insert_job_candidate_sqlx(
            pool,
            tenant_id,
            &job_candidate,
            "recent-watermark:session-memory",
            "recent-watermark:session-memory",
            now,
        )
        .await?;
        prepared += inserted;
        if restart_terminal && inserted == 0 {
            let job_id = sqlx::query_scalar::<_, String>(
                "SELECT id FROM session_memory_jobs WHERE tenant_id = ?1 AND session_id = ?2 AND source_revision = ?3 AND source_fingerprint = ?4 AND contract_version = ?5 AND prompt_version = ?6 LIMIT 1",
            )
            .bind(tenant_id)
            .bind(&job_candidate.session_id)
            .bind(job_candidate.source_revision)
            .bind(&job_candidate.source_fingerprint)
            .bind(SESSION_MEMORY_CONTRACT_VERSION)
            .bind(SESSION_MEMORY_PROMPT_VERSION)
            .fetch_optional(pool)
            .await
            .map_err(StoreError::Db)?;
            if let Some(job_id) = job_id {
                prepared += usize::from(
                    restart_session_memory_job_sqlx(pool, tenant_id, &job_id, now).await?,
                );
            }
        }
    }
    Ok(prepared)
}

pub(crate) async fn ensure_recent_session_memory_jobs_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    excluded_project_root: &str,
    now: &str,
    restart_terminal: bool,
) -> StoreResult<usize> {
    let policy = load_memory_generation_policy_sqlx(pool).await?;
    if !policy.enabled {
        return Ok(0);
    }
    let now_dt = DateTime::parse_from_rfc3339(now)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let cutoff = (now_dt - Duration::hours(48)).to_rfc3339();

    let source_revision = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT source_revision FROM conversation_search_index_state WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::Db)?
    .flatten()
    .unwrap_or(0);
    let sources = sqlx::query_as::<_, SessionMemorySourceCandidateRow>(
        r#"
        SELECT id, source_id, source_fingerprint, updated_at
        FROM conversation_sessions
        WHERE tenant_id = ?1
          AND missing = 0
          AND execution_origin = 'user'
          AND (?2 = '' OR project_path IS NULL OR (project_path <> ?2 AND instr(project_path, ?2 || '/') <> 1))
          AND updated_at >= ?3
        ORDER BY updated_at DESC
        LIMIT 50
        "#,
    )
    .bind(tenant_id)
    .bind(excluded_project_root)
    .bind(&cutoff)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Db)?;
    let mut prepared = 0usize;
    for row in sources {
        let candidate: SessionMemorySourceCandidate = row.into();
        if policy.excluded_session_ids.contains(&candidate.id)
            || policy.excluded_source_ids.contains(&candidate.source_id)
        {
            continue;
        }
        let job_candidate = candidate_with_not_before(&candidate, source_revision, now);
        let inserted = insert_job_candidate_sqlx(
            pool,
            tenant_id,
            &job_candidate,
            "backfill:session-memory",
            "backfill:session-memory",
            now,
        )
        .await?;
        prepared += inserted;
        if restart_terminal && inserted == 0 {
            let job_id = sqlx::query_scalar::<_, String>(
                "SELECT id FROM session_memory_jobs WHERE tenant_id = ?1 AND session_id = ?2 AND source_revision = ?3 AND source_fingerprint = ?4 AND contract_version = ?5 AND prompt_version = ?6 LIMIT 1",
            )
            .bind(tenant_id)
            .bind(&job_candidate.session_id)
            .bind(job_candidate.source_revision)
            .bind(&job_candidate.source_fingerprint)
            .bind(SESSION_MEMORY_CONTRACT_VERSION)
            .bind(SESSION_MEMORY_PROMPT_VERSION)
            .fetch_optional(pool)
            .await
            .map_err(StoreError::Db)?;
            if let Some(job_id) = job_id {
                prepared += usize::from(
                    restart_session_memory_job_sqlx(pool, tenant_id, &job_id, now).await?,
                );
            }
        }
    }
    Ok(prepared)
}

pub(crate) async fn load_memory_generation_policy_sqlx(
    pool: &SqlitePool,
) -> StoreResult<MemoryGenerationPolicy> {
    let settings = crate::backend::store::system::settings_repo::load_app_settings_sqlx(pool)
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
