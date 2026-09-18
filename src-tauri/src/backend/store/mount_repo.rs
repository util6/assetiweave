use crate::backend::models::{AssetMount, DeploymentState, DeploymentStrategy};
use crate::backend::runtime::AppResult;
use chrono::Utc;
use sqlx::{sqlite::SqliteRow, Row as SqlxRow, Sqlite, SqlitePool, Transaction};

use super::{
    codec::{decode_enum_app, encode_enum_app},
    sql,
};

pub(crate) async fn load_asset_mounts_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    asset_id: Option<&str>,
) -> AppResult<Vec<AssetMount>> {
    let rows = sqlx::query(sql::LIST_ASSET_MOUNTS)
        .bind(tenant_id)
        .bind(asset_id)
        .fetch_all(pool)
        .await?;

    rows.iter().map(map_sqlx_mount).collect()
}

pub(crate) async fn load_enabled_asset_mounts_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    profile_id: Option<&str>,
) -> AppResult<Vec<AssetMount>> {
    let rows = sqlx::query(sql::LIST_ENABLED_ASSET_MOUNTS)
        .bind(tenant_id)
        .bind(profile_id)
        .fetch_all(pool)
        .await?;

    rows.iter().map(map_sqlx_mount).collect()
}

pub(crate) async fn delete_orphan_asset_mounts_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<()> {
    sqlx::query(sql::DELETE_ORPHAN_ASSET_MOUNTS)
        .bind(tenant_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub(crate) async fn set_asset_mount_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    asset_id: &str,
    profile_id: &str,
    enabled: bool,
    strategy: DeploymentStrategy,
) -> AppResult<AssetMount> {
    let now = Utc::now().to_rfc3339();
    let created_at = load_asset_mount_sqlx(pool, tenant_id, asset_id, profile_id)
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
    upsert_asset_mount_sqlx(pool, tenant_id, &mount).await?;
    Ok(mount)
}

pub(crate) async fn persist_verified_mount_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    state: &DeploymentState,
    strategy: DeploymentStrategy,
) -> AppResult<AssetMount> {
    let mut tx = pool.begin().await?;
    upsert_deployment_state_tx(&mut tx, tenant_id, state).await?;
    let mount = set_asset_mount_tx(
        &mut tx,
        tenant_id,
        &state.asset_id,
        &state.profile_id,
        true,
        strategy,
    )
    .await?;
    tx.commit().await?;
    Ok(mount)
}

pub(crate) async fn persist_verified_unmount_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    asset_id: &str,
    profile_id: &str,
    target_path: &str,
    strategy: DeploymentStrategy,
) -> AppResult<AssetMount> {
    let mut tx = pool.begin().await?;
    delete_deployment_state_tx(&mut tx, tenant_id, profile_id, asset_id, target_path).await?;
    let mount =
        set_asset_mount_tx(&mut tx, tenant_id, asset_id, profile_id, false, strategy).await?;
    tx.commit().await?;
    Ok(mount)
}

async fn load_asset_mount_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<Option<AssetMount>> {
    sqlx::query(sql::GET_ASSET_MOUNT)
        .bind(tenant_id)
        .bind(asset_id)
        .bind(profile_id)
        .fetch_optional(pool)
        .await?
        .map(|row| map_sqlx_mount(&row))
        .transpose()
}

async fn load_asset_mount_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<Option<AssetMount>> {
    sqlx::query(sql::GET_ASSET_MOUNT)
        .bind(tenant_id)
        .bind(asset_id)
        .bind(profile_id)
        .fetch_optional(&mut **tx)
        .await?
        .map(|row| map_sqlx_mount(&row))
        .transpose()
}

async fn set_asset_mount_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    asset_id: &str,
    profile_id: &str,
    enabled: bool,
    strategy: DeploymentStrategy,
) -> AppResult<AssetMount> {
    let now = Utc::now().to_rfc3339();
    let created_at = load_asset_mount_tx(tx, tenant_id, asset_id, profile_id)
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
    upsert_asset_mount_tx(tx, tenant_id, &mount).await?;
    Ok(mount)
}

async fn upsert_asset_mount_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    mount: &AssetMount,
) -> AppResult<()> {
    sqlx::query(sql::UPSERT_ASSET_MOUNT)
        .bind(tenant_id)
        .bind(&mount.asset_id)
        .bind(&mount.profile_id)
        .bind(if mount.enabled { 1_i64 } else { 0_i64 })
        .bind(encode_enum_app(mount.strategy)?)
        .bind(&mount.created_at)
        .bind(&mount.updated_at)
        .execute(pool)
        .await?;
    Ok(())
}

async fn upsert_asset_mount_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    mount: &AssetMount,
) -> AppResult<()> {
    sqlx::query(sql::UPSERT_ASSET_MOUNT)
        .bind(tenant_id)
        .bind(&mount.asset_id)
        .bind(&mount.profile_id)
        .bind(if mount.enabled { 1_i64 } else { 0_i64 })
        .bind(encode_enum_app(mount.strategy)?)
        .bind(&mount.created_at)
        .bind(&mount.updated_at)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn upsert_deployment_state_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    state: &DeploymentState,
) -> AppResult<()> {
    sqlx::query(sql::UPSERT_DEPLOYMENT_STATE)
        .bind(tenant_id)
        .bind(&state.profile_id)
        .bind(&state.asset_id)
        .bind(&state.target_path)
        .bind(encode_enum_app(state.strategy)?)
        .bind(&state.source_hash)
        .bind(&state.deployed_at)
        .bind(&state.managed_by)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn delete_deployment_state_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    profile_id: &str,
    asset_id: &str,
    target_path: &str,
) -> AppResult<()> {
    sqlx::query(sql::DELETE_DEPLOYMENT_STATE)
        .bind(tenant_id)
        .bind(profile_id)
        .bind(asset_id)
        .bind(target_path)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn map_sqlx_mount(row: &SqliteRow) -> AppResult<AssetMount> {
    Ok(AssetMount {
        asset_id: row.try_get(0)?,
        profile_id: row.try_get(1)?,
        enabled: row.try_get::<i64, _>(2)? == 1,
        strategy: decode_enum_app(row.try_get::<String, _>(3)?)?,
        created_at: row.try_get(4)?,
        updated_at: row.try_get(5)?,
    })
}

#[cfg(test)]
#[path = "mount_repo_tests.rs"]
mod tests;
