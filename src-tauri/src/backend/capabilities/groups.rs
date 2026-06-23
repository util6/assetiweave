use super::prelude::*;

pub(crate) fn apply_skill_group_mount_record(
    db: &crate::backend::store::Database,
    group_id: &str,
    profile_id: &str,
    enabled: bool,
) -> AppResult<ApplyAssetGroupMountResult> {
    let pool = db.pool().clone();
    let group_id_to_load = group_id.to_string();
    let detail = db.block_on(async move {
        let assets = crate::backend::store::load_assets_sqlx(&pool, Some(AssetKind::Skill)).await?;
        crate::backend::store::load_skill_group_detail_sqlx(&pool, &group_id_to_load, &assets).await
    })?;
    if !detail.group.enabled {
        return Err(format!("asset group is disabled: {}", detail.group.name));
    }

    let mut mounts = Vec::new();
    let mut statuses = Vec::new();
    let mut errors = Vec::new();
    for member in &detail.members {
        let result = if enabled {
            mount_asset_mount_record(db, &member.asset_id, profile_id)
        } else {
            unmount_asset_mount_record(db, &member.asset_id, profile_id)
        };

        match result {
            Ok(update) => {
                mounts.push(update.mount);
                statuses.push(update.status);
            }
            Err(message) => errors.push(AssetGroupMountError {
                asset_id: member.asset_id.clone(),
                message,
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
pub(crate) fn apply_skill_group_exclusive_mount_record(
    db: &crate::backend::store::Database,
    input: &SkillGroupExclusiveMountInput,
) -> AppResult<ApplySkillGroupExclusiveMountResult> {
    let preview = build_skill_group_exclusive_mount_preview_sqlx(db, input)?;
    let pool = db.pool().clone();
    let profile_id = preview.profile_id.clone();
    let (assets, profile) = db.block_on(async move {
        let assets = crate::backend::store::load_assets_sqlx(&pool, Some(AssetKind::Skill)).await?;
        let profile = crate::backend::store::load_profile_sqlx(&pool, &profile_id)
            .await?
            .ok_or_else(|| format!("profile not found: {profile_id}"))?;
        AppResult::Ok((assets, profile))
    })?;
    let asset_by_id = assets
        .iter()
        .map(|asset| (asset.id.as_str(), asset))
        .collect::<HashMap<_, _>>();
    let mut statuses = Vec::new();
    let mut errors = Vec::new();

    for item in &preview.keep {
        if let Some(asset) = asset_by_id.get(item.asset_id.as_str()) {
            let inspection = crate::backend::targeting::inspect_mount(&profile, asset)?;
            statuses.push(asset_mount_status(&asset.id, &profile.id, inspection));
        }
    }

    for item in &preview.mount {
        match mount_asset_mount_record(db, &item.asset_id, &preview.profile_id) {
            Ok(update) => statuses.push(update.status),
            Err(message) => errors.push(SkillGroupExclusiveMountError {
                asset_id: item.asset_id.clone(),
                name: item.name.clone(),
                message,
            }),
        }
    }

    for item in &preview.unmount {
        match unmount_exclusive_skill_mount_record(db, &item.asset_id, &preview.profile_id) {
            Ok(update) => statuses.push(update.status),
            Err(message) => errors.push(SkillGroupExclusiveMountError {
                asset_id: item.asset_id.clone(),
                name: item.name.clone(),
                message,
            }),
        }
    }

    Ok(ApplySkillGroupExclusiveMountResult {
        preview,
        statuses,
        errors,
    })
}

pub(crate) fn build_skill_group_exclusive_mount_preview_sqlx(
    db: &crate::backend::store::Database,
    input: &SkillGroupExclusiveMountInput,
) -> AppResult<SkillGroupExclusiveMountPreview> {
    let pool = db.pool().clone();
    let profile_id = input.profile_id.clone();
    let requested_group_ids = input
        .group_ids
        .iter()
        .map(|group_id| group_id.trim())
        .filter(|group_id| !group_id.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    let (profile, skill_assets, sources, enabled_mounts, group_details, managed_targets) = db
        .block_on(async move {
            let profile = crate::backend::store::load_profile_sqlx(&pool, &profile_id)
                .await?
                .ok_or_else(|| format!("profile not found: {profile_id}"))?;
            let skill_assets =
                crate::backend::store::load_assets_sqlx(&pool, Some(AssetKind::Skill)).await?;
            let sources = crate::backend::store::load_sources_sqlx(&pool).await?;
            let enabled_mounts =
                crate::backend::store::load_enabled_asset_mounts_sqlx(&pool, Some(&profile_id))
                    .await?;
            let group_details = crate::backend::store::load_skill_group_details_by_ids_sqlx(
                &pool,
                &requested_group_ids,
                &skill_assets,
            )
            .await?;
            let managed_targets =
                crate::backend::store::load_managed_deployment_targets_by_profile_sqlx(
                    &pool,
                    &profile_id,
                )
                .await?;
            AppResult::Ok((
                profile,
                skill_assets,
                sources,
                enabled_mounts,
                group_details,
                managed_targets,
            ))
        })?;
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
                .ok_or_else(|| format!("asset group not found: {group_id}"))
        },
        move |asset_id, target_path| {
            Ok(managed_targets_by_asset
                .get(asset_id)
                .is_some_and(|targets| targets.contains(target_path)))
        },
    )
}

fn build_skill_group_exclusive_mount_preview_with_loaders<LoadGroup, IsManaged>(
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
        return Err("exclusive skill group mount requires mount_selected=true".to_string());
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
        let inspection = crate::backend::targeting::inspect_mount(profile, asset)?;
        match inspection.state {
            crate::backend::targeting::PhysicalMountState::Mounted => {
                keep.push(exclusive_item(asset))
            }
            crate::backend::targeting::PhysicalMountState::NotMounted => {
                match validate_exclusive_mount_candidate(asset, profile, &source_by_id) {
                    Ok(()) => mount.push(exclusive_item(asset)),
                    Err(reason) => skipped.push(exclusive_skipped_item(asset, reason)),
                }
            }
            crate::backend::targeting::PhysicalMountState::Conflict => {
                skipped.push(exclusive_skipped_item(
                    asset,
                    format!("target path is occupied: {}", inspection.target_path),
                ))
            }
            crate::backend::targeting::PhysicalMountState::Broken => {
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

        let inspection = crate::backend::targeting::inspect_mount(profile, asset)?;
        match inspection.state {
            crate::backend::targeting::PhysicalMountState::Mounted => {
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
            crate::backend::targeting::PhysicalMountState::NotMounted => {
                if enabled_mount_asset_ids.contains(&asset.id) {
                    unmount.push(exclusive_item(asset));
                }
            }
            crate::backend::targeting::PhysicalMountState::Conflict => {
                skipped.push(exclusive_skipped_item(
                    asset,
                    format!("target path is occupied: {}", inspection.target_path),
                ))
            }
            crate::backend::targeting::PhysicalMountState::Broken => {
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
        return Err(format!("profile is disabled: {}", profile.name));
    }
    if !profile.supported_kinds.contains(&AssetKind::Skill)
        || !profile.include.kinds.contains(&AssetKind::Skill)
    {
        return Err(format!(
            "profile {} does not support skill assets",
            profile.name
        ));
    }
    if !matches!(
        profile.deployment_strategy,
        DeploymentStrategy::SymlinkToSource
    ) {
        return Err(
            "exclusive skill group mount only supports symlink_to_source profiles".to_string(),
        );
    }
    Ok(())
}

fn validate_exclusive_mount_candidate(
    asset: &Asset,
    _profile: &TargetProfile,
    source_by_id: &HashMap<String, Source>,
) -> Result<(), String> {
    let source = source_by_id
        .get(&asset.source_id)
        .ok_or_else(|| format!("source not found: {}", asset.source_id))?;
    if matches!(
        source.source_origin,
        SourceOrigin::AppTarget | SourceOrigin::AppLocal
    ) {
        return Err("app-local skills must be backed up before mounting".to_string());
    }

    let source_path = expand_path(&asset.absolute_path)?;
    if !source_path.exists() {
        return Err(format!(
            "source asset path does not exist: {}",
            source_path.display()
        ));
    }

    Ok(())
}

fn unmount_exclusive_skill_mount_record(
    db: &crate::backend::store::Database,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<AssetMountUpdateResult> {
    let (asset, profile) = load_mount_asset_and_profile_sqlx(db, asset_id, profile_id)?;
    let inspection = crate::backend::targeting::inspect_mount(&profile, &asset)?;
    match inspection.state {
        crate::backend::targeting::PhysicalMountState::Mounted => {
            let pool = db.pool().clone();
            let profile_id = profile.id.clone();
            let asset_id = asset.id.clone();
            let target_path = inspection.target_path.clone();
            let is_managed = db.block_on(async move {
                crate::backend::store::is_managed_deployment_sqlx(
                    &pool,
                    &profile_id,
                    &asset_id,
                    &target_path,
                )
                .await
            })?;
            if !is_managed {
                return Err(format!(
                    "target is mounted but not managed by AssetIWeave: {}",
                    inspection.target_path
                ));
            }
        }
        crate::backend::targeting::PhysicalMountState::NotMounted => {}
        crate::backend::targeting::PhysicalMountState::Conflict
        | crate::backend::targeting::PhysicalMountState::Broken => {
            return Err(format!(
                "target is not a managed mount for this asset: {}",
                inspection.target_path
            ));
        }
    }

    unmount_asset_mount_record(db, asset_id, profile_id)
}

pub(crate) fn exclusive_item(asset: &Asset) -> SkillGroupExclusiveMountItem {
    SkillGroupExclusiveMountItem {
        asset_id: asset.id.clone(),
        name: asset.name.clone(),
    }
}

fn exclusive_skipped_item(asset: &Asset, reason: String) -> SkillGroupExclusiveMountSkippedItem {
    SkillGroupExclusiveMountSkippedItem {
        asset_id: asset.id.clone(),
        name: asset.name.clone(),
        reason,
    }
}

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
