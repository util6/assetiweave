use crate::backend::{
    dto::{
        AppResult, ApplyAssetGroupMountResult, ApplySkillGroupExclusiveMountResult,
        AssetGroupInput, AssetGroupMountError, AssetMountObservation, AssetMountStatus,
        AssetMountUpdateResult, CatalogAsset, PhysicalMountStateDto, SkillBackupAssetStatus,
        SkillBackupSettings, SkillBackupState, SkillGroupExclusiveMountError,
        SkillGroupExclusiveMountInput, SkillGroupExclusiveMountItem,
        SkillGroupExclusiveMountPreview, SkillGroupExclusiveMountSkippedItem, TargetProfileInput,
    },
    models::{
        AppKind, Asset, AssetGroup, AssetGroupDetail, AssetGroupRules, AssetKind, AssetMount,
        DeploymentState, DeploymentStrategy, ProfileSafety, RuleSet, Source, SourceKind,
        SourceOrigin, SourceScannerKind, TargetProfile,
    },
    operation_log::{
        asset_log_fields, log_error, log_info, log_warn, profile_log_fields, source_log_fields,
        LogField,
    },
    path_utils::{
        default_skill_backup_root, expand_path, find_git_root, git_browser_url,
        git_repository_for_path,
    },
};
use chrono::Utc;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;
use walkdir::WalkDir;

pub(crate) const SKILL_BACKUP_SOURCE_ID: &str = "assetiweave-library-skills";

pub(crate) fn mount_log_fields(
    db: &crate::backend::store::Database,
    asset_id: &str,
    profile_id: &str,
) -> Vec<LogField> {
    if let Ok((asset, profile)) = load_mount_asset_and_profile_sqlx(db, asset_id, profile_id) {
        let mut fields = asset_log_fields(&asset);
        fields.extend(profile_log_fields(&profile));
        return fields;
    }

    vec![
        ("asset_id", asset_id.to_string()),
        ("profile_id", profile_id.to_string()),
    ]
}

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

pub(crate) fn sync_asset_mount_observations(
    db: &crate::backend::store::Database,
    asset_id: Option<&str>,
) -> AppResult<Vec<AssetMountStatus>> {
    repair_ghost_mount_symlinks_sqlx(db, asset_id)?;
    let statuses = scan_asset_mount_statuses_sqlx(db, asset_id)?;
    persist_asset_mount_observation_snapshot(db, &statuses)?;
    Ok(statuses)
}

#[cfg(test)]
pub(crate) fn scan_asset_mount_statuses(
    conn: &rusqlite::Connection,
    asset_id: Option<&str>,
) -> AppResult<Vec<AssetMountStatus>> {
    let assets = catalog_visible_assets(conn, None)?;
    let profiles = crate::backend::store::load_profiles(conn)?;
    inspect_asset_mount_statuses(&assets, &profiles, asset_id)
}

pub(crate) fn scan_asset_mount_statuses_sqlx(
    db: &crate::backend::store::Database,
    asset_id: Option<&str>,
) -> AppResult<Vec<AssetMountStatus>> {
    let (assets, profiles) = load_mount_status_inputs_sqlx(db)?;
    inspect_asset_mount_statuses(&assets, &profiles, asset_id)
}

fn load_mount_status_inputs_sqlx(
    db: &crate::backend::store::Database,
) -> AppResult<(Vec<Asset>, Vec<TargetProfile>)> {
    let assets = catalog_visible_assets_sqlx(db, None)?;
    let pool = db.pool().clone();
    let profiles =
        db.block_on(async move { crate::backend::store::load_profiles_sqlx(&pool).await })?;
    Ok((assets, profiles))
}

fn persist_asset_mount_observation_snapshot(
    db: &crate::backend::store::Database,
    statuses: &[AssetMountStatus],
) -> AppResult<()> {
    let observed_at = Utc::now().to_rfc3339();
    let observations = statuses
        .iter()
        .map(|status| AssetMountObservation {
            asset_id: status.asset_id.clone(),
            profile_id: status.profile_id.clone(),
            target_dir: status.target_dir.clone(),
            target_path: status.target_path.clone(),
            state: status.state,
            linked_source: status.linked_source.clone(),
            observed_at: observed_at.clone(),
        })
        .collect::<Vec<_>>();
    db.block_on(async {
        let assets = crate::backend::store::load_assets_sqlx(db.pool(), None).await?;
        let profiles = crate::backend::store::load_profiles_sqlx(db.pool()).await?;
        crate::backend::store::persist_asset_mount_snapshot_sqlx(
            db.pool(),
            &observations,
            &assets,
            &profiles,
            statuses,
        )
        .await
    })
}

