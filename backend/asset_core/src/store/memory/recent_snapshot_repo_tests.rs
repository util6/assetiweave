use super::*;
use crate::backend::store::Database;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread")]
async fn recent_memory_job_lease_rejects_late_completion_and_caps_retries() {
    let root = std::env::temp_dir().join(format!("assetiweave-recent-job-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create test root");
    let database = Database::open_initialized_async(&root.join("app.db"))
        .await
        .expect("open database");
    let pool = database.pool();
    let now = "2026-09-16T10:00:00Z";
    let job_id = enqueue_recent_memory_job_sqlx(
        pool,
        "default",
        "recent-job-1",
        "2026-09-16T02:00:00Z",
        48,
        "target-fp",
        "content-fp",
        r#"{"workOrder":{},"payload":{}}"#,
        now,
    )
    .await
    .expect("enqueue job");
    assert_eq!(job_id, "recent-job-1");
    assert_eq!(
        enqueue_recent_memory_job_sqlx(
            pool,
            "default",
            "different-id",
            "2026-09-16T02:00:00Z",
            48,
            "target-fp",
            "content-fp",
            r#"{"workOrder":{},"payload":{}}"#,
            now,
        )
        .await
        .expect("idempotent enqueue"),
        "recent-job-1"
    );

    assert!(
        claim_recent_memory_job_with_lease_sqlx(pool, "default", &job_id, "owner-a", now,)
            .await
            .expect("claim job")
    );
    assert!(!finish_recent_memory_job_sqlx(
        pool,
        "default",
        &job_id,
        "owner-b",
        "succeeded",
        None,
        None,
        false,
        now,
    )
    .await
    .expect("late completion result"));
    assert!(finish_recent_memory_job_sqlx(
        pool,
        "default",
        &job_id,
        "owner-a",
        "failed",
        Some("TEMPORARY"),
        Some("temporary failure"),
        true,
        now,
    )
    .await
    .expect("failed completion"));

    sqlx::query(
            "UPDATE recent_memory_jobs SET status = 'failed', retry_count = ?1, retry_at = ?2 WHERE tenant_id = 'default' AND id = ?3",
        )
        .bind(MAX_RECENT_MEMORY_JOB_RETRIES)
        .bind(now)
        .bind(&job_id)
        .execute(pool)
        .await
        .expect("set retry cap");
    assert!(
        list_recent_memory_job_ids_for_scheduler_sqlx(pool, "default", now, 10)
            .await
            .expect("list due jobs")
            .is_empty()
    );

    assert!(restart_recent_memory_job_for_rebuild_sqlx(
        pool,
        "default",
        &job_id,
        "2026-09-16T10:01:00Z",
    )
    .await
    .expect("explicit rebuild restarts terminal job"));
    let restarted = load_recent_memory_job_sqlx(pool, "default", &job_id)
        .await
        .expect("load restarted job")
        .expect("restarted job exists");
    assert_eq!(restarted.status, "queued");
    assert_eq!(restarted.retry_count, 0);
    assert_eq!(
        list_recent_memory_job_ids_for_scheduler_sqlx(pool, "default", "2026-09-16T10:01:00Z", 10,)
            .await
            .expect("explicit rebuild becomes schedulable"),
        vec![job_id.clone()]
    );

    drop(database);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "multi_thread")]
async fn recent_memory_job_enqueue_upserts_when_content_fingerprint_changes() {
    let root =
        std::env::temp_dir().join(format!("assetiweave-recent-job-upsert-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create test root");
    let database = Database::open_initialized_async(&root.join("app.db"))
        .await
        .expect("open database");
    let pool = database.pool();
    let now = "2026-10-14T10:00:00Z";

    let job_id = enqueue_recent_memory_job_sqlx(
        pool,
        "default",
        "job-v1",
        "2026-10-14T02:00:00Z",
        24,
        "target-fp-1",
        "content-fp-v1",
        r#"{"version":1}"#,
        now,
    )
    .await
    .expect("initial enqueue");
    assert_eq!(job_id, "job-v1");

    // Simulate job completing or being marked as reused
    sqlx::query("UPDATE recent_memory_jobs SET status = 'reused' WHERE id = ?1")
        .bind(&job_id)
        .execute(pool)
        .await
        .expect("mark reused");

    // Enqueue again with same target watermark/fingerprint, but changed content fingerprint
    let returned_id = enqueue_recent_memory_job_sqlx(
        pool,
        "default",
        "job-v2",
        "2026-10-14T02:00:00Z",
        24,
        "target-fp-1",
        "content-fp-v2",
        r#"{"version":2}"#,
        "2026-10-14T10:05:00Z",
    )
    .await
    .expect("upsert enqueue with changed content fingerprint");
    assert_eq!(returned_id, "job-v1");

    let loaded = load_recent_memory_job_sqlx(pool, "default", &job_id)
        .await
        .expect("load job")
        .expect("job exists");
    assert_eq!(loaded.content_fingerprint, "content-fp-v2");
    assert_eq!(loaded.work_order_json, r#"{"version":2}"#);
    assert_eq!(loaded.status, "queued");

    // It should be claimable now
    let claimed = claim_recent_memory_job_with_lease_sqlx(
        pool,
        "default",
        &job_id,
        "new-owner",
        "2026-10-14T10:05:00Z",
    )
    .await
    .expect("claim after upsert");
    assert!(claimed);

    drop(database);
    let _ = std::fs::remove_dir_all(root);
}
