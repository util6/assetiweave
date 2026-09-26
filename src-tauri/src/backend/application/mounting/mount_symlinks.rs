use crate::backend::application::catalog::catalog_ops::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::catalog::Asset;
use crate::backend::domain::mounting::{
    AssetMountObservation, AssetMountStatus, DeploymentStrategy, PhysicalMountStateDto,
    TargetProfile,
};
use crate::backend::infrastructure::path_utils::display_path_or_original;
use chrono::Utc;
use sqlx::SqlitePool;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn create_mount_symlink(
    asset: &Asset,
    profile: &TargetProfile,
    target_path: &Path,
) -> AppResult<()> {
    ensure_target_within_profile(profile, target_path)?;
    let source_path =
        crate::backend::application::mounting::targeting::canonical_source_path(asset)?;
    prepare_target_for_mount_symlink(asset, target_path)?;

    let parent = target_path
        .parent()
        .ok_or_else(|| {
            format!(
                "target path is missing parent directory: {}",
                target_path.display()
            )
        })
        .map_err(AppError::external)?;
    fs::create_dir_all(parent).map_err(AppError::external)?;
    Ok(
        crate::backend::infrastructure::host_filesystem::HostFilesystem::current()
            .create_symlink(&source_path, target_path)?,
    )
}

pub(crate) fn prepare_target_for_mount_symlink(asset: &Asset, target_path: &Path) -> AppResult<()> {
    let metadata = match fs::symlink_metadata(target_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(AppError::External(error.to_string())),
    };
    if metadata.file_type().is_symlink() {
        return Err(AppError::Conflict(format!(
            "target symlink already exists: {}",
            target_path.display()
        )));
    }
    if crate::backend::application::mounting::targeting::target_is_asset_source(asset, target_path)?
    {
        return Err(AppError::Conflict(format!(
            "target path is the asset source path: {}",
            target_path.display()
        )));
    }
    if !crate::backend::application::mounting::targeting::target_content_matches_asset(
        asset,
        target_path,
    )? {
        return Err(AppError::Conflict(format!(
            "target exists with different content: {}",
            target_path.display()
        )));
    }

    if metadata.is_dir() {
        Ok(fs::remove_dir_all(target_path).map_err(AppError::external)?)
    } else if metadata.is_file() {
        Ok(fs::remove_file(target_path).map_err(AppError::external)?)
    } else {
        Err(AppError::Conflict(format!(
            "unsupported target type for replacement: {}",
            target_path.display()
        )))
    }
}

pub(crate) async fn repair_ghost_mount_symlinks_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    asset_id: Option<&str>,
) -> AppResult<()> {
    let (assets, profiles) = load_mount_status_inputs_sqlx(pool, tenant_id).await?;
    repair_ghost_mount_symlinks_for_assets(&assets, &profiles, asset_id)
}

pub(crate) fn repair_ghost_mount_symlinks_for_assets(
    assets: &[Asset],
    profiles: &[TargetProfile],
    asset_id: Option<&str>,
) -> AppResult<()> {
    for asset in assets
        .iter()
        .filter(|asset| asset_id.map(|id| asset.id == id).unwrap_or(true))
    {
        for profile in profiles.iter().filter(|profile| profile.enabled) {
            let inspection =
                crate::backend::application::mounting::targeting::inspect_mount(profile, asset)?;
            repair_mounted_symlink_to_real_source(asset, profile, inspection)?;
        }
    }
    Ok(())
}

