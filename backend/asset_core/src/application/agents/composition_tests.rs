use super::*;
use serde_json::json;

#[test]
fn resolve_agent_for_resolves_valid_assignment() {
    let settings = json!({
        "agentAssignments": {
            "memory.generation": {
                "agentId": "opencode",
                "modelId": "gpt-4o"
            }
        }
    });
    let action = ActionId::new("memory.generation");
    let (agent_id, model) = resolve_agent_for(&action, &settings).expect("should resolve");
    assert_eq!(agent_id.as_str(), "opencode");
    assert_eq!(model.as_deref(), Some("gpt-4o"));
}

#[test]
fn resolve_agent_for_fails_on_missing_assignment() {
    let settings = json!({});
    let action = ActionId::new("memory.generation");
    assert!(resolve_agent_for(&action, &settings).is_err());
}

#[test]
fn resolve_agent_for_falls_back_for_memory_actions() {
    let settings = json!({
        "agentAssignments": {
            "memory.generation": {
                "agentId": "opencode",
                "modelId": "opencode/mimo-v2.6-flash-free"
            }
        }
    });
    let action = ActionId::new("memory.extraction");
    let (agent_id, model) = resolve_agent_for(&action, &settings).expect("should fall back");
    assert_eq!(agent_id.as_str(), "opencode");
    assert_eq!(model.as_deref(), Some("opencode/mimo-v2.6-flash-free"));
}
