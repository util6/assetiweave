use super::*;
use std::path::Path;

#[test]
fn resolve_agent_execution_workspace_root_respects_env_override() {
    let temp_dir = std::env::temp_dir().join(format!("custom-runs-{}", uuid::Uuid::new_v4()));
    std::env::set_var(
        "ASSETIWEAVE_AGENT_RUNS_DIR",
        temp_dir.to_string_lossy().to_string(),
    );

    let resolved = resolve_agent_execution_workspace_root();
    assert_eq!(resolved, temp_dir);

    let stringified = agent_execution_workspace_root(Path::new("/dummy/app.db"));
    assert_eq!(stringified, temp_dir.to_string_lossy().to_string());

    std::env::remove_var("ASSETIWEAVE_AGENT_RUNS_DIR");
}

#[test]
fn create_workspace_for_purpose_routes_to_category_subfolder() {
    let temp_root = std::env::temp_dir().join(format!("agent-runs-test-{}", uuid::Uuid::new_v4()));

    let translation_ws = backends::acp_prompt::create_workspace_for_purpose(
        &temp_root,
        Some(AiExecutionPurpose::Translation),
    )
    .expect("create translation workspace");
    assert!(translation_ws.exists());
    assert!(translation_ws.to_string_lossy().contains("translations"));

    let memory_ws = backends::acp_prompt::create_workspace_for_purpose(
        &temp_root,
        Some(AiExecutionPurpose::SessionMemory),
    )
    .expect("create memory workspace");
    assert!(memory_ws.exists());
    assert!(memory_ws.to_string_lossy().contains("memory"));

    let general_ws = backends::acp_prompt::create_workspace_for_purpose(
        &temp_root,
        Some(AiExecutionPurpose::PromptOptimization),
    )
    .expect("create general workspace");
    assert!(general_ws.exists());
    assert!(general_ws.to_string_lossy().contains("general"));

    let _ = std::fs::remove_dir_all(&temp_root);
}
