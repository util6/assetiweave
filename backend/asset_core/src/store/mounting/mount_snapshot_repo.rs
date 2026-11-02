use crate::backend::{
    domain::{Asset, AssetMountStatus, PhysicalMountStateDto, TargetProfile},
    store::{StoreError, StoreResult},
};
use chrono::Utc;
use sqlx::SqlitePool;
use std::collections::HashMap;

use super::{codec::encode_enum_app, sql};

pub async fn persist_asset_mount_snapshot_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    assets: &[Asset],
    profiles: &[TargetProfile],
    statuses: &[AssetMountStatus],
) -> StoreResult<()> {
    let asset_by_id = assets
        .iter()
        .map(|asset| (asset.id.as_str(), asset))
        .collect::<HashMap<_, _>>();
    let profile_by_id = profiles
        .iter()
        .map(|profile| (profile.id.as_str(), profile))
        .collect::<HashMap<_, _>>();
    let mut tx = pool.begin().await?;

    for status in statuses {
        let asset = asset_by_id
            .get(status.asset_id.as_str())
            .ok_or_else(|| StoreError::NotFound(format!("asset not found: {}", status.asset_id)))?;
        let profile = profile_by_id
            .get(status.profile_id.as_str())
            .ok_or_else(|| {
                StoreError::NotFound(format!("profile not found: {}", status.profile_id))
            })?;

        let now = Utc::now().to_rfc3339();
        if matches!(status.state, PhysicalMountStateDto::Mounted) {
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
                .bind(true)
                .bind(encode_enum_app(profile.deployment_strategy)?)
                .bind(created_at.unwrap_or_else(|| now.clone()))
                .bind(now)
                .execute(&mut *tx)
                .await?;
        } else {
            sqlx::query(sql::DISABLE_ASSET_MOUNT_IF_ENABLED)
                .bind(tenant_id)
                .bind(&asset.id)
                .bind(&profile.id)
                .bind(now)
                .execute(&mut *tx)
                .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
#[path = "mount_snapshot_repo_tests.rs"]
mod tests;
