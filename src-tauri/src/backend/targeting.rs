use crate::backend::models::{Asset, AssetFormat, TargetProfile};
use crate::backend::{
    path_utils::{expand_path, hash_path},
    runtime::{AppError, AppResult},
};
use std::{
    fs::{self, Metadata},
    io::ErrorKind,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhysicalMountState {
    Mounted,
    NotMounted,
    Conflict,
    Broken,
}

#[derive(Debug, Clone)]
pub(crate) struct MountInspection {
    pub(crate) target_dir: String,
    pub(crate) target_path: String,
    pub(crate) state: PhysicalMountState,
    pub(crate) linked_source: Option<String>,
}

pub(crate) fn target_dir(profile: &TargetProfile) -> AppResult<PathBuf> {
    let target_root = profile
        .target_paths
        .first()
        .ok_or_else(|| AppError::Validation(format!("Profile {} 未配置目标路径", profile.name)))?;
    Ok(expand_path(target_root)?)
}

pub(crate) fn target_path(profile: &TargetProfile, asset: &Asset) -> AppResult<PathBuf> {
    Ok(target_dir_for_profile_asset(profile, asset)?.join(target_link_name(asset)))
}

pub(crate) fn inspect_mount(profile: &TargetProfile, asset: &Asset) -> AppResult<MountInspection> {
    let target_dir_path = target_dir_for_profile_asset(profile, asset)?;
    let target_path = target_path(profile, asset)?;
    let target_dir_label = target_dir_path.to_string_lossy().to_string();

    let source_path = canonical_source_path(asset)?;
    let metadata = match fs::symlink_metadata(&target_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(MountInspection {
                target_dir: target_dir_label,
                target_path: target_path.to_string_lossy().to_string(),
                state: PhysicalMountState::NotMounted,
                linked_source: None,
            });
        }
        Err(error) => return Err(AppError::External(error.to_string())),
    };

    if !metadata.file_type().is_symlink() {
        let state = if same_path(&target_path, &source_path)
            || target_content_matches_asset_source(asset, &source_path, &target_path, &metadata)
                .unwrap_or(false)
        {
            PhysicalMountState::NotMounted
        } else {
            PhysicalMountState::Conflict
        };
        return Ok(MountInspection {
            target_dir: target_dir_label,
            target_path: target_path.to_string_lossy().to_string(),
            state,
            linked_source: None,
        });
    }

    let linked_path =
        fs::read_link(&target_path).map_err(|error| AppError::External(error.to_string()))?;
    let resolved_link = resolve_link_target(&target_path, &linked_path);
    let state = if same_path(&resolved_link, &source_path) {
        PhysicalMountState::Mounted
    } else if !resolved_link.exists() {
        PhysicalMountState::Broken
    } else {
        PhysicalMountState::Conflict
    };

    Ok(MountInspection {
        target_dir: target_dir_label,
        target_path: target_path.to_string_lossy().to_string(),
        state,
        linked_source: Some(resolved_link.to_string_lossy().to_string()),
    })
}

pub(crate) fn inspect_mount_with_catalog(
    profile: &TargetProfile,
    asset: &Asset,
    catalog: &crate::backend::target_catalog::TargetCatalog,
) -> AppResult<MountInspection> {
    let target_dir_path = target_dir_for_asset(profile, asset, catalog)?;
    let target_path = target_dir_path.join(target_link_name(asset));
    let target_dir_label = target_dir_path.to_string_lossy().to_string();

    let source_path = canonical_source_path(asset)?;
    let metadata = match fs::symlink_metadata(&target_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(MountInspection {
                target_dir: target_dir_label,
                target_path: target_path.to_string_lossy().to_string(),
                state: PhysicalMountState::NotMounted,
                linked_source: None,
            });
        }
        Err(error) => return Err(AppError::External(error.to_string())),
    };

    if !metadata.file_type().is_symlink() {
        let state = if same_path(&target_path, &source_path)
            || target_content_matches_asset_source(asset, &source_path, &target_path, &metadata)
                .unwrap_or(false)
        {
            PhysicalMountState::NotMounted
        } else {
            PhysicalMountState::Conflict
        };
        return Ok(MountInspection {
            target_dir: target_dir_label,
            target_path: target_path.to_string_lossy().to_string(),
            state,
            linked_source: None,
        });
    }

    let linked_path =
        fs::read_link(&target_path).map_err(|error| AppError::External(error.to_string()))?;
    let resolved_link = resolve_link_target(&target_path, &linked_path);
    let state = if same_path(&resolved_link, &source_path) {
        PhysicalMountState::Mounted
    } else if !resolved_link.exists() {
        PhysicalMountState::Broken
    } else {
        PhysicalMountState::Conflict
    };

    Ok(MountInspection {
        target_dir: target_dir_label,
        target_path: target_path.to_string_lossy().to_string(),
        state,
        linked_source: Some(resolved_link.to_string_lossy().to_string()),
    })
}

