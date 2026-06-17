use crate::backend::models::{Asset, AssetFormat, TargetProfile};
use crate::backend::{dto::AppResult, path_utils::expand_path};
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
        Err(error) => return Err(error.to_string()),
    };

    if !metadata.file_type().is_symlink() {
        let state = if same_path(&target_path, &source_path) {
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

pub(crate) fn canonical_source_path(asset: &Asset) -> AppResult<PathBuf> {
    expand_path(&asset.absolute_path)?
        .canonicalize()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::models::{AppKind, AssetKind, DeploymentStrategy, ProfileSafety, RuleSet};

    #[test]
    fn inspect_mount_treats_source_at_target_path_as_not_mounted() {
        let target_root = unique_temp_dir("assetiweave-app-local-target");
        let asset_path = target_root.join("code-review-and-quality");
        fs::create_dir_all(&asset_path).expect("create app-local skill");
        let asset = test_asset("code-review-and-quality", &asset_path);
        let profile = test_profile(&target_root);

        let inspection = inspect_mount(&profile, &asset).expect("inspect app-local skill");

        fs::remove_dir_all(&target_root).ok();
        assert_eq!(inspection.state, PhysicalMountState::NotMounted);
        assert_eq!(inspection.linked_source, None);
    }

    #[test]
    fn inspect_mount_treats_unrelated_existing_target_as_conflict() {
        let source_root = unique_temp_dir("assetiweave-conflict-source");
        let target_root = unique_temp_dir("assetiweave-conflict-target");
        let asset_path = source_root.join("code-review-and-quality");
        fs::create_dir_all(&asset_path).expect("create source skill");
        fs::create_dir_all(target_root.join("code-review-and-quality"))
            .expect("create conflicting target");
        let asset = test_asset("code-review-and-quality", &asset_path);
        let profile = test_profile(&target_root);

        let inspection = inspect_mount(&profile, &asset).expect("inspect conflicting skill");

        fs::remove_dir_all(&source_root).ok();
        fs::remove_dir_all(&target_root).ok();
        assert_eq!(inspection.state, PhysicalMountState::Conflict);
        assert_eq!(inspection.linked_source, None);
    }

    fn test_asset(name: &str, absolute_path: &Path) -> Asset {
        Asset {
            id: format!("asset-{name}"),
            source_id: "codex-skills".to_string(),
            name: name.to_string(),
            kind: AssetKind::Skill,
            format: AssetFormat::Directory,
            relative_path: name.to_string(),
            absolute_path: absolute_path.to_string_lossy().to_string(),
            entry_file: Some(format!("{name}/SKILL.md")),
            description: None,
            content_hash: None,
            discovered_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn test_profile(target_root: &Path) -> TargetProfile {
        TargetProfile {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            app_kind: AppKind::Codex,
            target_paths: vec![target_root.to_string_lossy().to_string()],
            supported_kinds: vec![AssetKind::Skill],
            deployment_strategy: DeploymentStrategy::SymlinkToSource,
            enabled: true,
            include: RuleSet {
                kinds: vec![AssetKind::Skill],
                tags: vec![],
                groups: vec![],
                sources: vec![],
                path_patterns: vec![],
            },
            exclude: RuleSet {
                kinds: vec![],
                tags: vec![],
                groups: vec![],
                sources: vec![],
                path_patterns: vec![],
            },
            safety: ProfileSafety {
                allow_remove: false,
                allow_overwrite: false,
            },
        }
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()))
    }
}
