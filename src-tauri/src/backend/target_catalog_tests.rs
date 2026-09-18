use super::*;
use crate::backend::models::{AppKind, AssetKind};

#[test]
fn builtin_descriptors_cover_legacy_app_kinds() {
    let catalog = TargetCatalog::builtin().expect("builtin target catalog");
    for app_kind in [
        AppKind::Codex,
        AppKind::Claude,
        AppKind::Cursor,
        AppKind::OpenCode,
        AppKind::Gemini,
        AppKind::Antigravity,
        AppKind::OpenClaw,
        AppKind::Kiro,
        AppKind::Zcode,
        AppKind::Qoder,
        AppKind::Hermes,
        AppKind::Custom,
    ] {
        let descriptor = catalog
            .descriptors()
            .iter()
            .find(|descriptor| descriptor.app_kind_compat == Some(app_kind))
            .expect("legacy app kind descriptor");
        assert!(descriptor.supported_kinds.contains(&AssetKind::Skill));
    }
}

#[test]
fn a_new_provider_can_be_loaded_without_core_enum_changes() {
    let catalog = TargetCatalog::from_descriptors(vec![TargetProfileDescriptor {
        id: "fixture-agent".to_string(),
        name: "Fixture Agent".to_string(),
        app_kind_compat: None,
        default_targets: vec![crate::backend::models::TargetPathRule {
            asset_kind: AssetKind::Skill,
            path: "~/fixture-agent/skills".to_string(),
        }],
        supported_kinds: vec![AssetKind::Skill],
        deployment_strategy: crate::backend::models::DeploymentStrategy::SymlinkToSource,
        icon: None,
    }])
    .expect("fixture descriptor");
    assert_eq!(
        catalog.descriptor("fixture-agent").unwrap().name,
        "Fixture Agent"
    );
}

#[test]
fn invalid_provider_refresh_input_is_rejected_before_publication() {
    let error = TargetCatalog::from_descriptors(vec![TargetProfileDescriptor {
        id: "fixture-agent".to_string(),
        name: "Fixture Agent".to_string(),
        app_kind_compat: None,
        default_targets: vec![crate::backend::models::TargetPathRule {
            asset_kind: AssetKind::Skill,
            path: "  ".to_string(),
        }],
        supported_kinds: vec![AssetKind::Skill],
        deployment_strategy: crate::backend::models::DeploymentStrategy::SymlinkToSource,
        icon: None,
    }])
    .expect_err("empty provider target path must fail validation");

    assert!(error.to_string().contains("empty target path"));
}

#[test]
fn equal_specificity_target_paths_from_different_providers_are_rejected() {
    let descriptor = |id: &str| TargetProfileDescriptor {
        id: id.to_string(),
        name: id.to_string(),
        app_kind_compat: None,
        default_targets: vec![crate::backend::models::TargetPathRule {
            asset_kind: AssetKind::Skill,
            path: "~/shared/skills".to_string(),
        }],
        supported_kinds: vec![AssetKind::Skill],
        deployment_strategy: crate::backend::models::DeploymentStrategy::SymlinkToSource,
        icon: None,
    };

    let error = TargetCatalog::from_descriptors(vec![descriptor("first"), descriptor("second")])
        .expect_err("ambiguous target path must fail validation");
    assert!(error.to_string().contains("same target path"));
}

#[test]
fn app_owned_override_directory_extends_the_builtin_catalog() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-target-overrides-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create override directory");
    std::fs::write(
        root.join("fixture-agent.json"),
        serde_json::to_vec_pretty(&TargetProfileDescriptor {
            id: "fixture-agent".to_string(),
            name: "Fixture Agent".to_string(),
            app_kind_compat: None,
            default_targets: vec![crate::backend::models::TargetPathRule {
                asset_kind: AssetKind::Skill,
                path: "~/fixture-agent/skills".to_string(),
            }],
            supported_kinds: vec![AssetKind::Skill],
            deployment_strategy: crate::backend::models::DeploymentStrategy::SymlinkToSource,
            icon: None,
        })
        .expect("encode descriptor"),
    )
    .expect("write override");

    let catalog = TargetCatalog::load_with_overrides(&root).expect("load override catalog");
    assert_eq!(
        catalog
            .descriptor("fixture-agent")
            .expect("fixture descriptor")
            .name,
        "Fixture Agent"
    );
    assert!(catalog.descriptor("codex").is_some());

    std::fs::remove_dir_all(root).ok();
}
