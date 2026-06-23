use super::prelude::*;

pub(super) fn normalize_opencode_parts(
    rows: &[OpenCodePartRow],
    message_role: &str,
) -> Vec<NormalizedConversationPart> {
    let mut parts = Vec::new();
    for row in rows {
        if row.ignored {
            continue;
        }
        if matches!(
            row.kind.as_str(),
            "reasoning" | "step-start" | "step-finish" | "compaction" | "retry" | "snapshot"
        ) {
            continue;
        }
        let mapped_kind = match row.kind.as_str() {
            "tool" | "tool-call" | "tool-result" if row.command.is_some() => {
                ConversationPartKind::Command
            }
            "tool" | "tool-call" | "tool-result" => ConversationPartKind::Tool,
            "command" => ConversationPartKind::Command,
            "file" | "patch" => ConversationPartKind::FileChange,
            "subtask" | "agent" => ConversationPartKind::Subagent,
            _ => ConversationPartKind::Text,
        };
        let role = if message_role == "user" {
            ConversationPartRole::User
        } else if matches!(
            mapped_kind,
            ConversationPartKind::Command | ConversationPartKind::Tool
        ) {
            ConversationPartRole::Tool
        } else {
            ConversationPartRole::Assistant
        };
        if mapped_kind == ConversationPartKind::Text {
            parts.extend(split_markdown_text_parts(role, &row.text));
        } else {
            parts.push(NormalizedConversationPart {
                role,
                kind: mapped_kind,
                text: (!row.text.trim().is_empty()).then_some(row.text.clone()),
                language: None,
                command: row.command.clone(),
                cwd: row.cwd.clone(),
                status: row.status.clone(),
                exit_code: row.exit_code,
                metadata_json: row.metadata_json.clone(),
            });
        }
    }
    parts
}
