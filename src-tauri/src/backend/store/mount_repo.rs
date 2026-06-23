use crate::backend::dto::AppResult;
use crate::backend::models::{AssetMount, DeploymentState, DeploymentStrategy};
use chrono::Utc;
use sqlx::{sqlite::SqliteRow, Row as SqlxRow, Sqlite, SqlitePool, Transaction};

use super::{
    codec::{decode_enum, encode_enum},
    sql,
};

pub(crate) async fn load_asset_mounts_sqlx(
    pool: &SqlitePool,
    asset_id: Option<&str>,
) -> AppResult<Vec<AssetMount>> {
    let rows = sqlx::query(sql::LIST_ASSET_MOUNTS)
        .bind(asset_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;

    rows.iter().map(map_sqlx_mount).collect()
}

pub(crate) async fn load_enabled_asset_mounts_sqlx(
    pool: &SqlitePool,
    profile_id: Option<&str>,
) -> AppResult<Vec<AssetMount>> {
    let rows = sqlx::query(sql::LIST_ENABLED_ASSET_MOUNTS)
        .bind(profile_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;

    rows.iter().map(map_sqlx_mount).collect()
}

pub(crate) async fn delete_orphan_asset_mounts_sqlx(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query(sql::DELETE_ORPHAN_ASSET_MOUNTS)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) async fn set_asset_mount_sqlx(
    pool: &SqlitePool,
    asset_id: &str,
    profile_id: &str,
    enabled: bool,
    strategy: DeploymentStrategy,
) -> AppResult<AssetMount> {
    let now = Utc::now().to_rfc3339();
    let created_at = load_asset_mount_sqlx(pool, asset_id, profile_id)
        .await?
        .map(|mount| mount.created_at)
        .unwrap_or_else(|| now.clone());
    let mount = AssetMount {
        asset_id: asset_id.to_string(),
        profile_id: profile_id.to_string(),
        enabled,
        strategy,
        created_at,
        updated_at: now,
    };
    upsert_asset_mount_sqlx(pool, &mount).await?;
    Ok(mount)
}

pub(crate) async fn persist_verified_mount_sqlx(
    pool: &SqlitePool,
    state: &DeploymentState,
    strategy: DeploymentStrategy,
) -> AppResult<AssetMount> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    upsert_deployment_state_tx(&mut tx, state).await?;
    let mount =
        set_asset_mount_tx(&mut tx, &state.asset_id, &state.profile_id, true, strategy).await?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(mount)
}

pub(crate) async fn persist_verified_unmount_sqlx(
    pool: &SqlitePool,
    asset_id: &str,
    profile_id: &str,
    target_path: &str,
    strategy: DeploymentStrategy,
) -> AppResult<AssetMount> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    delete_deployment_state_tx(&mut tx, profile_id, asset_id, target_path).await?;
    let mount = set_asset_mount_tx(&mut tx, asset_id, profile_id, false, strategy).await?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(mount)
}

