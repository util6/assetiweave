use chrono::Utc;
use sqlx::SqlitePool;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub(crate) use super::mount_symlinks::*;
use crate::backend::application::catalog::catalog_ops::*;
use crate::backend::application::AppError as RuntimeAppError;
use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::catalog::{Asset, AssetKind, Source, SourceOrigin};
use crate::backend::domain::mounting::{
    AssetMount, AssetMountUpdateResult, DeploymentState, DeploymentStrategy, PhysicalMountState,
    TargetProfile,
};

pub(crate) async fn set_asset_mount_record(
    pool: &SqlitePool,
    tenant_id: &str,
    asset_id: &str,
    profile_id: &str,
    enabled: bool,
    strategy: Option<DeploymentStrategy>,
) -> AppResult<AssetMount> {
    if enabled {
        return mount_asset_mount_record(pool, tenant_id, asset_id, profile_id)
            .await
            .map(|result| result.mount);
    }

    let (asset, source, profile) =
        load_mount_target_sqlx(pool, tenant_id, asset_id, profile_id).await?;
    let default_strategy = validate_mount_target(&source, &profile)?;
    let inspection =
        crate::backend::application::mounting::targeting::inspect_mount(&profile, &asset)?;
    if matches!(
        inspection.state,
        crate::backend::domain::PhysicalMountState::Mounted
    ) {
        return unmount_asset_mount_record(pool, tenant_id, asset_id, profile_id)
            .await
            .map(|result| result.mount);
    }

    let strategy_to_save = strategy.unwrap_or(default_strategy);
    let result = crate::backend::store::set_asset_mount_sqlx(
        pool,
        tenant_id,
        asset_id,
        profile_id,
        enabled,
        strategy_to_save,
    )
    .await;
    match &result {
        Ok(_) => {
            tracing::info!(
                action = "skill.mount.preference",
                asset_id = %asset_id,
                profile_id = %profile_id,
                enabled = %enabled,
                "更新 skill 挂载关系成功"
            );
        }
        Err(error) => {
            tracing::error!(
                action = "skill.mount.preference",
                asset_id = %asset_id,
                profile_id = %profile_id,
                enabled = %enabled,
                error = %error,
                "更新 skill 挂载关系失败"
            );
        }
    }
    Ok(result?)
}

fn validate_mount_target(
    source: &Source,
    profile: &TargetProfile,
) -> AppResult<DeploymentStrategy> {
    if matches!(
        source.source_origin,
        SourceOrigin::AppTarget | SourceOrigin::AppLocal
    ) {
        return Err(AppError::Validation(
            "app-local skills must be backed up before mounting".to_string(),
        ));
    }

    Ok(profile.deployment_strategy)
}

