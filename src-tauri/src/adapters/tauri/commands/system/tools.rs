use std::collections::BTreeMap;
use tauri::{AppHandle, State};

use crate::adapters::app_state::AppState;
use crate::adapters::prompt_clipboard::{
    copy_prompt_card_to_clipboard as copy_prompt_card_to_clipboard_impl, PromptClipboardParams,
};
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;
use crate::backend::infrastructure::logs::LogSnapshot;

#[tauri::command]
pub(crate) fn reveal_path(path: String) -> RuntimeAppResult<()> {
    let result = crate::adapters::platform::reveal_path(path.clone());
    match &result {
        Ok(()) => tracing::info!(
            action = "path.reveal",
            resource = "filesystem_path",
            "打开路径成功"
        ),
        Err(error) => tracing::error!(
            action = "path.reveal",
            resource = "filesystem_path",
            error_code = %error.code(),
            "打开路径失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn get_cli_tools_status(
    app: AppHandle,
) -> RuntimeAppResult<crate::adapters::cli_tools::CliToolsStatus> {
    crate::adapters::cli_tools::status(&app)
}

#[tauri::command]
pub(crate) fn install_cli_tools(
    app: AppHandle,
) -> RuntimeAppResult<crate::adapters::cli_tools::CliToolsStatus> {
    let result = crate::adapters::cli_tools::install(&app);
    match &result {
        Ok(status) => tracing::info!(
            action = "cli.install",
            install_dir = %status.install_dir,
            path_configured = status.path_configured,
            "安装命令行工具成功"
        ),
        Err(error) => tracing::error!(
            action = "cli.install",
            error = %error,
            "安装命令行工具失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn logs_get_snapshot(
    state: State<'_, AppState>,
    file_name: Option<String>,
    line_limit: Option<usize>,
) -> RuntimeAppResult<LogSnapshot> {
    AppService::from_runtime(&state.runtime).logs_get_snapshot(file_name, line_limit)
}

#[tauri::command]
pub(crate) fn logs_open_log_directory(state: State<'_, AppState>) -> RuntimeAppResult<()> {
    AppService::from_runtime(&state.runtime).logs_open_log_directory()
}

#[tauri::command]
pub(crate) fn logs_write_operation(
    state: State<'_, AppState>,
    level: String,
    operation: String,
    message: String,
    fields: Option<BTreeMap<String, String>>,
) -> RuntimeAppResult<()> {
    AppService::from_runtime(&state.runtime).logs_write_operation(level, operation, message, fields)
}

#[tauri::command]
pub(crate) fn copy_prompt_card_to_clipboard(params: PromptClipboardParams) -> RuntimeAppResult<()> {
    copy_prompt_card_to_clipboard_impl(params)
}
