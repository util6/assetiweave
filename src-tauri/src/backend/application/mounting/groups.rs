use chrono::Utc;
use sqlx::SqlitePool;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

use super::groups_exclusive_preview::*;
use super::mount_ops::*;
use super::*;
use crate::backend::application::catalog::catalog_ops::*;
use crate::backend::application::AppError as RuntimeAppError;
use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::catalog::AssetKind;
use crate::backend::domain::mounting::{
    ApplyAssetGroupMountResult, ApplySkillGroupExclusiveMountResult, AssetGroup,
    AssetGroupMountError, AssetGroupRules, PhysicalMountState, SkillGroupExclusiveMountError,
};

pub(crate) async fn apply_skill_group_mount_record(
    pool: &SqlitePool,
    tenant_id: &str,
    group_id: &str,
    profile_id: &str,
    enabled: bool,
) -> AppResult<ApplyAssetGroupMountResult> {
    apply_skill_group_mount_record_with_progress(
        pool,
        tenant_id,
        group_id,
        profile_id,
        enabled,
        |_, _, _| Ok(()),
    )
    .await
}

pub(crate) async fn apply_skill_group_mount_record_with_progress<BeforeItem>(
    pool: &SqlitePool,
    tenant_id: &str,
    group_id: &str,
    profile_id: &str,
    enabled: bool,
    mut before_item: BeforeItem,
) -> AppResult<ApplyAssetGroupMountResult>
where
    BeforeItem: FnMut(usize, usize, &str) -> AppResult<()>,
{
    let assets =
        crate::backend::store::load_assets_sqlx(pool, tenant_id, Some(AssetKind::Skill)).await?;
    let detail =
        crate::backend::store::load_skill_group_detail_sqlx(pool, tenant_id, group_id, &assets)
            .await?;
    let sources = crate::backend::store::load_sources_sqlx(pool, tenant_id).await?;
    let profile = crate::backend::application::mounting::profile_ops::load_target_profile_sqlx(
        pool, tenant_id, profile_id,
    )
    .await?
    .ok_or_else(|| AppError::NotFound(format!("profile not found: {profile_id}")))?;

    if !detail.group.enabled {
        return Err(AppError::Validation(format!(
            "asset group is disabled: {}",
            detail.group.name
        )));
    }

    let mut mounts = Vec::new();
    let mut statuses = Vec::new();
    let mut errors = Vec::new();
    let asset_by_id = assets
        .iter()
        .map(|asset| (asset.id.as_str(), asset))
        .collect::<HashMap<_, _>>();
    let source_by_id = sources
        .iter()
        .map(|source| (source.id.as_str(), source))
        .collect::<HashMap<_, _>>();
    let total = detail.members.len();
    for (index, member) in detail.members.iter().enumerate() {
        before_item(index, total, &member.asset_id)?;
        let result = match asset_by_id.get(member.asset_id.as_str()) {
            Some(asset) if enabled => match source_by_id.get(asset.source_id.as_str()) {
                Some(source) => {
                    mount_preloaded_asset_mount_record(pool, tenant_id, asset, source, &profile)
                        .await
                }
                None => Err(AppError::NotFound(format!(
                    "source not found: {}",
                    asset.source_id
                ))),
            },
            Some(asset) => {
                unmount_preloaded_asset_mount_record(pool, tenant_id, asset, &profile).await
            }
            None => Err(AppError::NotFound(format!(
                "asset not found: {}",
                member.asset_id
            ))),
        };

        match result {
            Ok(update) => {
                mounts.push(update.mount);
                statuses.push(update.status);
            }
            Err(message) => errors.push(AssetGroupMountError {
                asset_id: member.asset_id.clone(),
                message: message.to_string(),
            }),
        }
    }

    Ok(ApplyAssetGroupMountResult {
        group_id: group_id.to_string(),
        profile_id: profile_id.to_string(),
        enabled,
        requested_count: detail.members.len(),
        updated_count: mounts.len(),
        error_count: errors.len(),
        mounts,
        statuses,
        errors,
    })
}

pub(crate) async fn apply_skill_group_exclusive_mount_record(
    pool: &SqlitePool,
    tenant_id: &str,
    input: &SkillGroupExclusiveMountInput,
) -> AppResult<ApplySkillGroupExclusiveMountResult> {
    apply_skill_group_exclusive_mount_record_with_progress(pool, tenant_id, input, |_, _, _| Ok(()))
        .await
}

