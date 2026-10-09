//! System 领域 Tauri Command 模块
//!
//! 包含租户、设置、语言初始化、退出拦截、导航、快捷键、CLI 工具、日志与公共任务等命令。

#[macro_use]
pub(crate) mod navigation;
#[macro_use]
pub(crate) mod settings;
#[macro_use]
pub(crate) mod tasks;
#[macro_use]
pub(crate) mod tenants;
#[macro_use]
pub(crate) mod tools;

pub(crate) use navigation::{
    get_navigation_model, list_app_shortcut_settings, list_app_shortcuts, update_app_shortcuts,
    update_navigation_model,
};
pub(crate) use settings::{
    cancel_app_close_prompt, complete_app_close, get_app_overview, get_app_settings,
    initialize_app_locale_if_unset, save_app_settings, set_app_window_icon,
};
pub(crate) use tasks::{
    cancel_public_task, clear_terminal_tasks, get_public_task, list_public_tasks, retry_public_task,
};
pub(crate) use tenants::{create_tenant, get_active_tenant, list_tenants, switch_tenant};
pub(crate) use tools::{
    copy_prompt_card_to_clipboard, get_cli_tools_status, install_cli_tools, logs_get_snapshot,
    logs_open_log_directory, logs_write_operation, reveal_path,
};

#[cfg(test)]
#[path = "system_tests.rs"]
mod tests;
