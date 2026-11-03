use super::*;
use crate::backend::domain::{
    mounting::PhysicalMountStateDto, AssetFormat, AssetKind, DeploymentStrategy, ProfileSafety,
    RuleSet,
};
use uuid::Uuid;

#[tokio::test]
async fn sqlx_mount_snapshot_persists_mounts_directly() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-mount-snapshot-direct-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .expect("open database");

    insert_asset(database.pool(), "asset-a")
        .await
        .expect("insert asset");
    let asset = test_asset("asset-a");
    let profile = test_profile("profile-a");
    let status = test_status(
        "asset-a",
        "profile-a",
        PhysicalMountStateDto::Mounted,
        Some("/source/a"),
    );

    persist_asset_mount_snapshot_sqlx(
        database.pool(),
        "default",
        std::slice::from_ref(&asset),
        std::slice::from_ref(&profile),
        std::slice::from_ref(&status),
    )
    .await
    .expect("persist snapshot directly");

    let mounts =
        crate::backend::store::load_asset_mounts_sqlx(database.pool(), "default", Some("asset-a"))
            .await
            .expect("load mounts");
    assert_eq!(mounts.len(), 1);
    assert!(mounts[0].enabled);
    assert_eq!(mounts[0].asset_id, "asset-a");
    assert_eq!(mounts[0].profile_id, "profile-a");

    let unmounted_status = AssetMountStatus {
        state: PhysicalMountStateDto::NotMounted,
        ..status
    };
    persist_asset_mount_snapshot_sqlx(
        database.pool(),
        "default",
        std::slice::from_ref(&asset),
        std::slice::from_ref(&profile),
        std::slice::from_ref(&unmounted_status),
    )
    .await
    .expect("persist unmounted snapshot directly");

    let mounts_after =
        crate::backend::store::load_asset_mounts_sqlx(database.pool(), "default", Some("asset-a"))
            .await
            .expect("load mounts after unmount");
    assert_eq!(mounts_after.len(), 1);
    assert!(!mounts_after[0].enabled);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_mount_snapshot_does_not_insert_unmounted_asset_into_mounts() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-mount-snapshot-no-cartesian-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .expect("open database");

    insert_asset(database.pool(), "asset-unmounted")
        .await
        .expect("insert asset");
    let asset = test_asset("asset-unmounted");
    let profile = test_profile("profile-a");
    let unmounted_status = test_status(
        "asset-unmounted",
        "profile-a",
        PhysicalMountStateDto::NotMounted,
        None,
    );

    // 未挂载资产扫描到 NotMounted 时，不应无脑插入 enabled = false，避免笛卡尔积膨胀
    persist_asset_mount_snapshot_sqlx(
        database.pool(),
        "default",
        std::slice::from_ref(&asset),
        std::slice::from_ref(&profile),
        std::slice::from_ref(&unmounted_status),
    )
    .await
    .expect("persist unmounted snapshot");

    let mounts = crate::backend::store::load_asset_mounts_sqlx(
        database.pool(),
        "default",
        Some("asset-unmounted"),
    )
    .await
    .expect("load mounts");
    assert!(
        mounts.is_empty(),
        "unmounted asset must not be inserted into asset_mounts if not previously mounted"
    );

    // 挂载后生成 enabled = true 记录
    let mounted_status = test_status(
        "asset-unmounted",
        "profile-a",
        PhysicalMountStateDto::Mounted,
        Some("/source/asset-unmounted"),
    );
    persist_asset_mount_snapshot_sqlx(
        database.pool(),
        "default",
        std::slice::from_ref(&asset),
        std::slice::from_ref(&profile),
        std::slice::from_ref(&mounted_status),
    )
    .await
    .expect("persist mounted snapshot");

    let mounts_after_mount = crate::backend::store::load_asset_mounts_sqlx(
        database.pool(),
        "default",
        Some("asset-unmounted"),
    )
    .await
    .expect("load mounts");
    assert_eq!(mounts_after_mount.len(), 1);
    assert!(mounts_after_mount[0].enabled);

    // 再次变更为 NotMounted 时，已存在的记录更新为 enabled = false
    persist_asset_mount_snapshot_sqlx(
        database.pool(),
        "default",
        std::slice::from_ref(&asset),
        std::slice::from_ref(&profile),
        std::slice::from_ref(&unmounted_status),
    )
    .await
    .expect("persist unmounted snapshot after mounted");

    let mounts_after_unmount = crate::backend::store::load_asset_mounts_sqlx(
        database.pool(),
        "default",
        Some("asset-unmounted"),
    )
    .await
    .expect("load mounts");
    assert_eq!(mounts_after_unmount.len(), 1);
    assert!(!mounts_after_unmount[0].enabled);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_mount_snapshot_rolls_back_when_status_references_missing_asset() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-mount-snapshot-rollback-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .expect("open database");
    let status = test_status(
        "missing-asset",
        "profile-a",
        PhysicalMountStateDto::Mounted,
        Some("/source/a"),
    );

    let error = persist_asset_mount_snapshot_sqlx(
        database.pool(),
        "default",
        &[],
        &[],
        std::slice::from_ref(&status),
    )
    .await
    .expect_err("missing asset must reject snapshot");

    assert!(error.to_string().contains("asset not found: missing-asset"));
    let mounts = crate::backend::store::load_asset_mounts_sqlx(database.pool(), "default", None)
        .await
        .expect("load mounts after rollback");
    assert!(mounts.is_empty());
    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_asset_mount_observations_table_is_dropped() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-mount-observations-dropped-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .expect("open database");

    let count_result: Result<i64, _> =
        sqlx::query_scalar("SELECT COUNT(*) FROM asset_mount_observations")
            .fetch_one(database.pool())
            .await;
    assert!(
        count_result.is_err(),
        "asset_mount_observations table must be dropped by migration"
    );

    drop(database);
    cleanup_database(&db_path);
}

