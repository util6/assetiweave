use super::*;

#[test]
fn test_agents_module_exports_expected_commands() {
    let _ = list_agent_catalog;
    let _ = list_agent_market;
    let _ = inspect_agent_market_item;
    let _ = refresh_agent_market;
    let _ = get_agent_market_refresh_task;
    let _ = list_agent_market_refresh_tasks;
    let _ = preview_agent_installation;
    let _ = preview_agent_uninstall;
    let _ = list_installed_agents;
    let _ = get_installed_agent;
    let _ = check_agent_runtime;
    let _ = get_agent_lifecycle_task;
    let _ = list_agent_lifecycle_tasks;
    let _ = cancel_agent_lifecycle_task;
    let _ = start_agent_installation;
    let _ = start_agent_update;
    let _ = start_agent_reinstallation;
    let _ = start_agent_uninstall;
    let _ = enable_agent;
    let _ = disable_agent;
    let _ = check_agent_connection;
    let _ = list_agent_models;
    let _ = cancel_agent_model_probe;
    let _ = get_ai_execution_task;
    let _ = list_ai_execution_tasks;
    let _ = cancel_ai_execution_task;
    let _ = agent_session_get;
}
