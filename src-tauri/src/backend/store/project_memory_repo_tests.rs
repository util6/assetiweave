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
        ..Default::default()
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

#[tokio::test(flavor = "multi_thread")]
async fn project_memory_job_failure_honors_retryable_flag_and_max_retries() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-project-memory-retry-limit-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open fixture");
    let tenant_id = "default";
    let project_path = "/workspace/project-retry";
    let project_id = project_memory_id(tenant_id, project_path);
    let job_id = project_memory_job_id(tenant_id, project_path);

    // 初始化 project_memories 锚点
    sqlx::query(
            "INSERT INTO project_memories (tenant_id, id, project_path, created_at, updated_at) VALUES (?1, ?2, ?3, '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z')",
        )
        .bind(tenant_id)
        .bind(&project_id)
        .bind(project_path)
        .execute(database.pool())
        .await
        .expect("insert project_memory");

    sqlx::query(
            "INSERT INTO project_memory_jobs (tenant_id, id, project_id, project_path, target_watermark, input_fingerprint, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 1, 'fp-1', 'queued', '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z')",
        )
        .bind(tenant_id)
        .bind(&job_id)
        .bind(&project_id)
        .bind(project_path)
        .execute(database.pool())
        .await
        .expect("insert job");

    // 1. retryable = false -> retry_at 为 None
    let claimed = claim_project_memory_job_with_lease_sqlx(
        database.pool(),
        tenant_id,
        &job_id,
        "2026-09-01T00:00:00Z",
        "owner-non-retry",
    )
    .await
    .expect("claim")
    .expect("job claimed");

    assert!(mark_project_memory_job_failed_with_lease_sqlx(
        database.pool(),
        tenant_id,
        &job_id,
        &claimed.ownership_token.unwrap(),
        "agent_not_found",
        "2026-09-01T00:00:01Z",
        false,
    )
    .await
    .expect("mark failed"));

    let (retry_count, retry_at): (i64, Option<String>) = sqlx::query_as(
        "SELECT retry_count, retry_at FROM project_memory_jobs WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(&job_id)
    .fetch_one(database.pool())
    .await
    .expect("load job");

    assert_eq!(retry_count, 1);
    assert!(retry_at.is_none());

    // 2. 连续可重试直到达到 MAX_PROJECT_MEMORY_JOB_RETRIES (5)
    for i in 1..MAX_PROJECT_MEMORY_JOB_RETRIES {
        let token = format!("owner-retry-{}", i);
        sqlx::query(
                "UPDATE project_memory_jobs SET status = 'running', ownership_token = ?1 WHERE tenant_id = ?2 AND id = ?3",
            )
            .bind(&token)
            .bind(tenant_id)
            .bind(&job_id)
            .execute(database.pool())
            .await
            .expect("set running");

        assert!(mark_project_memory_job_failed_with_lease_sqlx(
            database.pool(),
            tenant_id,
            &job_id,
            &token,
            "transient_error",
            "2026-09-01T00:00:00Z",
            true,
        )
        .await
        .expect("mark retryable failed"));

        let (rc, ra): (i64, Option<String>) = sqlx::query_as(
                "SELECT retry_count, retry_at FROM project_memory_jobs WHERE tenant_id = ?1 AND id = ?2",
            )
            .bind(tenant_id)
            .bind(&job_id)
            .fetch_one(database.pool())
            .await
            .expect("load job");

        assert_eq!(rc, i + 1);
        if i + 1 < MAX_PROJECT_MEMORY_JOB_RETRIES {
            assert!(ra.is_some(), "retry_at should be set for attempt {}", i + 1);
        } else {
            assert!(
                ra.is_none(),
                "retry_at must be None once max retries is reached"
            );
        }
    }

    drop(database);
    let _ = std::fs::remove_file(&path);
}
