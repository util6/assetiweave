use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::adapters::app_state::AppState;
use crate::backend::application::prelude::*;
use crate::backend::application::system::TaskView;
use crate::backend::application::AppResult as RuntimeAppResult;
use crate::backend::domain::Source;
use crate::backend::infrastructure::tasks::{CancelOutcome, TaskFilter, TaskKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceScanScope {
    All,
    Skills,
}

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
    state: State<'_, AppState>,
    kind: Option<AssetKind>,
    scope: Option<SourceScanScope>,
) -> RuntimeAppResult<TaskView> {
    let scope = scope.unwrap_or(SourceScanScope::All);
    let scan_kind = if scope == SourceScanScope::Skills {
        Some(AssetKind::Skill)
    } else {
        kind
    };
    let params = SourceScanParams {
        kind: scan_kind,
        dry_run: false,
    };
    let skill_sources_only = scope == SourceScanScope::Skills;
    let service = AppService::from_runtime(&state.runtime);
    let handle = service.start_source_scan_pipeline(params, skill_sources_only)?;
    let snapshot = state
        .runtime
        .task_runtime()
        .get(handle.task_id())
        .ok_or_else(|| AppError::NotFound(format!("任务未找到: {}", handle.task_id())))?;
    Ok(TaskView::from_snapshot(&snapshot))
}

#[tauri::command]
pub(crate) fn get_source_scan_task(
    state: State<'_, AppState>,
    task_id: String,
) -> RuntimeAppResult<TaskView> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    let snapshot = state
        .runtime
        .task_runtime()
        .get_for_tenant(&tenant_id, &task_id)
        .ok_or_else(|| AppError::NotFound(format!("任务未找到: {}", task_id)))?;
    Ok(TaskView::from_snapshot(&snapshot))
}

#[tauri::command]
pub(crate) fn list_source_scan_tasks(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<TaskView>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    let snapshots = state.runtime.task_runtime().list_for_tenant(
        &tenant_id,
        TaskFilter {
            kind: Some(TaskKind::Scan),
            active_only: false,
            user_visible_only: false,
        },
    );
    Ok(snapshots.iter().map(TaskView::from_snapshot).collect())
}

#[tauri::command]
pub(crate) fn cancel_source_scan(
    state: State<'_, AppState>,
    task_id: String,
) -> RuntimeAppResult<TaskView> {
    let snapshot = match state.runtime.task_runtime().cancel(&task_id) {
        CancelOutcome::Requested(s) | CancelOutcome::AlreadyFinished(s) => s,
        CancelOutcome::NotFound => {
            return Err(AppError::NotFound(format!("任务未找到: {}", task_id)).into())
        }
    };
    Ok(TaskView::from_snapshot(&snapshot))
}

#[tauri::command]
pub(crate) async fn probe_source_path(
    state: State<'_, AppState>,
    path: String,
) -> RuntimeAppResult<crate::backend::domain::SourceProbeResult> {
    AppService::from_runtime(&state.runtime)
        .probe_source_path(&path)
        .await
}

#[tauri::command]
pub(crate) async fn split_container_source(
    state: State<'_, AppState>,
    source_id: String,
) -> RuntimeAppResult<Vec<Source>> {
    AppService::from_runtime(&state.runtime)
        .split_container_source(&source_id)
        .await
}

pub(crate) const GIT_SOURCE_PULL_TASK_UPDATED_EVENT: &str = "git-source-pull-task-updated";

