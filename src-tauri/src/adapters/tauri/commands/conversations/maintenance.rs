use tauri::{AppHandle, Emitter, State};

use crate::adapters::app_state::AppState;
use crate::adapters::tauri::background_tasks::{
    BackgroundTaskRegistry, BackgroundTaskStatus, ConversationDataMaintenanceTaskSnapshot,
};
use crate::backend::application::mounting::ExecutionResult;
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;
use crate::backend::application::{
    BackgroundTaskGetParams, ConversationDataAuditParams, ConversationDataRepairParams,
    ConversationDataRollbackParams,
};
use crate::backend::domain::{AppOverview, DeploymentPlan};

fn start_conversation_data_maintenance_background(
    app: AppHandle,
    runtime: std::sync::Arc<crate::backend::infrastructure::runtime::AppRuntime>,
    background_tasks: std::sync::Arc<BackgroundTaskRegistry>,
    operation: &'static str,
    audit_params: ConversationDataAuditParams,
    repair_params: Option<ConversationDataRepairParams>,
) -> RuntimeAppResult<ConversationDataMaintenanceTaskSnapshot> {
    let tenant_id = runtime.context().tenant.id.clone();
    let dry_run = repair_params
        .as_ref()
        .map(|params| params.dry_run)
        .unwrap_or(true);
    let (snapshot, should_start) = background_tasks
        .begin_conversation_data_maintenance_for_tenant(
            &tenant_id,
            operation,
            audit_params.source_id.clone(),
            audit_params.record_kind.clone(),
            dry_run,
        )?;
    if !should_start {
        return Ok(snapshot);
    }

    let task_id = snapshot.id.clone();
    let task_runtime = background_tasks
        .task_runtime()
        .ok_or_else(|| AppError::Conflict("TaskRuntime 未初始化".to_string()))?;
    let task_detail = task_runtime
        .get(&task_id)
        .map(|task| task.detail)
        .ok_or_else(|| AppError::NotFound(format!("background task not found: {task_id}")))?;
    let task_background_tasks = background_tasks.clone();
    let task_app = app.clone();
    let task_id_for_runtime = task_id.clone();
    let outcome = task_runtime.start_external_with(
        &task_id,
        task_detail,
        Box::new(move |context| {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let service = AppService::from_runtime(&runtime);
                let progress_tasks = task_background_tasks.clone();
                let progress_app = task_app.clone();
                let progress_task_id = task_id_for_runtime.clone();
                let mut on_progress =
                    move |completed_stage: usize, total_stage: usize, note: Option<String>| {
                        if let Ok(snapshot) = progress_tasks
                            .update_conversation_data_maintenance_progress(
                                &progress_task_id,
                                completed_stage,
                                total_stage,
                                note.clone(),
                            )
                        {
                            if let Err(error) = progress_app
                                .emit("conversation-data-maintenance-task-updated", &snapshot)
                            {
                                tracing::error!(
                                    action = "conversation.data.maintenance",
                                    task_id = %progress_task_id,
                                    error = %error,
                                    "推送对话数据维护进度失败"
                                );
                            }
                        }
                    };
                let cancellation = context.cancellation();
                if context.is_cancelled() {
                    return Err(AppError::Cancelled(
                        "conversation data maintenance cancelled".to_string(),
                    ));
                }
                tauri::async_runtime::block_on(async {
                    if let Some(params) = repair_params {
                        service
                            .repair_conversation_data_with_progress_and_cancellation(
                                params,
                                Some(&cancellation),
                                &mut on_progress,
                            )
                            .await
                    } else {
                        service
                            .audit_conversation_data_with_progress_and_cancellation(
                                audit_params,
                                Some(&cancellation),
                                &mut on_progress,
                            )
                            .await
                    }
                })
            }))
            .unwrap_or_else(|_| {
                Err(AppError::Process(
                    "conversation data maintenance task panicked".to_string(),
                ))
            });
            match task_background_tasks
                .finish_conversation_data_maintenance(&task_id_for_runtime, result)
            {
                Ok(snapshot) => {
                    let _ = task_app.emit("conversation-data-maintenance-task-updated", &snapshot);
                    Ok(Value::Null)
                }
                Err(error) => Err(error.into()),
            }
        }),
    );
    if let Err(error) = outcome {
        return Err(error.into());
    }
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn audit_conversation_data(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationDataAuditParams,
) -> RuntimeAppResult<ConversationDataMaintenanceTaskSnapshot> {
    start_conversation_data_maintenance_background(
        app,
        state.runtime.clone(),
        state.background_tasks.clone(),
        "audit",
        params,
        None,
    )
}

