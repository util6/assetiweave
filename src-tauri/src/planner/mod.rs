use crate::targeting::{self, PhysicalMountState};
use assetiweave_core::{
    Asset, AssetKind, AssetMount, DeploymentAction, DeploymentActionType, DeploymentPlan,
    DeploymentPlanSummary, RiskLevel, TargetProfile,
};
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;

pub(crate) fn build_plan(
    assets: &[Asset],
    profiles: &[TargetProfile],
    enabled_mounts: &[AssetMount],
    requested_profile_id: Option<&str>,
) -> DeploymentPlan {
    let mut actions = Vec::new();
    let asset_map: HashMap<&str, &Asset> = assets
        .iter()
        .map(|asset| (asset.id.as_str(), asset))
        .collect();
    let profile_map: HashMap<&str, &TargetProfile> = profiles
        .iter()
        .map(|profile| (profile.id.as_str(), profile))
        .collect();

    for mount in enabled_mounts {
        if requested_profile_id.is_some_and(|requested| requested != mount.profile_id) {
            continue;
        }
        let Some(profile) = profile_map.get(mount.profile_id.as_str()) else {
            continue;
        };
        let Some(asset) = asset_map.get(mount.asset_id.as_str()) else {
            continue;
        };
        let supported = profile.supported_kinds.contains(&asset.kind)
            && profile.include.kinds.contains(&asset.kind);
        let Ok(inspection) = targeting::inspect_mount(profile, asset) else {
            continue;
        };
        let target_path = inspection.target_path;
        let action_type = if !profile.enabled {
            DeploymentActionType::Skip
        } else if !supported || matches!(asset.kind, AssetKind::Unclassified) {
            DeploymentActionType::Skip
        } else if matches!(inspection.state, PhysicalMountState::Mounted) {
            DeploymentActionType::Skip
        } else if matches!(
            inspection.state,
            PhysicalMountState::Conflict | PhysicalMountState::Broken
        ) {
            DeploymentActionType::Conflict
        } else {
            DeploymentActionType::Create
        };
        let reason = if !profile.enabled {
            format!("{} 已禁用，跳过已启用挂载关系", profile.name)
        } else if matches!(inspection.state, PhysicalMountState::Mounted) {
            "目标软链接已指向当前源资产".to_string()
        } else if matches!(action_type, DeploymentActionType::Skip) {
            format!(
                "{} 不支持 {:?} 或未命中 include 规则",
                profile.name, asset.kind
            )
        } else if matches!(action_type, DeploymentActionType::Conflict) {
            "目标路径已存在，当前版本默认不覆盖非本应用管理的文件".to_string()
        } else {
            format!(
                "{} 已启用挂载，将以 {:?} 投影到目标目录",
                profile.name, mount.strategy
            )
        };

        actions.push(DeploymentAction {
            id: Uuid::new_v4().to_string(),
            action_type,
            asset_id: Some(asset.id.clone()),
            profile_id: profile.id.clone(),
            source_path: Some(asset.absolute_path.clone()),
            target_path,
            strategy: mount.strategy,
            reason,
            risk: match action_type {
                DeploymentActionType::Skip => RiskLevel::Low,
                DeploymentActionType::Conflict => RiskLevel::High,
                _ => RiskLevel::Medium,
            },
            selectable: matches!(
                action_type,
                DeploymentActionType::Create | DeploymentActionType::Update
            ),
        });
    }

    let summary = DeploymentPlanSummary {
        create_count: actions
            .iter()
            .filter(|action| matches!(action.action_type, DeploymentActionType::Create))
            .count() as u32,
        update_count: actions
            .iter()
            .filter(|action| matches!(action.action_type, DeploymentActionType::Update))
            .count() as u32,
        remove_count: actions
            .iter()
            .filter(|action| matches!(action.action_type, DeploymentActionType::Remove))
            .count() as u32,
        skip_count: actions
            .iter()
            .filter(|action| matches!(action.action_type, DeploymentActionType::Skip))
            .count() as u32,
        conflict_count: actions
            .iter()
            .filter(|action| matches!(action.action_type, DeploymentActionType::Conflict))
            .count() as u32,
    };

    DeploymentPlan {
        id: Uuid::new_v4().to_string(),
        created_at: Utc::now().to_rfc3339(),
        profile_id: requested_profile_id.map(str::to_string),
        actions,
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assetiweave_core::{AppKind, AssetFormat, DeploymentStrategy, ProfileSafety, RuleSet};

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
}
