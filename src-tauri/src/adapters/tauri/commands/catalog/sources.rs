use tauri::{AppHandle, Emitter, State};

use crate::adapters::app_state::AppState;
use crate::adapters::tauri::background_tasks::{
    BackgroundTaskStatus, SourceScanScope, SourceScanTaskSnapshot,
};
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;
use crate::backend::domain::{Source, SourceOrigin};

pub(crate) const SOURCE_SCAN_TASK_UPDATED_EVENT: &str = "source-scan-task-updated";

#[tauri::command]
pub(crate) async fn list_sources(state: State<'_, AppState>) -> RuntimeAppResult<Vec<Source>> {
    AppService::from_runtime(&state.runtime)
        .list_sources()
        .await
}

#[tauri::command]
pub(crate) async fn list_skill_sources(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<Source>> {
    AppService::from_runtime(&state.runtime)
        .list_skill_sources()
        .await
}

#[tauri::command]
pub(crate) async fn create_source(
    state: State<'_, AppState>,
    source: SourceInput,
) -> RuntimeAppResult<Source> {
    let source_name = source.name.clone();
    let source_kind = format!("{:?}", source.kind);
    let result = AppService::from_runtime(&state.runtime)
        .add_source(source)
        .await;

    match &result {
        Ok(source) => tracing::info!(
            action = "source.create",
            source_id = %source.id,
            source_name = %source.name,
            source_kind = ?source.kind,
            "添加数据来源成功"
        ),
        Err(error) => tracing::error!(
            action = "source.create",
            source_name = %source_name,
            source_kind = %source_kind,
            error = %error,
            "添加数据来源失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn update_source(
    state: State<'_, AppState>,
    source: Source,
) -> RuntimeAppResult<Source> {
    let source_id = source.id.clone();
    let source_name = source.name.clone();
    let source_kind = format!("{:?}", source.kind);
    let result = AppService::from_runtime(&state.runtime)
        .update_source(source)
        .await;

    match &result {
        Ok(source) => tracing::info!(
            action = "source.update",
            source_id = %source.id,
            source_name = %source.name,
            source_kind = ?source.kind,
            "更新数据来源成功"
        ),
        Err(error) => tracing::error!(
            action = "source.update",
            source_id = %source_id,
            source_name = %source_name,
            source_kind = %source_kind,
            error = %error,
            "更新数据来源失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn delete_source(state: State<'_, AppState>, id: String) -> RuntimeAppResult<()> {
    let result = AppService::from_runtime(&state.runtime)
        .remove_source(SourceRemoveParams {
            id: id.clone(),
            dry_run: false,
            yes: true,
        })
        .await
        .map(|_| ());

    match &result {
        Ok(()) => tracing::info!(
            action = "source.delete",
            source_id = %id,
            "删除数据来源成功"
        ),
        Err(error) => tracing::error!(
            action = "source.delete",
            source_id = %id,
            error = %error,
            "删除数据来源失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn start_source_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: Option<AssetKind>,
    scope: Option<SourceScanScope>,
) -> RuntimeAppResult<SourceScanTaskSnapshot> {
    let scope = scope.unwrap_or(SourceScanScope::All);
    let scan_kind = if scope == SourceScanScope::Skills {
        Some(AssetKind::Skill)
    } else {
        kind
    };
    let tenant_id = state.runtime.context().tenant.id.clone();
    let (snapshot, started) = state
        .background_tasks
        .begin_source_scan(&tenant_id, scope, scan_kind)?;
    if !started {
        return state
            .background_tasks
            .source_scan_snapshot_for_tenant(&tenant_id, &snapshot.id);
    }

    let task_id = snapshot.id.clone();
    let runtime = state.runtime.clone();
    let tasks = state.background_tasks.clone();
    let task_context = runtime.task_runtime().task_context(&task_id)?;
    let params = SourceScanParams {
        kind: scan_kind,
        dry_run: false,
    };
    let skill_sources_only = scope == SourceScanScope::Skills;
    let worker_app = app.clone();
    let worker_task_id = task_id.clone();
    tauri::async_runtime::spawn(async move {
        let result = match tokio::spawn(async move {
            let service = AppService::from_runtime(&runtime);
            service
                .scan_sources_with_task_context(params, &task_context, skill_sources_only)
                .await
        })
        .await
        {
            Ok(inner_result) => inner_result,
            Err(_) => Err(AppError::Process("source scan worker panicked".to_string())),
        };
        if let Ok(snapshot) = tasks.finish_source_scan(&worker_task_id, result) {
            let _ = worker_app.emit(SOURCE_SCAN_TASK_UPDATED_EVENT, &snapshot);
        }
    });
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_source_scan_task(
    state: State<'_, AppState>,
    task_id: String,
) -> RuntimeAppResult<SourceScanTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .source_scan_snapshot_for_tenant(&tenant_id, &task_id)
}

#[tauri::command]
pub(crate) fn list_source_scan_tasks(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<SourceScanTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .source_scan_snapshots_for_tenant(&tenant_id)
}

#[tauri::command]
pub(crate) fn cancel_source_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> RuntimeAppResult<SourceScanTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    let snapshot = state
        .background_tasks
        .cancel_source_scan_for_tenant(&tenant_id, &task_id)?;
    let _ = app.emit(SOURCE_SCAN_TASK_UPDATED_EVENT, &snapshot);
    Ok(snapshot)
}
