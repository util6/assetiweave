use super::*;

#[test]
fn test_system_module_exports_expected_commands() {
    // 验证各系统命令符号在此模块作用域内均有效定义或重导出
    let _ = get_app_overview;
    let _ = set_app_window_icon;
    let _ = list_tenants;
    let _ = get_active_tenant;
    let _ = create_tenant;
    let _ = switch_tenant;
    let _ = get_app_settings;
    let _ = save_app_settings;
    let _ = initialize_app_locale_if_unset;
    let _ = cancel_app_close_prompt;
    let _ = complete_app_close;
    let _ = get_navigation_model;
    let _ = update_navigation_model;
    let _ = list_app_shortcuts;
    let _ = list_app_shortcut_settings;
    let _ = update_app_shortcuts;
    let _ = reveal_path;
    let _ = get_cli_tools_status;
    let _ = install_cli_tools;
    let _ = logs_get_snapshot;
    let _ = logs_open_log_directory;
    let _ = logs_write_operation;
    let _ = copy_prompt_card_to_clipboard;
    let _ = list_public_tasks;
    let _ = get_public_task;
    let _ = cancel_public_task;
    let _ = retry_public_task;
    let _ = clear_terminal_tasks;
}
