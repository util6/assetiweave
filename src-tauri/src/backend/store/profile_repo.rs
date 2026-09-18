use crate::backend::models::TargetProfile;
use crate::backend::path_utils::normalize_path_for_storage;
use crate::backend::runtime::AppResult;
use sqlx::SqlitePool;

use super::{
    codec::{decode_json_app, encode_json_app},
    sql,
};

pub(crate) async fn load_profiles_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<TargetProfile>> {
    let payloads = sqlx::query_scalar::<_, String>(sql::LIST_PROFILES)
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;
    payloads
        .into_iter()
        .map(|payload| {
            let profile = decode_json_app(payload)?;
            normalize_profile_paths(profile)
        })
        .collect()
}

pub(crate) async fn load_profile_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    profile_id: &str,
) -> AppResult<Option<TargetProfile>> {
    sqlx::query_scalar::<_, String>(sql::LOAD_PROFILE)
        .bind(tenant_id)
        .bind(profile_id)
        .fetch_optional(pool)
        .await?
        .map(|payload| {
            let profile = decode_json_app(payload)?;
            normalize_profile_paths(profile)
        })
        .transpose()
}

pub(crate) async fn upsert_profile_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    profile: &TargetProfile,
) -> AppResult<()> {
    let profile = normalize_profile_paths(profile.clone())?;
    sqlx::query(sql::UPSERT_PROFILE)
        .bind(tenant_id)
        .bind(&profile.id)
        .bind(encode_json_app(&profile)?)
        .execute(pool)
        .await?;
    Ok(())
}

fn normalize_profile_paths(mut profile: TargetProfile) -> AppResult<TargetProfile> {
    if profile.target_provider_id.trim().is_empty() {
        profile.target_provider_id = profile
            .app_kind
            .map(|kind| format!("{kind:?}").to_lowercase())
            .unwrap_or_else(|| profile.id.clone());
    }
    profile.target_paths = profile
        .target_paths
        .iter()
        .map(|path| normalize_path_for_storage(path))
        .collect::<AppResult<Vec<_>>>()?;
    Ok(profile)
}

pub(crate) async fn delete_profile_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    profile_id: &str,
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(sql::DELETE_APP_SHORTCUT_BY_PROFILE)
        .bind(tenant_id)
        .bind(profile_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(sql::DELETE_ASSET_MOUNT_OBSERVATIONS_BY_PROFILE)
        .bind(tenant_id)
        .bind(profile_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(sql::DELETE_ASSET_MOUNTS_BY_PROFILE)
        .bind(tenant_id)
        .bind(profile_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(sql::DELETE_PROFILE)
        .bind(tenant_id)
        .bind(profile_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
#[path = "profile_repo_tests.rs"]
mod tests;
