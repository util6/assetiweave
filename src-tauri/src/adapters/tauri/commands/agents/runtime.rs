use tauri::{AppHandle, Emitter, State};

use crate::adapters::app_state::AppState;
use crate::adapters::tauri::background_tasks::{AiExecutionTaskGetParams, AiExecutionTaskSnapshot};
use crate::backend::application::agents::{
    AgentConnectionCheckRequest, AgentConnectionResult, AgentModelsRequest, AgentModelsResult,
    AgentSessionGetParams, AgentSessionGetResult,
};
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;

pub(crate) const AI_EXECUTION_TASK_UPDATED_EVENT: &str = "ai-execution://task-updated";

fn emit_ai_execution_task(app: &AppHandle, snapshot: &AiExecutionTaskSnapshot) {
    if let Err(error) = app.emit(AI_EXECUTION_TASK_UPDATED_EVENT, snapshot) {
        tracing::error!(
            action = "ai_execution.task",
            task_id = %snapshot.id,
            error = %error,
            "推送 AI 执行任务状态失败"
        );
    }
}

#[tauri::command]
pub(crate) async fn check_agent_connection(
    state: State<'_, AppState>,
    params: AgentConnectionCheckRequest,
) -> RuntimeAppResult<AgentConnectionResult> {
    AppService::from_runtime(&state.runtime)
        .check_agent_connection(params)
        .await
}

#[tauri::command]
pub(crate) async fn list_agent_models(
    state: State<'_, AppState>,
    params: AgentModelsRequest,
) -> RuntimeAppResult<AgentModelsResult> {
    AppService::from_runtime(&state.runtime)
        .list_agent_models(params)
        .await
}

#[tauri::command]
pub(crate) async fn cancel_agent_model_probe(
    state: State<'_, AppState>,
    agent_id: String,
) -> RuntimeAppResult<()> {
    AppService::from_runtime(&state.runtime)
        .cancel_agent_model_probe(agent_id)
        .await
}

#[tauri::command]
pub(crate) fn get_ai_execution_task(
    state: State<'_, AppState>,
    params: AiExecutionTaskGetParams,
) -> RuntimeAppResult<Option<AiExecutionTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .ai_execution_snapshot_for_tenant(&tenant_id, &params.task_id)
}

#[tauri::command]
pub(crate) fn list_ai_execution_tasks(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<AiExecutionTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .ai_execution_snapshots_for_tenant(&tenant_id)
}

#[tauri::command]
pub(crate) fn cancel_ai_execution_task(
    app: AppHandle,
    state: State<'_, AppState>,
    params: AiExecutionTaskGetParams,
) -> RuntimeAppResult<AiExecutionTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    let snapshot = state
        .background_tasks
        .cancel_ai_execution_for_tenant(&tenant_id, &params.task_id)?;
    emit_ai_execution_task(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn agent_session_get(
    state: State<'_, AppState>,
    params: crate::backend::application::agents::AgentSessionGetParams,
) -> RuntimeAppResult<crate::backend::application::agents::AgentSessionGetResult> {
    let service = AppService::from_runtime(&state.runtime);
    service.get_agent_session(params)
}
