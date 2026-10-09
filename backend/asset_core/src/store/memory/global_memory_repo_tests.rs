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
    let left = global_input_set_from_projects(vec![project("b", "/b", 7), project("a", "/a", 3)]);
    let right = global_input_set_from_projects(vec![project("a", "/a", 3), project("b", "/b", 7)]);
    assert_eq!(left.fingerprint, right.fingerprint);
    assert_eq!(left.watermark, 7);
}

#[test]
fn global_ids_are_tenant_scoped() {
    assert_ne!(global_memory_id("tenant-a"), global_memory_id("tenant-b"));
}

#[tokio::test(flavor = "multi_thread")]
async fn global_memory_job_failure_honors_retryable_flag_and_max_retries() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-global-memory-retry-limit-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open fixture");
    let tenant_id = "default";
    let job_id = global_memory_id(tenant_id);

    sqlx::query(
            "INSERT INTO global_memories (tenant_id, id, created_at, updated_at) VALUES (?1, ?2, '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z')",
        )
        .bind(tenant_id)
        .bind(&job_id)
        .execute(database.pool())
        .await
        .expect("insert global_memory");

    sqlx::query(
            "INSERT INTO global_memory_jobs (tenant_id, id, target_watermark, input_fingerprint, status, created_at, updated_at) VALUES (?1, ?2, 1, 'fp-global', 'queued', '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z')",
        )
        .bind(tenant_id)
        .bind(&job_id)
        .execute(database.pool())
        .await
        .expect("insert global job");

    // 1. retryable = false -> retry_at 为 None
    let claimed = claim_global_memory_job_with_lease_sqlx(
        database.pool(),
        tenant_id,
        &job_id,
        "2026-09-01T00:00:00Z",
        "owner-non-retry",
    )
    .await
    .expect("claim")
    .expect("job claimed");

    assert!(mark_global_memory_job_failed_with_lease_sqlx(
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
        "SELECT retry_count, retry_at FROM global_memory_jobs WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(&job_id)
    .fetch_one(database.pool())
    .await
    .expect("load job");

    assert_eq!(retry_count, 1);
    assert!(retry_at.is_none());

    // 2. 连续可重试直到达到 MAX_GLOBAL_MEMORY_JOB_RETRIES (5)
    for i in 1..MAX_GLOBAL_MEMORY_JOB_RETRIES {
        let token = format!("owner-retry-{}", i);
        sqlx::query(
                "UPDATE global_memory_jobs SET status = 'running', ownership_token = ?1 WHERE tenant_id = ?2 AND id = ?3",
            )
            .bind(&token)
            .bind(tenant_id)
            .bind(&job_id)
            .execute(database.pool())
            .await
            .expect("set running");

        assert!(mark_global_memory_job_failed_with_lease_sqlx(
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
            "SELECT retry_count, retry_at FROM global_memory_jobs WHERE tenant_id = ?1 AND id = ?2",
        )
        .bind(tenant_id)
        .bind(&job_id)
        .fetch_one(database.pool())
        .await
        .expect("load job");

        assert_eq!(rc, i + 1);
        if i + 1 < MAX_GLOBAL_MEMORY_JOB_RETRIES {
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