#[tauri::command]
pub(crate) fn start_git_source_pull(
    app: AppHandle,
    state: State<'_, AppState>,
    source_ids: Vec<String>,
) -> RuntimeAppResult<crate::adapters::tauri::background_tasks::GitSourcePullTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    let (snapshot, started) = state
        .background_tasks
        .begin_git_source_pull(&tenant_id, source_ids.clone())?;
    if !started {
        return state
            .background_tasks
            .git_source_pull_snapshot_for_tenant(&tenant_id, &snapshot.id);
    }

    let task_id = snapshot.id.clone();
    let runtime = state.runtime.clone();
    let tasks = state.background_tasks.clone();
    let task_context = runtime.task_runtime().task_context(&task_id)?;
    let worker_app = app.clone();
    let worker_task_id = task_id.clone();

    tauri::async_runtime::spawn(async move {
        let service = AppService::from_runtime(&runtime);
        let all_sources = service.list_sources().await.unwrap_or_default();
        let mut item_results = Vec::new();
        let mut pulled_repos: std::collections::HashMap<
            std::path::PathBuf,
            crate::backend::infrastructure::path_utils::GitPullOutcome,
        > = std::collections::HashMap::new();
        let total = source_ids.len();

        for (index, sid) in source_ids.iter().enumerate() {
            if task_context.is_cancelled() {
                break;
            }
            let source = all_sources.iter().find(|s| s.id == *sid);
            let (source_name, outcome) = match source {
                Some(s) => {
                    let name = s.name.clone();
                    let raw_path = s.repo_root.as_deref().unwrap_or(&s.root_path);
                    let outcome = match crate::backend::infrastructure::path_utils::expand_path(
                        raw_path,
                    ) {
                        Ok(expanded) => {
                            let repo_root =
                                crate::backend::infrastructure::path_utils::find_git_root(
                                    &expanded,
                                )
                                .unwrap_or(expanded);
                            if let Some(cached) = pulled_repos.get(&repo_root) {
                                cached.clone()
                            } else {
                                let pull_res = crate::backend::infrastructure::path_utils::pull_git_repository(&repo_root)
                                    .unwrap_or_else(|err| {
                                        crate::backend::infrastructure::path_utils::GitPullOutcome {
                                            success: false,
                                            changed: false,
                                            message: err.to_string(),
                                        }
                                    });
                                pulled_repos.insert(repo_root, pull_res.clone());
                                pull_res
                            }
                        }
                        Err(err) => crate::backend::infrastructure::path_utils::GitPullOutcome {
                            success: false,
                            changed: false,
                            message: err.to_string(),
                        },
                    };
                    (name, outcome)
                }
                None => (
                    sid.clone(),
                    crate::backend::infrastructure::path_utils::GitPullOutcome {
                        success: false,
                        changed: false,
                        message: "Source not found".to_string(),
                    },
                ),
            };

            let item = crate::adapters::tauri::background_tasks::GitSourcePullItemResult {
                source_id: sid.clone(),
                source_name: source_name.clone(),
                success: outcome.success,
                changed: outcome.changed,
                message: outcome.message,
            };
            item_results.push(item.clone());

            if let Ok(progress_snapshot) = tasks.update_git_source_pull_progress(
                &worker_task_id,
                index + 1,
                total,
                Some(source_name),
                Some(item),
            ) {
                let _ = worker_app.emit(GIT_SOURCE_PULL_TASK_UPDATED_EVENT, &progress_snapshot);
            }
        }

        let has_changes = item_results.iter().any(|r| r.success && r.changed);
        if has_changes {
            let _ = service.scan_skill_sources().await;
        }

        if let Ok(finished_snapshot) =
            tasks.finish_git_source_pull(&worker_task_id, item_results, None)
        {
            let _ = worker_app.emit(GIT_SOURCE_PULL_TASK_UPDATED_EVENT, &finished_snapshot);
        }
    });

    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_git_source_pull_task(
    state: State<'_, AppState>,
    task_id: String,
) -> RuntimeAppResult<crate::adapters::tauri::background_tasks::GitSourcePullTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .git_source_pull_snapshot_for_tenant(&tenant_id, &task_id)
}

#[tauri::command]
pub(crate) fn list_git_source_pull_tasks(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<crate::adapters::tauri::background_tasks::GitSourcePullTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .git_source_pull_snapshots_for_tenant(&tenant_id)
}

#[tauri::command]
pub(crate) fn cancel_git_source_pull(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> RuntimeAppResult<crate::adapters::tauri::background_tasks::GitSourcePullTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    let snapshot = state
        .background_tasks
        .cancel_git_source_pull_for_tenant(&tenant_id, &task_id)?;
    let _ = app.emit(GIT_SOURCE_PULL_TASK_UPDATED_EVENT, &snapshot);
    Ok(snapshot)
}
