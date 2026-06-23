use crate::backend::models::{
    Asset, AssetKind, AssetMount, DeploymentAction, DeploymentActionType, DeploymentPlan,
    DeploymentPlanSummary, RiskLevel, TargetProfile,
};
use crate::backend::targeting::PhysicalMountState;
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
        let Ok(inspection) = crate::backend::targeting::inspect_mount(profile, asset) else {
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
