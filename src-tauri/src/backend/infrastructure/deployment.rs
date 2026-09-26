use crate::backend::{
    domain::{Asset, DeploymentAction, DeploymentState, DeploymentStrategy, TargetProfile},
    infrastructure::{host_filesystem::HostFilesystem, path_utils::expand_path},
};
use chrono::Utc;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub(crate) enum DeploymentError {
    Conflict(String),
    Failure(String),
}

pub(crate) fn execute_physical_deployment(
    profile: &TargetProfile,
    asset: &Asset,
    action: &DeploymentAction,
    managed_target: bool,
) -> Result<DeploymentState, DeploymentError> {
    let target_path = PathBuf::from(&action.target_path);
    ensure_target_within_profile(profile, &target_path)?;
    let source_path = canonical_source_path(asset)?;

    let filesystem = HostFilesystem::current();
    if target_path.exists()
        && !managed_target
        && !target_can_be_replaced_with_asset(asset, &source_path, &target_path)?
    {
        return Err(DeploymentError::Conflict(format!(
            "目标已存在且不是 AssetIWeave 托管文件: {}",
            target_path.display()
        )));
    }

    let parent = target_path.parent().ok_or_else(|| {
        DeploymentError::Failure(format!("目标路径缺少父目录: {}", target_path.display()))
    })?;
    fs::create_dir_all(parent).map_err(|error| DeploymentError::Failure(error.to_string()))?;

    if target_path.exists() {
        filesystem
            .remove_path(&target_path)
            .map_err(|error| DeploymentError::Failure(error.to_string()))?;
    }

    match action.strategy {
        DeploymentStrategy::SymlinkToSource => filesystem
            .create_symlink(&source_path, &target_path)
            .map_err(|error| DeploymentError::Failure(error.to_string()))?,
        DeploymentStrategy::CopyToTarget => copy_asset(&source_path, &target_path)?,
        other => {
            return Err(DeploymentError::Failure(format!(
                "当前版本暂不支持 {:?} 部署策略",
                other
            )))
        }
    }

    Ok(DeploymentState {
        profile_id: profile.id.clone(),
        asset_id: asset.id.clone(),
        target_path: action.target_path.clone(),
        strategy: action.strategy,
        source_hash: asset.content_hash.clone().unwrap_or_default(),
        deployed_at: Utc::now().to_rfc3339(),
        managed_by: "assetiweave".to_string(),
    })
}

fn target_can_be_replaced_with_asset(
    asset: &Asset,
    source_path: &Path,
    target_path: &Path,
) -> Result<bool, DeploymentError> {
    let filesystem = HostFilesystem::current();
    if filesystem.same_path(target_path, source_path) {
        return Ok(false);
    }
    let metadata = fs::symlink_metadata(target_path)
        .map_err(|error| DeploymentError::Failure(error.to_string()))?;
    if metadata.file_type().is_symlink() || source_path.is_dir() != metadata.is_dir() {
        return Ok(false);
    }

    let target_hash = crate::backend::infrastructure::path_utils::hash_path(target_path)
        .map_err(|error| DeploymentError::Failure(error.to_string()))?;
    if let Some(content_hash) = asset.content_hash.as_ref().filter(|hash| !hash.is_empty()) {
        return Ok(content_hash == &target_hash);
    }

    let source_hash = crate::backend::infrastructure::path_utils::hash_path(source_path)
        .map_err(|error| DeploymentError::Failure(error.to_string()))?;
    Ok(source_hash == target_hash)
}

fn ensure_target_within_profile(
    profile: &TargetProfile,
    target_path: &Path,
) -> Result<(), DeploymentError> {
    let filesystem = HostFilesystem::current();
    let mut allowed = false;
    for root in &profile.target_paths {
        let expanded =
            expand_path(root).map_err(|error| DeploymentError::Failure(error.to_string()))?;
        if filesystem.is_within(target_path, &expanded) {
            allowed = true;
            break;
        }
    }
    if !allowed {
        return Err(DeploymentError::Failure(format!(
            "拒绝写入 Profile 目标目录外部: {}",
            target_path.display()
        )));
    }
    Ok(())
}

fn canonical_source_path(asset: &Asset) -> Result<PathBuf, DeploymentError> {
    expand_path(&asset.absolute_path)
        .map_err(|error| DeploymentError::Failure(error.to_string()))?
        .canonicalize()
        .map_err(|error| DeploymentError::Failure(error.to_string()))
}

fn copy_asset(source: &Path, target: &Path) -> Result<(), DeploymentError> {
    if source.is_dir() {
        HostFilesystem::current()
            .copy_dir(source, target)
            .map_err(|error| DeploymentError::Failure(error.to_string()))
    } else {
        fs::copy(source, target)
            .map(|_| ())
            .map_err(|error| DeploymentError::Failure(error.to_string()))
    }
}