fn inspect_asset_mount_statuses(
    assets: &[Asset],
    profiles: &[TargetProfile],
    asset_id: Option<&str>,
) -> AppResult<Vec<AssetMountStatus>> {
    let mut statuses = Vec::new();

    for asset in assets
        .iter()
        .filter(|asset| asset_id.map_or(true, |requested| requested == asset.id))
    {
        for profile in profiles {
            let inspection = crate::backend::targeting::inspect_mount(profile, asset)?;
            statuses.push(AssetMountStatus {
                asset_id: asset.id.clone(),
                profile_id: profile.id.clone(),
                target_dir: inspection.target_dir,
                target_path: inspection.target_path,
                state: PhysicalMountStateDto::from(inspection.state),
                linked_source: inspection.linked_source,
            });
        }
    }

    Ok(statuses)
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
pub(crate) fn set_asset_mount_record(
    db: &crate::backend::store::Database,
    asset_id: &str,
    profile_id: &str,
    enabled: bool,
    strategy: Option<DeploymentStrategy>,
) -> AppResult<AssetMount> {
    if enabled {
        return mount_asset_mount_record(db, asset_id, profile_id).map(|result| result.mount);
    }

    let (asset, source, profile) = load_mount_target_sqlx(db, asset_id, profile_id)?;
    let default_strategy = validate_mount_target(&source, &profile)?;
    let inspection = crate::backend::targeting::inspect_mount(&profile, &asset)?;
    if matches!(
        inspection.state,
        crate::backend::targeting::PhysicalMountState::Mounted
    ) {
        return unmount_asset_mount_record(db, asset_id, profile_id).map(|result| result.mount);
    }

    let pool = db.pool().clone();
    let asset_id_to_save = asset_id.to_string();
    let profile_id_to_save = profile_id.to_string();
    let strategy_to_save = strategy.unwrap_or(default_strategy);
    let result = db.block_on(async move {
        crate::backend::store::set_asset_mount_sqlx(
            &pool,
            &asset_id_to_save,
            &profile_id_to_save,
            enabled,
            strategy_to_save,
        )
        .await
    });
    match &result {
        Ok(_) => {
            let mut fields = mount_log_fields(db, asset_id, profile_id);
            fields.push(("enabled", enabled.to_string()));
            log_info("skill.mount.preference", "更新 skill 挂载关系成功", &fields);
        }
        Err(error) => log_error(
            "skill.mount.preference",
            "更新 skill 挂载关系失败",
            error,
            &mount_log_fields(db, asset_id, profile_id),
        ),
    }
    result
}
pub(crate) fn scan_selected_sources(
    conn: &rusqlite::Connection,
    db: &crate::backend::store::Database,
    sources: Vec<Source>,
    scan: fn(&Source) -> AppResult<Vec<Asset>>,
) -> AppResult<Vec<Asset>> {
    for mut source in prune_missing_sources(conn, sources)? {
        if !source.enabled {
            log_info(
                "source.scan.skip",
                "跳过已禁用来源",
                &source_log_fields(&source),
            );
            continue;
        }

        log_info(
            "source.scan.start",
            "开始扫描来源",
            &source_log_fields(&source),
        );
        let now = Utc::now().to_rfc3339();
        match scan(&source) {
            Ok(assets) => {
                crate::backend::store::replace_source_assets(&conn, &source.id, &assets)?;
                source.last_scanned_at = Some(now);
                source.last_scan_status = Some(format!("ok: {} assets", assets.len()));
                crate::backend::store::upsert_source(&conn, &source)?;
                let mut fields = source_log_fields(&source);
                fields.push(("asset_count", assets.len().to_string()));
                log_info("source.scan.success", "扫描来源成功", &fields);
                for asset in &assets {
                    if matches!(asset.kind, AssetKind::Skill) {
                        log_info(
                            "skill.scan.success",
                            "扫描到 skill",
                            &asset_log_fields(asset),
                        );
                    }
                }
            }
            Err(error) => {
                if should_remove_source_on_scan_error(&error) {
                    let mut fields = source_log_fields(&source);
                    fields.push(("error", error.clone()));
                    log_warn("source.scan.removed", "来源路径不存在，已移除", &fields);
                    crate::backend::store::delete_source(conn, &source.id)?;
                    continue;
                }
                source.last_scanned_at = Some(now);
                source.last_scan_status = Some(format!("error: {error}"));
                crate::backend::store::upsert_source(&conn, &source)?;
                log_error(
                    "source.scan.error",
                    "扫描来源失败",
                    &error,
                    &source_log_fields(&source),
                );
            }
        }
    }

    cleanup_orphan_asset_records(conn, db)?;
    let pool = db.pool().clone();
    db.block_on(async move { crate::backend::store::load_assets_sqlx(&pool, None).await })
}

pub(crate) fn refresh_all_sources(
    conn: &rusqlite::Connection,
    db: &crate::backend::store::Database,
) -> AppResult<Vec<Asset>> {
    let pool = db.pool().clone();
    let sources =
        db.block_on(async move { crate::backend::store::load_sources_sqlx(&pool).await })?;
    scan_selected_sources(conn, db, sources, crate::backend::scanner::scan_source)
}

pub(crate) fn refresh_recorded_assets(
    conn: &rusqlite::Connection,
    db: &crate::backend::store::Database,
) -> AppResult<Vec<Asset>> {
    let pool = db.pool().clone();
    let sources =
        db.block_on(async move { crate::backend::store::load_sources_sqlx(&pool).await })?;
    let sources = prune_missing_sources(conn, sources)?;
    let source_map: HashMap<&str, &Source> = sources
        .iter()
        .map(|source| (source.id.as_str(), source))
        .collect();
    let mut assets_by_source: HashMap<String, Vec<Asset>> = sources
        .iter()
        .map(|source| (source.id.clone(), Vec::new()))
        .collect();
    let mut removed_by_source: HashMap<String, usize> = HashMap::new();
    let mut updated_by_source: HashMap<String, usize> = HashMap::new();
    let mut orphan_source_ids = Vec::new();
    let now = Utc::now().to_rfc3339();

    let pool = db.pool().clone();
    let assets =
        db.block_on(async move { crate::backend::store::load_assets_sqlx(&pool, None).await })?;
    for asset in assets {
        let Some(source) = source_map.get(asset.source_id.as_str()) else {
            orphan_source_ids.push(asset.source_id.clone());
            continue;
        };

        match crate::backend::scanner::refresh_recorded_asset(source, &asset, &now) {
            Ok(Some(refreshed)) => {
                if refreshed.content_hash != asset.content_hash
                    || refreshed.description != asset.description
                {
                    *updated_by_source.entry(source.id.clone()).or_default() += 1;
                }
                assets_by_source
                    .entry(source.id.clone())
                    .or_default()
                    .push(refreshed);
            }
            Ok(None) => {
                *removed_by_source.entry(source.id.clone()).or_default() += 1;
            }
            Err(_) => {
                assets_by_source
                    .entry(source.id.clone())
                    .or_default()
                    .push(asset);
            }
        }
    }

    for source in sources {
        let retained_assets = assets_by_source.remove(&source.id).unwrap_or_default();
        let retained_count = retained_assets.len();
        crate::backend::store::replace_source_assets(conn, &source.id, &retained_assets)?;

        let removed_count = removed_by_source.get(&source.id).copied().unwrap_or(0);
        let updated_count = updated_by_source.get(&source.id).copied().unwrap_or(0);
        let mut source = source;
        source.last_scanned_at = Some(now.clone());
        source.last_scan_status = Some(format!(
            "validated: {retained_count} assets, {removed_count} removed, {updated_count} updated"
        ));
        crate::backend::store::upsert_source(conn, &source)?;
    }

    orphan_source_ids.sort();
    orphan_source_ids.dedup();
    for source_id in orphan_source_ids {
        crate::backend::store::replace_source_assets(conn, &source_id, &[])?;
    }

    cleanup_orphan_asset_records(conn, db)?;
    let pool = db.pool().clone();
    db.block_on(async move { crate::backend::store::load_assets_sqlx(&pool, None).await })
}

pub(crate) fn cleanup_orphan_asset_records(
    _conn: &rusqlite::Connection,
    db: &crate::backend::store::Database,
) -> AppResult<()> {
    let pool = db.pool().clone();
    db.block_on(async move {
        crate::backend::store::delete_orphan_asset_mounts_sqlx(&pool).await?;
        crate::backend::store::delete_orphan_deployment_state_sqlx(&pool).await?;
        crate::backend::store::delete_orphan_skill_remote_sources_sqlx(&pool).await?;
        crate::backend::store::delete_orphan_asset_group_members_sqlx(&pool).await
    })
}

fn prune_missing_sources(
    conn: &rusqlite::Connection,
    sources: Vec<Source>,
) -> AppResult<Vec<Source>> {
    let mut retained_sources = Vec::new();
    for source in sources {
        if source_root_is_missing(&source) {
            log_warn(
                "source.prune_missing",
                "来源路径不存在，已从索引移除",
                &source_log_fields(&source),
            );
            crate::backend::store::delete_source(conn, &source.id)?;
        } else {
            retained_sources.push(source);
        }
    }
    Ok(retained_sources)
}

fn source_root_is_missing(source: &Source) -> bool {
    expand_path(&source.root_path)
        .map(|root| !root.exists())
        .unwrap_or(false)
}

fn should_remove_source_on_scan_error(error: &str) -> bool {
    error.starts_with("source path does not exist:")
}

fn validate_mount_target(
    source: &Source,
    profile: &TargetProfile,
) -> AppResult<DeploymentStrategy> {
    if matches!(
        source.source_origin,
        SourceOrigin::AppTarget | SourceOrigin::AppLocal
    ) {
        return Err("app-local skills must be backed up before mounting".to_string());
    }

    Ok(profile.deployment_strategy)
}

pub(crate) fn mount_asset_mount_record(
    db: &crate::backend::store::Database,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<AssetMountUpdateResult> {
    let result = (|| {
        let (asset, source, profile) = load_mount_target_sqlx(db, asset_id, profile_id)?;
        let strategy = validate_mount_target(&source, &profile)?;
        if !matches!(strategy, DeploymentStrategy::SymlinkToSource) {
            return Err("immediate mount only supports symlink_to_source profiles".to_string());
        }
        validate_immediate_mount_support(&asset, &profile)?;

        let inspection = crate::backend::targeting::inspect_mount(&profile, &asset)?;
        match inspection.state {
            crate::backend::targeting::PhysicalMountState::Mounted => {
                let inspection =
                    repair_mounted_symlink_to_real_source(&asset, &profile, inspection)?;
                let mount = persist_verified_mount(
                    db,
                    &asset,
                    &profile,
                    &inspection.target_path,
                    strategy,
                )?;
                return Ok(AssetMountUpdateResult {
                    mount,
                    status: asset_mount_status(&asset.id, &profile.id, inspection),
                });
            }
            crate::backend::targeting::PhysicalMountState::NotMounted => {}
            crate::backend::targeting::PhysicalMountState::Conflict
            | crate::backend::targeting::PhysicalMountState::Broken => {
                return Err(format!(
                    "target is not available for mounting: {}",
                    inspection.target_path
                ));
            }
        }

        let target_path = PathBuf::from(&inspection.target_path);
        create_mount_symlink(&asset, &profile, &target_path)?;
        let inspection = crate::backend::targeting::inspect_mount(&profile, &asset)?;
        if !matches!(
            inspection.state,
            crate::backend::targeting::PhysicalMountState::Mounted
        ) {
            remove_created_mount_symlink(&target_path).ok();
            return Err(format!(
                "mount verification failed for {asset_id} on {profile_id}: {}",
                inspection.target_path
            ));
        }

        let mount =
            match persist_verified_mount(db, &asset, &profile, &inspection.target_path, strategy) {
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
    })();

    match &result {
        Ok(update) => {
            let mut fields = mount_log_fields(db, asset_id, profile_id);
            fields.push(("target_path", update.status.target_path.clone()));
            fields.push(("state", format!("{:?}", update.status.state)));
            log_info("skill.mount.success", "skill 挂载成功", &fields);
        }
        Err(error) => log_error(
            "skill.mount.error",
            "skill 挂载失败",
            error,
            &mount_log_fields(db, asset_id, profile_id),
        ),
    }
    result
}

pub(crate) fn unmount_asset_mount_record(
    db: &crate::backend::store::Database,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<AssetMountUpdateResult> {
    let result = (|| {
        let (asset, profile) = load_mount_asset_and_profile_sqlx(db, asset_id, profile_id)?;
        let inspection = crate::backend::targeting::inspect_mount(&profile, &asset)?;
        let target_path = PathBuf::from(&inspection.target_path);
        let removed_link = matches!(
            inspection.state,
            crate::backend::targeting::PhysicalMountState::Mounted
        );

        match inspection.state {
            crate::backend::targeting::PhysicalMountState::Mounted => {
                remove_mounted_symlink(&inspection.target_path)?
            }
            crate::backend::targeting::PhysicalMountState::NotMounted => {}
            crate::backend::targeting::PhysicalMountState::Conflict
            | crate::backend::targeting::PhysicalMountState::Broken => {
                return Err(format!(
                    "target is not a symlink to this asset: {}",
                    inspection.target_path
                ));
            }
        }

        let inspection = crate::backend::targeting::inspect_mount(&profile, &asset)?;
        if !matches!(
            inspection.state,
            crate::backend::targeting::PhysicalMountState::NotMounted
        ) {
            return Err(format!(
                "unmount verification failed for {asset_id} on {profile_id}: {}",
                inspection.target_path
            ));
        }

        match persist_verified_unmount(db, &asset, &profile, &inspection.target_path) {
            Ok(mount) => Ok(AssetMountUpdateResult {
                mount,
                status: asset_mount_status(&asset.id, &profile.id, inspection),
            }),
            Err(error) => {
                if removed_link {
                    create_mount_symlink(&asset, &profile, &target_path).ok();
                }
                Err(error)
            }
        }
    })();

    match &result {
        Ok(update) => {
            let mut fields = mount_log_fields(db, asset_id, profile_id);
            fields.push(("target_path", update.status.target_path.clone()));
            fields.push(("state", format!("{:?}", update.status.state)));
            log_info("skill.unmount.success", "skill 卸载成功", &fields);
        }
        Err(error) => log_error(
            "skill.unmount.error",
            "skill 卸载失败",
            error,
            &mount_log_fields(db, asset_id, profile_id),
        ),
    }
    result
}

fn load_mount_asset_and_profile_sqlx(
    db: &crate::backend::store::Database,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<(Asset, TargetProfile)> {
    let pool = db.pool().clone();
    let asset_id = asset_id.to_string();
    let profile_id = profile_id.to_string();
    db.block_on(async move {
        let asset = crate::backend::store::load_asset_sqlx(&pool, &asset_id)
            .await?
            .ok_or_else(|| format!("asset not found: {asset_id}"))?;
        let profile = crate::backend::store::load_profile_sqlx(&pool, &profile_id)
            .await?
            .ok_or_else(|| format!("profile not found: {profile_id}"))?;
        AppResult::Ok((asset, profile))
    })
}

fn load_mount_target_sqlx(
    db: &crate::backend::store::Database,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<(Asset, Source, TargetProfile)> {
    let pool = db.pool().clone();
    let asset_id = asset_id.to_string();
    let profile_id = profile_id.to_string();
    db.block_on(async move {
        let asset = crate::backend::store::load_asset_sqlx(&pool, &asset_id)
            .await?
            .ok_or_else(|| format!("asset not found: {asset_id}"))?;
        let source = crate::backend::store::load_source_sqlx(&pool, &asset.source_id)
            .await?
            .ok_or_else(|| format!("source not found: {}", asset.source_id))?;
        let profile = crate::backend::store::load_profile_sqlx(&pool, &profile_id)
            .await?
            .ok_or_else(|| format!("profile not found: {profile_id}"))?;
        AppResult::Ok((asset, source, profile))
    })
}

fn validate_immediate_mount_support(asset: &Asset, profile: &TargetProfile) -> AppResult<()> {
    if !profile.enabled {
        return Err(format!("profile is disabled: {}", profile.name));
    }
    if matches!(asset.kind, AssetKind::Unclassified)
        || !profile.supported_kinds.contains(&asset.kind)
        || !profile.include.kinds.contains(&asset.kind)
    {
        return Err(format!(
            "profile {} does not support {:?}",
            profile.name, asset.kind
        ));
    }

    Ok(())
}

fn create_mount_symlink(
    asset: &Asset,
    profile: &TargetProfile,
    target_path: &Path,
) -> AppResult<()> {
    ensure_target_within_profile(profile, target_path)?;
    let source_path = crate::backend::targeting::canonical_source_path(asset)?;
    prepare_target_for_mount_symlink(asset, target_path)?;

    let parent = target_path.parent().ok_or_else(|| {
        format!(
            "target path is missing parent directory: {}",
            target_path.display()
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    create_symlink(&source_path, target_path)
}

fn prepare_target_for_mount_symlink(asset: &Asset, target_path: &Path) -> AppResult<()> {
    let metadata = match fs::symlink_metadata(target_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "target symlink already exists: {}",
            target_path.display()
        ));
    }
    if crate::backend::targeting::target_is_asset_source(asset, target_path)? {
        return Err(format!(
            "target path is the asset source path: {}",
            target_path.display()
        ));
    }
    if !crate::backend::targeting::target_content_matches_asset(asset, target_path)? {
        return Err(format!(
            "target exists with different content: {}",
            target_path.display()
        ));
    }

    if metadata.is_dir() {
        fs::remove_dir_all(target_path).map_err(|error| error.to_string())
    } else if metadata.is_file() {
        fs::remove_file(target_path).map_err(|error| error.to_string())
    } else {
        Err(format!(
            "unsupported target type for replacement: {}",
            target_path.display()
        ))
    }
}

fn repair_ghost_mount_symlinks_sqlx(
    db: &crate::backend::store::Database,
    asset_id: Option<&str>,
) -> AppResult<()> {
    let (assets, profiles) = load_mount_status_inputs_sqlx(db)?;
    repair_ghost_mount_symlinks_for_assets(&assets, &profiles, asset_id)
}

fn repair_ghost_mount_symlinks_for_assets(
    assets: &[Asset],
    profiles: &[TargetProfile],
    asset_id: Option<&str>,
) -> AppResult<()> {
    for asset in assets
        .iter()
        .filter(|asset| asset_id.map(|id| asset.id == id).unwrap_or(true))
    {
        for profile in profiles.iter().filter(|profile| profile.enabled) {
            let inspection = crate::backend::targeting::inspect_mount(profile, asset)?;
            repair_mounted_symlink_to_real_source(asset, profile, inspection)?;
        }
    }
    Ok(())
}

fn repair_mounted_symlink_to_real_source(
    asset: &Asset,
    profile: &TargetProfile,
    inspection: crate::backend::targeting::MountInspection,
) -> AppResult<crate::backend::targeting::MountInspection> {
    if !matches!(
        inspection.state,
        crate::backend::targeting::PhysicalMountState::Mounted
    ) {
        return Ok(inspection);
    }

    let target_path = PathBuf::from(&inspection.target_path);
    let expected_source_path = crate::backend::targeting::canonical_source_path(asset)?;
    let linked_source = inspection
        .linked_source
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_default();
    if linked_source == expected_source_path {
        return Ok(inspection);
    }

    let metadata = fs::symlink_metadata(&target_path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_symlink() {
        return Ok(inspection);
    }

    let previous_link = fs::read_link(&target_path).map_err(|error| error.to_string())?;
    fs::remove_file(&target_path).map_err(|error| error.to_string())?;
    if let Err(error) = create_symlink(&expected_source_path, &target_path) {
        create_symlink(&previous_link, &target_path).ok();
        return Err(error);
    }

    let repaired = crate::backend::targeting::inspect_mount(profile, asset)?;
    if !matches!(
        repaired.state,
        crate::backend::targeting::PhysicalMountState::Mounted
    ) {
        fs::remove_file(&target_path).ok();
        create_symlink(&previous_link, &target_path).ok();
        return Err(format!(
            "ghost symlink repair verification failed: {}",
            repaired.target_path
        ));
    }
    Ok(repaired)
}

fn persist_verified_mount(
    db: &crate::backend::store::Database,
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
    db.block_on(async {
        crate::backend::store::persist_verified_mount_sqlx(db.pool(), &state, strategy).await
    })
}

fn persist_verified_unmount(
    db: &crate::backend::store::Database,
    asset: &Asset,
    profile: &TargetProfile,
    target_path: &str,
) -> AppResult<AssetMount> {
    db.block_on(async {
        crate::backend::store::persist_verified_unmount_sqlx(
            db.pool(),
            &asset.id,
            &profile.id,
            target_path,
            profile.deployment_strategy,
        )
        .await
    })
}

fn ensure_target_within_profile(profile: &TargetProfile, target_path: &Path) -> AppResult<()> {
    let target_dir = crate::backend::targeting::target_dir(profile)?;
    if !target_path.starts_with(&target_dir) {
        return Err(format!(
            "refusing to write outside profile target directory: {}",
            target_path.display()
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn create_symlink(source: &Path, target: &Path) -> AppResult<()> {
    std::os::unix::fs::symlink(source, target).map_err(|error| error.to_string())
}

#[cfg(windows)]
fn create_symlink(source: &Path, target: &Path) -> AppResult<()> {
    if source.is_dir() {
        std::os::windows::fs::symlink_dir(source, target)
    } else {
        std::os::windows::fs::symlink_file(source, target)
    }
    .map_err(|error| error.to_string())
}

fn remove_created_mount_symlink(target_path: &Path) -> AppResult<()> {
    let metadata = fs::symlink_metadata(target_path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_symlink() {
        return Ok(());
    }
    fs::remove_file(target_path).map_err(|error| error.to_string())
}

fn remove_mounted_symlink(target_path: &str) -> AppResult<()> {
    let path = Path::new(target_path);
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_symlink() {
        return Err(format!("target is not a symlink: {}", path.display()));
    }
    fs::remove_file(path).map_err(|error| error.to_string())
}

fn asset_mount_status(
    asset_id: &str,
    profile_id: &str,
    inspection: crate::backend::targeting::MountInspection,
) -> AssetMountStatus {
    AssetMountStatus {
        asset_id: asset_id.to_string(),
        profile_id: profile_id.to_string(),
        target_dir: inspection.target_dir,
        target_path: inspection.target_path,
        state: PhysicalMountStateDto::from(inspection.state),
        linked_source: inspection.linked_source,
    }
}

pub(crate) fn assetiweave_library_source() -> Source {
    let root_path = default_skill_backup_root()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|_| "~/.assetiweave/library/skills".to_string());
    assetiweave_library_source_with_root(root_path)
}

pub(crate) fn assetiweave_library_source_with_root(root_path: String) -> Source {
    Source {
        id: SKILL_BACKUP_SOURCE_ID.to_string(),
        name: "AssetIWeave Skill Backup Library".to_string(),
        kind: SourceKind::Local,
        root_path,
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::AssetiweaveLibrary,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: vec![
            "**/.git/**".to_string(),
            "**/node_modules/**".to_string(),
            "**/target/**".to_string(),
            "**/dist/**".to_string(),
        ],
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: -100,
        last_scanned_at: None,
        last_scan_status: Some("pending".to_string()),
    }
}

pub(crate) fn skill_backup_root_sqlx(db: &crate::backend::store::Database) -> AppResult<PathBuf> {
    let pool = db.pool().clone();
    let sources =
        db.block_on(async move { crate::backend::store::load_sources_sqlx(&pool).await })?;
    let root_path = skill_backup_root_path(sources);
    let root = expand_path(&root_path)?;
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root)
}

pub(crate) fn skill_backup_settings_sqlx(
    db: &crate::backend::store::Database,
) -> AppResult<SkillBackupSettings> {
    let pool = db.pool().clone();
    let sources =
        db.block_on(async move { crate::backend::store::load_sources_sqlx(&pool).await })?;
    build_skill_backup_settings(sources)
}

fn skill_backup_root_path(sources: Vec<Source>) -> String {
    sources
        .into_iter()
        .find(|source| source.id == SKILL_BACKUP_SOURCE_ID)
        .map(|source| source.root_path)
        .unwrap_or_else(|| {
            default_skill_backup_root()
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_else(|_| "~/.assetiweave/library/skills".to_string())
        })
}

fn build_skill_backup_settings(sources: Vec<Source>) -> AppResult<SkillBackupSettings> {
    let default_root = default_skill_backup_root()?;
    let source = sources
        .into_iter()
        .find(|source| source.id == SKILL_BACKUP_SOURCE_ID)
        .unwrap_or_else(|| assetiweave_library_source());
    let expanded_root = expand_path(&source.root_path)?;
    Ok(SkillBackupSettings {
        root_path: source.root_path,
        expanded_root_path: expanded_root.to_string_lossy().to_string(),
        default_root_path: default_root.to_string_lossy().to_string(),
        is_default_root: same_path_or_text(&expanded_root, &default_root),
        exists: expanded_root.exists(),
    })
}

pub(crate) fn catalog_assets_sqlx(
    db: &crate::backend::store::Database,
    kind: Option<AssetKind>,
) -> AppResult<Vec<CatalogAsset>> {
    let pool = db.pool().clone();
    db.block_on(async move {
        let assets = crate::backend::store::load_assets_sqlx(&pool, kind).await?;
        let sources = crate::backend::store::load_sources_sqlx(&pool).await?;
        AppResult::Ok(build_catalog_assets(assets, &sources))
    })
}

#[cfg(test)]
pub(crate) fn catalog_visible_assets(
    conn: &rusqlite::Connection,
    kind: Option<AssetKind>,
) -> AppResult<Vec<Asset>> {
    let assets = crate::backend::store::load_assets_by_kind(conn, kind)?;
    let sources = crate::backend::store::load_sources(conn)?;
    Ok(build_catalog_asset_entries(assets, &sources)
        .into_iter()
        .map(|catalog_asset| catalog_asset.asset)
        .collect())
}

pub(crate) fn catalog_visible_assets_sqlx(
    db: &crate::backend::store::Database,
    kind: Option<AssetKind>,
) -> AppResult<Vec<Asset>> {
    let pool = db.pool().clone();
    db.block_on(async move {
        let assets = crate::backend::store::load_assets_sqlx(&pool, kind).await?;
        let sources = crate::backend::store::load_sources_sqlx(&pool).await?;
        AppResult::Ok(
            build_catalog_asset_entries(assets, &sources)
                .into_iter()
                .map(|catalog_asset| catalog_asset.asset)
                .collect(),
        )
    })
}

pub(crate) fn build_catalog_assets(assets: Vec<Asset>, sources: &[Source]) -> Vec<CatalogAsset> {
    let mut catalog_assets = build_catalog_asset_entries(assets, sources);
    attach_git_repository_info(&mut catalog_assets);
    catalog_assets
}

fn build_catalog_asset_entries(assets: Vec<Asset>, sources: &[Source]) -> Vec<CatalogAsset> {
    let source_by_id = sources
        .iter()
        .map(|source| (source.id.as_str(), source))
        .collect::<HashMap<_, _>>();
    let mut content_groups: BTreeMap<String, Vec<Asset>> = BTreeMap::new();
    let mut without_identity = Vec::new();

    for asset in assets {
        if asset.kind == AssetKind::Skill {
            if let Some(content_hash) = asset.content_hash.clone().filter(|hash| !hash.is_empty()) {
                content_groups.entry(content_hash).or_default().push(asset);
                continue;
            }
        }
        without_identity.push(CatalogAsset {
            backup_status: standalone_backup_status(
                &asset,
                source_by_id.get(asset.source_id.as_str()).copied(),
            ),
            repository: None,
            asset,
        });
    }

    let mut catalog_assets = without_identity;
    for mut group in content_groups.into_values() {
        if group.len() == 1 {
            let asset = group.remove(0);
            catalog_assets.push(CatalogAsset {
                backup_status: standalone_backup_status(
                    &asset,
                    source_by_id.get(asset.source_id.as_str()).copied(),
                ),
                repository: None,
                asset,
            });
            continue;
        }

        group.sort_by(|left, right| {
            let left_score =
                canonical_asset_score(left, source_by_id.get(left.source_id.as_str()).copied());
            let right_score =
                canonical_asset_score(right, source_by_id.get(right.source_id.as_str()).copied());
            left_score
                .cmp(&right_score)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.absolute_path.cmp(&right.absolute_path))
        });

        let canonical = group.remove(0);
        let hidden_asset_ids = group
            .iter()
            .map(|asset| asset.id.clone())
            .collect::<Vec<_>>();
        let backup_path = std::iter::once(&canonical)
            .chain(group.iter())
            .find(|asset| {
                backup_entry_state(asset, source_by_id.get(asset.source_id.as_str()).copied())
                    == Some(SkillBackupState::BackedUp)
            })
            .map(|asset| asset.absolute_path.clone());
        let backup_status = if let Some(backup_path) = backup_path {
            Some(SkillBackupAssetStatus {
                state: SkillBackupState::BackedUp,
                backup_path: Some(backup_path),
                hidden_asset_ids,
            })
        } else {
            standalone_backup_status(
                &canonical,
                source_by_id.get(canonical.source_id.as_str()).copied(),
            )
            .map(|mut status| {
                status.hidden_asset_ids = hidden_asset_ids;
                status
            })
        };

        catalog_assets.push(CatalogAsset {
            asset: canonical,
            backup_status,
            repository: None,
        });
    }

    catalog_assets.sort_by(|left, right| {
        left.asset
            .name
            .cmp(&right.asset.name)
            .then_with(|| left.asset.relative_path.cmp(&right.asset.relative_path))
    });
    catalog_assets
}

fn attach_git_repository_info(catalog_assets: &mut [CatalogAsset]) {
    let mut repository_by_root = HashMap::new();
    for catalog_asset in catalog_assets {
        let asset_path = Path::new(&catalog_asset.asset.absolute_path);
        let Some(repository_root) = find_git_root(asset_path) else {
            continue;
        };
        let repository = repository_by_root
            .entry(repository_root.clone())
            .or_insert_with(|| git_repository_for_path(&repository_root));
        catalog_asset.repository = repository.clone().map(|mut repository| {
            repository.web_url = repository
                .remote_url
                .as_deref()
                .and_then(|remote| git_browser_url(remote, &repository_root, asset_path));
            repository
        });
    }
}

fn standalone_backup_status(
    asset: &Asset,
    source: Option<&Source>,
) -> Option<SkillBackupAssetStatus> {
    backup_entry_state(asset, source).map(|state| SkillBackupAssetStatus {
        state,
        backup_path: Some(asset.absolute_path.clone()),
        hidden_asset_ids: Vec::new(),
    })
}

fn backup_entry_state(asset: &Asset, source: Option<&Source>) -> Option<SkillBackupState> {
    let source = source?;
    if source.id != SKILL_BACKUP_SOURCE_ID
        && !matches!(source.source_origin, SourceOrigin::AssetiweaveLibrary)
    {
        return None;
    }

    if asset.relative_path.starts_with("downloaded/")
        || asset.relative_path.starts_with("imported/")
    {
        return Some(SkillBackupState::Downloaded);
    }
    if asset.relative_path.starts_with("backed-up/") {
        return Some(SkillBackupState::BackedUp);
    }
    None
}

fn canonical_asset_score(asset: &Asset, source: Option<&Source>) -> u8 {
    let Some(source) = source else {
        return 50;
    };
    match source.source_origin {
        SourceOrigin::AppTarget | SourceOrigin::AppLocal => 40,
        SourceOrigin::AssetiweaveLibrary => match backup_entry_state(asset, Some(source)) {
            Some(SkillBackupState::Downloaded) => 20,
            Some(SkillBackupState::BackedUp) => 30,
            None => 25,
        },
        SourceOrigin::GitRepo | SourceOrigin::LocalFolder | SourceOrigin::Custom => 0,
    }
}

pub(crate) fn target_profile_from_input(input: TargetProfileInput) -> AppResult<TargetProfile> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err("profile name is required".to_string());
    }

    let id = input
        .id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| slug_profile_id(&name));
    if id.is_empty() {
        return Err("profile id is required".to_string());
    }

    let target_paths = input
        .target_paths
        .unwrap_or_default()
        .into_iter()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .collect::<Vec<_>>();
    if target_paths.is_empty() {
        return Err("profile target path is required".to_string());
    }

    let profile = TargetProfile {
        id,
        name,
        app_kind: input.app_kind.unwrap_or(AppKind::Custom),
        target_paths,
        supported_kinds: input
            .supported_kinds
            .unwrap_or_else(|| vec![AssetKind::Skill]),
        deployment_strategy: input
            .deployment_strategy
            .unwrap_or(DeploymentStrategy::SymlinkToSource),
        enabled: input.enabled.unwrap_or(true),
        include: input.include.unwrap_or_else(default_profile_include),
        exclude: input.exclude.unwrap_or_else(default_profile_exclude),
        safety: input.safety.unwrap_or(ProfileSafety {
            allow_remove: false,
            allow_overwrite: false,
        }),
    };
    validate_target_profile(&profile)?;
    Ok(profile)
}

pub(crate) fn ensure_profile_can_be_deleted_sqlx(
    db: &crate::backend::store::Database,
    profile_id: &str,
) -> AppResult<()> {
    if crate::backend::defaults::is_default_app_profile_id(profile_id) {
        return Err(format!("default app cannot be deleted: {profile_id}"));
    }

    let deployment_count = db.block_on(async {
        crate::backend::store::count_deployment_state_by_profile_sqlx(db.pool(), profile_id).await
    })?;
    if deployment_count > 0 {
        return Err(format!("profile has managed deployments: {profile_id}"));
    }

    if scan_asset_mount_statuses_sqlx(db, None)?
        .iter()
        .any(|status| {
            status.profile_id == profile_id && status.state == PhysicalMountStateDto::Mounted
        })
    {
        return Err(format!("profile has mounted assets: {profile_id}"));
    }

    Ok(())
}

#[cfg(test)]
pub(crate) fn ensure_profile_can_be_deleted(
    conn: &rusqlite::Connection,
    profile_id: &str,
) -> AppResult<()> {
    if crate::backend::defaults::is_default_app_profile_id(profile_id) {
        return Err(format!("default app cannot be deleted: {profile_id}"));
    }

    if crate::backend::store::count_deployment_state_by_profile(conn, profile_id)? > 0 {
        return Err(format!("profile has managed deployments: {profile_id}"));
    }

    if scan_asset_mount_statuses(conn, None)?.iter().any(|status| {
        status.profile_id == profile_id && status.state == PhysicalMountStateDto::Mounted
    }) {
        return Err(format!("profile has mounted assets: {profile_id}"));
    }

    Ok(())
}

pub(crate) fn ensure_default_profile_update_is_allowed(
    existing: &TargetProfile,
    next: &TargetProfile,
) -> AppResult<()> {
    if crate::backend::defaults::is_default_app_profile_id(&existing.id) && existing.id != next.id {
        return Err(format!(
            "default app profile id cannot be changed: {}",
            existing.id
        ));
    }
    Ok(())
}

pub(crate) fn validate_target_profile(profile: &TargetProfile) -> AppResult<()> {
    if profile.id.trim().is_empty() {
        return Err("profile id is required".to_string());
    }
    if profile.name.trim().is_empty() {
        return Err("profile name is required".to_string());
    }
    if profile
        .target_paths
        .iter()
        .map(|path| path.trim())
        .all(str::is_empty)
    {
        return Err("profile target path is required".to_string());
    }
    Ok(())
}

fn default_profile_include() -> RuleSet {
    RuleSet {
        kinds: vec![AssetKind::Skill],
        tags: vec![],
        groups: vec![],
        sources: vec![],
        path_patterns: vec![],
    }
}

fn default_profile_exclude() -> RuleSet {
    RuleSet {
        kinds: vec![AssetKind::Unclassified],
        tags: vec![],
        groups: vec![],
        sources: vec![],
        path_patterns: vec![],
    }
}

fn slug_profile_id(name: &str) -> String {
    let mut id = String::new();
    let mut last_was_separator = false;
    for character in name.trim().chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            id.push(character);
            last_was_separator = false;
        } else if !last_was_separator && !id.is_empty() {
            id.push('-');
            last_was_separator = true;
        }
    }
    id.trim_matches('-').to_string()
}

pub(crate) fn copy_dir(source: &Path, target: &Path) -> AppResult<()> {
    for entry in WalkDir::new(source).into_iter().filter_map(Result::ok) {
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| error.to_string())?;
        let destination = target.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::copy(entry.path(), destination).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

pub(crate) fn copy_dir_without_conflicts(source: &Path, target: &Path) -> AppResult<()> {
    if !source.exists() {
        return Ok(());
    }
    if !source.is_dir() {
        return Err(format!(
            "backup source is not a directory: {}",
            source.display()
        ));
    }

    for entry in WalkDir::new(source).into_iter().filter_map(Result::ok) {
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| error.to_string())?;
        let destination = target.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }

        if destination.exists() {
            if !destination.is_file() {
                return Err(format!(
                    "backup migration target is not a file: {}",
                    destination.display()
                ));
            }
            let source_bytes = fs::read(entry.path()).map_err(|error| error.to_string())?;
            let destination_bytes = fs::read(&destination).map_err(|error| error.to_string())?;
            if source_bytes != destination_bytes {
                return Err(format!(
                    "backup migration target already has different content: {}",
                    destination.display()
                ));
            }
            continue;
        }

        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::copy(entry.path(), destination).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn same_path_or_text(left: &Path, right: &Path) -> bool {
    let normalized_left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let normalized_right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    normalized_left == normalized_right || left == right
}
