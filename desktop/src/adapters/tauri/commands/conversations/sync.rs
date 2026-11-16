use tauri::{AppHandle, Emitter, State};

use crate::adapters::app_state::AppState;
use crate::adapters::tauri::background_tasks::{
    BackgroundTaskStatus, ConversationSyncTaskSnapshot,
};
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;
use crate::backend::application::{BackgroundTaskGetParams, ConversationSyncParams};

pub(crate) fn start_conversation_sync_background(
    app: AppHandle,
    runtime: std::sync::Arc<crate::backend::infrastructure::runtime::AppRuntime>,
    background_tasks: std::sync::Arc<
        crate::adapters::tauri::background_tasks::BackgroundTaskRegistry,
    >,
    params: ConversationSyncParams,
) -> RuntimeAppResult<ConversationSyncTaskSnapshot> {
    let tenant_id = runtime.context().tenant.id.clone();
    let (snapshot, should_start) =
        background_tasks.begin_conversation_sync_for_tenant(&tenant_id, &params)?;
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
                let progress_app = task_app.clone();
                let progress_tasks = task_background_tasks.clone();
                let progress_task_id = task_id_for_runtime.clone();
                let mut on_progress =
                    move |completed_source_count: usize,
                          total_source_count: usize,
                          current_source_name: Option<String>,
                          completed_adapter_ids: &[String]| {
                        match progress_tasks.update_conversation_sync_progress(
                            &progress_task_id,
                            completed_source_count,
                            total_source_count,
                            current_source_name,
                            completed_adapter_ids.to_vec(),
                        ) {
                            Ok(snapshot) => {
                                if let Err(error) =
                                    progress_app.emit("conversation-sync-task-updated", &snapshot)
                                {
                                    tracing::error!(
                                        action = "conversation.sync",
                                        task_id = %progress_task_id,
                                        error = %error,
                                        "推送后台同步进度失败"
                                    );
                                }
                            }
                            Err(error) => tracing::error!(
                                action = "conversation.sync",
                                task_id = %progress_task_id,
                                error = %error,
                                "更新后台同步进度失败"
                            ),
                        }
                    };
                let cancellation = context.cancellation();
                if context.is_cancelled() {
                    return Err(AppError::Cancelled(
                        "conversation sync cancelled".to_string(),
                    ));
                }
                tauri::async_runtime::block_on(async {
                    AppService::from_runtime(&runtime)
                        .sync_conversations_with_control(
                            params,
                            Some(&cancellation),
                            Some(&task_id_for_runtime),
                            &mut on_progress,
                        )
                        .await
                })
            }))
            .unwrap_or_else(|_| {
                Err(AppError::Process(
                    "conversation sync task panicked".to_string(),
                ))
            });
            match &result {
                Ok(value) => tracing::info!(
                    action = "conversation.sync",
                    task_id = %task_id_for_runtime,
                    result = %value,
                    "后台同步对话记录成功"
                ),
                Err(error) => tracing::error!(
                    action = "conversation.sync",
                    task_id = %task_id_for_runtime,
                    error = %error,
                    "后台同步对话记录失败"
                ),
            }
            match task_background_tasks.finish_conversation_sync(&task_id_for_runtime, result) {
                Ok(snapshot) => {
                    if let Err(error) = task_app.emit("conversation-sync-task-updated", &snapshot) {
                        tracing::error!(
                            action = "conversation.sync",
                            task_id = %task_id_for_runtime,
                            error = %error,
                            "推送后台同步任务状态失败"
                        );
                    }
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

fn start_conversation_usage_scan(
    runtime: std::sync::Arc<crate::backend::infrastructure::runtime::AppRuntime>,
    background_tasks: std::sync::Arc<
        crate::adapters::tauri::background_tasks::BackgroundTaskRegistry,
    >,
    options: crate::backend::application::conversations::UsageScanOptions,
) -> RuntimeAppResult<crate::backend::application::system::TaskView> {
    let handle = AppService::from_runtime(&runtime).start_conversation_usage_scan(options)?;
    let task_runtime = background_tasks
        .task_runtime()
        .ok_or_else(|| AppError::Conflict("TaskRuntime 未初始化".to_string()))?;
    let snapshot = task_runtime.get(handle.task_id()).ok_or_else(|| {
        AppError::NotFound(format!("background task not found: {}", handle.task_id()))
    })?;
    Ok(crate::backend::application::system::TaskView::from_snapshot(&snapshot))
}

#[tauri::command]
pub(crate) fn sync_conversations(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationSyncParams,
) -> RuntimeAppResult<ConversationSyncTaskSnapshot> {
    start_conversation_sync_background(
        app,
        state.runtime.clone(),
        state.background_tasks.clone(),
        params,
    )
}

#[tauri::command]
pub(crate) fn get_conversation_sync_task(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Option<ConversationSyncTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .conversation_sync_snapshot_for_tenant(&tenant_id)
}

#[tauri::command]
pub(crate) fn list_conversation_sync_tasks(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<ConversationSyncTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .conversation_sync_snapshots_for_tenant(&tenant_id)
}

#[tauri::command]
pub(crate) fn cancel_conversation_sync(
    state: State<'_, AppState>,
    params: BackgroundTaskGetParams,
) -> RuntimeAppResult<ConversationSyncTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .cancel_conversation_sync_for_tenant(&tenant_id, &params.task_id)
}

#[tauri::command]
pub(crate) async fn get_conversation_usage_dashboard(
    state: State<'_, AppState>,
    filter: crate::backend::application::conversations::UsageDashboardFilter,
) -> RuntimeAppResult<crate::backend::application::conversations::UsageDashboardDto> {
    AppService::from_runtime(&state.runtime)
        .get_conversation_usage_dashboard(filter)
        .await
}

#[tauri::command]
pub(crate) async fn get_conversation_usage_scan_status(
    state: State<'_, AppState>,
) -> RuntimeAppResult<crate::backend::application::conversations::UsageScanStatusDto> {
    AppService::from_runtime(&state.runtime)
        .get_conversation_usage_scan_status()
        .await
}

#[tauri::command]
pub(crate) fn scan_conversation_usage(
    state: State<'_, AppState>,
    options: crate::backend::application::conversations::UsageScanOptions,
) -> RuntimeAppResult<crate::backend::application::system::TaskView> {
    start_conversation_usage_scan(
        state.runtime.clone(),
        state.background_tasks.clone(),
        options,
    )
}
