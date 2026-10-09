use serde_json::Value;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

use crate::adapters::app_state::AppState;
use crate::adapters::tauri::background_tasks::{
    BackgroundTaskRegistry, BackgroundTaskStatus, ConversationScriptInstallTaskSnapshot,
};
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;
use crate::backend::application::{
    AppError, ConversationAdapterCatalogRefreshParams, ConversationAdapterLocalRegisterParams,
    ConversationAdapterPackageCatalogParams, ConversationAdapterPackageChangeParams,
    ConversationAdapterPackageInspectParams, ConversationAdapterPackageInstallParams,
    ConversationAdapterPackageReleaseListParams, ConversationAdapterPackageUninstallParams,
    ConversationAdapterPackageUpdateCheckParams, ConversationAdapterPackageUpdatePolicyParams,
    ConversationAdapterPackageVersionChangeParams, ConversationScriptCatalogParams,
    ConversationScriptInstallParams,
};
use crate::backend::domain::AppErrorView;
use crate::backend::infrastructure::tasks::TaskContext;

fn emit_conversation_script_install_task(
    app: &AppHandle,
    snapshot: &ConversationScriptInstallTaskSnapshot,
) {
    if let Err(error) = app.emit("conversation-script-install-task-updated", snapshot) {
        tracing::error!(
            action = "conversation.script.install",
            task_id = %snapshot.id,
            error = %error,
            "推送对话脚本后台安装任务状态失败"
        );
    }
}

fn spawn_conversation_lifecycle_task<F>(
    app: AppHandle,
    background_tasks: Arc<BackgroundTaskRegistry>,
    task_id: String,
    operation: &'static str,
    work: F,
) -> RuntimeAppResult<()>
where
    F: FnOnce() -> RuntimeAppResult<Value> + Send + 'static,
{
    let task_id_for_runtime = task_id.clone();
    let background_tasks_for_runtime = background_tasks.clone();
    let app_for_runtime = app.clone();
    let operation_for_runtime = operation;
    let task = Box::new(move |context: TaskContext| {
        let result = if context.is_cancelled() {
            Err(AppError::Cancelled(format!(
                "{operation_for_runtime} task cancelled"
            )))
        } else {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).unwrap_or_else(|_| {
                Err(AppError::Process(format!(
                    "{operation_for_runtime} task panicked"
                )))
            })
        };
        match &result {
            Ok(value) => tracing::info!(
                action = operation_for_runtime,
                task_id = %task_id_for_runtime,
                result = %value,
                "扩展生命周期任务成功"
            ),
            Err(error) => tracing::error!(
                action = operation_for_runtime,
                task_id = %task_id_for_runtime,
                error = %error,
                "扩展生命周期任务失败"
            ),
        }
        let projection_result = match &result {
            Ok(value) => Ok(value.clone()),
            Err(error) => Err(AppError::from(error.view())),
        };
        match background_tasks_for_runtime
            .finish_conversation_script_install(&task_id_for_runtime, projection_result)
        {
            Ok(snapshot) => emit_conversation_script_install_task(&app_for_runtime, &snapshot),
            Err(error) => tracing::error!(
                action = operation_for_runtime,
                task_id = %task_id_for_runtime,
                error = %error,
                "更新扩展生命周期任务状态失败"
            ),
        }
        result.map_err(AppErrorView::from)
    });
    if let Err(error) = background_tasks.spawn_extension_lifecycle(&task_id, task) {
        let projection_error = AppError::from(error.view());
        let _ =
            background_tasks.finish_conversation_script_install(&task_id, Err(projection_error));
        return Err(error);
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn list_conversation_script_catalog(
    state: State<'_, AppState>,
    params: ConversationScriptCatalogParams,
) -> RuntimeAppResult<Vec<crate::backend::application::ConversationScriptCatalogEntry>> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_script_catalog(params)
        .await
}

#[tauri::command]
pub(crate) async fn register_conversation_adapter_local(
    state: State<'_, AppState>,
    params: ConversationAdapterLocalRegisterParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .register_conversation_adapter_local(params)
        .await
}

#[tauri::command]
pub(crate) async fn inspect_conversation_adapter_package(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageInspectParams,
) -> RuntimeAppResult<crate::backend::application::ConversationAdapterPackageInspection> {
    AppService::from_runtime(&state.runtime)
        .inspect_conversation_adapter_package(params)
        .await
}

#[tauri::command]
pub(crate) async fn prepare_conversation_adapter_package_change(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageChangeParams,
) -> RuntimeAppResult<crate::backend::application::ConversationAdapterPackageChangePreflight> {
    let mut preflight = AppService::from_runtime(&state.runtime)
        .prepare_conversation_adapter_package_change(params)
        .await?;
    if state
        .background_tasks
        .conversation_script_install_snapshot()?
        .is_some_and(|task| task.status == BackgroundTaskStatus::Running)
    {
        preflight.task_conflicts.push("package_change".to_string());
    }
    Ok(preflight)
}

#[tauri::command]
pub(crate) async fn list_conversation_adapter_packages(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageCatalogParams,
) -> RuntimeAppResult<Vec<crate::backend::application::ConversationAdapterPackageCatalogEntry>> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_adapter_packages(params)
        .await
}

#[tauri::command]
pub(crate) async fn list_conversation_adapter_package_releases(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageReleaseListParams,
) -> RuntimeAppResult<Vec<crate::backend::domain::ConversationAdapterCatalogRelease>> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_adapter_package_releases(params)
        .await
}

