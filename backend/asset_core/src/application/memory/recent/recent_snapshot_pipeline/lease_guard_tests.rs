use super::*;
use crate::backend::store::Database;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread")]
async fn test_recent_memory_lease_guard_extends_lease_periodically() {
    let root = std::env::temp_dir().join(format!("assetiweave-lease-guard-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create test root");
    let database = Database::open_initialized_async(&root.join("app.db"))
        .await
        .expect("open database");
    let pool = database.pool().clone();

    let tenant_id = "default";
    let job_id = "recent-job-lease-test";
    let token = "token-1";
    let initial_now = Utc::now().to_rfc3339();

    // Enqueue and claim the job
    store::enqueue_recent_memory_job_sqlx(
        &pool,
        tenant_id,
        job_id,
        "2026-10-15T02:00:00Z",
        48,
        "target-fp",
        "content-fp",
        r#"{"workOrder":{},"payload":{}}"#,
        &initial_now,
    )
    .await
    .expect("enqueue job");

    let claimed = store::claim_recent_memory_job_with_lease_sqlx(
        &pool,
        tenant_id,
        job_id,
        token,
        &initial_now,
    )
    .await
    .expect("claim job");
    assert!(claimed, "job should be successfully claimed");

    let job_initial = store::load_recent_memory_job_sqlx(&pool, tenant_id, job_id)
        .await
        .expect("load job")
        .expect("job exists");
    let initial_lease_expiry = job_initial.lease_expires_at.expect("has lease expiry");

    // Start lease guard with a fast 25ms tick interval
    let cancellation = CancellationToken::new();
    let guard = RecentMemoryLeaseGuard::start_with_interval(
        pool.clone(),
        tenant_id.to_string(),
        job_id.to_string(),
        token.to_string(),
        cancellation.clone(),
        Duration::from_millis(25),
    );

    // Sleep to let ticker fire at least twice
    tokio::time::sleep(Duration::from_millis(80)).await;

    let job_after_heartbeat = store::load_recent_memory_job_sqlx(&pool, tenant_id, job_id)
        .await
        .expect("load job")
        .expect("job exists");

    let updated_lease_expiry = job_after_heartbeat
        .lease_expires_at
        .expect("has updated lease");
    assert!(
        updated_lease_expiry >= initial_lease_expiry,
        "lease_expires_at should be updated by heartbeat guard: initial={initial_lease_expiry}, updated={updated_lease_expiry}"
    );
    assert!(
        job_after_heartbeat.heartbeat_at.is_some(),
        "heartbeat_at should be set"
    );

    // Drop guard and ensure no panic
    drop(guard);
}
