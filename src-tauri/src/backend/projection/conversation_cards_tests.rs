use super::*;
use crate::backend::models::{
    ConversationPart, ConversationPartKind, ConversationPartRole, NormalizedConversationPart,
};

#[test]
fn conversation_card_contract_accepts_app_specific_kind_with_supported_renderer() {
    let part = part_with_metadata(
        r#"{"content_card":{"schema_version":1,"kind":"claude-code.reasoning","semantic_role":"reasoning","renderer":"markdown"}}"#,
    );

    let card = project_conversation_content_card(&part, "claude-code", &[])
        .expect("project card")
        .expect("declared card");

    assert_eq!(card.kind, "claude-code.reasoning");
    assert_eq!(card.semantic_role.as_deref(), Some("reasoning"));
    assert_eq!(card.renderer, ConversationCardRenderer::Markdown);
    assert_eq!(card.body, "Visible body");
    assert_eq!(card.node_id, "part-1");
    assert_eq!(card.adapter_id, "claude-code");
}

#[test]
fn conversation_card_contract_maps_legacy_result_plain_to_terminal_output() {
    let part = part_with_metadata(
        r#"{"content_card":{"type":"result","format":"plain","suffix":"result"}}"#,
    );

    let card = project_conversation_content_card(&part, "legacy", &[])
        .expect("project card")
        .expect("declared card");

    assert_eq!(card.kind, "result");
    assert_eq!(card.renderer, ConversationCardRenderer::TerminalOutput);
    assert_eq!(card.node_id, "part-1");
    assert_eq!(card.legacy_anchor_ids, vec!["part-1-result"]);
}

#[test]
fn conversation_card_contract_keeps_status_only_result_cards() {
    let mut part = part_with_metadata(
        r#"{"content_card":{"type":"result","format":"plain","suffix":"result"}}"#,
    );
    part.text = None;
    part.status = Some("completed".to_string());
    part.exit_code = Some(0);

    let card = project_conversation_content_card(&part, "legacy", &[])
        .expect("project status-only result")
        .expect("status-only result card");

    assert_eq!(card.body, "");
    assert_eq!(card.status.as_deref(), Some("completed"));
    assert_eq!(card.exit_code, Some(0));
}

