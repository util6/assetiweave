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
fn target_path_uses_the_profile_rule_for_the_asset_kind() {
    let root = unique_temp_dir("assetiweave-multi-target");
    let profile = TargetProfile {
        id: "fixture".to_string(),
        name: "Fixture".to_string(),
        app_kind: None,
        target_provider_id: "fixture".to_string(),
        target_paths: vec![
            root.join("skills").to_string_lossy().to_string(),
            root.join("prompts").to_string_lossy().to_string(),
        ],
        supported_kinds: vec![AssetKind::Skill, AssetKind::Prompt],
        deployment_strategy: DeploymentStrategy::SymlinkToSource,
        enabled: true,
        include: RuleSet {
            kinds: vec![AssetKind::Skill, AssetKind::Prompt],
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
    };
    let mut asset = test_asset("prompt", &root.join("source").join("prompt"));
    asset.kind = AssetKind::Prompt;
    asset.format = AssetFormat::Markdown;

    let target = target_path(&profile, &asset).expect("prompt target path");
    assert!(target.starts_with(root.join("prompts")));
}

#[test]
fn inspect_mount_treats_unrelated_existing_target_as_conflict() {
    let source_root = unique_temp_dir("assetiweave-conflict-source");
    let target_root = unique_temp_dir("assetiweave-conflict-target");
    let asset_path = source_root.join("code-review-and-quality");
    fs::create_dir_all(&asset_path).expect("create source skill");
    fs::create_dir_all(target_root.join("code-review-and-quality"))
        .expect("create conflicting target");
    fs::write(asset_path.join("SKILL.md"), "description: source").expect("write source skill");
    fs::write(
        target_root.join("code-review-and-quality").join("SKILL.md"),
        "description: target",
    )
    .expect("write conflicting skill");
    let asset = test_asset("code-review-and-quality", &asset_path);
    let profile = test_profile(&target_root);

    let inspection = inspect_mount(&profile, &asset).expect("inspect conflicting skill");

    fs::remove_dir_all(&source_root).ok();
    fs::remove_dir_all(&target_root).ok();
    assert_eq!(inspection.state, PhysicalMountState::Conflict);
    assert_eq!(inspection.linked_source, None);
}

#[test]
fn inspect_mount_treats_identical_existing_target_as_not_mounted() {
    let source_root = unique_temp_dir("assetiweave-identical-source");
    let target_root = unique_temp_dir("assetiweave-identical-target");
    let asset_path = source_root.join("code-review-and-quality");
    let target_path = target_root.join("code-review-and-quality");
    fs::create_dir_all(&asset_path).expect("create source skill");
    fs::create_dir_all(&target_path).expect("create target skill");
    fs::write(asset_path.join("SKILL.md"), "description: same").expect("write source skill");
    fs::write(target_path.join("SKILL.md"), "description: same").expect("write target skill");
    let mut asset = test_asset("code-review-and-quality", &asset_path);
    asset.content_hash = Some(crate::backend::path_utils::hash_path(&asset_path).unwrap());
    let profile = test_profile(&target_root);

    let inspection = inspect_mount(&profile, &asset).expect("inspect identical target");

    fs::remove_dir_all(&source_root).ok();
    fs::remove_dir_all(&target_root).ok();
    assert_eq!(inspection.state, PhysicalMountState::NotMounted);
    assert_eq!(inspection.linked_source, None);
}

fn test_asset(name: &str, absolute_path: &Path) -> Asset {
    Asset {
        id: format!("asset-{name}"),
        source_id: "codex-skills".to_string(),
        name: name.to_string(),
        kind: AssetKind::Skill,
        detector_id: "legacy.classifier".to_string(),
        detector_version: 1,
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
        app_kind: Some(AppKind::Codex),
        target_provider_id: "codex".to_string(),
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