pub(crate) async fn mount_asset_mount_record(
    pool: &SqlitePool,
    tenant_id: &str,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<AssetMountUpdateResult> {
    let (asset, source, profile) =
        load_mount_target_sqlx(pool, tenant_id, asset_id, profile_id).await?;
    mount_preloaded_asset_mount_record(pool, tenant_id, &asset, &source, &profile).await
}

pub(crate) async fn mount_preloaded_asset_mount_record(
    pool: &SqlitePool,
    tenant_id: &str,
    asset: &Asset,
    source: &Source,
    profile: &TargetProfile,
) -> AppResult<AssetMountUpdateResult> {
    let asset_id = asset.id.as_str();
    let profile_id = profile.id.as_str();
    let result = (async {
        let strategy = validate_mount_target(source, profile)?;
        if !matches!(strategy, DeploymentStrategy::SymlinkToSource) {
            return Err(AppError::Validation(
                "immediate mount only supports symlink_to_source profiles".to_string(),
            ));
        }
        validate_immediate_mount_support(asset, profile)?;

        let inspection =
            crate::backend::application::mounting::targeting::inspect_mount(profile, asset)?;
        match inspection.state {
            crate::backend::domain::PhysicalMountState::Mounted => {
                let inspection = repair_mounted_symlink_to_real_source(asset, profile, inspection)?;
                let mount = persist_verified_mount(
                    pool,
                    tenant_id,
                    asset,
                    profile,
                    &inspection.target_path,
                    strategy,
                )
                .await?;
                return Ok(AssetMountUpdateResult {
                    mount,
                    status: asset_mount_status(&asset.id, &profile.id, inspection),
                });
            }
            crate::backend::domain::PhysicalMountState::NotMounted => {}
            crate::backend::domain::PhysicalMountState::Conflict
            | crate::backend::domain::PhysicalMountState::Broken => {
                return Err(AppError::Conflict(format!(
                    "target is not available for mounting: {}",
                    inspection.target_path
                )));
            }
        }

        let target_path = PathBuf::from(&inspection.target_path);
        create_mount_symlink(asset, profile, &target_path)?;
        let inspection =
            crate::backend::application::mounting::targeting::inspect_mount(profile, asset)?;
        if !matches!(
            inspection.state,
            crate::backend::domain::PhysicalMountState::Mounted
        ) {
            remove_created_mount_symlink(&target_path).ok();
            return Err(AppError::Conflict(format!(
                "mount verification failed for {asset_id} on {profile_id}: {}",
                inspection.target_path
            )));
        }

        let mount = match persist_verified_mount(
            pool,
            tenant_id,
            asset,
            profile,
            &inspection.target_path,
            strategy,
        )
        .await
        {
            Ok(mount) => mount,
            Err(error) => {
                remove_created_mount_symlink(&target_path).ok();
                return Err(error);
            }
        };
        Ok(AssetMountUpdateResult {
            mount,
            status: asset_mount_status(&asset.id, &profile.id, inspection),
        })
    })
    .await;

    match &result {
        Ok(update) => {
            tracing::info!(
                action = "skill.mount.success",
                asset_id = %asset.id,
                skill_name = %asset.name,
                source_id = %asset.source_id,
                profile_id = %profile.id,
                profile_name = %profile.name,
                target_path = %update.status.target_path,
                state = ?update.status.state,
                "skill 挂载成功"
            );
        }
        Err(error) => {
            tracing::error!(
                action = "skill.mount.error",
                asset_id = %asset.id,
                skill_name = %asset.name,
                source_id = %asset.source_id,
                profile_id = %profile.id,
                profile_name = %profile.name,
                error = %error,
                "skill 挂载失败"
            );
        }
    }
    result
}

pub(crate) async fn unmount_asset_mount_record(
    pool: &SqlitePool,
    tenant_id: &str,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<AssetMountUpdateResult> {
    let (asset, profile) =
        load_mount_asset_and_profile_sqlx(pool, tenant_id, asset_id, profile_id).await?;
    unmount_preloaded_asset_mount_record(pool, tenant_id, &asset, &profile).await
}

pub(crate) async fn unmount_preloaded_asset_mount_record(
    pool: &SqlitePool,
    tenant_id: &str,
    asset: &Asset,
    profile: &TargetProfile,
) -> AppResult<AssetMountUpdateResult> {
    let asset_id = asset.id.as_str();
    let profile_id = profile.id.as_str();
    let result = (async {
        let inspection =
            crate::backend::application::mounting::targeting::inspect_mount(profile, asset)?;
        let target_path = PathBuf::from(&inspection.target_path);
        let removed_link = matches!(
            inspection.state,
            crate::backend::domain::PhysicalMountState::Mounted
        );

        match inspection.state {
            crate::backend::domain::PhysicalMountState::Mounted => {
                remove_mounted_symlink(&inspection.target_path)?
            }
            crate::backend::domain::PhysicalMountState::NotMounted => {}
            crate::backend::domain::PhysicalMountState::Conflict
            | crate::backend::domain::PhysicalMountState::Broken => {
                return Err(AppError::Conflict(format!(
                    "target is not a symlink to this asset: {}",
                    inspection.target_path
                )));
            }
        }

        let inspection =
            crate::backend::application::mounting::targeting::inspect_mount(profile, asset)?;
        if !matches!(
            inspection.state,
            crate::backend::domain::PhysicalMountState::NotMounted
        ) {
            return Err(AppError::Conflict(format!(
                "unmount verification failed for {asset_id} on {profile_id}: {}",
                inspection.target_path
            )));
        }

        match persist_verified_unmount(pool, tenant_id, asset, profile, &inspection.target_path)
            .await
        {
            Ok(mount) => Ok(AssetMountUpdateResult {
                mount,
                status: asset_mount_status(&asset.id, &profile.id, inspection),
            }),
            Err(error) => {
                if removed_link {
                    create_mount_symlink(asset, profile, &target_path).ok();
                }
                Err(error)
            }
        }
    })
    .await;

    match &result {
        Ok(update) => {
            tracing::info!(
                action = "skill.unmount.success",
                asset_id = %asset.id,
                skill_name = %asset.name,
                source_id = %asset.source_id,
                profile_id = %profile.id,
                profile_name = %profile.name,
                target_path = %update.status.target_path,
                state = ?update.status.state,
                "skill 卸载成功"
            );
        }
        Err(error) => {
            tracing::error!(
                action = "skill.unmount.error",
                asset_id = %asset.id,
                skill_name = %asset.name,
                source_id = %asset.source_id,
                profile_id = %profile.id,
                profile_name = %profile.name,
                error = %error,
                "skill 卸载失败"
            );
        }
    }
    result
}

pub(crate) async fn load_mount_asset_and_profile_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<(Asset, TargetProfile)> {
    let asset = crate::backend::store::load_asset_sqlx(pool, tenant_id, asset_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("asset not found: {asset_id}")))?;
    let profile = crate::backend::application::mounting::profile_ops::load_target_profile_sqlx(
        pool, tenant_id, profile_id,
    )
    .await?
    .ok_or_else(|| AppError::NotFound(format!("profile not found: {profile_id}")))?;
    Ok((asset, profile))
}

pub(crate) async fn load_batch_mount_inputs_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    profile_id: &str,
) -> AppResult<(Vec<Asset>, Vec<Source>, TargetProfile)> {
    let assets = crate::backend::store::load_assets_sqlx(pool, tenant_id, None).await?;
    let sources = crate::backend::store::load_sources_sqlx(pool, tenant_id).await?;
    let profile = crate::backend::application::mounting::profile_ops::load_target_profile_sqlx(
        pool, tenant_id, profile_id,
    )
    .await?
    .ok_or_else(|| AppError::NotFound(format!("profile not found: {profile_id}")))?;
    Ok((assets, sources, profile))
}

