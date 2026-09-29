use tauri::{AppHandle, State};

use crate::adapters::app_state::AppState;
use crate::adapters::tauri::app_icon::set_application_icon;
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;
use crate::backend::infrastructure::app_settings::{AppLocale, AppSettingsFile};

#[tauri::command]
pub(crate) async fn set_app_window_icon(app: AppHandle, icon: Vec<u8>) -> RuntimeAppResult<()> {
    set_application_icon(app, icon).map_err(AppError::external)
}

#[tauri::command]
pub(crate) async fn get_app_overview(state: State<'_, AppState>) -> RuntimeAppResult<AppOverview> {
    AppService::from_runtime(&state.runtime).overview().await
}

#[tauri::command]
pub(crate) async fn get_app_settings(
    state: State<'_, AppState>,
) -> RuntimeAppResult<AppSettingsFile> {
    AppService::from_runtime(&state.runtime)
        .get_app_settings()
        .await
}

#[tauri::command]
pub(crate) async fn save_app_settings(
    state: State<'_, AppState>,
    settings: serde_json::Value,
) -> RuntimeAppResult<AppSettingsFile> {
    AppService::from_runtime(&state.runtime)
        .save_app_settings(settings)
        .await
}

#[tauri::command]
pub(crate) async fn initialize_app_locale_if_unset(
    state: State<'_, AppState>,
    locale: AppLocale,
) -> RuntimeAppResult<AppSettingsFile> {
    AppService::from_runtime(&state.runtime)
        .initialize_app_locale_if_unset(locale)
        .await
}

#[tauri::command]
pub(crate) fn cancel_app_close_prompt(state: State<'_, AppState>) -> RuntimeAppResult<()> {
    state
        .exit_prompt_open
        .store(false, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub(crate) async fn complete_app_close(
    app: AppHandle,
    state: State<'_, AppState>,
    backup_database: bool,
) -> RuntimeAppResult<()> {
    let shutdown_sync_done = state.shutdown_sync_done.clone();
    let exit_prompt_open = state.exit_prompt_open.clone();
    let allow_close = state.allow_close.clone();
    let allow_exit = state.allow_exit.clone();
    let db_path = state.db_path.clone();
    let background_tasks = state.background_tasks.clone();
    let runtime = state.runtime.clone();

    crate::converge_ai_executions_before_close(background_tasks).await;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);

    let unfinished_tasks = runtime.stop_tasks_until(deadline).await;
    if !unfinished_tasks.is_empty() {
        tracing::warn!(
            action = "app.close.tasks",
            unfinished_tasks = unfinished_tasks.len(),
            "关闭前仍有后台任务未收敛"
        );
    }

    if !shutdown_sync_done.swap(true, std::sync::atomic::Ordering::SeqCst) {
        crate::sync_before_close_with_runtime(&runtime, &db_path, backup_database).await;
    }

    let shutdown_report = runtime.shutdown_until(deadline).await;
    if !shutdown_report.is_clean() {
        tracing::warn!(
            action = "app.close.runtime",
            unfinished_tasks = shutdown_report.unfinished_task_ids.len(),
            dispatcher_remaining_events = shutdown_report.dispatcher_remaining_events,
            dispatcher_timed_out = shutdown_report.dispatcher_timed_out,
            unfinished_stages = %shutdown_report.unfinished_stages.join(","),
            "应用运行时在关闭期限内未完全收敛"
        );
    }

    exit_prompt_open.store(false, std::sync::atomic::Ordering::SeqCst);
    allow_close.store(true, std::sync::atomic::Ordering::SeqCst);
    allow_exit.store(true, std::sync::atomic::Ordering::SeqCst);
    app.exit(0);
    Ok(())
}
