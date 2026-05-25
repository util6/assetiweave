use crate::path_utils::expand_path;
use assetiweave_core::{
    Asset, AssetFormat, AssetKind, DeploymentAction, DeploymentActionType, DeploymentPlan,
    DeploymentPlanSummary, RiskLevel, TargetProfile,
};
use chrono::Utc;
use std::{path::Path, path::PathBuf};
use uuid::Uuid;

pub(crate) fn build_plan(
    assets: &[Asset],
    profiles: &[TargetProfile],
    requested_profile_id: Option<&str>,
) -> DeploymentPlan {
    let mut actions = Vec::new();

    for profile in profiles {
        if requested_profile_id.is_some_and(|requested| requested != profile.id) || !profile.enabled
        {
            continue;
        }
        let Some(target_root) = profile.target_paths.first() else {
            continue;
        };

        for asset in assets {
            let strategy = profile.deployment_strategy;
            let supported = profile.supported_kinds.contains(&asset.kind)
                && profile.include.kinds.contains(&asset.kind);
            let target_path = plan_target_path(target_root, profile, asset);
            let target_exists = Path::new(&target_path).exists();
            let action_type = if !supported || matches!(asset.kind, AssetKind::Unclassified) {
                DeploymentActionType::Skip
            } else if target_exists {
                DeploymentActionType::Conflict
            } else {
                DeploymentActionType::Create
            };
            let reason = if matches!(action_type, DeploymentActionType::Skip) {
                format!(
                    "{} 不支持 {:?} 或未命中 include 规则",
                    profile.name, asset.kind
                )
            } else if matches!(action_type, DeploymentActionType::Conflict) {
                "目标路径已存在，MVP 默认不覆盖非本应用管理的文件".to_string()
            } else {
                format!(
                    "{} 支持 {:?}，将以 {:?} 投影到目标目录",
                    profile.name, asset.kind, strategy
                )
            };

            actions.push(DeploymentAction {
                id: Uuid::new_v4().to_string(),
                action_type,
                asset_id: Some(asset.id.clone()),
                profile_id: profile.id.clone(),
                source_path: Some(asset.absolute_path.clone()),
                target_path,
                strategy,
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

fn plan_target_path(target_root: &str, profile: &TargetProfile, asset: &Asset) -> String {
    let bucket = match asset.kind {
        AssetKind::Skill => "skills",
        AssetKind::Prompt => "prompts",
        AssetKind::Rule => "rules",
        AssetKind::Custom => "custom",
        _ => "assets",
    };
    let name = if matches!(asset.format, AssetFormat::Directory) {
        asset.name.clone()
    } else {
        Path::new(&asset.relative_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| format!("{}.{}", asset.name, extension))
            .unwrap_or_else(|| asset.name.clone())
    };
    let root = expand_path(target_root).unwrap_or_else(|_| PathBuf::from(target_root));
    root.join(&profile.id)
        .join(bucket)
        .join(name)
        .to_string_lossy()
        .to_string()
}
