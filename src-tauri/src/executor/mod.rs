use crate::models::{
    Asset, DeploymentAction, DeploymentActionType, DeploymentPlan, DeploymentState,
    DeploymentStrategy, TargetProfile,
};
use crate::{
    logs, store, targeting,
    types::{AppResult, ExecutionResult},
};
use chrono::Utc;
use rusqlite::Connection;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

type LogField = (&'static str, String);

fn log_action_info(message: &str, fields: &[LogField]) {
    logs::record_info("deployment_plan.action", message, fields);
}

fn log_action_warn(message: &str, fields: &[LogField]) {
    logs::record_warn("deployment_plan.action", message, fields);
}

fn log_action_error(message: &str, error: &str, fields: &[LogField]) {
    let mut fields = fields.to_vec();
    fields.push(("error", error.to_string()));
    logs::record_error("deployment_plan.action", message, &fields);
}

fn action_log_fields(
    action: &DeploymentAction,
    asset: Option<&Asset>,
    profile: Option<&TargetProfile>,
) -> Vec<LogField> {
    let mut fields = vec![
        ("action_id", action.id.clone()),
        ("action_type", format!("{:?}", action.action_type)),
        ("profile_id", action.profile_id.clone()),
        ("target_path", action.target_path.clone()),
        ("strategy", format!("{:?}", action.strategy)),
    ];

    if let Some(asset_id) = &action.asset_id {
        fields.push(("asset_id", asset_id.clone()));
    }
    if let Some(source_path) = &action.source_path {
        fields.push(("source_path", source_path.clone()));
    }
    if let Some(asset) = asset {
        fields.extend([
            ("skill_name", asset.name.clone()),
            ("asset_kind", format!("{:?}", asset.kind)),
            ("relative_path", asset.relative_path.clone()),
        ]);
    }
    if let Some(profile) = profile {
        fields.extend([
            ("profile_name", profile.name.clone()),
            ("app_kind", format!("{:?}", profile.app_kind)),
        ]);
    }

    fields
}

pub(crate) fn execute_deployment_plan(
    conn: &Connection,
    profiles: &[TargetProfile],
    assets: &[Asset],
    plan: &DeploymentPlan,
    requested_action_ids: Option<&[String]>,
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
            log_action_info(
                "跳过不可执行的部署动作",
                &action_log_fields(action, None, None),
            );
            continue;
        }

        let Some(asset_id) = action.asset_id.as_deref() else {
            result.skipped_count += 1;
            log_action_warn(
                "跳过缺少 skill 的部署动作",
                &action_log_fields(action, None, None),
            );
            continue;
        };
        let Some(asset) = asset_map.get(asset_id) else {
            let message = format!("asset not found: {asset_id}");
            result.errors.push(message.clone());
            log_action_error(
                "部署动作失败：未找到 skill",
                &message,
                &action_log_fields(action, None, None),
            );
            continue;
        };
        let Some(profile) = profile_map.get(action.profile_id.as_str()) else {
            let message = format!("profile not found: {}", action.profile_id);
            result.errors.push(message.clone());
            log_action_error(
                "部署动作失败：未找到目标 APP 配置",
                &message,
                &action_log_fields(action, Some(asset), None),
            );
            continue;
        };

        match execute_deployment_action(conn, profile, asset, action) {
            Ok(()) => {
                result.executed_count += 1;
                log_action_info(
                    "部署动作执行成功",
                    &action_log_fields(action, Some(asset), Some(profile)),
                );
            }
            Err(DeploymentError::Conflict(message)) => {
                result.conflict_count += 1;
                result.errors.push(message.clone());
                log_action_warn(
                    "部署动作出现冲突",
                    &[
                        action_log_fields(action, Some(asset), Some(profile)),
                        vec![("error", message)],
                    ]
                    .concat(),
                );
            }
            Err(DeploymentError::Failure(message)) => {
                result.errors.push(message.clone());
                log_action_error(
                    "部署动作执行失败",
                    &message,
                    &action_log_fields(action, Some(asset), Some(profile)),
                );
            }
        }
    }

    Ok(result)
}

enum DeploymentError {
    Conflict(String),
    Failure(String),
}

