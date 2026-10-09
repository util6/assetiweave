use tauri::State;

use crate::adapters::app_state::AppState;
use crate::backend::application::memory::MemoryTaskView;
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;

#[tauri::command]
pub(crate) fn list_memory_public_tasks(
    state: State<'_, AppState>,
    params: MemoryTaskListParams,
) -> RuntimeAppResult<Vec<MemoryTaskView>> {
    AppService::from_runtime(&state.runtime).list_memory_task_views(params)
}

#[tauri::command]
pub(crate) fn get_memory_public_task(
    state: State<'_, AppState>,
    params: MemoryTaskGetParams,
) -> RuntimeAppResult<Option<MemoryTaskView>> {
    AppService::from_runtime(&state.runtime).get_memory_task_view(params)
}

#[tauri::command]
pub(crate) fn cancel_memory_public_task(
    state: State<'_, AppState>,
    params: MemoryTaskGetParams,
) -> RuntimeAppResult<MemoryTaskView> {
    AppService::from_runtime(&state.runtime).cancel_memory_task_view(params)
}

#[tauri::command]
pub(crate) async fn retry_memory_public_task(
    state: State<'_, AppState>,
    params: MemoryTaskRetryParams,
) -> RuntimeAppResult<MemoryTaskView> {
    AppService::from_runtime(&state.runtime)
        .retry_memory_task(params)
        .await
        .into()
}