#[tauri::command]
pub(crate) async fn list_installed_conversation_adapter_package_versions(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageVersionChangeParams,
) -> RuntimeAppResult<Vec<crate::backend::domain::ConversationAdapterPackageVersion>> {
    AppService::from_runtime(&state.runtime)
        .list_installed_conversation_adapter_package_versions(params)
        .await
}

#[tauri::command]
pub(crate) async fn switch_conversation_adapter_package_version(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageVersionChangeParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .switch_conversation_adapter_package_version(params)
        .await
}

#[tauri::command]
pub(crate) async fn rollback_conversation_adapter_package_version(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageVersionChangeParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .rollback_conversation_adapter_package_version(params)
        .await
}

#[tauri::command]
pub(crate) async fn delete_conversation_adapter_package_version(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageVersionChangeParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .delete_conversation_adapter_package_version(params)
        .await
}

#[tauri::command]
pub(crate) async fn refresh_conversation_adapter_catalogs(
    state: State<'_, AppState>,
    params: ConversationAdapterCatalogRefreshParams,
) -> RuntimeAppResult<Vec<crate::backend::domain::ConversationAdapterCatalogRelease>> {
    AppService::from_runtime(&state.runtime)
        .refresh_conversation_adapter_catalogs(params)
        .await
}

#[tauri::command]
pub(crate) async fn check_conversation_adapter_package_updates(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageUpdateCheckParams,
) -> RuntimeAppResult<Vec<crate::backend::application::ConversationAdapterPackageUpdateStatus>> {
    AppService::from_runtime(&state.runtime)
        .check_conversation_adapter_package_updates(params)
        .await
}

#[tauri::command]
pub(crate) async fn set_conversation_adapter_package_update_policy(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageUpdatePolicyParams,
) -> RuntimeAppResult<crate::backend::domain::ConversationAdapterPackage> {
    AppService::from_runtime(&state.runtime)
        .set_conversation_adapter_package_update_policy(params)
        .await
}

#[tauri::command]
pub(crate) fn install_conversation_adapter_package(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationAdapterPackageInstallParams,
) -> RuntimeAppResult<ConversationScriptInstallTaskSnapshot> {
    let (snapshot, should_start) = state
        .background_tasks
        .begin_conversation_adapter_package_install(&params)?;
    if !should_start {
        return Ok(snapshot);
    }

    let runtime = state.runtime.clone();
    let task_id = snapshot.id.clone();
    spawn_conversation_lifecycle_task(
        app,
        state.background_tasks.clone(),
        task_id,
        "conversation.adapter_package.install",
        move || {
            tauri::async_runtime::block_on(async {
                AppService::from_runtime(&runtime)
                    .install_conversation_adapter_package(params)
                    .await
            })
        },
    )?;

    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn update_conversation_adapter_package(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationAdapterPackageInstallParams,
) -> RuntimeAppResult<ConversationScriptInstallTaskSnapshot> {
    let (snapshot, should_start) = state
        .background_tasks
        .begin_conversation_adapter_package_update(&params)?;
    if !should_start {
        return Ok(snapshot);
    }

    let runtime = state.runtime.clone();
    let task_id = snapshot.id.clone();
    spawn_conversation_lifecycle_task(
        app,
        state.background_tasks.clone(),
        task_id,
        "conversation.adapter_package.update",
        move || {
            tauri::async_runtime::block_on(async {
                AppService::from_runtime(&runtime)
                    .update_conversation_adapter_package(params)
                    .await
            })
        },
    )?;

    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn uninstall_conversation_adapter_package(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationAdapterPackageUninstallParams,
) -> RuntimeAppResult<ConversationScriptInstallTaskSnapshot> {
    let (snapshot, should_start) = state
        .background_tasks
        .begin_conversation_adapter_package_uninstall(&params)?;
    if !should_start {
        return Ok(snapshot);
    }
    let runtime = state.runtime.clone();
    let task_id = snapshot.id.clone();
    spawn_conversation_lifecycle_task(
        app,
        state.background_tasks.clone(),
        task_id,
        "conversation.adapter_package.uninstall",
        move || {
            tauri::async_runtime::block_on(async {
                AppService::from_runtime(&runtime)
                    .uninstall_conversation_adapter_package(params)
                    .await
            })
        },
    )?;
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_conversation_adapter_package_task(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Option<ConversationScriptInstallTaskSnapshot>> {
    state
        .background_tasks
        .conversation_script_install_snapshot()
}

#[tauri::command]
pub(crate) fn install_conversation_script(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationScriptInstallParams,
) -> RuntimeAppResult<ConversationScriptInstallTaskSnapshot> {
    let (snapshot, should_start) = state
        .background_tasks
        .begin_conversation_script_install(&params)?;
    if !should_start {
        return Ok(snapshot);
    }

    let runtime = state.runtime.clone();
    let task_id = snapshot.id.clone();
    spawn_conversation_lifecycle_task(
        app,
        state.background_tasks.clone(),
        task_id,
        "conversation.script.install",
        move || {
            tauri::async_runtime::block_on(async {
                AppService::from_runtime(&runtime)
                    .install_conversation_script(params)
                    .await
            })
        },
    )?;

    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_conversation_script_install_task(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Option<ConversationScriptInstallTaskSnapshot>> {
    state
        .background_tasks
        .conversation_script_install_snapshot()
}
