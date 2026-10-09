use tauri::State;

use crate::adapters::app_state::AppState;
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;
use crate::backend::application::{
    ConversationSessionExportParams, ConversationSessionGetParams, ConversationSessionListParams,
    ConversationSessionOutlineParams,
};

#[tauri::command]
pub(crate) async fn list_conversation_sessions(
    state: State<'_, AppState>,
    params: ConversationSessionListParams,
) -> RuntimeAppResult<Vec<crate::backend::domain::ConversationSessionListItem>> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_sessions(params)
        .await
}

#[tauri::command]
pub(crate) async fn get_conversation_session(
    state: State<'_, AppState>,
    params: ConversationSessionGetParams,
) -> RuntimeAppResult<crate::backend::domain::ConversationSessionDetail> {
    AppService::from_runtime(&state.runtime)
        .get_conversation_session(params)
        .await
}

#[tauri::command]
pub(crate) async fn get_conversation_session_outline(
    state: State<'_, AppState>,
    params: ConversationSessionOutlineParams,
) -> RuntimeAppResult<crate::backend::domain::ConversationSessionOutline> {
    AppService::from_runtime(&state.runtime)
        .get_conversation_session_outline(params)
        .await
}

#[tauri::command]
pub(crate) async fn export_conversation_session(
    state: State<'_, AppState>,
    params: ConversationSessionExportParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .export_conversation_session(params)
        .await
}

#[tauri::command]
pub(crate) async fn list_web_record_sessions(
    state: State<'_, AppState>,
    params: ConversationSessionListParams,
) -> RuntimeAppResult<Vec<crate::backend::domain::ConversationSessionListItem>> {
    AppService::from_runtime(&state.runtime)
        .list_web_record_sessions(params)
        .await
}

#[tauri::command]
pub(crate) async fn get_web_record_session(
    state: State<'_, AppState>,
    params: ConversationSessionGetParams,
) -> RuntimeAppResult<crate::backend::domain::ConversationSessionDetail> {
    AppService::from_runtime(&state.runtime)
        .get_web_record_session(params)
        .await
}

#[tauri::command]
pub(crate) async fn export_web_record_session(
    state: State<'_, AppState>,
    params: ConversationSessionExportParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .export_web_record_session(params)
        .await
}

#[tauri::command]
pub(crate) async fn replay_conversation_session_projection(
    state: State<'_, AppState>,
    session_id: String,
) -> RuntimeAppResult<crate::backend::domain::ConversationSessionDetail> {
    AppService::from_runtime(&state.runtime)
        .replay_conversation_session_projection(&session_id)
        .await
}
