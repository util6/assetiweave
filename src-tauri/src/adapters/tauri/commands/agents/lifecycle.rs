use tauri::{AppHandle, State};

use crate::adapters::app_state::AppState;
use crate::adapters::tauri::agent_market;
use crate::backend::application::prelude::*;
use crate::backend::application::{
    AgentInstallPreview, AgentUninstallPreview, AppResult as RuntimeAppResult,
};
use crate::backend::infrastructure::agent_market::{
    AgentInstallPreviewRequest, AgentInstallStartRequest, AgentInstallationView,
    AgentLifecycleTaskSnapshot, AgentUninstallStartRequest,
};

#[tauri::command]
pub(crate) async fn preview_agent_installation(
    state: State<'_, AppState>,
    params: crate::backend::infrastructure::agent_market::AgentInstallPreviewRequest,
) -> crate::backend::application::AppResult<crate::backend::application::AgentInstallPreview> {
    crate::adapters::tauri::agent_market::preview_agent_installation(state, params).await
}

#[tauri::command]
pub(crate) async fn preview_agent_uninstall(
    state: State<'_, AppState>,
    agent_id: String,
) -> crate::backend::application::AppResult<crate::backend::application::AgentUninstallPreview> {
    crate::adapters::tauri::agent_market::preview_agent_uninstall(state, agent_id).await
}

#[tauri::command]
pub(crate) async fn list_installed_agents(
    state: State<'_, AppState>,
) -> crate::backend::application::AppResult<
    Vec<crate::backend::infrastructure::agent_market::AgentInstallationView>,
> {
    crate::adapters::tauri::agent_market::list_installed_agents(state).await
}

#[tauri::command]
pub(crate) async fn get_installed_agent(
    state: State<'_, AppState>,
    agent_id: String,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentInstallationView,
> {
    crate::adapters::tauri::agent_market::get_installed_agent(state, agent_id).await
}

#[tauri::command]
pub(crate) async fn check_agent_runtime(
    state: State<'_, AppState>,
    agent_id: String,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentInstallationView,
> {
    crate::adapters::tauri::agent_market::check_agent_runtime(state, agent_id).await
}

#[tauri::command]
pub(crate) fn get_agent_lifecycle_task(
    state: State<'_, AppState>,
    task_id: String,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentLifecycleTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::get_agent_lifecycle_task(state, task_id)
}

#[tauri::command]
pub(crate) fn list_agent_lifecycle_tasks(
    state: State<'_, AppState>,
) -> crate::backend::application::AppResult<
    Vec<crate::backend::infrastructure::agent_market::AgentLifecycleTaskSnapshot>,
> {
    crate::adapters::tauri::agent_market::list_agent_lifecycle_tasks(state)
}

#[tauri::command]
pub(crate) fn cancel_agent_lifecycle_task(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentLifecycleTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::cancel_agent_lifecycle_task(app, state, task_id)
}

#[tauri::command]
pub(crate) fn start_agent_installation(
    app: AppHandle,
    state: State<'_, AppState>,
    params: crate::backend::infrastructure::agent_market::AgentInstallStartRequest,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentLifecycleTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::start_agent_installation(app, state, params)
}

#[tauri::command]
pub(crate) fn start_agent_update(
    app: AppHandle,
    state: State<'_, AppState>,
    params: crate::backend::infrastructure::agent_market::AgentInstallStartRequest,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentLifecycleTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::start_agent_update(app, state, params)
}

#[tauri::command]
pub(crate) fn start_agent_reinstallation(
    app: AppHandle,
    state: State<'_, AppState>,
    params: crate::backend::infrastructure::agent_market::AgentInstallStartRequest,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentLifecycleTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::start_agent_reinstallation(app, state, params)
}

#[tauri::command]
pub(crate) fn start_agent_uninstall(
    app: AppHandle,
    state: State<'_, AppState>,
    params: crate::backend::infrastructure::agent_market::AgentUninstallStartRequest,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentLifecycleTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::start_agent_uninstall(app, state, params)
}

#[tauri::command]
pub(crate) async fn enable_agent(
    state: State<'_, AppState>,
    agent_id: String,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentInstallationView,
> {
    crate::adapters::tauri::agent_market::enable_agent(state, agent_id).await
}

#[tauri::command]
pub(crate) async fn disable_agent(
    state: State<'_, AppState>,
    agent_id: String,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentInstallationView,
> {
    crate::adapters::tauri::agent_market::disable_agent(state, agent_id).await
}