fn test_status(
    asset_id: &str,
    profile_id: &str,
    state: PhysicalMountStateDto,
    linked_source: Option<&str>,
) -> AssetMountStatus {
    AssetMountStatus {
        asset_id: asset_id.to_string(),
        profile_id: profile_id.to_string(),
        target_dir: "/target".to_string(),
        target_path: format!("/target/{asset_id}"),
        display_target_dir: "/target".to_string(),
        display_target_path: format!("/target/{asset_id}"),
        display_linked_source: linked_source.map(str::to_string),
        state,
        linked_source: linked_source.map(str::to_string),
    }
}

fn test_asset(asset_id: &str) -> Asset {
    Asset {
        id: asset_id.to_string(),
        source_id: "source-a".to_string(),
        name: asset_id.to_string(),
        kind: AssetKind::Skill,
        detector_id: "builtin.skill".to_string(),
        detector_version: 1,
        format: AssetFormat::Directory,
        relative_path: asset_id.to_string(),
        absolute_path: format!("/source/{asset_id}"),
        entry_file: None,
        description: None,
        content_hash: None,
        discovered_at: "2026-06-18T00:00:00Z".to_string(),
        updated_at: "2026-06-18T00:00:00Z".to_string(),
    }
}

fn test_profile(profile_id: &str) -> TargetProfile {
    TargetProfile {
        id: profile_id.to_string(),
        name: profile_id.to_string(),
        app_kind: None,
        target_provider_id: "generic".to_string(),
        target_paths: vec!["/target".to_string()],
        supported_kinds: vec![AssetKind::Skill],
        deployment_strategy: DeploymentStrategy::SymlinkToSource,
        enabled: true,
        include: RuleSet {
            kinds: Vec::new(),
            tags: Vec::new(),
            groups: Vec::new(),
            sources: Vec::new(),
            path_patterns: Vec::new(),
        },
        exclude: RuleSet {
            kinds: Vec::new(),
            tags: Vec::new(),
            groups: Vec::new(),
            sources: Vec::new(),
            path_patterns: Vec::new(),
        },
        safety: ProfileSafety {
            allow_remove: true,
            allow_overwrite: true,
        },
    }
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
