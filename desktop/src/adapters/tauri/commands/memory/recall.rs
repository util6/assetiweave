use tauri::State;

use crate::adapters::app_state::AppState;
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;

#[tauri::command]
pub(crate) async fn search_memory_recall(
    state: State<'_, AppState>,
    params: MemoryRecallSearchParams,
) -> RuntimeAppResult<crate::backend::domain::MemoryRecallSearchResult> {
    AppService::from_runtime(&state.runtime)
        .search_memory_recall(params)
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn create_memory_recall_session(
    state: State<'_, AppState>,
    params: MemoryRecallSessionCreateParams,
) -> RuntimeAppResult<crate::backend::domain::MemoryRecallSession> {
    AppService::from_runtime(&state.runtime)
        .create_memory_recall_session(params)
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn get_memory_recall_session(
    state: State<'_, AppState>,
    params: MemoryRecallSessionGetParams,
) -> RuntimeAppResult<crate::backend::domain::MemoryRecallSession> {
    AppService::from_runtime(&state.runtime)
        .get_memory_recall_session(params)
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn send_memory_recall_turn(
    state: State<'_, AppState>,
    params: MemoryRecallTurnSendParams,
) -> RuntimeAppResult<crate::backend::domain::MemoryRecallSession> {
    AppService::from_runtime(&state.runtime)
        .send_memory_recall_turn(params)
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn cancel_memory_recall_turn(
    state: State<'_, AppState>,
    params: MemoryRecallTurnCancelParams,
) -> RuntimeAppResult<crate::backend::domain::MemoryRecallSession> {
    AppService::from_runtime(&state.runtime)
        .cancel_memory_recall_turn(params)
        .await
        .into()
}
