use crate::backend::{
    dto::{AssetMountObservation, AssetMountStatus, PhysicalMountStateDto},
    models::{Asset, DeploymentState, TargetProfile},
    runtime::{AppError, AppResult},
};
use chrono::Utc;
#[cfg(test)]
use sqlx::{sqlite::SqliteRow, Row as SqlxRow};
use sqlx::{SqliteConnection, SqlitePool};
use std::collections::HashMap;

#[cfg(test)]
use super::codec::decode_enum_app;
use super::{codec::encode_enum_app, sql};

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
            .bind(encode_enum_app(observation.state)?)
            .bind(&observation.linked_source)
            .bind(&observation.observed_at)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) async fn upsert_asset_mount_observations_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    observations: &[AssetMountObservation],
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    upsert_asset_mount_observations_connection(&mut tx, tenant_id, observations).await?;
    tx.commit().await?;
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
    let mut tx = pool.begin().await?;

    upsert_asset_mount_observations_connection(&mut tx, tenant_id, observations).await?;
    for status in statuses {
        let asset = asset_by_id
            .get(status.asset_id.as_str())
            .ok_or_else(|| AppError::NotFound(format!("asset not found: {}", status.asset_id)))?;
        let profile = profile_by_id
            .get(status.profile_id.as_str())
            .ok_or_else(|| {
                AppError::NotFound(format!("profile not found: {}", status.profile_id))
            })?;
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
                .bind(encode_enum_app(state.strategy)?)
                .bind(&state.source_hash)
                .bind(&state.deployed_at)
                .bind(&state.managed_by)
                .execute(&mut *tx)
                .await?;
        } else {
            sqlx::query(sql::DELETE_DEPLOYMENT_STATE)
                .bind(tenant_id)
                .bind(&profile.id)
                .bind(&asset.id)
                .bind(&status.target_path)
                .execute(&mut *tx)
                .await?;
        }

        let now = Utc::now().to_rfc3339();
        let created_at: Option<String> = sqlx::query_scalar(sql::GET_ASSET_MOUNT_CREATED_AT)
            .bind(tenant_id)
            .bind(&asset.id)
            .bind(&profile.id)
            .fetch_optional(&mut *tx)
            .await?;
        sqlx::query(sql::UPSERT_ASSET_MOUNT)
            .bind(tenant_id)
            .bind(&asset.id)
            .bind(&profile.id)
            .bind(enabled)
            .bind(encode_enum_app(profile.deployment_strategy)?)
            .bind(created_at.unwrap_or_else(|| now.clone()))
            .bind(now)
            .execute(&mut *tx)
            .await?;
    }

    sqlx::query(sql::DELETE_ORPHAN_ASSET_MOUNT_OBSERVATIONS)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
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
        .await?;

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
        .await?;
    Ok(())
}

#[cfg(test)]
fn map_sqlx_observation(row: &SqliteRow) -> AppResult<AssetMountObservation> {
    Ok(AssetMountObservation {
        asset_id: row.try_get(0)?,
        profile_id: row.try_get(1)?,
        target_dir: row.try_get(2)?,
        target_path: row.try_get(3)?,
        state: decode_enum_app(row.try_get::<String, _>(4)?)?,
        linked_source: row.try_get(5)?,
        observed_at: row.try_get(6)?,
    })
}

#[cfg(test)]
#[path = "mount_observation_repo_tests.rs"]
mod tests;
