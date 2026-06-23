use super::*;
use crate::backend::models::{
    AppKind, Asset, AssetFormat, AssetKind, AssetMount, DeploymentStrategy, ProfileSafety, RuleSet,
    TargetProfile,
};

#[test]
fn build_plan_only_uses_enabled_mounts() {
    let assets = vec![test_asset("asset-a"), test_asset("asset-b")];
    let profiles = vec![test_profile("codex", true)];
    let mounts = vec![test_mount("asset-a", "codex")];

    let plan = build_plan(&assets, &profiles, &mounts, None);

    assert_eq!(plan.actions.len(), 1);
    assert_eq!(plan.actions[0].asset_id.as_deref(), Some("asset-a"));
    assert_eq!(plan.summary.create_count, 1);
}

#[test]
fn build_plan_filters_requested_profile() {
    let assets = vec![test_asset("asset-a")];
    let profiles = vec![test_profile("codex", true), test_profile("claude", true)];
    let mounts = vec![
        test_mount("asset-a", "codex"),
        test_mount("asset-a", "claude"),
    ];

    let plan = build_plan(&assets, &profiles, &mounts, Some("claude"));

    assert_eq!(plan.actions.len(), 1);
    assert_eq!(plan.actions[0].profile_id, "claude");
}

#[test]
fn build_plan_targets_profile_directory_directly() {
    let asset = test_asset("asset-a");
    let profile = test_profile("codex", true);
    let expected_target_path = std::path::Path::new(&profile.target_paths[0]).join(&asset.name);
    let plan = build_plan(
        &[asset],
        &[profile],
        &[test_mount("asset-a", "codex")],
        None,
    );

    assert_eq!(plan.actions.len(), 1);
    assert_eq!(
        plan.actions[0].target_path,
        expected_target_path.to_string_lossy()
    );
}

fn test_asset(id: &str) -> Asset {
    let absolute_path = std::env::temp_dir().join(format!(
        "assetiweave-plan-source-{id}-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&absolute_path).expect("create test asset source");
    Asset {
        id: id.to_string(),
        source_id: "source-a".to_string(),
        name: id.to_string(),
        kind: AssetKind::Skill,
        format: AssetFormat::Directory,
        relative_path: id.to_string(),
        absolute_path: absolute_path.to_string_lossy().to_string(),
        entry_file: None,
        description: None,
        content_hash: None,
        discovered_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

fn test_profile(id: &str, enabled: bool) -> TargetProfile {
    TargetProfile {
        id: id.to_string(),
        name: id.to_string(),
        app_kind: AppKind::Custom,
        target_paths: vec![std::env::temp_dir()
            .join(format!(
                "assetiweave-plan-test-{id}-{}",
                uuid::Uuid::new_v4()
            ))
            .to_string_lossy()
            .to_string()],
        supported_kinds: vec![AssetKind::Skill],
        deployment_strategy: DeploymentStrategy::SymlinkToSource,
        enabled,
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

fn test_mount(asset_id: &str, profile_id: &str) -> AssetMount {
    AssetMount {
        asset_id: asset_id.to_string(),
        profile_id: profile_id.to_string(),
        enabled: true,
        strategy: DeploymentStrategy::SymlinkToSource,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}
