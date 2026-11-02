use super::*;
use uuid::Uuid;

#[tokio::test]
async fn sqlx_mount_repo_sets_lists_filters_and_cleans_orphans() {
    let db_path =
        std::env::temp_dir().join(format!("assetiweave-mount-sqlx-{}.sqlite", Uuid::new_v4()));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .expect("open database");

    insert_asset(database.pool(), "asset-a")
        .await
        .expect("insert asset");
    let initial = set_asset_mount_sqlx(
        database.pool(),
        "default",
        "asset-a",
        "profile-a",
        true,
        DeploymentStrategy::SymlinkToSource,
    )
    .await
    .expect("set asset mount");
    let updated = set_asset_mount_sqlx(
        database.pool(),
        "default",
        "asset-a",
        "profile-a",
        false,
        DeploymentStrategy::CopyToTarget,
    )
    .await
    .expect("update asset mount");
    set_asset_mount_sqlx(
        database.pool(),
        "default",
        "asset-b",
        "profile-a",
        true,
        DeploymentStrategy::SymlinkToSource,
    )
    .await
    .expect("set second asset mount");

    let scoped = load_asset_mounts_sqlx(database.pool(), "default", Some("asset-a"))
        .await
        .expect("load scoped mounts");
    let enabled = load_enabled_asset_mounts_sqlx(database.pool(), "default", Some("profile-a"))
        .await
        .expect("load enabled mounts");
    delete_orphan_asset_mounts_sqlx(database.pool(), "default")
        .await
        .expect("delete orphan mounts");
    let all_after_cleanup = load_asset_mounts_sqlx(database.pool(), "default", None)
        .await
        .expect("load all mounts after cleanup");

    assert_eq!(initial.created_at, updated.created_at);
    assert!(!updated.enabled);
    assert_eq!(updated.strategy, DeploymentStrategy::CopyToTarget);
    assert_eq!(scoped, vec![updated]);
    assert_eq!(enabled.len(), 1);
    assert_eq!(enabled[0].asset_id, "asset-b");
    assert_eq!(all_after_cleanup.len(), 1);
    assert_eq!(all_after_cleanup[0].asset_id, "asset-a");

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_verified_mount_persistence_updates_mount_atomically() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-verified-mount-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .expect("open database");

    insert_asset(database.pool(), "asset-a")
        .await
        .expect("insert asset");

    let mounted = persist_verified_mount_sqlx(
        database.pool(),
        "default",
        "asset-a",
        "profile-a",
        DeploymentStrategy::SymlinkToSource,
    )
    .await
    .expect("persist verified mount");
    let unmounted = persist_verified_unmount_sqlx(
        database.pool(),
        "default",
        "asset-a",
        "profile-a",
        DeploymentStrategy::CopyToTarget,
    )
    .await
    .expect("persist verified unmount");
    let stored_mounts = load_asset_mounts_sqlx(database.pool(), "default", Some("asset-a"))
        .await
        .expect("load stored mounts");

    assert!(mounted.enabled);
    assert!(!unmounted.enabled);
    assert_eq!(unmounted.strategy, DeploymentStrategy::CopyToTarget);
    assert_eq!(mounted.created_at, unmounted.created_at);
    assert_eq!(stored_mounts, vec![unmounted]);

    drop(database);
    cleanup_database(&db_path);
}

async fn insert_asset(pool: &SqlitePool, asset_id: &str) -> StoreResult<()> {
    sqlx::query(
        r#"
        INSERT INTO assets (
            id, source_id, name, kind, format, relative_path, absolute_path,
            entry_file, description, content_hash, discovered_at, updated_at
        ) VALUES (?1, 'source-a', ?1, 'skill', 'markdown', ?1, ?1, NULL, NULL, NULL, ?2, ?2)
        "#,
    )
    .bind(asset_id)
    .bind("2026-06-18T00:00:00Z")
    .execute(pool)
    .await?;
    Ok(())
}

fn cleanup_database(db_path: &std::path::Path) {
    let _ = std::fs::remove_file(db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}
