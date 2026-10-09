use tauri::State;

use crate::adapters::app_state::AppState;
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;

#[tauri::command]
pub(crate) async fn get_navigation_model(
    state: State<'_, AppState>,
) -> RuntimeAppResult<NavigationModel> {
    AppService::from_runtime(&state.runtime)
        .navigation_model()
        .await
}

#[tauri::command]
pub(crate) async fn update_navigation_model(
    state: State<'_, AppState>,
    model: NavigationModel,
) -> RuntimeAppResult<NavigationModel> {
    let active_rail_id = model.active_rail_id.clone();
    let active_header_tab_id = model.active_header_tab_id.clone();
    let active_sub_nav_id = model.active_sub_nav_id.clone();
    let rail_count = model.rail_items.len();
    let result = AppService::from_runtime(&state.runtime)
        .update_navigation_model(model)
        .await;

    match &result {
        Ok(_) => tracing::info!(
            action = "navigation.update",
            active_rail_id = %active_rail_id,
            active_header_tab_id = %active_header_tab_id,
            active_sub_nav_id = %active_sub_nav_id,
            rail_count = rail_count,
            "更新导航配置成功"
        ),
        Err(error) => tracing::error!(
            action = "navigation.update",
            active_rail_id = %active_rail_id,
            active_header_tab_id = %active_header_tab_id,
            active_sub_nav_id = %active_sub_nav_id,
            rail_count = rail_count,
            error = %error,
            "更新导航配置失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn list_app_shortcuts(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<AppShortcut>> {
    AppService::from_runtime(&state.runtime)
        .list_app_shortcuts()
        .await
}

#[tauri::command]
pub(crate) async fn list_app_shortcut_settings(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<AppShortcut>> {
    AppService::from_runtime(&state.runtime)
        .list_app_shortcut_settings()
        .await
}

#[tauri::command]
pub(crate) async fn update_app_shortcuts(
    state: State<'_, AppState>,
    shortcuts: Vec<AppShortcut>,
) -> RuntimeAppResult<Vec<AppShortcut>> {
    let shortcut_count = shortcuts.len();
    let result = AppService::from_runtime(&state.runtime)
        .update_app_shortcuts(shortcuts)
        .await;

    match &result {
        Ok(shortcuts) => tracing::info!(
            action = "settings.app_shortcuts.update",
            shortcut_count = shortcuts.len(),
            "更新 APP 快捷入口配置成功"
        ),
        Err(error) => tracing::error!(
            action = "settings.app_shortcuts.update",
            shortcut_count = shortcut_count,
            error = %error,
            "更新 APP 快捷入口配置失败"
        ),
    }
    result
}
