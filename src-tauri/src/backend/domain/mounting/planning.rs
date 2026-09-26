use super::{
    DeploymentAction, DeploymentActionType, DeploymentPlan, DeploymentPlanSummary,
    DeploymentStrategy, PhysicalMountState, RiskLevel,
};
use crate::backend::domain::AssetKind;

pub(crate) struct DeploymentPlanCandidate {
    pub(crate) asset_id: String,
    pub(crate) asset_kind: AssetKind,
    pub(crate) profile_id: String,
    pub(crate) profile_name: String,
    pub(crate) source_path: String,
    pub(crate) display_source_path: String,
    pub(crate) target_path: String,
    pub(crate) display_target_path: String,
    pub(crate) strategy: DeploymentStrategy,
    pub(crate) profile_enabled: bool,
    pub(crate) supported: bool,
    pub(crate) state: PhysicalMountState,
}

pub(crate) fn build_deployment_plan(
    candidates: Vec<DeploymentPlanCandidate>,
    requested_profile_id: Option<&str>,
) -> DeploymentPlan {
    let mut actions = Vec::new();
    for candidate in candidates {
        if requested_profile_id.is_some_and(|requested| requested != candidate.profile_id) {
            continue;
        }

        let action_type = if !candidate.profile_enabled {
            DeploymentActionType::Skip
        } else if !candidate.supported || matches!(candidate.asset_kind, AssetKind::Unclassified) {
            DeploymentActionType::Skip
        } else if matches!(candidate.state, PhysicalMountState::Mounted) {
            DeploymentActionType::Skip
        } else if matches!(
            candidate.state,
            PhysicalMountState::Conflict | PhysicalMountState::Broken
        ) {
            DeploymentActionType::Conflict
        } else {
            DeploymentActionType::Create
        };

        let reason = if !candidate.profile_enabled {
            format!("{} 已禁用，跳过已启用挂载关系", candidate.profile_name)
        } else if matches!(candidate.state, PhysicalMountState::Mounted) {
            "目标软链接已指向当前源资产".to_string()
        } else if matches!(action_type, DeploymentActionType::Skip) {
            format!(
                "{} 不支持 {:?} 或未命中 include 规则",
                candidate.profile_name, candidate.asset_kind
            )
        } else if matches!(action_type, DeploymentActionType::Conflict) {
            "目标路径已存在，当前版本默认不覆盖非本应用管理的文件".to_string()
        } else {
            format!(
                "{} 已启用挂载，将以 {:?} 投影到目标目录",
                candidate.profile_name, candidate.strategy
            )
        };

        actions.push(DeploymentAction {
            id: uuid::Uuid::new_v4().to_string(),
            action_type,
            asset_id: Some(candidate.asset_id),
            profile_id: candidate.profile_id,
            source_path: Some(candidate.source_path),
            display_source_path: Some(candidate.display_source_path),
            display_target_path: Some(candidate.display_target_path),
            target_path: candidate.target_path,
            strategy: candidate.strategy,
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
        create_count: count_actions(&actions, DeploymentActionType::Create),
        update_count: count_actions(&actions, DeploymentActionType::Update),
        remove_count: count_actions(&actions, DeploymentActionType::Remove),
        skip_count: count_actions(&actions, DeploymentActionType::Skip),
        conflict_count: count_actions(&actions, DeploymentActionType::Conflict),
    };

    DeploymentPlan {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        profile_id: requested_profile_id.map(str::to_string),
        actions,
        summary,
    }
}

fn count_actions(actions: &[DeploymentAction], action_type: DeploymentActionType) -> u32 {
    actions
        .iter()
        .filter(|action| action.action_type == action_type)
        .count() as u32
}