fn target_dir_for_asset(
    profile: &TargetProfile,
    asset: &Asset,
    catalog: &crate::backend::target_catalog::TargetCatalog,
) -> AppResult<PathBuf> {
    let descriptor = catalog.require_descriptor(&profile.target_provider_id)?;
    let target_index = descriptor
        .default_targets
        .iter()
        .position(|target| target.asset_kind == asset.kind);
    let target_root = target_index
        .and_then(|index| profile.target_paths.get(index))
        .or_else(|| profile.target_paths.first())
        .ok_or_else(|| AppError::Validation(format!("Profile {} 未配置目标路径", profile.name)))?;
    expand_path(target_root)
}

fn target_dir_for_profile_asset(profile: &TargetProfile, asset: &Asset) -> AppResult<PathBuf> {
    let target_index = profile
        .supported_kinds
        .iter()
        .position(|kind| *kind == asset.kind);
    let target_root = target_index
        .and_then(|index| profile.target_paths.get(index))
        .or_else(|| profile.target_paths.first())
        .ok_or_else(|| AppError::Validation(format!("Profile {} 未配置目标路径", profile.name)))?;
    expand_path(target_root)
}

fn target_link_name(asset: &Asset) -> String {
    if matches!(asset.format, AssetFormat::Directory) {
        return asset.name.clone();
    }

    Path::new(&asset.relative_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!("{}.{}", asset.name, extension))
        .unwrap_or_else(|| asset.name.clone())
}

fn resolve_link_target(link_path: &Path, linked_path: &Path) -> PathBuf {
    if linked_path.is_absolute() {
        return linked_path.to_path_buf();
    }

    link_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(linked_path)
}

fn same_path(left: &Path, right: &Path) -> bool {
    crate::backend::host_filesystem::HostFilesystem::current().same_path(left, right)
}

pub(crate) fn canonical_source_path(asset: &Asset) -> AppResult<PathBuf> {
    expand_path(&asset.absolute_path)?
        .canonicalize()
        .map_err(|error| AppError::External(error.to_string()))
}

pub(crate) fn target_is_asset_source(asset: &Asset, target_path: &Path) -> AppResult<bool> {
    let source_path = canonical_source_path(asset)?;
    Ok(same_path(target_path, &source_path))
}

pub(crate) fn target_content_matches_asset(asset: &Asset, target_path: &Path) -> AppResult<bool> {
    let source_path = canonical_source_path(asset)?;
    let metadata =
        fs::symlink_metadata(target_path).map_err(|error| AppError::External(error.to_string()))?;
    target_content_matches_asset_source(asset, &source_path, target_path, &metadata)
}

fn target_content_matches_asset_source(
    asset: &Asset,
    source_path: &Path,
    target_path: &Path,
    target_metadata: &Metadata,
) -> AppResult<bool> {
    if target_metadata.file_type().is_symlink() {
        return Ok(false);
    }
    if source_path.is_dir() != target_metadata.is_dir() {
        return Ok(false);
    }

    let target_hash = hash_path(target_path)?;
    if let Some(content_hash) = asset.content_hash.as_ref().filter(|hash| !hash.is_empty()) {
        return Ok(content_hash == &target_hash);
    }

    Ok(hash_path(source_path)? == target_hash)
}

#[cfg(test)]
#[path = "targeting_tests.rs"]
mod tests;
