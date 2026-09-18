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
async fn sqlx_verified_mount_persistence_updates_mount_and_deployment_state_atomically() {
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
    let state = DeploymentState {
        profile_id: "profile-a".to_string(),
        asset_id: "asset-a".to_string(),
        target_path: "/target/a".to_string(),
        strategy: DeploymentStrategy::SymlinkToSource,
        source_hash: "hash-a".to_string(),
        deployed_at: "2026-06-18T00:00:00Z".to_string(),
        managed_by: "assetiweave".to_string(),
    };

    let mounted = persist_verified_mount_sqlx(
        database.pool(),
        "default",
        &state,
        DeploymentStrategy::SymlinkToSource,
    )
    .await
    .expect("persist verified mount");
    let managed_after_mount = managed_by(
        database.pool(),
        "default",
        "profile-a",
        "asset-a",
        "/target/a",
    )
    .await
    .expect("check managed after mount");
    let unmounted = persist_verified_unmount_sqlx(
        database.pool(),
        "default",
        "asset-a",
        "profile-a",
        "/target/a",
        DeploymentStrategy::CopyToTarget,
    )
    .await
    .expect("persist verified unmount");
    let managed_after_unmount = managed_by(
        database.pool(),
        "default",
        "profile-a",
        "asset-a",
        "/target/a",
    )
    .await
    .expect("check managed after unmount");
    let stored_mounts = load_asset_mounts_sqlx(database.pool(), "default", Some("asset-a"))
        .await
        .expect("load stored mounts");

    assert!(mounted.enabled);
    assert_eq!(managed_after_mount.as_deref(), Some("assetiweave"));
    assert!(!unmounted.enabled);
    assert_eq!(unmounted.strategy, DeploymentStrategy::CopyToTarget);
    assert_eq!(mounted.created_at, unmounted.created_at);
    assert!(managed_after_unmount.is_none());
    assert_eq!(stored_mounts, vec![unmounted]);

    drop(database);
    cleanup_database(&db_path);
}

async fn managed_by(
    pool: &SqlitePool,
    tenant_id: &str,
    profile_id: &str,
    asset_id: &str,
    target_path: &str,
) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar(sql::GET_MANAGED_DEPLOYMENT)
        .bind(tenant_id)
        .bind(profile_id)
        .bind(asset_id)
        .bind(target_path)
        .fetch_optional(pool)
        .await?)
}

async fn insert_asset(pool: &SqlitePool, asset_id: &str) -> AppResult<()> {
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
