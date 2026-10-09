use tauri::State;

use crate::adapters::app_state::AppState;
use crate::backend::application::memory::{
    MemoryContextResult, MemoryProjectView, MemoryRebuildResult,
};
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;

#[tauri::command]
pub(crate) async fn resolve_memory_context(
    state: State<'_, AppState>,
    params: MemoryContextResolveParams,
) -> RuntimeAppResult<MemoryContextResult> {
    AppService::from_runtime(&state.runtime)
        .resolve_memory_context(params)
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn get_memory_project(
    state: State<'_, AppState>,
    params: MemoryProjectGetParams,
) -> RuntimeAppResult<Option<MemoryProjectView>> {
    AppService::from_runtime(&state.runtime)
        .get_memory_project(params)
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn rebuild_memory_scope(
    state: State<'_, AppState>,
    params: MemoryScopeRebuildParams,
) -> RuntimeAppResult<MemoryRebuildResult> {
    AppService::from_runtime(&state.runtime)
        .rebuild_memory_scope(params)
        .await
        .into()
}