async fn load_asset_mount_sqlx(
    pool: &SqlitePool,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<Option<AssetMount>> {
    sqlx::query(sql::GET_ASSET_MOUNT)
        .bind(asset_id)
        .bind(profile_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?
        .map(|row| map_sqlx_mount(&row))
        .transpose()
}

async fn load_asset_mount_tx(
    tx: &mut Transaction<'_, Sqlite>,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<Option<AssetMount>> {
    sqlx::query(sql::GET_ASSET_MOUNT)
        .bind(asset_id)
        .bind(profile_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|error| error.to_string())?
        .map(|row| map_sqlx_mount(&row))
        .transpose()
}

async fn set_asset_mount_tx(
    tx: &mut Transaction<'_, Sqlite>,
    asset_id: &str,
    profile_id: &str,
    enabled: bool,
    strategy: DeploymentStrategy,
) -> AppResult<AssetMount> {
    let now = Utc::now().to_rfc3339();
    let created_at = load_asset_mount_tx(tx, asset_id, profile_id)
        .await?
        .map(|mount| mount.created_at)
        .unwrap_or_else(|| now.clone());
    let mount = AssetMount {
        asset_id: asset_id.to_string(),
        profile_id: profile_id.to_string(),
        enabled,
        strategy,
        created_at,
        updated_at: now,
    };
    upsert_asset_mount_tx(tx, &mount).await?;
    Ok(mount)
}

async fn upsert_asset_mount_sqlx(pool: &SqlitePool, mount: &AssetMount) -> AppResult<()> {
    sqlx::query(sql::UPSERT_ASSET_MOUNT)
        .bind(&mount.asset_id)
        .bind(&mount.profile_id)
        .bind(if mount.enabled { 1_i64 } else { 0_i64 })
        .bind(encode_enum(mount.strategy)?)
        .bind(&mount.created_at)
        .bind(&mount.updated_at)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn upsert_asset_mount_tx(
    tx: &mut Transaction<'_, Sqlite>,
    mount: &AssetMount,
) -> AppResult<()> {
    sqlx::query(sql::UPSERT_ASSET_MOUNT)
        .bind(&mount.asset_id)
        .bind(&mount.profile_id)
        .bind(if mount.enabled { 1_i64 } else { 0_i64 })
        .bind(encode_enum(mount.strategy)?)
        .bind(&mount.created_at)
        .bind(&mount.updated_at)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn upsert_deployment_state_tx(
    tx: &mut Transaction<'_, Sqlite>,
    state: &DeploymentState,
) -> AppResult<()> {
    sqlx::query(sql::UPSERT_DEPLOYMENT_STATE)
        .bind(&state.profile_id)
        .bind(&state.asset_id)
        .bind(&state.target_path)
        .bind(encode_enum(state.strategy)?)
        .bind(&state.source_hash)
        .bind(&state.deployed_at)
        .bind(&state.managed_by)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn delete_deployment_state_tx(
    tx: &mut Transaction<'_, Sqlite>,
    profile_id: &str,
    asset_id: &str,
    target_path: &str,
) -> AppResult<()> {
    sqlx::query(sql::DELETE_DEPLOYMENT_STATE)
        .bind(profile_id)
        .bind(asset_id)
        .bind(target_path)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn map_sqlx_mount(row: &SqliteRow) -> AppResult<AssetMount> {
    Ok(AssetMount {
        asset_id: row.try_get(0).map_err(|error| error.to_string())?,
        profile_id: row.try_get(1).map_err(|error| error.to_string())?,
        enabled: row
            .try_get::<i64, _>(2)
            .map_err(|error| error.to_string())?
            == 1,
        strategy: decode_enum(
            row.try_get::<String, _>(3)
                .map_err(|error| error.to_string())?,
        )?,
        created_at: row.try_get(4).map_err(|error| error.to_string())?,
        updated_at: row.try_get(5).map_err(|error| error.to_string())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn sqlx_mount_repo_sets_lists_filters_and_cleans_orphans() {
        let db_path =
            std::env::temp_dir().join(format!("assetiweave-mount-sqlx-{}.sqlite", Uuid::new_v4()));
        let database = crate::backend::store::Database::open(&db_path).expect("open database");

        database
            .block_on(async {
                insert_asset(database.pool(), "asset-a").await?;
                let initial = set_asset_mount_sqlx(
                    database.pool(),
                    "asset-a",
                    "profile-a",
                    true,
                    DeploymentStrategy::SymlinkToSource,
                )
                .await?;
                let updated = set_asset_mount_sqlx(
                    database.pool(),
                    "asset-a",
                    "profile-a",
                    false,
                    DeploymentStrategy::CopyToTarget,
                )
                .await?;
                set_asset_mount_sqlx(
                    database.pool(),
                    "asset-b",
                    "profile-a",
                    true,
                    DeploymentStrategy::SymlinkToSource,
                )
                .await?;

                let scoped = load_asset_mounts_sqlx(database.pool(), Some("asset-a")).await?;
                let enabled =
                    load_enabled_asset_mounts_sqlx(database.pool(), Some("profile-a")).await?;
                delete_orphan_asset_mounts_sqlx(database.pool()).await?;
                let all_after_cleanup = load_asset_mounts_sqlx(database.pool(), None).await?;

                AppResult::Ok((initial, updated, scoped, enabled, all_after_cleanup))
            })
            .map(|(initial, updated, scoped, enabled, all_after_cleanup)| {
                assert_eq!(initial.created_at, updated.created_at);
                assert!(!updated.enabled);
                assert_eq!(updated.strategy, DeploymentStrategy::CopyToTarget);
                assert_eq!(scoped, vec![updated]);
                assert_eq!(enabled.len(), 1);
                assert_eq!(enabled[0].asset_id, "asset-b");
                assert_eq!(all_after_cleanup.len(), 1);
                assert_eq!(all_after_cleanup[0].asset_id, "asset-a");
            })
            .expect("query SQLx mount repo");
        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_verified_mount_persistence_updates_mount_and_deployment_state_atomically() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-verified-mount-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open(&db_path).expect("open database");

        database
            .block_on(async {
                insert_asset(database.pool(), "asset-a").await?;
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
                    &state,
                    DeploymentStrategy::SymlinkToSource,
                )
                .await?;
                let managed_after_mount =
                    managed_by(database.pool(), "profile-a", "asset-a", "/target/a").await?;
                let unmounted = persist_verified_unmount_sqlx(
                    database.pool(),
                    "asset-a",
                    "profile-a",
                    "/target/a",
                    DeploymentStrategy::CopyToTarget,
                )
                .await?;
                let managed_after_unmount =
                    managed_by(database.pool(), "profile-a", "asset-a", "/target/a").await?;
                let stored_mounts =
                    load_asset_mounts_sqlx(database.pool(), Some("asset-a")).await?;

                AppResult::Ok((
                    mounted,
                    managed_after_mount,
                    unmounted,
                    managed_after_unmount,
                    stored_mounts,
                ))
            })
            .map(
                |(
                    mounted,
                    managed_after_mount,
                    unmounted,
                    managed_after_unmount,
                    stored_mounts,
                )| {
                    assert!(mounted.enabled);
                    assert_eq!(managed_after_mount.as_deref(), Some("assetiweave"));
                    assert!(!unmounted.enabled);
                    assert_eq!(unmounted.strategy, DeploymentStrategy::CopyToTarget);
                    assert_eq!(mounted.created_at, unmounted.created_at);
                    assert!(managed_after_unmount.is_none());
                    assert_eq!(stored_mounts, vec![unmounted]);
                },
            )
            .expect("persist verified mount SQLx transaction");
        drop(database);
        cleanup_database(&db_path);
    }

    async fn managed_by(
        pool: &SqlitePool,
        profile_id: &str,
        asset_id: &str,
        target_path: &str,
    ) -> AppResult<Option<String>> {
        sqlx::query_scalar(sql::GET_MANAGED_DEPLOYMENT)
            .bind(profile_id)
            .bind(asset_id)
            .bind(target_path)
            .fetch_optional(pool)
            .await
            .map_err(|error| error.to_string())
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
        .await
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn cleanup_database(db_path: &std::path::Path) {
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    }
}