pub(crate) fn repair_mounted_symlink_to_real_source(
    asset: &Asset,
    profile: &TargetProfile,
    inspection: crate::backend::application::mounting::targeting::MountInspection,
) -> AppResult<crate::backend::application::mounting::targeting::MountInspection> {
    if !matches!(
        inspection.state,
        crate::backend::domain::PhysicalMountState::Mounted
    ) {
        return Ok(inspection);
    }

    let target_path = PathBuf::from(&inspection.target_path);
    let expected_source_path =
        crate::backend::application::mounting::targeting::canonical_source_path(asset)?;
    let linked_source = inspection
        .linked_source
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_default();
    if linked_source == expected_source_path {
        return Ok(inspection);
    }

    let metadata = fs::symlink_metadata(&target_path).map_err(AppError::external)?;
    if !metadata.file_type().is_symlink() {
        return Ok(inspection);
    }

    let previous_link = fs::read_link(&target_path).map_err(AppError::external)?;
    let filesystem = crate::backend::infrastructure::host_filesystem::HostFilesystem::current();
    let previous_kind = filesystem.symlink_kind(&target_path)?;
    filesystem.remove_symlink(&target_path)?;
    if let Err(error) = filesystem.create_symlink(&expected_source_path, &target_path) {
        filesystem
            .create_symlink_with_kind(&previous_link, &target_path, previous_kind)
            .ok();
        return Err(AppError::from(error));
    }

    let repaired = crate::backend::application::mounting::targeting::inspect_mount(profile, asset)?;
    if !matches!(
        repaired.state,
        crate::backend::domain::PhysicalMountState::Mounted
    ) {
        filesystem.remove_symlink(&target_path).ok();
        filesystem
            .create_symlink_with_kind(&previous_link, &target_path, previous_kind)
            .ok();
        return Err(AppError::Conflict(format!(
            "ghost symlink repair verification failed: {}",
            repaired.target_path
        )));
    }
    Ok(repaired)
}

pub(crate) fn ensure_target_within_profile(
    profile: &TargetProfile,
    target_path: &Path,
) -> AppResult<()> {
    let target_dir = crate::backend::application::mounting::targeting::target_dir(profile)?;
    if !crate::backend::infrastructure::host_filesystem::HostFilesystem::current()
        .is_within(target_path, &target_dir)
    {
        return Err(AppError::Conflict(format!(
            "refusing to write outside profile target directory: {}",
            target_path.display()
        )));
    }
    Ok(())
}

pub(crate) fn remove_created_mount_symlink(target_path: &Path) -> AppResult<()> {
    let metadata = fs::symlink_metadata(target_path).map_err(AppError::external)?;
    if !metadata.file_type().is_symlink() {
        return Ok(());
    }
    Ok(
        crate::backend::infrastructure::host_filesystem::HostFilesystem::current()
            .remove_symlink(target_path)?,
    )
}

pub(crate) fn remove_mounted_symlink(target_path: &str) -> AppResult<()> {
    let path = Path::new(target_path);
    Ok(
        crate::backend::infrastructure::host_filesystem::HostFilesystem::current()
            .remove_symlink(path)?,
    )
}

pub(crate) async fn sync_asset_mount_observations(
    pool: &SqlitePool,
    tenant_id: &str,
    asset_id: Option<&str>,
) -> AppResult<Vec<AssetMountStatus>> {
    repair_ghost_mount_symlinks_sqlx(pool, tenant_id, asset_id).await?;
    let statuses = scan_asset_mount_statuses_sqlx(pool, tenant_id, asset_id).await?;
    persist_asset_mount_observation_snapshot(pool, tenant_id, &statuses).await?;
    Ok(statuses)
}

pub(crate) async fn scan_asset_mount_statuses_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    asset_id: Option<&str>,
) -> AppResult<Vec<AssetMountStatus>> {
    let (assets, profiles) = load_mount_status_inputs_sqlx(pool, tenant_id).await?;
    inspect_asset_mount_statuses(&assets, &profiles, asset_id)
}

pub(crate) async fn load_mount_status_inputs_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<(Vec<Asset>, Vec<TargetProfile>)> {
    let assets = catalog_visible_assets_sqlx(pool, tenant_id, None).await?;
    let profiles = crate::backend::application::mounting::profile_ops::load_target_profiles_sqlx(
        pool, tenant_id,
    )
    .await?;
    Ok((assets, profiles))
}

pub(crate) async fn persist_asset_mount_observation_snapshot(
    pool: &SqlitePool,
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
    let assets = crate::backend::store::load_assets_sqlx(pool, tenant_id, None).await?;
    let profiles = crate::backend::application::mounting::profile_ops::load_target_profiles_sqlx(
        pool, tenant_id,
    )
    .await?;
    Ok(crate::backend::store::persist_asset_mount_snapshot_sqlx(
        pool,
        tenant_id,
        &observations,
        &assets,
        &profiles,
        statuses,
    )
    .await?)
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
            let inspection =
                crate::backend::application::mounting::targeting::inspect_mount(profile, asset)?;
            statuses.push(asset_mount_status(&asset.id, &profile.id, inspection));
        }
    }

    Ok(statuses)
}

pub(crate) fn asset_mount_status(
    asset_id: &str,
    profile_id: &str,
    inspection: crate::backend::application::mounting::targeting::MountInspection,
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
