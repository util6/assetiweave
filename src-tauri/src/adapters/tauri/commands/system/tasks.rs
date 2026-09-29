use tauri::State;

use crate::adapters::app_state::AppState;
use crate::backend::application::prelude::*;
use crate::backend::application::system::{
    TaskCancelParams, TaskClearParams, TaskGetParams, TaskListParams, TaskRetryParams, TaskView,
};
use crate::backend::application::AppResult as RuntimeAppResult;

#[tauri::command]
pub(crate) fn list_public_tasks(
    state: State<'_, AppState>,
    params: TaskListParams,
) -> RuntimeAppResult<Vec<TaskView>> {
    let service = AppService::from_runtime(&state.runtime);
    service.list_public_tasks(params)
}

#[tauri::command]
pub(crate) fn get_public_task(
    state: State<'_, AppState>,
    params: TaskGetParams,
) -> RuntimeAppResult<Option<TaskView>> {
    let service = AppService::from_runtime(&state.runtime);
    service.get_public_task(params)
}

#[tauri::command]
pub(crate) fn cancel_public_task(
    state: State<'_, AppState>,
    params: TaskCancelParams,
) -> RuntimeAppResult<TaskView> {
    let service = AppService::from_runtime(&state.runtime);
    service.cancel_public_task(params)
}

#[tauri::command]
pub(crate) async fn retry_public_task(
    state: State<'_, AppState>,
    params: TaskRetryParams,
) -> RuntimeAppResult<TaskView> {
    let service = AppService::from_runtime(&state.runtime);
    service.retry_public_task(params).await
}

#[tauri::command]
pub(crate) fn clear_terminal_tasks(
    state: State<'_, AppState>,
    params: TaskClearParams,
) -> RuntimeAppResult<usize> {
    let service = AppService::from_runtime(&state.runtime);
    service.clear_terminal_tasks(params)
}
