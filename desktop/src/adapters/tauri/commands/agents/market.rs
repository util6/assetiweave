use tauri::{AppHandle, State};

use crate::adapters::app_state::AppState;
use crate::adapters::tauri::agent_market;
use crate::adapters::tauri::background_tasks::AgentMarketRefreshTaskSnapshot;
use crate::backend::application::prelude::*;
use crate::backend::application::{AgentMarketItemView, AppResult as RuntimeAppResult};
use crate::backend::domain::AgentCatalogEntry;
use crate::backend::infrastructure::agent_market::AgentMarketListRequest;

#[tauri::command]
pub(crate) async fn list_agent_catalog(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<AgentCatalogEntry>> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).list_agent_catalog()
    })
    .await
    .map_err(|error| AppError::External(error.to_string()))?
}

#[tauri::command]
pub(crate) async fn list_agent_market(
    state: State<'_, AppState>,
    params: crate::backend::infrastructure::agent_market::AgentMarketListRequest,
) -> crate::backend::application::AppResult<Vec<crate::backend::application::AgentMarketItemView>> {
    crate::adapters::tauri::agent_market::list_agent_market(state, params).await
}

#[tauri::command]
pub(crate) async fn inspect_agent_market_item(
    state: State<'_, AppState>,
    agent_id: String,
) -> crate::backend::application::AppResult<crate::backend::application::AgentMarketItemView> {
    crate::adapters::tauri::agent_market::inspect_agent_market_item(state, agent_id).await
}

#[tauri::command]
pub(crate) fn refresh_agent_market(
    app: AppHandle,
    state: State<'_, AppState>,
) -> crate::backend::application::AppResult<
    crate::adapters::tauri::background_tasks::AgentMarketRefreshTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::refresh_agent_market(app, state)
}

#[tauri::command]
pub(crate) fn get_agent_market_refresh_task(
    state: State<'_, AppState>,
    task_id: String,
) -> crate::backend::application::AppResult<
    crate::adapters::tauri::background_tasks::AgentMarketRefreshTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::get_agent_market_refresh_task(state, task_id)
}

#[tauri::command]
pub(crate) fn list_agent_market_refresh_tasks(
    state: State<'_, AppState>,
) -> crate::backend::application::AppResult<
    Vec<crate::adapters::tauri::background_tasks::AgentMarketRefreshTaskSnapshot>,
> {
    crate::adapters::tauri::agent_market::list_agent_market_refresh_tasks(state)
}
