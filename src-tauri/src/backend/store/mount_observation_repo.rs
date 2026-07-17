use crate::backend::{
    dto::{AppResult, AssetMountObservation, AssetMountStatus, PhysicalMountStateDto},
    models::{Asset, DeploymentState, TargetProfile},
};
use chrono::Utc;
#[cfg(test)]
use sqlx::{sqlite::SqliteRow, Row as SqlxRow};
use sqlx::{SqliteConnection, SqlitePool};
use std::collections::HashMap;

#[cfg(test)]
use super::codec::decode_enum;
use super::{codec::encode_enum, sql};

async fn upsert_asset_mount_observations_connection(
    conn: &mut SqliteConnection,
    tenant_id: &str,
    observations: &[AssetMountObservation],
) -> AppResult<()> {
    for observation in observations {
        sqlx::query(sql::UPSERT_ASSET_MOUNT_OBSERVATION)
            .bind(tenant_id)
            .bind(&observation.asset_id)
            .bind(&observation.profile_id)
            .bind(&observation.target_dir)
            .bind(&observation.target_path)
            .bind(encode_enum(observation.state)?)
            .bind(&observation.linked_source)
            .bind(&observation.observed_at)
            .execute(&mut *conn)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) async fn upsert_asset_mount_observations_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    observations: &[AssetMountObservation],
) -> AppResult<()> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    upsert_asset_mount_observations_connection(&mut tx, tenant_id, observations).await?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) async fn persist_asset_mount_snapshot_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    observations: &[AssetMountObservation],
    assets: &[Asset],
    profiles: &[TargetProfile],
    statuses: &[AssetMountStatus],
) -> AppResult<()> {
    let asset_by_id = assets
        .iter()
        .map(|asset| (asset.id.as_str(), asset))
        .collect::<HashMap<_, _>>();
    let profile_by_id = profiles
        .iter()
        .map(|profile| (profile.id.as_str(), profile))
        .collect::<HashMap<_, _>>();
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;

    upsert_asset_mount_observations_connection(&mut tx, tenant_id, observations).await?;
    for status in statuses {
        let asset = asset_by_id
            .get(status.asset_id.as_str())
            .ok_or_else(|| format!("asset not found: {}", status.asset_id))?;
        let profile = profile_by_id
            .get(status.profile_id.as_str())
            .ok_or_else(|| format!("profile not found: {}", status.profile_id))?;
        let enabled = matches!(status.state, PhysicalMountStateDto::Mounted);

        if enabled {
            let state = DeploymentState {
                profile_id: profile.id.clone(),
                asset_id: asset.id.clone(),
                target_path: status.target_path.clone(),
                strategy: profile.deployment_strategy,
                source_hash: asset.content_hash.clone().unwrap_or_default(),
                deployed_at: Utc::now().to_rfc3339(),
                managed_by: "assetiweave".to_string(),
            };
            sqlx::query(sql::UPSERT_DEPLOYMENT_STATE)
                .bind(tenant_id)
                .bind(&state.profile_id)
                .bind(&state.asset_id)
                .bind(&state.target_path)
                .bind(encode_enum(state.strategy)?)
                .bind(&state.source_hash)
                .bind(&state.deployed_at)
                .bind(&state.managed_by)
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
        } else {
            sqlx::query(sql::DELETE_DEPLOYMENT_STATE)
                .bind(tenant_id)
                .bind(&profile.id)
                .bind(&asset.id)
                .bind(&status.target_path)
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
        }

        let now = Utc::now().to_rfc3339();
        let created_at: Option<String> = sqlx::query_scalar(sql::GET_ASSET_MOUNT_CREATED_AT)
            .bind(tenant_id)
            .bind(&asset.id)
            .bind(&profile.id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
        sqlx::query(sql::UPSERT_ASSET_MOUNT)
            .bind(tenant_id)
            .bind(&asset.id)
            .bind(&profile.id)
            .bind(enabled)
            .bind(encode_enum(profile.deployment_strategy)?)
            .bind(created_at.unwrap_or_else(|| now.clone()))
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
    }

    sqlx::query(sql::DELETE_ORPHAN_ASSET_MOUNT_OBSERVATIONS)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
pub(crate) async fn load_asset_mount_observations_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<AssetMountObservation>> {
    let rows = sqlx::query(sql::LIST_ASSET_MOUNT_OBSERVATIONS)
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;

    rows.iter().map(map_sqlx_observation).collect()
}

#[cfg(test)]
pub(crate) async fn delete_orphan_asset_mount_observations_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<()> {
    sqlx::query(sql::DELETE_ORPHAN_ASSET_MOUNT_OBSERVATIONS)
        .bind(tenant_id)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
fn map_sqlx_observation(row: &SqliteRow) -> AppResult<AssetMountObservation> {
    Ok(AssetMountObservation {
        asset_id: row.try_get(0).map_err(|error| error.to_string())?,
        profile_id: row.try_get(1).map_err(|error| error.to_string())?,
        target_dir: row.try_get(2).map_err(|error| error.to_string())?,
        target_path: row.try_get(3).map_err(|error| error.to_string())?,
        state: decode_enum(
            row.try_get::<String, _>(4)
                .map_err(|error| error.to_string())?,
        )?,
        linked_source: row.try_get(5).map_err(|error| error.to_string())?,
        observed_at: row.try_get(6).map_err(|error| error.to_string())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::dto::PhysicalMountStateDto;
    use uuid::Uuid;

    #[test]
    fn sqlx_mount_observation_repo_upserts_and_cleans_orphans() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-mount-observation-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open(&db_path).expect("open database");

        database
            .block_on(async {
                insert_asset(database.pool(), "asset-a").await?;
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
                .await?;
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
                .await?;

                let before_cleanup =
                    load_asset_mount_observations_sqlx(database.pool(), "default").await?;
                delete_orphan_asset_mount_observations_sqlx(database.pool(), "default").await?;
                let after_cleanup =
                    load_asset_mount_observations_sqlx(database.pool(), "default").await?;

                AppResult::Ok((before_cleanup, after_cleanup))
            })
            .map(|(before_cleanup, after_cleanup)| {
                assert_eq!(before_cleanup.len(), 2);
                let retained = before_cleanup
                    .iter()
                    .find(|observation| observation.asset_id == "asset-a")
                    .expect("retained observation");
                assert_eq!(retained.state, PhysicalMountStateDto::Broken);
                assert_eq!(retained.linked_source.as_deref(), Some("/source/new"));
                assert_eq!(after_cleanup.len(), 1);
                assert_eq!(after_cleanup[0].asset_id, "asset-a");
            })
            .expect("query SQLx mount observation repo");
        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_mount_snapshot_rolls_back_when_status_references_missing_asset() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-mount-snapshot-rollback-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open(&db_path).expect("open database");
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

        let error = database
            .block_on(persist_asset_mount_snapshot_sqlx(
                database.pool(),
                "default",
                std::slice::from_ref(&observation),
                &[],
                &[],
                std::slice::from_ref(&status),
            ))
            .expect_err("missing asset must reject snapshot");
        let observations = database
            .block_on(load_asset_mount_observations_sqlx(
                database.pool(),
                "default",
            ))
            .expect("load observations after rollback");

        assert!(error.contains("asset not found: missing-asset"));
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