#[tauri::command]
pub(crate) fn repair_conversation_data(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationDataRepairParams,
) -> RuntimeAppResult<ConversationDataMaintenanceTaskSnapshot> {
    let audit_params = ConversationDataAuditParams {
        source_id: params.source_id.clone(),
        record_kind: params.record_kind.clone(),
        include_resolved: false,
    };
    start_conversation_data_maintenance_background(
        app,
        state.runtime.clone(),
        state.background_tasks.clone(),
        "repair",
        audit_params,
        Some(params),
    )
}

#[tauri::command]
pub(crate) fn get_conversation_data_maintenance_task(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Option<ConversationDataMaintenanceTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .conversation_data_maintenance_snapshot_for_tenant(&tenant_id)
}

#[tauri::command]
pub(crate) fn list_conversation_data_maintenance_tasks(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<ConversationDataMaintenanceTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .conversation_data_maintenance_snapshots_for_tenant(&tenant_id)
}

#[tauri::command]
pub(crate) fn cancel_conversation_data_maintenance(
    state: State<'_, AppState>,
    params: BackgroundTaskGetParams,
) -> RuntimeAppResult<ConversationDataMaintenanceTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .cancel_conversation_data_maintenance_for_tenant(&tenant_id, &params.task_id)
}

#[tauri::command]
pub(crate) async fn rollback_conversation_data(
    state: State<'_, AppState>,
    params: ConversationDataRollbackParams,
) -> RuntimeAppResult<Value> {
    AppService::from_runtime(&state.runtime)
        .rollback_conversation_data(params)
        .await
}

#[tauri::command]
pub(crate) async fn create_plan(
    state: State<'_, AppState>,
    profile_id: Option<String>,
) -> RuntimeAppResult<DeploymentPlan> {
    let result = AppService::from_runtime(&state.runtime)
        .create_plan(profile_id.as_deref())
        .await;

    match &result {
        Ok(plan) => {
            tracing::info!(
                action = "deployment_plan.create",
                profile_id = ?profile_id,
                plan_id = %plan.id,
                action_count = plan.actions.len(),
                create_count = plan.summary.create_count,
                skip_count = plan.summary.skip_count,
                conflict_count = plan.summary.conflict_count,
                "创建部署计划成功"
            );
        }
        Err(error) => tracing::error!(
            action = "deployment_plan.create",
            profile_id = ?profile_id,
            error = %error,
            "创建部署计划失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn execute_plan(
    state: State<'_, AppState>,
    plan: DeploymentPlan,
    action_ids: Option<Vec<String>>,
) -> RuntimeAppResult<ExecutionResult> {
    let plan_id = plan.id.clone();
    let action_count = plan.actions.len();
    let requested_action_count = action_ids.as_ref().map(Vec::len).unwrap_or(0);
    let result = AppService::from_runtime(&state.runtime)
        .execute_plan(plan, action_ids)
        .await;

    match &result {
        Ok(result) => {
            if result.conflict_count > 0 || !result.errors.is_empty() {
                tracing::warn!(
                    action = "deployment_plan.execute",
                    plan_id = %plan_id,
                    action_count = action_count,
                    requested_action_count = requested_action_count,
                    executed_count = result.executed_count,
                    skipped_count = result.skipped_count,
                    conflict_count = result.conflict_count,
                    error_count = result.errors.len(),
                    "执行部署计划完成但存在冲突或失败"
                );
            } else {
                tracing::info!(
                    action = "deployment_plan.execute",
                    plan_id = %plan_id,
                    action_count = action_count,
                    requested_action_count = requested_action_count,
                    executed_count = result.executed_count,
                    skipped_count = result.skipped_count,
                    conflict_count = result.conflict_count,
                    error_count = result.errors.len(),
                    "执行部署计划成功"
                );
            }
        }
        Err(error) => tracing::error!(
            action = "deployment_plan.execute",
            plan_id = %plan_id,
            action_count = action_count,
            requested_action_count = requested_action_count,
            error = %error,
            "执行部署计划失败"
        ),
    }
    result
}
