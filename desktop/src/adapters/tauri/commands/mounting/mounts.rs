use tauri::State;

use crate::adapters::app_state::AppState;
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;
use crate::backend::domain::PhysicalMountStateDto;

#[tauri::command]
pub(crate) async fn list_asset_mounts(
    state: State<'_, AppState>,
    asset_id: Option<String>,
) -> RuntimeAppResult<Vec<AssetMount>> {
    AppService::from_runtime(&state.runtime)
        .list_asset_mounts(asset_id.as_deref())
        .await
}

#[tauri::command]
pub(crate) async fn list_asset_mount_statuses(
    state: State<'_, AppState>,
    asset_id: Option<String>,
) -> RuntimeAppResult<Vec<AssetMountStatus>> {
    AppService::from_runtime(&state.runtime)
        .list_asset_mount_statuses(asset_id.as_deref())
        .await
}

#[tauri::command]
pub(crate) async fn refresh_asset_mount_statuses(
    state: State<'_, AppState>,
    asset_id: Option<String>,
) -> RuntimeAppResult<Vec<AssetMountStatus>> {
    let result = AppService::from_runtime(&state.runtime)
        .refresh_asset_mount_statuses(asset_id.as_deref())
        .await;

    match &result {
        Ok(statuses) => {
            let mounted = statuses
                .iter()
                .filter(|status| status.state == PhysicalMountStateDto::Mounted)
                .count();
            let issues = statuses
                .iter()
                .filter(|status| {
                    matches!(
                        status.state,
                        PhysicalMountStateDto::Conflict | PhysicalMountStateDto::Broken
                    )
                })
                .count();
            tracing::info!(
                action = "mount_status.refresh",
                asset_id = ?asset_id,
                count = statuses.len(),
                mounted = mounted,
                issues = issues,
                "刷新挂载状态成功"
            );
        }
        Err(error) => tracing::error!(
            action = "mount_status.refresh",
            asset_id = ?asset_id,
            error = %error,
            "刷新挂载状态失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn toggle_asset_mount(
    state: State<'_, AppState>,
    asset_id: String,
    profile_id: String,
) -> RuntimeAppResult<AssetMount> {
    let result = AppService::from_runtime(&state.runtime)
        .toggle_asset_mount(&asset_id, &profile_id)
        .await;

    if let Err(error) = &result {
        tracing::error!(
            action = "skill.mount.toggle",
            asset_id = %asset_id,
            profile_id = %profile_id,
            error = %error,
            "切换 skill 挂载失败"
        );
    }
    result
}

#[tauri::command]
pub(crate) async fn unmount_asset_mount(
    state: State<'_, AppState>,
    asset_id: String,
    profile_id: String,
) -> RuntimeAppResult<AssetMountUpdateResult> {
    let result = AppService::from_runtime(&state.runtime)
        .unmount_asset_by_id(&asset_id, &profile_id)
        .await;

    if let Err(error) = &result {
        tracing::error!(
            action = "skill.unmount.command",
            asset_id = %asset_id,
            profile_id = %profile_id,
            error = %error,
            "卸载 skill 命令失败"
        );
    }
    result
}

#[tauri::command]
pub(crate) async fn mount_asset_mount(
    state: State<'_, AppState>,
    asset_id: String,
    profile_id: String,
) -> RuntimeAppResult<AssetMountUpdateResult> {
    let result = AppService::from_runtime(&state.runtime)
        .mount_asset_by_id(&asset_id, &profile_id)
        .await;

    if let Err(error) = &result {
        tracing::error!(
            action = "skill.mount.command",
            asset_id = %asset_id,
            profile_id = %profile_id,
            error = %error,
            "挂载 skill 命令失败"
        );
    }
    result
}

#[tauri::command]
pub(crate) async fn set_asset_mount(
    state: State<'_, AppState>,
    asset_id: String,
    profile_id: String,
    enabled: bool,
    strategy: Option<DeploymentStrategy>,
) -> RuntimeAppResult<AssetMount> {
    let result = AppService::from_runtime(&state.runtime)
        .set_asset_mount(&asset_id, &profile_id, enabled, strategy)
        .await;

    if let Err(error) = &result {
        tracing::error!(
            action = "skill.mount.set",
            asset_id = %asset_id,
            profile_id = %profile_id,
            enabled = enabled,
            error = %error,
            "设置 skill 挂载关系失败"
        );
    }
    result
}
