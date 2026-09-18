use super::*;
use crate::backend::store::Database;

#[tokio::test]
async fn lease_completion_is_fenced_and_retries_are_capped() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-memory-v2-maintenance-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = Database::open_initialized_async(&db_path)
        .await
        .expect("database");
    let tenant_id = "default";
    let now = "2026-09-16T10:00:00Z";
    let job_id = enqueue_memory_v2_maintenance_job_sqlx(
        database.pool(),
        tenant_id,
        "maintenance-test",
        "project_consolidation",
        Some("/tmp/project"),
        Some("/tmp/project"),
        "fp",
        "{}",
        now,
    )
    .await
    .expect("enqueue");
    assert_eq!(job_id, "maintenance-test");
    assert!(claim_memory_v2_maintenance_job_with_lease_sqlx(
        database.pool(),
        tenant_id,
        &job_id,
        "owner-a",
        now,
    )
    .await
    .expect("claim"));
    assert!(!finish_memory_v2_maintenance_job_sqlx(
        database.pool(),
        tenant_id,
        &job_id,
        "owner-b",
        "succeeded",
        None,
        None,
        false,
        now,
    )
    .await
    .expect("fenced completion"));
    assert!(finish_memory_v2_maintenance_job_sqlx(
        database.pool(),
        tenant_id,
        &job_id,
        "owner-a",
        "failed",
        Some("TEST"),
        Some("failure"),
        true,
        now,
    )
    .await
    .expect("completion"));

    for attempt in 1..MAX_MEMORY_V2_MAINTENANCE_RETRIES {
        assert!(
            retry_memory_v2_maintenance_job_sqlx(database.pool(), tenant_id, &job_id, now,)
                .await
                .expect("retry request")
        );
        assert!(claim_memory_v2_maintenance_job_with_lease_sqlx(
            database.pool(),
            tenant_id,
            &job_id,
            "owner-a",
            now,
        )
        .await
        .expect("claim retry"));
        assert!(
            finish_memory_v2_maintenance_job_sqlx(
                database.pool(),
                tenant_id,
                &job_id,
                "owner-a",
                "failed",
                Some("TEST"),
                Some("failure"),
                true,
                now,
            )
            .await
            .expect("finish retry"),
            "attempt {attempt}"
        );
    }
    assert!(
        !retry_memory_v2_maintenance_job_sqlx(database.pool(), tenant_id, &job_id, now,)
            .await
            .expect("retry cap")
    );
    drop(database);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}
