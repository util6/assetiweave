use super::prelude::*;

pub(crate) fn mount_log_fields(
    db: &crate::backend::store::Database,
    tenant_id: &str,
    asset_id: &str,
    profile_id: &str,
) -> Vec<LogField> {
    if let Ok((asset, profile)) =
        load_mount_asset_and_profile_sqlx(db, tenant_id, asset_id, profile_id)
    {
        let mut fields = asset_log_fields(&asset);
        fields.extend(profile_log_fields(&profile));
        return fields;
    }

    vec![
        ("asset_id", asset_id.to_string()),
        ("profile_id", profile_id.to_string()),
    ]
}

pub(crate) fn sync_asset_mount_observations(
    db: &crate::backend::store::Database,
    tenant_id: &str,
    asset_id: Option<&str>,
) -> AppResult<Vec<AssetMountStatus>> {
    repair_ghost_mount_symlinks_sqlx(db, tenant_id, asset_id)?;
    let statuses = scan_asset_mount_statuses_sqlx(db, tenant_id, asset_id)?;
    persist_asset_mount_observation_snapshot(db, tenant_id, &statuses)?;
    Ok(statuses)
}

pub(crate) fn scan_asset_mount_statuses_sqlx(
    db: &crate::backend::store::Database,
    tenant_id: &str,
    asset_id: Option<&str>,
) -> AppResult<Vec<AssetMountStatus>> {
    let (assets, profiles) = load_mount_status_inputs_sqlx(db, tenant_id)?;
    inspect_asset_mount_statuses(&assets, &profiles, asset_id)
}

fn load_mount_status_inputs_sqlx(
    db: &crate::backend::store::Database,
    tenant_id: &str,
) -> AppResult<(Vec<Asset>, Vec<TargetProfile>)> {
    let assets = catalog_visible_assets_sqlx(db, tenant_id, None)?;
    let pool = db.pool().clone();
    let tenant_id = tenant_id.to_string();
    let profiles =
        db.block_on(
            async move { crate::backend::store::load_profiles_sqlx(&pool, &tenant_id).await },
        )?;
    Ok((assets, profiles))
}

fn persist_asset_mount_observation_snapshot(
    db: &crate::backend::store::Database,
    tenant_id: &str,
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
        let assets = crate::backend::store::load_assets_sqlx(db.pool(), tenant_id, None).await?;
        let profiles = crate::backend::store::load_profiles_sqlx(db.pool(), tenant_id).await?;
        crate::backend::store::persist_asset_mount_snapshot_sqlx(
            db.pool(),
            tenant_id,
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
            statuses.push(asset_mount_status(&asset.id, &profile.id, inspection));
        }
    }

    Ok(statuses)
}

