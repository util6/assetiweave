use tauri::State;

use crate::adapters::app_state::AppState;
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;
use crate::backend::application::{
    ConversationAdapterUnregisterParams, ConversationSourceDisableParams,
    ConversationSourceUpsertParams,
};
use crate::backend::domain::{ConversationAdapter, ConversationSource};
use crate::backend::infrastructure::conversations::{
    ConversationCommandProjection, ConversationCommandProjectionParams,
    ExternalAdapterRegisterParams, ExternalAdapterScaffoldParams, ExternalAdapterTryRunParams,
    ExternalAdapterValidateParams,
};

#[tauri::command]
pub(crate) fn list_conversation_adapters(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<ConversationAdapter>> {
    AppService::from_runtime(&state.runtime).list_conversation_adapters()
}

#[tauri::command]
pub(crate) fn scaffold_conversation_adapter(
    state: State<'_, AppState>,
    params: ExternalAdapterScaffoldParams,
) -> RuntimeAppResult<crate::backend::infrastructure::conversations::ExternalAdapterScaffoldResult>
{
    AppService::from_runtime(&state.runtime).scaffold_conversation_adapter(params)
}

#[tauri::command]
pub(crate) fn validate_conversation_adapter(
    state: State<'_, AppState>,
    params: ExternalAdapterValidateParams,
) -> RuntimeAppResult<crate::backend::infrastructure::conversations::ExternalAdapterValidationResult>
{
    AppService::from_runtime(&state.runtime).validate_conversation_adapter(params)
}

#[tauri::command]
pub(crate) async fn list_conversation_adapter_runtime_statuses(
    state: State<'_, AppState>,
) -> RuntimeAppResult<
    Vec<crate::backend::infrastructure::conversations::ConversationAdapterRuntimeStatus>,
> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_adapter_runtime_statuses()
        .await
}

#[tauri::command]
pub(crate) async fn register_conversation_adapter(
    state: State<'_, AppState>,
    params: ExternalAdapterRegisterParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .register_conversation_adapter(params)
        .await
}

#[tauri::command]
pub(crate) async fn unregister_conversation_adapter(
    state: State<'_, AppState>,
    params: ConversationAdapterUnregisterParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .unregister_conversation_adapter(params)
        .await
}

#[tauri::command]
pub(crate) async fn try_run_conversation_adapter(
    state: State<'_, AppState>,
    params: ExternalAdapterTryRunParams,
) -> RuntimeAppResult<crate::backend::infrastructure::conversations::ExternalAdapterRunResult> {
    AppService::from_runtime(&state.runtime)
        .try_run_conversation_adapter(params)
        .await
}

#[tauri::command]
pub(crate) async fn project_conversation_command_parts(
    state: State<'_, AppState>,
    params: ConversationCommandProjectionParams,
) -> RuntimeAppResult<Vec<ConversationCommandProjection>> {
    AppService::from_runtime(&state.runtime)
        .project_conversation_command_parts(params)
        .await
}

#[tauri::command]
pub(crate) async fn list_conversation_sources(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<ConversationSource>> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_sources()
        .await
}

#[tauri::command]
pub(crate) async fn upsert_conversation_source(
    state: State<'_, AppState>,
    params: ConversationSourceUpsertParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .upsert_conversation_source(params)
        .await
}

#[tauri::command]
pub(crate) async fn disable_conversation_source(
    state: State<'_, AppState>,
    params: ConversationSourceDisableParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .disable_conversation_source(params)
        .await
}
