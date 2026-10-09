use tauri::{AppHandle, Emitter, State};

use crate::adapters::app_state::AppState;
use crate::adapters::tauri::background_tasks::{BackgroundTaskStatus, BatchMountTaskSnapshot};
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;

pub(crate) const BATCH_MOUNT_TASK_UPDATED_EVENT: &str = "batch-mount-task-updated";

#[tauri::command]
pub(crate) fn start_batch_mount(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: String,
    group_id: Option<String>,
    profile_id: String,
    enabled: Option<bool>,
    group_ids: Option<Vec<String>>,
    asset_ids: Option<Vec<String>>,
) -> RuntimeAppResult<BatchMountTaskSnapshot> {
    let mode = mode.trim().to_ascii_lowercase();
    if !matches!(mode.as_str(), "explicit" | "group" | "exclusive") {
        return Err(AppError::Validation(format!(
            "unsupported batch mount mode: {mode}"
        )));
    }
    if profile_id.trim().is_empty() {
        return Err(AppError::Validation("profile_id is required".to_string()));
    }
    let group_id = group_id.map(|value| value.trim().to_string());
    let group_ids = group_ids
        .unwrap_or_default()
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let asset_ids = asset_ids
        .unwrap_or_default()
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if mode == "explicit" && asset_ids.is_empty() {
        return Err(AppError::Validation(
            "asset_ids are required for explicit batch mount".to_string(),
        ));
    }
    if mode == "group" && group_id.as_deref().is_none_or(str::is_empty) {
        return Err(AppError::Validation(
            "group_id is required for group batch mount".to_string(),
        ));
    }
    if mode == "exclusive" && group_ids.is_empty() {
        return Err(AppError::Validation(
            "group_ids are required for exclusive batch mount".to_string(),
        ));
    }
    let workflow_input = match mode.as_str() {
        "explicit" => crate::backend::application::BatchMountWorkflowInput::Explicit {
            asset_ids: asset_ids.clone(),
            profile_id: profile_id.clone(),
            enabled: enabled.unwrap_or(true),
        },
        "group" => crate::backend::application::BatchMountWorkflowInput::Group {
            group_id: group_id.clone().unwrap_or_default(),
            profile_id: profile_id.clone(),
            enabled: enabled.unwrap_or(true),
        },
        "exclusive" => crate::backend::application::BatchMountWorkflowInput::Exclusive {
            group_ids: group_ids.clone(),
            profile_id: profile_id.clone(),
        },
        _ => unreachable!("batch mode was validated"),
    };

    let tenant_id = state.runtime.context().tenant.id.clone();
    let dedup_suffix = if mode == "explicit" {
        asset_ids.join(",")
    } else if mode == "group" {
        format!(
            "{}:{}",
            group_id.as_deref().unwrap_or_default(),
            enabled.unwrap_or(true)
        )
    } else {
        group_ids.join(",")
    };
    let (snapshot, started) =
        state
            .background_tasks
            .begin_batch_mount(&tenant_id, &mode, &profile_id, &dedup_suffix)?;
    if !started {
        return state
            .background_tasks
            .batch_mount_snapshot_for_tenant(&tenant_id, &snapshot.id);
    }

    let task_id = snapshot.id.clone();
    let runtime = state.runtime.clone();
    let tasks = state.background_tasks.clone();
    let task_context = runtime.task_runtime().task_context(&task_id)?;
    let worker_app = app.clone();
    let worker_input = workflow_input;
    let worker_task_id = task_id.clone();
    let spawn_result = std::thread::Builder::new()
        .name(format!("aiw-batch-mount-{}", &task_id[..8]))
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let service = AppService::from_runtime(&runtime);
                if task_context.is_cancelled() {
                    return Err(AppError::Cancelled(
                        "batch mount cancelled before execution".to_string(),
                    ));
                }
                tauri::async_runtime::block_on(async {
                    service
                        .run_batch_mount_workflow_with_progress(
                            worker_input,
                            |completed, total, current_id| {
                                if task_context.is_cancelled() {
                                    return Err(AppError::Cancelled(
                                        "batch mount cancelled".to_string(),
                                    ));
                                }
                                tasks
                                    .update_batch_mount_progress(
                                        &worker_task_id,
                                        completed as u64,
                                        Some(total as u64),
                                        Some(current_id),
                                    )
                                    .map(|_| ())
                            },
                        )
                        .await
                })
                .and_then(|value| {
                    serde_json::to_value(value)
                        .map_err(|error| AppError::External(error.to_string()))
                })
            }))
            .unwrap_or_else(|_| {
                Err(AppError::External(
                    "batch mount worker panicked".to_string(),
                ))
            });
            let mut result = result;
            if let Ok(value) = result.as_mut() {
                let partial = value
                    .get("errorCount")
                    .or_else(|| value.get("error_count"))
                    .and_then(Value::as_u64)
                    .is_some_and(|count| count > 0)
                    || value
                        .get("errors")
                        .and_then(Value::as_array)
                        .is_some_and(|errors| !errors.is_empty());
                if let Some(object) = value.as_object_mut() {
                    object.insert(
                        "status".to_string(),
                        Value::String(
                            if partial {
                                "partial_failure"
                            } else {
                                "succeeded"
                            }
                            .to_string(),
                        ),
                    );
                }
            }
            let (completed, total) = result
                .as_ref()
                .ok()
                .and_then(|value| {
                    let total = value
                        .get("requestedCount")
                        .or_else(|| value.get("requested_count"))
                        .and_then(Value::as_u64)
                        .or_else(|| {
                            let preview = value.get("preview")?;
                            let mount = preview.get("mountCount")?.as_u64()?;
                            let unmount = preview.get("unmountCount")?.as_u64()?;
                            Some(mount + unmount)
                        });
                    total.map(|total| (total, Some(total)))
                })
                .unwrap_or((0, None));
            let _ = tasks.update_batch_mount_progress(&worker_task_id, completed, total, None);
            let finish = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                tasks.finish_batch_mount(&worker_task_id, result)
            }))
            .unwrap_or_else(|_| {
                tasks.finish_batch_mount(
                    &worker_task_id,
                    Err(AppError::Process("batch mount worker panicked".to_string())),
                )
            });
            if let Ok(snapshot) = finish {
                let _ = worker_app.emit(BATCH_MOUNT_TASK_UPDATED_EVENT, &snapshot);
            }
        });
    if let Err(error) = spawn_result {
        let failure = state.background_tasks.finish_batch_mount(
            &task_id,
            Err(AppError::Process(format!(
                "启动 batch mount worker 失败: {error}"
            ))),
        )?;
        let _ = app.emit(BATCH_MOUNT_TASK_UPDATED_EVENT, &failure);
        return Ok(failure);
    }
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_batch_mount_task(
    state: State<'_, AppState>,
    task_id: String,
) -> RuntimeAppResult<BatchMountTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .batch_mount_snapshot_for_tenant(&tenant_id, &task_id)
}

#[tauri::command]
pub(crate) fn list_batch_mount_tasks(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<BatchMountTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .batch_mount_snapshots_for_tenant(&tenant_id)
}

#[tauri::command]
pub(crate) fn cancel_batch_mount(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> RuntimeAppResult<BatchMountTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    let snapshot = state
        .background_tasks
        .cancel_batch_mount_for_tenant(&tenant_id, &task_id)?;
    let _ = app.emit(BATCH_MOUNT_TASK_UPDATED_EVENT, &snapshot);
    Ok(snapshot)
}
