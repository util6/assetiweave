use super::*;
use crate::backend::domain::{
    ConversationContentCardDescriptor, ConversationPartKind, ConversationPartRole,
    NormalizedConversationPart,
};

#[test]
fn test_extract_command_result_paired_links() {
    let turn_id = "turn-123";
    let cmd_part = NormalizedConversationPart {
        role: ConversationPartRole::Assistant,
        kind: ConversationPartKind::Command,
        text: None,
        language: None,
        command: Some("cargo check".to_string()),
        cwd: Some("/repo".to_string()),
        status: None,
        exit_code: None,
        command_label: None,
        source_execution_id: Some("exec-99".to_string()),
        content_card: Some(ConversationContentCardDescriptor {
            schema_version: 1,
            kind: "codex.command".to_string(),
            semantic_role: Some("command".to_string()),
            renderer: Some("command".to_string()),
        }),
        metadata_json: None,
    };

    let res_part = NormalizedConversationPart {
        role: ConversationPartRole::Tool,
        kind: ConversationPartKind::Tool,
        text: Some("Build finished successfully".to_string()),
        language: None,
        command: None,
        cwd: None,
        status: Some("success".to_string()),
        exit_code: Some(0),
        command_label: None,
        source_execution_id: Some("exec-99".to_string()),
        content_card: Some(ConversationContentCardDescriptor {
            schema_version: 1,
            kind: "codex.result".to_string(),
            semantic_role: Some("result".to_string()),
            renderer: Some("terminal_output".to_string()),
        }),
        metadata_json: None,
    };

    let parts = vec![cmd_part, res_part];
    let links = extract_part_links_for_turn(turn_id, &parts);

    let cmd_id = stable_id("conversation-part", &[turn_id, "0"]);
    let res_id = stable_id("conversation-part", &[turn_id, "1"]);

    // Command should link to execution
    assert!(links.iter().any(|l| l.part_id == cmd_id
        && l.relation == "execution"
        && l.target_kind == "execution"
        && l.target_id == "exec-99"));

    // Result should link to execution
    assert!(links.iter().any(|l| l.part_id == res_id
        && l.relation == "execution"
        && l.target_kind == "execution"
        && l.target_id == "exec-99"));

    // Command should link to Result with execution_result
    assert!(links.iter().any(|l| l.part_id == cmd_id
        && l.relation == "execution_result"
        && l.target_kind == "part"
        && l.target_id == res_id));

    // Result should link to Command with execution_command
    assert!(links.iter().any(|l| l.part_id == res_id
        && l.relation == "execution_command"
        && l.target_kind == "part"
        && l.target_id == cmd_id));
}

#[test]
fn test_extract_subagent_spawned_session_links() {
    let turn_id = "turn-456";
    let subagent_part = NormalizedConversationPart {
        role: ConversationPartRole::Assistant,
        kind: ConversationPartKind::Subagent,
        text: Some(
            r#"{"agent_role": "Tester", "task": "Run tests", "child_session_id": "sub-session-001"}"#
                .to_string(),
        ),
        language: None,
        command: None,
        cwd: None,
        status: Some("running".to_string()),
        exit_code: None,
        command_label: None,
        source_execution_id: None,
        content_card: Some(ConversationContentCardDescriptor {
            schema_version: 1,
            kind: "antigravity.subagent".to_string(),
            semantic_role: Some("subagent".to_string()),
            renderer: Some("subagent_tree".to_string()),
        }),
        metadata_json: Some(
            r#"{"type": "subagent", "child_session_id": "sub-session-001"}"#.to_string(),
        ),
    };

    let links = extract_part_links_for_turn(turn_id, &[subagent_part]);
    let part_id = stable_id("conversation-part", &[turn_id, "0"]);

    assert!(links.iter().any(|l| l.part_id == part_id
        && l.relation == "spawned_session"
        && l.target_kind == "session"
        && l.target_id == "sub-session-001"));
}