pub(crate) fn set_asset_mount_record(
    db: &crate::backend::store::Database,
    tenant_id: &str,
    asset_id: &str,
    profile_id: &str,
    enabled: bool,
    strategy: Option<DeploymentStrategy>,
) -> AppResult<AssetMount> {
    if enabled {
        return mount_asset_mount_record(db, tenant_id, asset_id, profile_id)
            .map(|result| result.mount);
    }

    let (asset, source, profile) = load_mount_target_sqlx(db, tenant_id, asset_id, profile_id)?;
    let default_strategy = validate_mount_target(&source, &profile)?;
    let inspection = crate::backend::targeting::inspect_mount(&profile, &asset)?;
    if matches!(
        inspection.state,
        crate::backend::targeting::PhysicalMountState::Mounted
    ) {
        return unmount_asset_mount_record(db, tenant_id, asset_id, profile_id)
            .map(|result| result.mount);
    }

    let pool = db.pool().clone();
    let tenant_id_to_save = tenant_id.to_string();
    let asset_id_to_save = asset_id.to_string();
    let profile_id_to_save = profile_id.to_string();
    let strategy_to_save = strategy.unwrap_or(default_strategy);
    let result = db.block_on(async move {
        crate::backend::store::set_asset_mount_sqlx(
            &pool,
            &tenant_id_to_save,
            &asset_id_to_save,
            &profile_id_to_save,
            enabled,
            strategy_to_save,
        )
        .await
    });
    match &result {
        Ok(_) => {
            let mut fields = mount_log_fields(db, tenant_id, asset_id, profile_id);
            fields.push(("enabled", enabled.to_string()));
            log_info("skill.mount.preference", "更新 skill 挂载关系成功", &fields);
        }
        Err(error) => log_error(
            "skill.mount.preference",
            "更新 skill 挂载关系失败",
            error,
            &mount_log_fields(db, tenant_id, asset_id, profile_id),
        ),
    }
    result
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
    tenant_id: &str,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<AssetMountUpdateResult> {
    let result = (|| {
        let (asset, source, profile) = load_mount_target_sqlx(db, tenant_id, asset_id, profile_id)?;
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
                    tenant_id,
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

        let mount = match persist_verified_mount(
            db,
            tenant_id,
            &asset,
            &profile,
            &inspection.target_path,
            strategy,
        ) {
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
            let mut fields = mount_log_fields(db, tenant_id, asset_id, profile_id);
            fields.push(("target_path", update.status.target_path.clone()));
            fields.push(("state", format!("{:?}", update.status.state)));
            log_info("skill.mount.success", "skill 挂载成功", &fields);
        }
        Err(error) => log_error(
            "skill.mount.error",
            "skill 挂载失败",
            error,
            &mount_log_fields(db, tenant_id, asset_id, profile_id),
        ),
    }
    result
}

pub(crate) fn unmount_asset_mount_record(
    db: &crate::backend::store::Database,
    tenant_id: &str,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<AssetMountUpdateResult> {
    let result = (|| {
        let (asset, profile) =
            load_mount_asset_and_profile_sqlx(db, tenant_id, asset_id, profile_id)?;
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

        match persist_verified_unmount(db, tenant_id, &asset, &profile, &inspection.target_path) {
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
            let mut fields = mount_log_fields(db, tenant_id, asset_id, profile_id);
            fields.push(("target_path", update.status.target_path.clone()));
            fields.push(("state", format!("{:?}", update.status.state)));
            log_info("skill.unmount.success", "skill 卸载成功", &fields);
        }
        Err(error) => log_error(
            "skill.unmount.error",
            "skill 卸载失败",
            error,
            &mount_log_fields(db, tenant_id, asset_id, profile_id),
        ),
    }
    result
}

pub(super) fn load_mount_asset_and_profile_sqlx(
    db: &crate::backend::store::Database,
    tenant_id: &str,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<(Asset, TargetProfile)> {
    let pool = db.pool().clone();
    let tenant_id = tenant_id.to_string();
    let asset_id = asset_id.to_string();
    let profile_id = profile_id.to_string();
    db.block_on(async move {
        let asset = crate::backend::store::load_asset_sqlx(&pool, &tenant_id, &asset_id)
            .await?
            .ok_or_else(|| format!("asset not found: {asset_id}"))?;
        let profile = crate::backend::store::load_profile_sqlx(&pool, &tenant_id, &profile_id)
            .await?
            .ok_or_else(|| format!("profile not found: {profile_id}"))?;
        AppResult::Ok((asset, profile))
    })
}

fn load_mount_target_sqlx(
    db: &crate::backend::store::Database,
    tenant_id: &str,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<(Asset, Source, TargetProfile)> {
    let pool = db.pool().clone();
    let tenant_id = tenant_id.to_string();
    let asset_id = asset_id.to_string();
    let profile_id = profile_id.to_string();
    db.block_on(async move {
        let asset = crate::backend::store::load_asset_sqlx(&pool, &tenant_id, &asset_id)
            .await?
            .ok_or_else(|| format!("asset not found: {asset_id}"))?;
        let source = crate::backend::store::load_source_sqlx(&pool, &tenant_id, &asset.source_id)
            .await?
            .ok_or_else(|| format!("source not found: {}", asset.source_id))?;
        let profile = crate::backend::store::load_profile_sqlx(&pool, &tenant_id, &profile_id)
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
    crate::backend::host_filesystem::HostFilesystem::current()
        .create_symlink(&source_path, target_path)
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
    tenant_id: &str,
    asset_id: Option<&str>,
) -> AppResult<()> {
    let (assets, profiles) = load_mount_status_inputs_sqlx(db, tenant_id)?;
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
    let filesystem = crate::backend::host_filesystem::HostFilesystem::current();
    let previous_kind = filesystem.symlink_kind(&target_path)?;
    filesystem.remove_symlink(&target_path)?;
    if let Err(error) = filesystem.create_symlink(&expected_source_path, &target_path) {
        filesystem
            .create_symlink_with_kind(&previous_link, &target_path, previous_kind)
            .ok();
        return Err(error);
    }

    let repaired = crate::backend::targeting::inspect_mount(profile, asset)?;
    if !matches!(
        repaired.state,
        crate::backend::targeting::PhysicalMountState::Mounted
    ) {
        filesystem.remove_symlink(&target_path).ok();
        filesystem
            .create_symlink_with_kind(&previous_link, &target_path, previous_kind)
            .ok();
        return Err(format!(
            "ghost symlink repair verification failed: {}",
            repaired.target_path
        ));
    }
    Ok(repaired)
}

fn persist_verified_mount(
    db: &crate::backend::store::Database,
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
    db.block_on(async {
        crate::backend::store::persist_verified_mount_sqlx(db.pool(), tenant_id, &state, strategy)
            .await
    })
}

fn persist_verified_unmount(
    db: &crate::backend::store::Database,
    tenant_id: &str,
    asset: &Asset,
    profile: &TargetProfile,
    target_path: &str,
) -> AppResult<AssetMount> {
    db.block_on(async {
        crate::backend::store::persist_verified_unmount_sqlx(
            db.pool(),
            tenant_id,
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
    if !crate::backend::host_filesystem::HostFilesystem::current()
        .is_within(target_path, &target_dir)
    {
        return Err(format!(
            "refusing to write outside profile target directory: {}",
            target_path.display()
        ));
    }
    Ok(())
}

fn remove_created_mount_symlink(target_path: &Path) -> AppResult<()> {
    let metadata = fs::symlink_metadata(target_path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_symlink() {
        return Ok(());
    }
    crate::backend::host_filesystem::HostFilesystem::current().remove_symlink(target_path)
}

fn remove_mounted_symlink(target_path: &str) -> AppResult<()> {
    let path = Path::new(target_path);
    crate::backend::host_filesystem::HostFilesystem::current().remove_symlink(path)
}

pub(crate) fn asset_mount_status(
    asset_id: &str,
    profile_id: &str,
    inspection: crate::backend::targeting::MountInspection,
) -> AssetMountStatus {
    let display_target_dir = display_path_or_original(&inspection.target_dir);
    let display_target_path = display_path_or_original(&inspection.target_path);
    let display_linked_source = inspection
        .linked_source
        .as_deref()
        .map(display_path_or_original);
    AssetMountStatus {
        asset_id: asset_id.to_string(),
        profile_id: profile_id.to_string(),
        target_dir: inspection.target_dir,
        target_path: inspection.target_path,
        display_target_dir,
        display_target_path,
        display_linked_source,
        state: PhysicalMountStateDto::from(inspection.state),
        linked_source: inspection.linked_source,
    }
}
