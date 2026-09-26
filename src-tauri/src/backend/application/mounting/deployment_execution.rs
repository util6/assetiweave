use crate::backend::{
    application::{mounting::ExecutionResult, AppResult},
    domain::{Asset, DeploymentAction, DeploymentActionType, DeploymentPlan, TargetProfile},
    infrastructure::deployment::DeploymentError,
};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

pub(crate) async fn execute_deployment_plan(
    pool: &SqlitePool,
    tenant_id: &str,
    profiles: &[TargetProfile],
    assets: &[Asset],
    plan: &DeploymentPlan,
    requested_action_ids: Option<&[String]>,
    target_catalog: &crate::backend::application::mounting::target_catalog::TargetCatalog,
) -> AppResult<ExecutionResult> {
    let requested: Option<HashSet<&str>> =
        requested_action_ids.map(|ids| ids.iter().map(String::as_str).collect());
    let asset_map: HashMap<&str, &Asset> = assets
        .iter()
        .map(|asset| (asset.id.as_str(), asset))
        .collect();
    let profile_map: HashMap<&str, &TargetProfile> = profiles
        .iter()
        .map(|profile| (profile.id.as_str(), profile))
        .collect();
    let mut result = ExecutionResult {
        executed_count: 0,
        skipped_count: 0,
        conflict_count: 0,
        errors: Vec::new(),
    };

    for action in &plan.actions {
        if requested
            .as_ref()
            .is_some_and(|ids| !ids.contains(action.id.as_str()))
        {
            continue;
        }
        if !matches!(
            action.action_type,
            DeploymentActionType::Create | DeploymentActionType::Update
        ) || !action.selectable
        {
            result.skipped_count += 1;
            tracing::info!(
                action = "deployment_plan.action",
                action_id = %action.id,
                action_type = ?action.action_type,
                profile_id = %action.profile_id,
                strategy = ?action.strategy,
                "跳过不可执行的部署动作"
            );
            continue;
        }

        let Some(asset_id) = action.asset_id.as_deref() else {
            result.skipped_count += 1;
            tracing::warn!(
                action = "deployment_plan.action",
                action_id = %action.id,
                action_type = ?action.action_type,
                profile_id = %action.profile_id,
                strategy = ?action.strategy,
                "跳过缺少 skill 的部署动作"
            );
            continue;
        };
        let Some(asset) = asset_map.get(asset_id) else {
            let message = format!("asset not found: {asset_id}");
            result.errors.push(message.clone());
            tracing::error!(
                action = "deployment_plan.action",
                action_id = %action.id,
                action_type = ?action.action_type,
                profile_id = %action.profile_id,
                error = %message,
                "部署动作失败：未找到 skill"
            );
            continue;
        };
        let Some(profile) = profile_map.get(action.profile_id.as_str()) else {
            let message = format!("profile not found: {}", action.profile_id);
            result.errors.push(message.clone());
            tracing::error!(
                action = "deployment_plan.action",
                action_id = %action.id,
                action_type = ?action.action_type,
                profile_id = %action.profile_id,
                skill_name = %asset.name,
                error = %message,
                "部署动作失败：未找到目标 APP 配置"
            );
            continue;
        };
        if let Err(error) = target_catalog.require_descriptor(&profile.target_provider_id) {
            let message = error.to_string();
            result.errors.push(message.clone());
            tracing::error!(
                action = "deployment_plan.action",
                action_id = %action.id,
                action_type = ?action.action_type,
                profile_id = %action.profile_id,
                profile_name = %profile.name,
                skill_name = %asset.name,
                error = %message,
                "部署动作失败：目标 Provider 不可用"
            );
            continue;
        }

        match execute_deployment_action(pool, tenant_id, profile, asset, action).await {
            Ok(()) => {
                result.executed_count += 1;
                tracing::info!(
                    action = "deployment_plan.action",
                    action_id = %action.id,
                    action_type = ?action.action_type,
                    profile_id = %action.profile_id,
                    profile_name = %profile.name,
                    skill_name = %asset.name,
                    strategy = ?action.strategy,
                    "部署动作执行成功"
                );
            }
            Err(DeploymentError::Conflict(message)) => {
                result.conflict_count += 1;
                result.errors.push(message.clone());
                tracing::warn!(
                    action = "deployment_plan.action",
                    action_id = %action.id,
                    action_type = ?action.action_type,
                    profile_id = %action.profile_id,
                    profile_name = %profile.name,
                    skill_name = %asset.name,
                    error = %message,
                    "部署动作出现冲突"
                );
            }
            Err(DeploymentError::Failure(message)) => {
                result.errors.push(message.clone());
                tracing::error!(
                    action = "deployment_plan.action",
                    action_id = %action.id,
                    action_type = ?action.action_type,
                    profile_id = %action.profile_id,
                    profile_name = %profile.name,
                    skill_name = %asset.name,
                    error = %message,
                    "部署动作执行失败"
                );
            }
        }
    }

    Ok(result)
}

async fn execute_deployment_action(
    pool: &SqlitePool,
    tenant_id: &str,
    profile: &TargetProfile,
    asset: &Asset,
    action: &DeploymentAction,
) -> Result<(), DeploymentError> {
    let managed_target = crate::backend::store::is_managed_deployment_sqlx(
        pool,
        tenant_id,
        &profile.id,
        &asset.id,
        &action.target_path,
    )
    .await
    .map_err(|error| DeploymentError::Failure(error.to_string()))?;
    let state = crate::backend::infrastructure::deployment::execute_physical_deployment(
        profile,
        asset,
        action,
        managed_target,
    )?;
    crate::backend::store::upsert_deployment_state_sqlx(pool, tenant_id, &state)
        .await
        .map_err(|error| DeploymentError::Failure(error.to_string()))?;
    Ok(())
}
