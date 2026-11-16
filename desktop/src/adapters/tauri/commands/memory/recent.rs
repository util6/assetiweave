use tauri::{Emitter, State};

use crate::adapters::app_state::AppState;
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;

#[tauri::command]
pub(crate) async fn get_memory_recent_snapshot(
    state: State<'_, AppState>,
) -> RuntimeAppResult<crate::backend::domain::RecentMemoryStateView> {
    AppService::from_runtime(&state.runtime)
        .get_recent_memory_snapshot()
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn refresh_memory_recent_state(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    params: Option<MemoryRecentRefreshParams>,
) -> RuntimeAppResult<crate::backend::domain::RecentMemoryStateView> {
    let result = AppService::from_runtime(&state.runtime)
        .refresh_memory_recent_state_view(params.unwrap_or_default())
        .await;
    if result.is_ok() {
        let _ = app.emit("memory-task-updated", ());
    }
    result.into()
}

#[tauri::command]
pub(crate) async fn update_memory_node_status(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    params: MemoryNodeStatusUpdateParams,
) -> RuntimeAppResult<crate::backend::domain::MemoryTreeNode> {
    let result = AppService::from_runtime(&state.runtime)
        .update_memory_node_status_for_ui(params)
        .await;
    if result.is_ok() {
        let _ = app.emit("memory-task-updated", ());
    }
    result.into()
}

#[tauri::command]
pub(crate) async fn get_memory_project_state(
    state: State<'_, AppState>,
    params: MemoryProjectStateGetParams,
) -> RuntimeAppResult<Option<crate::backend::domain::MemoryProjectStateView>> {
    AppService::from_runtime(&state.runtime)
        .get_memory_project_state_view(&params.project_slug)
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn duplicate_memory_generation_skill(
    state: State<'_, AppState>,
) -> RuntimeAppResult<crate::backend::domain::CatalogAsset> {
    AppService::from_runtime(&state.runtime)
        .duplicate_generation_skill_to_library()
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn reset_memory_generation_skill_to_default(
    state: State<'_, AppState>,
) -> RuntimeAppResult<()> {
    AppService::from_runtime(&state.runtime)
        .reset_generation_skill_to_default()
        .await
        .into()
}