pub(crate) async fn apply_skill_group_exclusive_mount_record_with_progress<BeforeItem>(
    pool: &SqlitePool,
    tenant_id: &str,
    input: &SkillGroupExclusiveMountInput,
    mut before_item: BeforeItem,
) -> AppResult<ApplySkillGroupExclusiveMountResult>
where
    BeforeItem: FnMut(usize, usize, &str) -> AppResult<()>,
{
    let preview = build_skill_group_exclusive_mount_preview_sqlx(pool, tenant_id, input).await?;
    let profile_id = &preview.profile_id;
    let assets =
        crate::backend::store::load_assets_sqlx(pool, tenant_id, Some(AssetKind::Skill)).await?;
    let sources = crate::backend::store::load_sources_sqlx(pool, tenant_id).await?;
    let profile = crate::backend::application::mounting::profile_ops::load_target_profile_sqlx(
        pool, tenant_id, profile_id,
    )
    .await?
    .ok_or_else(|| AppError::NotFound(format!("profile not found: {profile_id}")))?;
    let managed_targets = crate::backend::store::load_managed_deployment_targets_by_profile_sqlx(
        pool, tenant_id, profile_id,
    )
    .await?;
    let asset_by_id = assets
        .iter()
        .map(|asset| (asset.id.as_str(), asset))
        .collect::<HashMap<_, _>>();
    let source_by_id = sources
        .iter()
        .map(|source| (source.id.as_str(), source))
        .collect::<HashMap<_, _>>();
    let managed_targets = managed_targets
        .into_iter()
        .collect::<HashSet<(String, String)>>();
    let mut statuses = Vec::new();
    let mut errors = Vec::new();

    for item in &preview.keep {
        if let Some(asset) = asset_by_id.get(item.asset_id.as_str()) {
            let inspection =
                crate::backend::application::mounting::targeting::inspect_mount(&profile, asset)?;
            statuses.push(asset_mount_status(&asset.id, &profile.id, inspection));
        }
    }

    let total_changes = preview.mount.len() + preview.unmount.len();
    let mut change_index = 0;
    for item in &preview.mount {
        before_item(change_index, total_changes, &item.asset_id)?;
        change_index += 1;
        let result = match asset_by_id.get(item.asset_id.as_str()) {
            Some(asset) => match source_by_id.get(asset.source_id.as_str()) {
                Some(source) => {
                    mount_preloaded_asset_mount_record(pool, tenant_id, asset, source, &profile)
                        .await
                }
                None => Err(AppError::NotFound(format!(
                    "source not found: {}",
                    asset.source_id
                ))),
            },
            None => Err(AppError::NotFound(format!(
                "asset not found: {}",
                item.asset_id
            ))),
        };
        match result {
            Ok(update) => statuses.push(update.status),
            Err(message) => errors.push(SkillGroupExclusiveMountError {
                asset_id: item.asset_id.clone(),
                name: item.name.clone(),
                message: message.to_string(),
            }),
        }
    }

    for item in &preview.unmount {
        before_item(change_index, total_changes, &item.asset_id)?;
        change_index += 1;
        let result = match asset_by_id.get(item.asset_id.as_str()) {
            Some(asset) => {
                let inspection = crate::backend::application::mounting::targeting::inspect_mount(
                    &profile, asset,
                )?;
                match inspection.state {
                    crate::backend::domain::PhysicalMountState::Mounted
                        if !managed_targets
                            .contains(&(asset.id.clone(), inspection.target_path.clone())) =>
                    {
                        Err(AppError::Conflict(format!(
                            "target is mounted but not managed by AssetIWeave: {}",
                            inspection.target_path
                        )))
                    }
                    crate::backend::domain::PhysicalMountState::Conflict
                    | crate::backend::domain::PhysicalMountState::Broken => {
                        Err(AppError::Conflict(format!(
                            "target is not a managed mount for this asset: {}",
                            inspection.target_path
                        )))
                    }
                    _ => {
                        unmount_preloaded_asset_mount_record(pool, tenant_id, asset, &profile).await
                    }
                }
            }
            None => Err(AppError::NotFound(format!(
                "asset not found: {}",
                item.asset_id
            ))),
        };
        match result {
            Ok(update) => statuses.push(update.status),
            Err(message) => errors.push(SkillGroupExclusiveMountError {
                asset_id: item.asset_id.clone(),
                name: item.name.clone(),
                message: message.to_string(),
            }),
        }
    }

    Ok(ApplySkillGroupExclusiveMountResult {
        preview,
        statuses,
        errors,
    })
}

pub(crate) use super::groups_exclusive_preview::*;

pub(crate) fn asset_group_from_input(
    input: AssetGroupInput,
    created_at: String,
    updated_at: String,
) -> AssetGroup {
    AssetGroup {
        id: input.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: input.name,
        description: input.description,
        color: input.color.unwrap_or_else(|| "#10b981".to_string()),
        asset_kind: AssetKind::Skill,
        display_icon: input.display_icon,
        icon_svg: input.icon_svg,
        enabled: input.enabled.unwrap_or(true),
        sort_order: input.sort_order.unwrap_or(0),
        rules: input.rules.unwrap_or(AssetGroupRules {
            source_ids: vec![],
            relative_path_globs: vec![],
            name_contains: None,
        }),
        created_at,
        updated_at,
    }
}

#[cfg(test)]
#[path = "groups_tests.rs"]
mod tests;