async fn load_mount_target_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<(Asset, Source, TargetProfile)> {
    let asset = crate::backend::store::load_asset_sqlx(pool, tenant_id, asset_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("asset not found: {asset_id}")))?;
    let source = crate::backend::store::load_source_sqlx(pool, tenant_id, &asset.source_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("source not found: {}", asset.source_id)))?;
    let profile = crate::backend::application::mounting::profile_ops::load_target_profile_sqlx(
        pool, tenant_id, profile_id,
    )
    .await?
    .ok_or_else(|| AppError::NotFound(format!("profile not found: {profile_id}")))?;
    Ok((asset, source, profile))
}

fn validate_immediate_mount_support(asset: &Asset, profile: &TargetProfile) -> AppResult<()> {
    if !profile.enabled {
        return Err(AppError::Validation(format!(
            "profile is disabled: {}",
            profile.name
        )));
    }
    if matches!(asset.kind, AssetKind::Unclassified)
        || !profile.supported_kinds.contains(&asset.kind)
        || !profile.include.kinds.contains(&asset.kind)
    {
        return Err(AppError::Validation(format!(
            "profile {} does not support {:?}",
            profile.name, asset.kind
        )));
    }

    Ok(())
}

async fn persist_verified_mount(
    pool: &SqlitePool,
    tenant_id: &str,
    asset: &Asset,
    profile: &TargetProfile,
    target_path: &str,
    strategy: DeploymentStrategy,
) -> AppResult<AssetMount> {
    let state = DeploymentState {
        profile_id: profile.id.clone(),
        asset_id: asset.id.clone(),
        target_path: target_path.to_string(),
        strategy,
        source_hash: asset.content_hash.clone().unwrap_or_default(),
        deployed_at: Utc::now().to_rfc3339(),
        managed_by: "assetiweave".to_string(),
    };
    Ok(
        crate::backend::store::persist_verified_mount_sqlx(pool, tenant_id, &state, strategy)
            .await?,
    )
}

async fn persist_verified_unmount(
    pool: &SqlitePool,
    tenant_id: &str,
    asset: &Asset,
    profile: &TargetProfile,
    target_path: &str,
) -> AppResult<AssetMount> {
    Ok(crate::backend::store::persist_verified_unmount_sqlx(
        pool,
        tenant_id,
        &asset.id,
        &profile.id,
        target_path,
        profile.deployment_strategy,
    )
    .await?)
}
