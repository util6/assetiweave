use crate::backend::models::DeploymentState;
use crate::backend::runtime::AppResult;
use sqlx::{Row, SqlitePool};

use super::{codec::encode_enum_app, sql};

pub(crate) async fn upsert_deployment_state_sqlx(
    pool: &SqlitePool,
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
        .execute(pool)
        .await?;
    Ok(())
}

pub(crate) async fn is_managed_deployment_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    profile_id: &str,
    asset_id: &str,
    target_path: &str,
) -> AppResult<bool> {
    let managed_by: Option<String> = sqlx::query_scalar(sql::GET_MANAGED_DEPLOYMENT)
        .bind(tenant_id)
        .bind(profile_id)
        .bind(asset_id)
        .bind(target_path)
        .fetch_optional(pool)
        .await?;
    Ok(managed_by.as_deref() == Some("assetiweave"))
}

pub(crate) async fn count_deployment_state_by_profile_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    profile_id: &str,
) -> AppResult<usize> {
    let count: i64 = sqlx::query_scalar(sql::COUNT_DEPLOYMENT_STATE_BY_PROFILE)
        .bind(tenant_id)
        .bind(profile_id)
        .fetch_one(pool)
        .await?;
    Ok(count as usize)
}

pub(crate) async fn load_managed_deployment_targets_by_profile_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    profile_id: &str,
) -> AppResult<Vec<(String, String)>> {
    let rows = sqlx::query(sql::LIST_MANAGED_DEPLOYMENT_TARGETS_BY_PROFILE)
        .bind(tenant_id)
        .bind(profile_id)
        .fetch_all(pool)
        .await?;
    rows.into_iter()
        .map(|row| Ok((row.try_get(0)?, row.try_get(1)?)))
        .collect()
}

pub(crate) async fn delete_orphan_deployment_state_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<()> {
    sqlx::query(sql::DELETE_ORPHAN_DEPLOYMENT_STATE)
        .bind(tenant_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
#[path = "deployment_repo_tests.rs"]
mod tests;
