use super::*;

#[test]
fn test_memory_module_exports_expected_commands() {
    let _ = get_memory_recent_snapshot;
    let _ = duplicate_memory_generation_skill;
    let _ = reset_memory_generation_skill_to_default;
    let _ = search_memory_recall;
    let _ = resolve_memory_context;
    let _ = get_memory_project;
    let _ = rebuild_memory_scope;
    let _ = list_memory_public_tasks;
    let _ = get_memory_public_task;
    let _ = cancel_memory_public_task;
    let _ = retry_memory_public_task;
    let _ = create_memory_recall_session;
    let _ = get_memory_recall_session;
    let _ = send_memory_recall_turn;
    let _ = cancel_memory_recall_turn;
}
