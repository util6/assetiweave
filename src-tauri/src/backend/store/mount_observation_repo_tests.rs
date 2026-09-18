use super::*;
use crate::backend::dto::PhysicalMountStateDto;
use uuid::Uuid;

#[tokio::test]
async fn sqlx_mount_observation_repo_upserts_and_cleans_orphans() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-mount-observation-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .expect("open database");

    insert_asset(database.pool(), "asset-a")
        .await
        .expect("insert asset");
    upsert_asset_mount_observations_sqlx(
        database.pool(),
        "default",
        &[
            test_observation(
                "asset-a",
                "profile-a",
                PhysicalMountStateDto::Mounted,
                Some("/source/a"),
            ),
            test_observation(
                "asset-b",
                "profile-a",
                PhysicalMountStateDto::Conflict,
                None,
            ),
        ],
    )
    .await
    .expect("upsert observations");
    upsert_asset_mount_observations_sqlx(
        database.pool(),
        "default",
        &[test_observation(
            "asset-a",
            "profile-a",
            PhysicalMountStateDto::Broken,
            Some("/source/new"),
        )],
    )
    .await
    .expect("upsert broken observation");

    let before_cleanup = load_asset_mount_observations_sqlx(database.pool(), "default")
        .await
        .expect("load before cleanup");
    delete_orphan_asset_mount_observations_sqlx(database.pool(), "default")
        .await
        .expect("delete orphans");
    let after_cleanup = load_asset_mount_observations_sqlx(database.pool(), "default")
        .await
        .expect("load after cleanup");

    assert_eq!(before_cleanup.len(), 2);
    let retained = before_cleanup
        .iter()
        .find(|observation| observation.asset_id == "asset-a")
        .expect("retained observation");
    assert_eq!(retained.state, PhysicalMountStateDto::Broken);
    assert_eq!(retained.linked_source.as_deref(), Some("/source/new"));
    assert_eq!(after_cleanup.len(), 1);
    assert_eq!(after_cleanup[0].asset_id, "asset-a");

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
    let observation = test_observation(
        "missing-asset",
        "profile-a",
        PhysicalMountStateDto::Mounted,
        Some("/source/a"),
    );
    let status = AssetMountStatus {
        asset_id: observation.asset_id.clone(),
        profile_id: observation.profile_id.clone(),
        target_dir: observation.target_dir.clone(),
        target_path: observation.target_path.clone(),
        display_target_dir: observation.target_dir.clone(),
        display_target_path: observation.target_path.clone(),
        display_linked_source: observation.linked_source.clone(),
        state: observation.state,
        linked_source: observation.linked_source.clone(),
    };

    let error = persist_asset_mount_snapshot_sqlx(
        database.pool(),
        "default",
        std::slice::from_ref(&observation),
        &[],
        &[],
        std::slice::from_ref(&status),
    )
    .await
    .expect_err("missing asset must reject snapshot");
    let observations = load_asset_mount_observations_sqlx(database.pool(), "default")
        .await
        .expect("load observations after rollback");

    assert!(error.to_string().contains("asset not found: missing-asset"));
    assert!(observations.is_empty());
    drop(database);
    cleanup_database(&db_path);
}

fn test_observation(
    asset_id: &str,
    profile_id: &str,
    state: PhysicalMountStateDto,
    linked_source: Option<&str>,
) -> AssetMountObservation {
    AssetMountObservation {
        asset_id: asset_id.to_string(),
        profile_id: profile_id.to_string(),
        target_dir: "/target".to_string(),
        target_path: format!("/target/{asset_id}"),
        state,
        linked_source: linked_source.map(str::to_string),
        observed_at: "2026-06-18T00:00:00Z".to_string(),
    }
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
