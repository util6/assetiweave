use tauri::State;

use crate::adapters::app_state::AppState;
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;
use crate::backend::domain::TargetProfileDescriptor;

#[tauri::command]
pub(crate) async fn list_profiles(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<TargetProfile>> {
    AppService::from_runtime(&state.runtime)
        .list_profiles()
        .await
}

#[tauri::command]
pub(crate) fn list_target_profile_descriptors(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<TargetProfileDescriptor>> {
    AppService::from_runtime(&state.runtime).list_target_profile_descriptors()
}

#[tauri::command]
pub(crate) async fn refresh_target_profile_descriptors(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<TargetProfileDescriptor>> {
    AppService::from_runtime(&state.runtime)
        .refresh_target_profile_descriptors()
        .await
}

#[tauri::command]
pub(crate) async fn create_profile(
    state: State<'_, AppState>,
    input: TargetProfileInput,
) -> RuntimeAppResult<TargetProfile> {
    let profile_name = input.name.clone();
    let target_path_count = input
        .target_paths
        .as_ref()
        .map(|paths| paths.len())
        .unwrap_or(0);
    let app_kind = input.app_kind.map(|k| format!("{k:?}"));
    let result = AppService::from_runtime(&state.runtime)
        .create_profile(input)
        .await;

    match &result {
        Ok(profile) => tracing::info!(
            action = "profile.create",
            profile_id = %profile.id,
            profile_name = %profile.name,
            target_path_count = profile.target_paths.len(),
            "添加目标 APP 配置成功"
        ),
        Err(error) => tracing::error!(
            action = "profile.create",
            profile_name = %profile_name,
            target_path_count = target_path_count,
            app_kind = ?app_kind,
            error = %error,
            "添加目标 APP 配置失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn update_profile(
    state: State<'_, AppState>,
    profile: TargetProfile,
) -> RuntimeAppResult<TargetProfile> {
    let profile_id = profile.id.clone();
    let profile_name = profile.name.clone();
    let target_path_count = profile.target_paths.len();
    let result = AppService::from_runtime(&state.runtime)
        .update_profile(profile)
        .await;

    match &result {
        Ok(profile) => tracing::info!(
            action = "profile.update",
            profile_id = %profile.id,
            profile_name = %profile.name,
            target_path_count = profile.target_paths.len(),
            "更新目标 APP 配置成功"
        ),
        Err(error) => tracing::error!(
            action = "profile.update",
            profile_id = %profile_id,
            profile_name = %profile_name,
            target_path_count = target_path_count,
            error = %error,
            "更新目标 APP 配置失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn delete_profile(state: State<'_, AppState>, id: String) -> RuntimeAppResult<()> {
    let result = AppService::from_runtime(&state.runtime)
        .delete_profile(id.clone())
        .await;

    match &result {
        Ok(()) => tracing::info!(
            action = "profile.delete",
            profile_id = %id,
            "删除目标 APP 配置成功"
        ),
        Err(error) => tracing::error!(
            action = "profile.delete",
            profile_id = %id,
            error = %error,
            "删除目标 APP 配置失败"
        ),
    }
    result
}
