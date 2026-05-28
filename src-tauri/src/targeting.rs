use crate::{path_utils::expand_path, types::AppResult};
use assetiweave_core::{Asset, AssetFormat, TargetProfile};
use std::{
    fs,
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
        .ok_or_else(|| format!("Profile {} 未配置目标路径", profile.name))?;
    expand_path(target_root)
}

pub(crate) fn target_path(profile: &TargetProfile, asset: &Asset) -> AppResult<PathBuf> {
    Ok(target_dir(profile)?.join(target_link_name(asset)))
}

pub(crate) fn inspect_mount(profile: &TargetProfile, asset: &Asset) -> AppResult<MountInspection> {
    let target_dir_path = target_dir(profile)?;
    let target_path = target_path(profile, asset)?;
    let target_dir_label = profile
        .target_paths
        .first()
        .cloned()
        .unwrap_or_else(|| target_dir_path.to_string_lossy().to_string());

    let source_path = expand_path(&asset.absolute_path)?;
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
        Err(error) => return Err(error.to_string()),
    };

    if !metadata.file_type().is_symlink() {
        return Ok(MountInspection {
            target_dir: target_dir_label,
            target_path: target_path.to_string_lossy().to_string(),
            state: PhysicalMountState::Conflict,
            linked_source: None,
        });
    }

    let linked_path = fs::read_link(&target_path).map_err(|error| error.to_string())?;
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
    normalize_path(left) == normalize_path(right)
}

fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
