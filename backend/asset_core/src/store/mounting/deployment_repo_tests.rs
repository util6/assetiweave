use super::*;
use uuid::Uuid;

#[tokio::test]
async fn sqlx_deployment_state_table_is_dropped() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-deployment-dropped-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .expect("open database");

    let count_result: Result<i64, _> = sqlx::query_scalar("SELECT COUNT(*) FROM deployment_state")
        .fetch_one(database.pool())
        .await;
    assert!(
        count_result.is_err(),
        "deployment_state table must be dropped by migration"
    );

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_managed_deployment_helpers_return_empty() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-deployment-helpers-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .expect("open database");

    let managed = is_managed_deployment_sqlx(
        database.pool(),
        "default",
        "profile-a",
        "asset-a",
        "/target/a",
    )
    .await
    .expect("check managed");
    assert!(!managed);

    let targets =
        load_managed_deployment_targets_by_profile_sqlx(database.pool(), "default", "profile-a")
            .await
            .expect("load managed targets");
    assert!(targets.is_empty());

    drop(database);
    cleanup_database(&db_path);
}

fn cleanup_database(db_path: &std::path::Path) {
    let _ = std::fs::remove_file(db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}
