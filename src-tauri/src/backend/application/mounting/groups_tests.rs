use super::*;
use crate::backend::domain::catalog::Asset;
use crate::backend::domain::mounting::{DeploymentStrategy, TargetProfile};

fn fixture_asset() -> Asset {
    Asset {
        id: "asset-1".to_string(),
        source_id: "missing-source".to_string(),
        kind: AssetKind::Skill,
        name: "fixture".to_string(),
        detector_id: "fixture".to_string(),
        detector_version: 1,
        format: crate::backend::domain::AssetFormat::Directory,
        relative_path: "fixture".to_string(),
        absolute_path: "/fixture".to_string(),
        entry_file: None,
        description: None,
        content_hash: None,
        discovered_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

fn fixture_profile() -> TargetProfile {
    TargetProfile {
        id: "profile-1".to_string(),
        name: "fixture".to_string(),
        app_kind: None,
        target_provider_id: "fixture".to_string(),
        target_paths: vec![],
        supported_kinds: vec![AssetKind::Skill],
        deployment_strategy: DeploymentStrategy::SymlinkToSource,
        enabled: true,
        include: crate::backend::domain::RuleSet {
            kinds: vec![AssetKind::Skill],
            tags: vec![],
            groups: vec![],
            sources: vec![],
            path_patterns: vec![],
        },
        exclude: crate::backend::domain::RuleSet {
            kinds: vec![],
            tags: vec![],
            groups: vec![],
            sources: vec![],
            path_patterns: vec![],
        },
        safety: crate::backend::domain::ProfileSafety {
            allow_remove: false,
            allow_overwrite: false,
        },
    }
}

#[test]
fn exclusive_mount_candidate_reports_missing_source_as_typed_not_found() {
    let error =
        validate_exclusive_mount_candidate(&fixture_asset(), &fixture_profile(), &HashMap::new())
            .expect_err("missing source must fail");

    assert!(matches!(error, AppError::NotFound(_)));
}