#[test]
fn conversation_card_contract_accepts_explicit_diff_renderer() {
    let mut part = normalized_part_with_metadata(r#"{"source_type":"file_change"}"#);
    part.kind = ConversationPartKind::FileChange;
    part.text = Some("diff --git a/a.txt b/a.txt\n@@ -1 +1 @@\n-old\n+new".to_string());
    part.content_card = Some(ConversationContentCardDescriptor {
        schema_version: 1,
        kind: "opencode.result".to_string(),
        renderer: Some("diff".to_string()),
    });
    let declarations = vec![ConversationCardKindDefinition {
        id: "opencode.result".to_string(),
        semantic_role: Some("result".to_string()),
        label: "Result".to_string(),
        default_renderer: "terminal_output".to_string(),
        allowed_renderers: vec!["terminal_output".to_string(), "diff".to_string()],
        icon_hint: None,
    }];

    validate_normalized_content_card(&part, "opencode", Some(1), &declarations)
        .expect("validate explicit diff card");
    let projected = project_resolved_content_card(
        ConversationCardProjectionSource {
            content_card: part.content_card.as_ref(),
            text: part.text.as_deref(),
            metadata_json: part.metadata_json.as_deref(),
            ..Default::default()
        },
        &declarations,
    )
    .expect("project diff card")
    .expect("diff card");
    assert_eq!(projected.renderer, ConversationCardRenderer::Diff);
}

#[test]
fn conversation_card_contract_preserves_legacy_json_renderer() {
    let part = part_with_metadata(r#"{"content_card":{"type":"tool","format":"json"}}"#);

    let card = project_conversation_content_card(&part, "legacy", &[])
        .expect("project card")
        .expect("declared card");

    assert_eq!(card.renderer, ConversationCardRenderer::Json);
}

#[test]
fn conversation_card_contract_accepts_a_declared_local_path_renderer() {
    let mut part = normalized_part_with_metadata(r#"{"skill_name":"session-exporter"}"#);
    part.role = ConversationPartRole::System;
    part.kind = ConversationPartKind::Metadata;
    part.text = Some("/Users/test/.codex/skills/session-exporter/SKILL.md".to_string());
    part.content_card = Some(ConversationContentCardDescriptor {
        schema_version: 1,
        kind: "codex.skill".to_string(),
        renderer: Some("path".to_string()),
    });
    let declarations = vec![ConversationCardKindDefinition {
        id: "codex.skill".to_string(),
        semantic_role: Some("skill".to_string()),
        label: "Skill".to_string(),
        default_renderer: "path".to_string(),
        allowed_renderers: vec!["path".to_string()],
        icon_hint: Some("book-open".to_string()),
    }];

    validate_normalized_content_card(&part, "codex", Some(1), &declarations)
        .expect("validate path card");
    let projected = project_resolved_content_card(
        ConversationCardProjectionSource {
            content_card: part.content_card.as_ref(),
            metadata_json: part.metadata_json.as_deref(),
            text: part.text.as_deref(),
            ..ConversationCardProjectionSource::default()
        },
        &declarations,
    )
    .expect("project path card")
    .expect("path card");

    assert_eq!(projected.renderer, ConversationCardRenderer::Path);
    assert_eq!(projected.semantic_role.as_deref(), Some("skill"));
    assert!(projected.body.ends_with("/SKILL.md"));
}

#[test]
fn conversation_card_contract_upgrades_legacy_metadata_to_namespaced_descriptor() {
    let mut part =
        normalized_part_with_metadata(r#"{"content_card":{"type":"answer","format":"markdown"}}"#);
    let declarations = vec![ConversationCardKindDefinition {
        id: "claude-code.answer".to_string(),
        semantic_role: Some("answer".to_string()),
        label: "Answer".to_string(),
        default_renderer: "markdown".to_string(),
        allowed_renderers: vec!["markdown".to_string()],
        icon_hint: None,
    }];

    canonicalize_normalized_content_card(&mut part, "claude-code", Some(1), &declarations)
        .expect("upgrade legacy descriptor");

    assert_eq!(
        part.content_card,
        Some(ConversationContentCardDescriptor {
            schema_version: 1,
            kind: "claude-code.answer".to_string(),
            renderer: Some("markdown".to_string()),
        })
    );
    validate_normalized_content_card(&part, "claude-code", Some(1), &declarations)
        .expect("legacy metadata and semantic role remain compatible");
}

#[test]
fn conversation_card_contract_preserves_legacy_type_anchor_after_namespacing() {
    let mut part = part_with_metadata(r#"{"content_card":{"type":"answer","format":"markdown"}}"#);
    part.content_card = Some(ConversationContentCardDescriptor {
        schema_version: 1,
        kind: "claude-code.answer".to_string(),
        renderer: Some("markdown".to_string()),
    });
    let definitions = vec![ConversationCardKindDefinition {
        id: "claude-code.answer".to_string(),
        semantic_role: Some("answer".to_string()),
        label: "Answer".to_string(),
        default_renderer: "markdown".to_string(),
        allowed_renderers: vec!["markdown".to_string()],
        icon_hint: None,
    }];

    let card = project_conversation_content_card(&part, "claude-code", &definitions)
        .expect("project namespaced card")
        .expect("card");

    assert_eq!(
        card.legacy_anchor_ids,
        vec!["part-1-claude-code.answer", "part-1-answer"]
    );
}

#[test]
fn persisted_row_projection_matches_part_projection() {
    let definitions = vec![ConversationCardKindDefinition {
        id: "claude-code.reasoning".to_string(),
        semantic_role: Some("reasoning".to_string()),
        label: "Reasoning".to_string(),
        default_renderer: "markdown".to_string(),
        allowed_renderers: vec!["markdown".to_string()],
        icon_hint: Some("brain".to_string()),
    }];
    let descriptor = r#"{"schema_version":1,"kind":"claude-code.reasoning","renderer":"markdown"}"#;

    let projected = project_persisted_content_card(
        PersistedConversationCardProjectionSource {
            content_card_json: Some(descriptor),
            metadata_json: Some(r#"{"source_type":"thinking"}"#),
            text: Some("Compare both paths"),
            language: None,
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
        },
        &definitions,
    )
    .expect("project persisted card")
    .expect("card");

    assert_eq!(projected.kind, "claude-code.reasoning");
    assert_eq!(projected.semantic_role.as_deref(), Some("reasoning"));
    assert_eq!(projected.renderer, ConversationCardRenderer::Markdown);
    assert_eq!(projected.body, "Compare both paths");
}

#[test]
fn historical_shell_projection_metadata_does_not_split_the_raw_part() {
    let mut part = part_with_metadata(
        r#"{"shell_execution_projection":{"schema_version":1,"nodes":[{"command":"rg TODO","command_label":"inspect"},{"command":"git status --short","command_label":"status"}]}}"#,
    );
    part.role = ConversationPartRole::Tool;
    part.kind = ConversationPartKind::Command;
    part.text = None;
    part.command = Some("printf '--- inspect ---'; rg TODO; git status --short".to_string());
    part.source_execution_id = Some("execution-1".to_string());
    part.content_card = Some(ConversationContentCardDescriptor {
        schema_version: 1,
        kind: "codex.command".to_string(),
        renderer: Some("command".to_string()),
    });

    let cards = project_conversation_content_cards(&part, "codex", &[])
        .expect("project raw Codex shell Part");

    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].node_id, "part-1");
    assert_eq!(cards[0].part_id, "part-1");
    assert_eq!(
        cards[0].body,
        "printf '--- inspect ---'; rg TODO; git status --short"
    );
    assert_eq!(cards[0].command_label, None);
    assert_eq!(cards[0].source_execution_id.as_deref(), Some("execution-1"));
}

#[test]
fn conversation_card_contract_rejects_structured_legacy_semantic_conflict() {
    let mut part =
        normalized_part_with_metadata(r#"{"content_card":{"type":"answer","format":"markdown"}}"#);
    part.content_card = Some(ConversationContentCardDescriptor {
        schema_version: 1,
        kind: "fixture.reasoning".to_string(),
        renderer: Some("markdown".to_string()),
    });
    let declarations = vec![ConversationCardKindDefinition {
        id: "fixture.reasoning".to_string(),
        semantic_role: Some("reasoning".to_string()),
        label: "Reasoning".to_string(),
        default_renderer: "markdown".to_string(),
        allowed_renderers: vec!["markdown".to_string()],
        icon_hint: None,
    }];

    let error = validate_normalized_content_card(&part, "fixture", Some(1), &declarations)
        .expect_err("different legacy semantics must conflict");

    assert!(matches!(error, ProjectionError::LegacyConflict { .. }));
    assert!(error.contains("conflicts with legacy metadata"));
}

#[test]
fn conversation_card_contract_rejects_invalid_new_kind_at_adapter_boundary() {
    let part = normalized_part_with_metadata(
        r#"{"content_card":{"schema_version":1,"kind":"Invalid Kind","presentation":{"renderer":"plain"}}}"#,
    );

    let error = validate_normalized_content_card(&part, "fixture", None, &[])
        .expect_err("invalid adapter-declared kind must fail validation");

    assert!(matches!(error, ProjectionError::InvalidCardKind { .. }));
    assert!(error.contains("content card kind"));
}

#[test]
fn conversation_card_contract_rejects_unknown_new_renderer_but_reads_history_safely() {
    let metadata = r#"{"content_card":{"schema_version":1,"kind":"future-card","presentation":{"renderer":"future-ui"}}}"#;
    let normalized = normalized_part_with_metadata(metadata);
    let error = validate_normalized_content_card(&normalized, "fixture", None, &[])
        .expect_err("unsupported new renderer must fail validation");
    assert!(matches!(error, ProjectionError::UnsupportedRenderer { .. }));
    assert!(error.contains("unsupported conversation card renderer"));

    let historical = part_with_metadata(metadata);
    let card = project_conversation_content_card(&historical, "future", &[])
        .expect("historical projection must stay readable")
        .expect("historical card");
    assert_eq!(card.kind, "future-card");
    assert_eq!(card.renderer, ConversationCardRenderer::Plain);
}

#[test]
fn conversation_card_contract_ignores_parts_without_a_card_declaration() {
    let mut part = part_with_metadata(r#"{"source_type":"assistant"}"#);
    part.metadata_json = Some(r#"{"source_type":"assistant"}"#.to_string());

    assert!(project_conversation_content_card(&part, "legacy", &[])
        .expect("project undeclared part")
        .is_none());
}

fn part_with_metadata(metadata: &str) -> ConversationPart {
    ConversationPart {
        id: "part-1".to_string(),
        turn_id: "turn-1".to_string(),
        part_index: 0,
        role: ConversationPartRole::Assistant,
        kind: ConversationPartKind::Text,
        text: Some("Visible body".to_string()),
        language: None,
        command: None,
        cwd: None,
        status: None,
        exit_code: None,
        command_label: None,
        source_execution_id: None,
        content_card: None,
        metadata_json: Some(metadata.to_string()),
        translated_text: None,
    }
}

fn normalized_part_with_metadata(metadata: &str) -> NormalizedConversationPart {
    NormalizedConversationPart {
        role: ConversationPartRole::Assistant,
        kind: ConversationPartKind::Text,
        text: Some("Visible body".to_string()),
        language: None,
        command: None,
        cwd: None,
        status: None,
        exit_code: None,
        command_label: None,
        source_execution_id: None,
        content_card: None,
        metadata_json: Some(metadata.to_string()),
    }
}
