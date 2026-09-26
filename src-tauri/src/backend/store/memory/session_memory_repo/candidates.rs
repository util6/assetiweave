use super::*;

pub(crate) async fn load_session_candidates_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
    session_ids: &[String],
    excluded_project_root: &str,
) -> StoreResult<Vec<SessionMemorySourceCandidate>> {
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT id, source_id, source_fingerprint, updated_at FROM conversation_sessions WHERE tenant_id = ",
    );
    query.push_bind(tenant_id);
    query.push(" AND source_id = ");
    query.push_bind(source_id);
    query.push(" AND missing = 0 AND execution_origin = 'user' AND (");
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
        .map_err(StoreError::Db)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let mut candidate: SessionMemorySourceCandidate = row.into();
            candidate.source_id = source_id.to_string();
            candidate
        })
        .collect())
}

pub(crate) fn candidate_with_not_before(
    candidate: &SessionMemorySourceCandidate,
    source_revision: i64,
    now: &str,
) -> SessionMemoryJobCandidate {
    let not_before = candidate
        .updated_at
        .as_deref()
        .and_then(crate::backend::domain::parse_conversation_timestamp)
        .map(|value| (value + SESSION_IDLE_DELAY).to_rfc3339())
        .unwrap_or_else(|| now.to_string());
    let default_recipe = crate::backend::domain::MemoryRecipe::default_builtin();
    let recipe_snapshot = crate::backend::domain::MemoryRecipeSnapshot::from(&default_recipe);
    let budget_policy = crate::backend::domain::BoundedMemoryBudgetPolicy::default();
    let source_fingerprint = candidate
        .source_fingerprint
        .clone()
        .unwrap_or_else(|| fallback_fingerprint(&candidate.id, source_revision));
    let work_order = crate::backend::domain::MemoryExecutionWorkOrder::new(
        format!("wo-{}", uuid::Uuid::new_v4()),
        candidate.id.clone(),
        candidate.source_id.clone(),
        source_revision,
        source_fingerprint.clone(),
        &default_recipe,
        budget_policy,
        now.to_string(),
    );
    let work_order_json = serde_json::to_string(&work_order).ok();

    SessionMemoryJobCandidate {
        session_id: candidate.id.clone(),
        source_id: candidate.source_id.clone(),
        source_revision,
        source_fingerprint,
        not_before,
        recipe_id: Some(recipe_snapshot.recipe_id),
        recipe_revision: Some(recipe_snapshot.revision),
        recipe_content_hash: Some(recipe_snapshot.content_hash),
        budget_policy_version: Some(
            crate::backend::domain::DEFAULT_BUDGET_POLICY_VERSION.to_string(),
        ),
        work_order_json,
    }
}

pub(crate) fn fallback_fingerprint(session_id: &str, source_revision: i64) -> String {
    digest(&format!("{session_id}\0{source_revision}"))
}

pub(crate) fn digest(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) async fn insert_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_candidate: &SessionMemorySourceCandidate,
    source_revision: i64,
    source_event_id: &str,
    sync_run_id: &str,
    now: &str,
) -> StoreResult<usize> {
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

pub(crate) async fn insert_job_candidate_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    candidate: &SessionMemoryJobCandidate,
    source_event_id: &str,
    sync_run_id: &str,
    now: &str,
) -> StoreResult<usize> {
    let recipe_hash = candidate
        .recipe_content_hash
        .as_deref()
        .unwrap_or("default");
    let budget_version = candidate
        .budget_policy_version
        .as_deref()
        .unwrap_or(crate::backend::domain::DEFAULT_BUDGET_POLICY_VERSION);

    let has_reusable_projection = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM session_memories WHERE tenant_id = ?1 AND session_id = ?2 AND source_id = ?3 AND status = 'active' AND source_fingerprint = ?4 AND contract_version = ?5 AND prompt_version = ?6 AND COALESCE(recipe_content_hash, '') = COALESCE(?7, ''))",
    )
    .bind(tenant_id)
    .bind(&candidate.session_id)
    .bind(&candidate.source_id)
    .bind(&candidate.source_fingerprint)
    .bind(SESSION_MEMORY_CONTRACT_VERSION)
    .bind(SESSION_MEMORY_PROMPT_VERSION)
    .bind(&candidate.recipe_content_hash)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Db)?;
    if has_reusable_projection {
        return Ok(0);
    }

    sqlx::query(
        "UPDATE session_memories SET status = 'invalid', updated_at = ?1 WHERE tenant_id = ?2 AND session_id = ?3 AND status = 'active' AND (source_revision < ?4 OR source_fingerprint <> ?5 OR contract_version <> ?6 OR prompt_version <> ?7 OR (recipe_content_hash IS NOT NULL AND recipe_content_hash <> ?8))",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(&candidate.session_id)
    .bind(candidate.source_revision)
    .bind(&candidate.source_fingerprint)
    .bind(SESSION_MEMORY_CONTRACT_VERSION)
    .bind(SESSION_MEMORY_PROMPT_VERSION)
    .bind(recipe_hash)
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;
    sqlx::query(
        "UPDATE session_memory_jobs SET status = 'skipped', last_error = 'superseded_by_newer_watermark', finished_at = ?1, updated_at = ?1, retry_at = NULL, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL WHERE tenant_id = ?2 AND session_id = ?3 AND source_revision < ?4 AND status IN ('queued', 'failed')",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(&candidate.session_id)
    .bind(candidate.source_revision)
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;
    let id = format!(
        "session-memory-job-{}",
        digest(&format!(
            "{tenant_id}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
            candidate.session_id,
            candidate.source_id,
            candidate.source_revision,
            candidate.source_fingerprint,
            SESSION_MEMORY_CONTRACT_VERSION,
            SESSION_MEMORY_PROMPT_VERSION,
            recipe_hash,
            budget_version,
        ))
    );
    let result = sqlx::query(
        r#"
        INSERT OR IGNORE INTO session_memory_jobs (
            tenant_id, id, session_id, source_id, source_revision,
            source_fingerprint, contract_version, prompt_version,
            source_event_id, source_sync_run_id, status, not_before,
            attempt_count, created_at, updated_at,
            recipe_id, recipe_revision, recipe_content_hash,
            budget_policy_version, work_order_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'queued', ?11, 0, ?12, ?12, ?13, ?14, ?15, ?16, ?17)
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
    .bind(&candidate.recipe_id)
    .bind(candidate.recipe_revision.unwrap_or(1))
    .bind(&candidate.recipe_content_hash)
    .bind(&candidate.budget_policy_version)
    .bind(&candidate.work_order_json)
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(result.rows_affected() as usize)
}
