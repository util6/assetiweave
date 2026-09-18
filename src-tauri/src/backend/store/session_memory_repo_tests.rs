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
    let old = SessionMemoryJobCandidate {
        session_id: "session-invalidation".to_string(),
        source_id: "source-invalidation".to_string(),
        source_revision: 1,
        source_fingerprint: "fingerprint-old".to_string(),
        not_before: "2026-08-31T00:00:00Z".to_string(),
        ..Default::default()
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
    sqlx::query(
            "INSERT INTO session_memories (tenant_id,id,session_id,source_id,source_revision,source_fingerprint,contract_version,prompt_version,status,project_path,summary,goal,result,decisions_json,verification_json,blockers_json,follow_up_json,topics_json,raw_output_json,generated_at,created_at,updated_at) VALUES ('default','memory-old','session-invalidation','source-invalidation',1,'fingerprint-old','session-memory.v1','session-memory-prompt.v1','active','/project','old summary','','','[]','[]','[]','[]','[]','{}','2026-08-31T00:00:00Z','2026-08-31T00:00:00Z','2026-08-31T00:00:00Z')",
        )
        .execute(database.pool())
        .await
        .expect("insert active projection");
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

#[tokio::test(flavor = "multi_thread")]
async fn unchanged_session_fingerprint_reuses_active_projection_across_source_watermarks() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-session-memory-fingerprint-reuse-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open fingerprint reuse fixture");
    sqlx::query(
            "INSERT INTO session_memories (tenant_id,id,session_id,source_id,source_revision,source_fingerprint,contract_version,prompt_version,status,project_path,summary,goal,result,decisions_json,verification_json,blockers_json,follow_up_json,topics_json,raw_output_json,generated_at,created_at,updated_at,recipe_content_hash) VALUES ('default','memory-reusable','session-reusable','source-reusable',1,'fingerprint-stable','session-memory.v1','session-memory-prompt.v1','active','/project','stable summary','','','[]','[]','[]','[]','[]','{}','2026-09-16T00:00:00Z','2026-09-16T00:00:00Z','2026-09-16T00:00:00Z',NULL)",
        )
        .execute(database.pool())
        .await
        .expect("insert reusable projection");
    let candidate = SessionMemoryJobCandidate {
        session_id: "session-reusable".to_string(),
        source_id: "source-reusable".to_string(),
        source_revision: 2,
        source_fingerprint: "fingerprint-stable".to_string(),
        not_before: "2026-09-17T00:00:00Z".to_string(),
        ..Default::default()
    };

    assert_eq!(
        insert_job_candidate_sqlx(
            database.pool(),
            "default",
            &candidate,
            "event-new-watermark",
            "sync-new-watermark",
            "2026-09-17T00:00:00Z",
        )
        .await
        .expect("reuse unchanged projection"),
        0
    );
    let state: String = sqlx::query_scalar(
            "SELECT status FROM session_memories WHERE tenant_id = 'default' AND id = 'memory-reusable'",
        )
        .fetch_one(database.pool())
        .await
        .expect("read reusable projection");
    assert_eq!(state, "active");
    let job_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM session_memory_jobs WHERE tenant_id = 'default' AND session_id = 'session-reusable'",
        )
        .fetch_one(database.pool())
        .await
        .expect("count redundant jobs");
    assert_eq!(job_count, 0);

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
        ..Default::default()
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
            true,
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
        true,
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

#[tokio::test(flavor = "multi_thread")]
async fn session_memory_job_failure_honors_retryable_flag_and_max_retries() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-session-memory-retry-limit-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open fixture");
    sqlx::query(
        r#"
            INSERT INTO conversation_sessions (
                tenant_id, id, source_id, adapter_id, external_id, title,
                project_path, started_at, updated_at, source_locator,
                source_fingerprint, missing, created_at, imported_at
            ) VALUES (
                'default', 's-retry-limit', 'source-1', 'adapter-1',
                'ext-1', 'Title', NULL,
                '2026-08-30T00:00:00Z', '2026-08-30T00:00:00Z',
                'fixture://s-retry-limit', 'fp1', 0,
                '2026-08-30T00:00:00Z', '2026-08-30T00:00:00Z'
            )
            "#,
    )
    .execute(database.pool())
    .await
    .expect("insert session");

    assert_eq!(
        enqueue_session_memory_jobs_sqlx(
            database.pool(),
            "default",
            "source-1",
            "sync-1",
            1,
            "event-1",
            Some(&["s-retry-limit".to_string()]),
            "",
            "2026-09-01T00:00:00Z",
        )
        .await
        .expect("enqueue candidate"),
        1
    );

    let job_id: String = sqlx::query_scalar(
            "SELECT id FROM session_memory_jobs WHERE tenant_id = 'default' AND session_id = 's-retry-limit'",
        )
        .fetch_one(database.pool())
        .await
        .expect("load job id");

    // 1. Non-retryable error -> retry_at is None
    let _claimed = claim_session_memory_job_with_lease_sqlx(
        database.pool(),
        "default",
        &job_id,
        "2026-09-01T00:00:00Z",
        true,
        "owner-non-retry",
        Duration::seconds(30),
    )
    .await
    .expect("claim")
    .expect("job");

    assert!(mark_session_memory_job_failed_with_lease_sqlx(
        database.pool(),
        "default",
        &job_id,
        "owner-non-retry",
        "agent_not_found",
        "2026-09-01T00:00:01Z",
        false,
    )
    .await
    .expect("mark failed"));

    let loaded = load_session_memory_job_sqlx(database.pool(), "default", &job_id)
        .await
        .expect("load")
        .expect("exists");
    assert_eq!(loaded.status, SessionMemoryJobStatus::Failed);
    assert_eq!(loaded.retry_at, None);
    assert_eq!(loaded.retry_count, 1);

    // 2. Retryable error under max retries -> sets retry_at
    sqlx::query(
            "UPDATE session_memory_jobs SET status = 'running', retry_count = 4, ownership_token = 'owner-max' WHERE id = ?1",
        )
        .bind(&job_id)
        .execute(database.pool())
        .await
        .expect("setup job at retry limit");

    assert!(mark_session_memory_job_failed_with_lease_sqlx(
        database.pool(),
        "default",
        &job_id,
        "owner-max",
        "transient_error",
        "2026-09-01T00:00:02Z",
        true,
    )
    .await
    .expect("mark failed at limit"));

    let loaded_max = load_session_memory_job_sqlx(database.pool(), "default", &job_id)
        .await
        .expect("load")
        .expect("exists");
    assert_eq!(loaded_max.status, SessionMemoryJobStatus::Failed);
    assert_eq!(loaded_max.retry_at, None);
    assert_eq!(loaded_max.retry_count, 5);

    assert!(
        retry_session_memory_job_sqlx(database.pool(), "default", &job_id)
            .await
            .expect("explicit retry restarts capped job")
    );
    let restarted = load_session_memory_job_sqlx(database.pool(), "default", &job_id)
        .await
        .expect("load restarted job")
        .expect("restarted job exists");
    assert_eq!(restarted.status, SessionMemoryJobStatus::Queued);
    assert_eq!(restarted.retry_count, 0);
    assert_eq!(restarted.retry_at, None);

    drop(database);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test(flavor = "multi_thread")]