fn execute_deployment_action(
    conn: &Connection,
    profile: &TargetProfile,
    asset: &Asset,
    action: &DeploymentAction,
) -> Result<(), DeploymentError> {
    let target_path = PathBuf::from(&action.target_path);
    ensure_target_within_profile(profile, &target_path)?;

    if target_path.exists()
        && !store::is_managed_deployment(conn, &profile.id, &asset.id, &action.target_path)
            .map_err(DeploymentError::Failure)?
    {
        return Err(DeploymentError::Conflict(format!(
            "目标已存在且不是 AssetIWeave 托管文件: {}",
            target_path.display()
        )));
    }

    let parent = target_path.parent().ok_or_else(|| {
        DeploymentError::Failure(format!("目标路径缺少父目录: {}", target_path.display()))
    })?;
    fs::create_dir_all(parent).map_err(|error| DeploymentError::Failure(error.to_string()))?;

    if target_path.exists() {
        remove_existing_target(&target_path)?;
    }

    let source_path = targeting::canonical_source_path(asset).map_err(DeploymentError::Failure)?;

    match action.strategy {
        DeploymentStrategy::SymlinkToSource => create_symlink(&source_path, &target_path)?,
        DeploymentStrategy::CopyToTarget => copy_asset(&source_path, &target_path)?,
        other => {
            return Err(DeploymentError::Failure(format!(
                "当前版本暂不支持 {:?} 部署策略",
                other
            )))
        }
    }

    let state = DeploymentState {
        profile_id: profile.id.clone(),
        asset_id: asset.id.clone(),
        target_path: action.target_path.clone(),
        strategy: action.strategy,
        source_hash: asset.content_hash.clone().unwrap_or_default(),
        deployed_at: Utc::now().to_rfc3339(),
        managed_by: "assetiweave".to_string(),
    };
    store::upsert_deployment_state(conn, &state).map_err(DeploymentError::Failure)?;
    Ok(())
}

fn ensure_target_within_profile(
    profile: &TargetProfile,
    target_path: &Path,
) -> Result<(), DeploymentError> {
    let allowed_root = targeting::target_dir(profile).map_err(DeploymentError::Failure)?;
    if !target_path.starts_with(&allowed_root) {
        return Err(DeploymentError::Failure(format!(
            "拒绝写入 Profile 目标目录外部: {}",
            target_path.display()
        )));
    }
    Ok(())
}

fn remove_existing_target(path: &Path) -> Result<(), DeploymentError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| DeploymentError::Failure(error.to_string()))?;
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path).map_err(|error| DeploymentError::Failure(error.to_string()))
    } else if metadata.is_dir() {
        fs::remove_dir_all(path).map_err(|error| DeploymentError::Failure(error.to_string()))
    } else {
        Err(DeploymentError::Failure(format!(
            "不支持移除该目标类型: {}",
            path.display()
        )))
    }
}

#[cfg(unix)]
fn create_symlink(source: &Path, target: &Path) -> Result<(), DeploymentError> {
    std::os::unix::fs::symlink(source, target)
        .map_err(|error| DeploymentError::Failure(error.to_string()))
}

#[cfg(windows)]
fn create_symlink(source: &Path, target: &Path) -> Result<(), DeploymentError> {
    if source.is_dir() {
        std::os::windows::fs::symlink_dir(source, target)
    } else {
        std::os::windows::fs::symlink_file(source, target)
    }
    .map_err(|error| DeploymentError::Failure(error.to_string()))
}

fn copy_asset(source: &Path, target: &Path) -> Result<(), DeploymentError> {
    if source.is_dir() {
        copy_dir(source, target)
    } else {
        fs::copy(source, target)
            .map(|_| ())
            .map_err(|error| DeploymentError::Failure(error.to_string()))
    }
}

fn copy_dir(source: &Path, target: &Path) -> Result<(), DeploymentError> {
    for entry in WalkDir::new(source).into_iter().filter_map(Result::ok) {
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| DeploymentError::Failure(error.to_string()))?;
        let destination = target.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination)
                .map_err(|error| DeploymentError::Failure(error.to_string()))?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| DeploymentError::Failure(error.to_string()))?;
            }
            fs::copy(entry.path(), destination)
                .map_err(|error| DeploymentError::Failure(error.to_string()))?;
        }
    }
    Ok(())
}
