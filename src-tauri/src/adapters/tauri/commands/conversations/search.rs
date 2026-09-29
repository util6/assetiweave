use tauri::{AppHandle, Emitter, State};

use crate::adapters::app_state::AppState;
use crate::adapters::tauri::background_tasks::{
    BackgroundTaskStatus, ConversationSearchIndexTaskSnapshot,
};
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;
use crate::backend::application::{
    BackgroundTaskGetParams, ConversationBlockGetParams, ConversationBlockListParams,
    ConversationQuestionGetParams, ConversationQuestionListParams, ConversationQuestionMergeParams,
    ConversationQuestionSplitParams, ConversationSearchParams, ConversationSearchResult,
};
use crate::backend::domain::{AppErrorView, ConversationSearchIndexStatus};

#[tauri::command]
pub(crate) async fn search_conversation_records(
    state: State<'_, AppState>,
    params: ConversationSearchParams,
) -> RuntimeAppResult<ConversationSearchResult> {
    AppService::from_runtime(&state.runtime)
        .search_conversation_records(params)
        .await
}

#[tauri::command]
pub(crate) async fn search_recent_incremental_conversation_records(
    state: State<'_, AppState>,
    params: crate::backend::application::ConversationIncrementalSearchParams,
) -> RuntimeAppResult<ConversationSearchResult> {
    AppService::from_runtime(&state.runtime)
        .search_recent_incremental_conversation_records(params)
        .await
}

#[tauri::command]
pub(crate) async fn get_conversation_search_index_status(
    state: State<'_, AppState>,
) -> RuntimeAppResult<ConversationSearchIndexStatus> {
    AppService::from_runtime(&state.runtime)
        .get_conversation_search_index_status()
        .await
}

#[tauri::command]
pub(crate) fn start_conversation_search_index_rebuild(
    app: AppHandle,
    state: State<'_, AppState>,
) -> RuntimeAppResult<ConversationSearchIndexTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    let (snapshot, should_start) = state
        .background_tasks
        .begin_conversation_search_index_rebuild_for_tenant(&tenant_id)?;
    if !should_start {
        return Ok(snapshot);
    }

    let runtime = state.runtime.clone();
    let background_tasks = state.background_tasks.clone();
    let task_id = snapshot.id.clone();
    let task_runtime = background_tasks
        .task_runtime()
        .ok_or_else(|| AppError::Conflict("TaskRuntime 未初始化".to_string()))?;
    let task_detail = task_runtime
        .get(&task_id)
        .map(|task| task.detail)
        .ok_or_else(|| AppError::NotFound(format!("background task not found: {task_id}")))?;
    let app = app.clone();
    let task_id_for_runtime = task_id.clone();
    let background_tasks_for_runtime = background_tasks.clone();
    let outcome = task_runtime.start_external_with(
        &task_id,
        task_detail,
        Box::new(move |_context| {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                tauri::async_runtime::block_on(async {
                    AppService::from_runtime(&runtime)
                        .rebuild_conversation_search_index()
                        .await
                        .and_then(|report| {
                            serde_json::to_value(report).map_err(|error| {
                                AppError::External(format!(
                                    "serialize conversation search index report: {error}"
                                ))
                            })
                        })
                })
            }))
            .unwrap_or_else(|_| {
                Err(AppError::Process(
                    "conversation search index rebuild panicked".to_string(),
                ))
            });
            let projection_result = match &result {
                Ok(value) => Ok(value.clone()),
                Err(error) => Err(AppError::from(error.view())),
            };
            match background_tasks_for_runtime
                .finish_conversation_search_index_rebuild(&task_id_for_runtime, projection_result)
            {
                Ok(snapshot) => {
                    if let Err(error) =
                        app.emit("conversation-search-index-task-updated", &snapshot)
                    {
                        tracing::error!(
                            action = "conversation.search.index.rebuild",
                            task_id = %task_id_for_runtime,
                            error = %error,
                            "推送对话搜索索引任务状态失败"
                        );
                    }
                }
                Err(error) => tracing::error!(
                    action = "conversation.search.index.rebuild",
                    task_id = %task_id_for_runtime,
                    error = %error,
                    "更新对话搜索索引任务状态失败"
                ),
            }
            result.map_err(AppErrorView::from)
        }),
    );
    if let Err(error) = outcome {
        let projection_error = AppError::from(error.view());
        let _ = background_tasks
            .finish_conversation_search_index_rebuild(&task_id, Err(projection_error));
        return Err(error.into());
    }
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_conversation_search_index_task(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Option<ConversationSearchIndexTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .conversation_search_index_snapshot_for_tenant(&tenant_id)
}

#[tauri::command]
pub(crate) async fn list_conversation_questions(
    state: State<'_, AppState>,
    params: ConversationQuestionListParams,
) -> RuntimeAppResult<Vec<crate::backend::domain::ConversationQuestionDetail>> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_questions(params)
        .await
}

#[tauri::command]
pub(crate) async fn get_conversation_question(
    state: State<'_, AppState>,
    params: ConversationQuestionGetParams,
) -> RuntimeAppResult<crate::backend::domain::ConversationQuestionDetail> {
    AppService::from_runtime(&state.runtime)
        .get_conversation_question(params)
        .await
}

#[tauri::command]
pub(crate) async fn list_conversation_blocks(
    state: State<'_, AppState>,
    params: ConversationBlockListParams,
) -> RuntimeAppResult<Vec<crate::backend::domain::ConversationBlockLocator>> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_blocks(params)
        .await
}

#[tauri::command]
pub(crate) async fn get_conversation_block(
    state: State<'_, AppState>,
    params: ConversationBlockGetParams,
) -> RuntimeAppResult<crate::backend::domain::ConversationBlockDetail> {
    AppService::from_runtime(&state.runtime)
        .get_conversation_block(params)
        .await
}

#[tauri::command]
pub(crate) async fn merge_conversation_questions(
    state: State<'_, AppState>,
    params: ConversationQuestionMergeParams,
) -> RuntimeAppResult<crate::backend::domain::ConversationMutationResult> {
    AppService::from_runtime(&state.runtime)
        .merge_conversation_questions(params)
        .await
}

#[tauri::command]
pub(crate) async fn split_conversation_question(
    state: State<'_, AppState>,
    params: ConversationQuestionSplitParams,
) -> RuntimeAppResult<crate::backend::domain::ConversationMutationResult> {
    AppService::from_runtime(&state.runtime)
        .split_conversation_question(params)
        .await
}
