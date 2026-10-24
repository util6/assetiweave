use super::super::id::derive_part_id;
use super::super::{
    ConversationContentCardDescriptor, ConversationPart, ConversationPartKind, ConversationPartRole,
};
use super::types::{TriadPart, TurnProcess};

/// Restores a full ordered list of ConversationPart domain models from the 3-part triad
/// (prompt, process, response) and database parts.
pub fn merge_turn_triad(
    adapter_id: &str,
    prefix: &str,
    turn_id: &str,
    _prompt: &str,
    process: Option<&TurnProcess>,
    response: Option<&str>,
    parts: &[TriadPart],
) -> Vec<ConversationPart> {
    let mut restored = Vec::new();

    // 1. 还原 process.items 为命令零件
    if let Some(p) = process {
        for item in &p.items {
            let part_id = derive_part_id(prefix, turn_id, item.seq);
            let cwd = item.cwd.clone().or_else(|| p.cwd.clone());
            let source_execution_id = Some(format!("exec-{turn_id}-{}", item.seq));

            restored.push(ConversationPart {
                id: part_id,
                turn_id: turn_id.to_string(),
                part_index: item.seq,
                role: ConversationPartRole::Tool,
                kind: ConversationPartKind::Command,
                text: item.summary.clone(),
                language: None,
                command: item.content.clone(),
                cwd,
                status: item.status.clone(),
                exit_code: item.exit_code,
                command_label: item.label.clone(),
                source_execution_id,
                content_card: Some(ConversationContentCardDescriptor {
                    schema_version: 1,
                    kind: item.kind.clone(),
                    renderer: None,
                    semantic_role: None,
                }),
                metadata_json: None,
                translated_text: None,
                signal: item.signal.clone(),
                tool_name: None,
                file_path: None,
            });
        }
    }

    // 2. 还原保留的中间过程零件
    for p in parts {
        let part_id = derive_part_id(prefix, turn_id, p.part_index);
        let mut language = None;
        let mut signal = None;
        let mut tool_name = None;
        let mut file_path = None;
        let mut status = None;
        let mut exit_code = None;
        let mut source_execution_id = None;

        if let Some(ref attrs_str) = p.attrs_json {
            if let Ok(serde_json::Value::Object(map)) =
                serde_json::from_str::<serde_json::Value>(attrs_str)
            {
                if let Some(cmd_seq) = map.get("cmd_seq").and_then(|v| v.as_i64()) {
                    source_execution_id = Some(format!("exec-{turn_id}-{cmd_seq}"));
                }
                if let Some(lang) = map.get("language").and_then(|v| v.as_str()) {
                    language = Some(lang.to_string());
                }
                if let Some(sig) = map.get("signal").and_then(|v| v.as_str()) {
                    signal = Some(sig.to_string());
                }
                if let Some(t) = map.get("tool_name").and_then(|v| v.as_str()) {
                    tool_name = Some(t.to_string());
                }
                if let Some(f) = map.get("file_path").and_then(|v| v.as_str()) {
                    file_path = Some(f.to_string());
                }
                if let Some(st) = map.get("status").and_then(|v| v.as_str()) {
                    status = Some(st.to_string());
                }
                if let Some(code) = map.get("exit_code").and_then(|v| v.as_i64()) {
                    exit_code = Some(code as i32);
                }
            }
        }

        let kind_lower = p.kind.to_ascii_lowercase();
        let (role, kind) = if kind_lower.ends_with(".command") || kind_lower == "command" {
            (ConversationPartRole::Tool, ConversationPartKind::Command)
        } else if kind_lower.ends_with(".reasoning")
            || kind_lower.ends_with(".answer")
            || kind_lower == "reasoning"
            || kind_lower == "answer"
        {
            (ConversationPartRole::Assistant, ConversationPartKind::Text)
        } else if kind_lower.ends_with(".file-change")
            || kind_lower.ends_with(".file_change")
            || kind_lower == "file-change"
            || kind_lower == "file_change"
        {
            (ConversationPartRole::Tool, ConversationPartKind::FileChange)
        } else if kind_lower.ends_with(".code") || kind_lower == "code" {
            (
                ConversationPartRole::Assistant,
                ConversationPartKind::CodeBlock,
            )
        } else if kind_lower.ends_with(".subagent") || kind_lower == "subagent" {
            (ConversationPartRole::Tool, ConversationPartKind::Subagent)
        } else if kind_lower.ends_with(".skill") || kind_lower == "skill" {
            (ConversationPartRole::System, ConversationPartKind::Metadata)
        } else {
            (ConversationPartRole::Tool, ConversationPartKind::Tool)
        };

        restored.push(ConversationPart {
            id: part_id,
            turn_id: turn_id.to_string(),
            part_index: p.part_index,
            role,
            kind,
            text: p.content.clone(),
            language,
            command: None,
            cwd: None,
            status,
            exit_code,
            command_label: None,
            source_execution_id,
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: p.kind.clone(),
                renderer: None,
                semantic_role: None,
            }),
            metadata_json: None,
            translated_text: p.translated_text.clone(),
            signal,
            tool_name,
            file_path,
        });
    }

    // 3. 还原末尾唯一最终回答
    if let Some(resp) = response {
        if !resp.trim().is_empty() {
            let max_existing_seq = restored.iter().map(|p| p.part_index).max().unwrap_or(-1);
            let resp_seq = process
                .and_then(|p| p.response_seq)
                .unwrap_or(max_existing_seq + 1);

            let part_id = derive_part_id(prefix, turn_id, resp_seq);
            let resp_card_kind = if adapter_id == "web" {
                "answer".to_string()
            } else {
                format!("{adapter_id}.answer")
            };

            restored.push(ConversationPart {
                id: part_id,
                turn_id: turn_id.to_string(),
                part_index: resp_seq,
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some(resp.to_string()),
                language: None,
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
                command_label: None,
                source_execution_id: None,
                content_card: Some(ConversationContentCardDescriptor {
                    schema_version: 1,
                    kind: resp_card_kind,
                    renderer: None,
                    semantic_role: None,
                }),
                metadata_json: None,
                translated_text: process.and_then(|p| p.response_translated_text.clone()),
                signal: None,
                tool_name: None,
                file_path: None,
            });
        }
    }

    // 4. 按原始序号升序排序
    restored.sort_by_key(|p| p.part_index);
    restored
}
