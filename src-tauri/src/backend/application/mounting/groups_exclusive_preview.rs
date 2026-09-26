use sqlx::SqlitePool;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::Path,
};

use super::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::catalog::{Asset, AssetKind, Source, SourceOrigin};
use crate::backend::domain::mounting::{
    AssetGroupDetail, AssetMount, DeploymentStrategy, PhysicalMountState,
    SkillGroupExclusiveMountItem, SkillGroupExclusiveMountPreview,
    SkillGroupExclusiveMountSkippedItem, TargetProfile,
};
use crate::backend::infrastructure::path_utils::expand_path;

pub(crate) async fn build_skill_group_exclusive_mount_preview_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    input: &SkillGroupExclusiveMountInput,
) -> AppResult<SkillGroupExclusiveMountPreview> {
    let profile_id = &input.profile_id;
    let requested_group_ids = input
        .group_ids
        .iter()
        .map(|group_id| group_id.trim())
        .filter(|group_id| !group_id.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    let profile = crate::backend::application::mounting::profile_ops::load_target_profile_sqlx(
        pool, tenant_id, profile_id,
    )
    .await?
    .ok_or_else(|| AppError::NotFound(format!("profile not found: {profile_id}")))?;
    let skill_assets =
        crate::backend::store::load_assets_sqlx(pool, tenant_id, Some(AssetKind::Skill)).await?;
    let sources = crate::backend::store::load_sources_sqlx(pool, tenant_id).await?;
    let enabled_mounts =
        crate::backend::store::load_enabled_asset_mounts_sqlx(pool, tenant_id, Some(profile_id))
            .await?;
    let group_details = crate::backend::store::load_skill_group_details_by_ids_sqlx(
        pool,
        tenant_id,
        &requested_group_ids,
        &skill_assets,
    )
    .await?;
    let managed_targets = crate::backend::store::load_managed_deployment_targets_by_profile_sqlx(
        pool, tenant_id, profile_id,
    )
    .await?;

    let group_details_by_id = group_details
        .into_iter()
        .map(|detail| (detail.group.id.clone(), detail))
        .collect::<HashMap<_, _>>();
    let mut managed_targets_by_asset = HashMap::<String, HashSet<String>>::new();
    for (asset_id, target_path) in managed_targets {
        managed_targets_by_asset
            .entry(asset_id)
            .or_default()
            .insert(target_path);
    }

    build_skill_group_exclusive_mount_preview_with_loaders(
        input,
        &profile,
        skill_assets,
        sources,
        enabled_mounts,
        move |group_id, _| {
            group_details_by_id
                .get(group_id)
                .cloned()
                .ok_or_else(|| AppError::NotFound(format!("asset group not found: {group_id}")))
        },
        move |asset_id, target_path| {
            Ok(managed_targets_by_asset
                .get(asset_id)
                .is_some_and(|targets| targets.contains(target_path)))
        },
    )
}

pub(super) fn build_skill_group_exclusive_mount_preview_with_loaders<LoadGroup, IsManaged>(
    input: &SkillGroupExclusiveMountInput,
    profile: &TargetProfile,
    skill_assets: Vec<Asset>,
    sources: Vec<Source>,
    enabled_mounts: Vec<AssetMount>,
    mut load_group: LoadGroup,
    mut is_managed_deployment: IsManaged,
) -> AppResult<SkillGroupExclusiveMountPreview>
where
    LoadGroup: FnMut(&str, &[Asset]) -> AppResult<AssetGroupDetail>,
    IsManaged: FnMut(&str, &str) -> AppResult<bool>,
{
    if !input.mount_selected {
        return Err(AppError::Validation(
            "exclusive skill group mount requires mount_selected=true".to_string(),
        ));
    }
    let _dry_run_requested = input.dry_run;
    validate_exclusive_skill_profile(profile)?;

    let skill_asset_by_id = skill_assets
        .iter()
        .map(|asset| (asset.id.clone(), asset.clone()))
        .collect::<BTreeMap<_, _>>();
    let source_by_id = sources
        .into_iter()
        .map(|source| (source.id.clone(), source))
        .collect::<HashMap<_, _>>();
    let enabled_mount_asset_ids = enabled_mounts
        .into_iter()
        .filter(|mount| mount.profile_id == profile.id && mount.enabled)
        .map(|mount| mount.asset_id)
        .collect::<BTreeSet<_>>();

    let mut group_ids = Vec::new();
    let mut selected_skill_ids = BTreeSet::new();
    let mut seen_group_ids = BTreeSet::new();
    for group_id in input
        .group_ids
        .iter()
        .map(|group_id| group_id.trim())
        .filter(|group_id| !group_id.is_empty())
    {
        if !seen_group_ids.insert(group_id.to_string()) {
            continue;
        }

        let detail = load_group(group_id, &skill_assets)?;
        if !detail.group.enabled {
            continue;
        }

        group_ids.push(detail.group.id.clone());
        for member in detail.members {
            if skill_asset_by_id.contains_key(&member.asset_id) {
                selected_skill_ids.insert(member.asset_id);
            }
        }
    }

    let mut keep = Vec::new();
    let mut mount = Vec::new();
    let mut unmount = Vec::new();
    let mut skipped = Vec::new();

    for asset_id in &selected_skill_ids {
        let Some(asset) = skill_asset_by_id.get(asset_id) else {
            continue;
        };
        let inspection =
            crate::backend::application::mounting::targeting::inspect_mount(profile, asset)?;
        match inspection.state {
            crate::backend::domain::PhysicalMountState::Mounted => keep.push(exclusive_item(asset)),
            crate::backend::domain::PhysicalMountState::NotMounted => {
                match validate_exclusive_mount_candidate(asset, profile, &source_by_id) {
                    Ok(()) => mount.push(exclusive_item(asset)),
                    Err(error) => skipped.push(exclusive_skipped_item(asset, error.view().message)),
                }
            }
            crate::backend::domain::PhysicalMountState::Conflict => {
                skipped.push(exclusive_skipped_item(
                    asset,
                    format!("target path is occupied: {}", inspection.target_path),
                ))
            }
            crate::backend::domain::PhysicalMountState::Broken => {
                skipped.push(exclusive_skipped_item(
                    asset,
                    format!("target symlink is broken: {}", inspection.target_path),
                ))
            }
        }
    }

    for asset in &skill_assets {
        if selected_skill_ids.contains(&asset.id) {
            continue;
        }

        let inspection =
            crate::backend::application::mounting::targeting::inspect_mount(profile, asset)?;
        match inspection.state {
            crate::backend::domain::PhysicalMountState::Mounted => {
                if is_managed_deployment(&asset.id, &inspection.target_path)? {
                    unmount.push(exclusive_item(asset));
                } else {
                    skipped.push(exclusive_skipped_item(
                        asset,
                        format!(
                            "target is mounted but not managed by AssetIWeave: {}",
                            inspection.target_path
                        ),
                    ));
                }
            }
            crate::backend::domain::PhysicalMountState::NotMounted => {
                if enabled_mount_asset_ids.contains(&asset.id) {
                    unmount.push(exclusive_item(asset));
                }
            }
            crate::backend::domain::PhysicalMountState::Conflict => {
                skipped.push(exclusive_skipped_item(
                    asset,
                    format!("target path is occupied: {}", inspection.target_path),
                ))
            }
            crate::backend::domain::PhysicalMountState::Broken => {
                skipped.push(exclusive_skipped_item(
                    asset,
                    format!("target symlink is broken: {}", inspection.target_path),
                ))
            }
        }
    }

    let selected_skill_ids = selected_skill_ids.into_iter().collect::<Vec<_>>();
    let keep_count = keep.len();
    let mount_count = mount.len();
    let unmount_count = unmount.len();
    let skipped_count = skipped.len();

    Ok(SkillGroupExclusiveMountPreview {
        profile_id: profile.id.clone(),
        group_ids,
        selected_skill_ids,
        keep,
        mount,
        unmount,
        skipped,
        keep_count,
        mount_count,
        unmount_count,
        skipped_count,
    })
}

fn validate_exclusive_skill_profile(profile: &TargetProfile) -> AppResult<()> {
    if !profile.enabled {
        return Err(AppError::Validation(format!(
            "profile is disabled: {}",
            profile.name
        )));
    }
    if !profile.supported_kinds.contains(&AssetKind::Skill)
        || !profile.include.kinds.contains(&AssetKind::Skill)
    {
        return Err(AppError::Validation(format!(
            "profile {} does not support skill assets",
            profile.name
        )));
    }
    if !matches!(
        profile.deployment_strategy,
        DeploymentStrategy::SymlinkToSource
    ) {
        return Err(AppError::Validation(
            "exclusive skill group mount only supports symlink_to_source profiles".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_exclusive_mount_candidate(
    asset: &Asset,
    _profile: &TargetProfile,
    source_by_id: &HashMap<String, Source>,
) -> AppResult<()> {
    let source = source_by_id
        .get(&asset.source_id)
        .ok_or_else(|| AppError::NotFound(format!("source not found: {}", asset.source_id)))?;
    if matches!(
        source.source_origin,
        SourceOrigin::AppTarget | SourceOrigin::AppLocal
    ) {
        return Err(AppError::Conflict(
            "app-local skills must be backed up before mounting".to_string(),
        ));
    }

    let source_path = expand_path(&asset.absolute_path)?;
    if !source_path.exists() {
        return Err(AppError::NotFound(format!(
            "source asset path does not exist: {}",
            source_path.display()
        )));
    }

    Ok(())
}

pub(crate) fn exclusive_item(asset: &Asset) -> SkillGroupExclusiveMountItem {
    SkillGroupExclusiveMountItem {
        asset_id: asset.id.clone(),
        name: asset.name.clone(),
    }
}

pub(super) fn exclusive_skipped_item(
    asset: &Asset,
    reason: String,
) -> SkillGroupExclusiveMountSkippedItem {
    SkillGroupExclusiveMountSkippedItem {
        asset_id: asset.id.clone(),
        name: asset.name.clone(),
        reason,
    }
}