async fn explicit_recent_rebuild_reopens_matching_phase1_failure_only() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-session-memory-manual-rebuild-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open fixture");
    sqlx::query(
        r#"
            INSERT INTO conversation_sessions (
                tenant_id, id, source_id, adapter_id, external_id, title,
                project_path, started_at, updated_at, source_locator,
                source_fingerprint, missing, created_at, imported_at
            ) VALUES (
                'default', 'manual-session', 'source-1', 'adapter-1',
                'manual-ext', 'Manual fixture', NULL,
                '2026-09-16T08:00:00Z', '2026-09-16T09:00:00Z',
                'fixture://manual-session', 'manual-fp', 0,
                '2026-09-16T08:00:00Z', '2026-09-16T09:00:00Z'
            )
            "#,
    )
    .execute(database.pool())
    .await
    .expect("insert session");

    let now = "2026-09-16T10:00:00Z";
    assert_eq!(
        backfill_session_memory_jobs_sqlx(database.pool(), "default", "", now)
            .await
            .expect("automatic backfill"),
        1
    );
    sqlx::query(
            "UPDATE session_memory_jobs SET status = 'failed', retry_count = ?1, retry_at = NULL, last_error = 'agent_not_found' WHERE tenant_id = 'default' AND session_id = 'manual-session'",
        )
        .bind(MAX_SESSION_MEMORY_JOB_RETRIES)
        .execute(database.pool())
        .await
        .expect("make job terminal");

    assert_eq!(
        backfill_session_memory_jobs_sqlx(database.pool(), "default", "", now)
            .await
            .expect("automatic backfill remains idempotent"),
        0
    );
    assert_eq!(
        rebuild_recent_session_memory_jobs_sqlx(
            database.pool(),
            "default",
            "",
            "2026-09-16T10:01:00Z",
        )
        .await
        .expect("explicit rebuild"),
        1
    );
    let restarted: (String, i64, Option<String>) = sqlx::query_as(
            "SELECT status, retry_count, last_error FROM session_memory_jobs WHERE tenant_id = 'default' AND session_id = 'manual-session'",
        )
        .fetch_one(database.pool())
        .await
        .expect("load restarted job");
    assert_eq!(restarted, ("queued".to_string(), 0, None));

    sqlx::query(
        r#"
            INSERT INTO conversation_sessions (
                tenant_id, id, source_id, adapter_id, external_id, title,
                project_path, started_at, updated_at, source_locator,
                source_fingerprint, missing, created_at, imported_at,
                execution_origin, user_visible
            ) VALUES (
                'default', 'watermark-window-session', 'source-1', 'adapter-1',
                'watermark-window-ext', 'Watermark window fixture', NULL,
                '2026-09-13T08:00:00Z', '2026-09-13T09:00:00Z',
                'fixture://watermark-window-session', 'watermark-window-fp', 0,
                '2026-09-13T08:00:00Z', '2026-09-13T09:00:00Z',
                'user', 1
            )
            "#,
    )
    .execute(database.pool())
    .await
    .expect("insert exact watermark-window session");
    assert_eq!(
        ensure_session_memory_jobs_for_sessions_sqlx(
            database.pool(),
            "default",
            &["watermark-window-session".to_string()],
            "",
            "2026-09-16T10:02:00Z",
            true,
        )
        .await
        .expect("explicit candidate rebuild"),
        1
    );
    let exact_job_status: String = sqlx::query_scalar(
            "SELECT status FROM session_memory_jobs WHERE tenant_id = 'default' AND session_id = 'watermark-window-session'",
        )
        .fetch_one(database.pool())
        .await
        .expect("load exact-window job");
    assert_eq!(exact_job_status, "queued");

    drop(database);
    let _ = std::fs::remove_file(&path);
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
