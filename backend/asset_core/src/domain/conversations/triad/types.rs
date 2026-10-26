use std::borrow::Cow;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::super::{
    ConversationPart, ConversationPartKind, ConversationPartRole, NormalizedConversationPart,
};

/// Structure stored in `conversation_turns.process` or `web_record_turns.process`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct TurnProcess {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_seq: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_translated_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_card: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_card_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_signal: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<TurnProcessItem>,
}

/// A compact command execution node within a turn's process chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TurnProcessItem {
    pub seq: i64,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translated_text: Option<String>,
}

/// An intermediate conversation part retained in `conversation_parts` (8 columns)
/// or `web_record_parts` (7 columns).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriadPart {
    pub part_index: i64,
    pub kind: String,
    pub content: Option<String>,
    pub attrs_json: Option<String>,
    pub translated_text: Option<String>,
}

/// The result of splitting a turn into the 3-part triad plus intermediate parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitTurnTriad {
    pub prompt: String,
    pub process: Option<TurnProcess>,
    pub response: Option<String>,
    pub parts: Vec<TriadPart>,
}

/// Zero-copy adapter view for incoming raw parts from normalized or persisted sources.
#[derive(Debug, Clone)]
pub struct TriadRawPart<'a> {
    pub role: ConversationPartRole,
    pub kind: ConversationPartKind,
    pub custom_kind: Option<Cow<'a, str>>,
    pub text: Option<&'a str>,
    pub command: Option<&'a str>,
    pub language: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub status: Option<&'a str>,
    pub exit_code: Option<i32>,
    pub command_label: Option<&'a str>,
    pub source_execution_id: Option<&'a str>,
    pub signal: Option<&'a str>,
    pub tool_name: Option<&'a str>,
    pub file_path: Option<&'a str>,
    pub metadata_json: Option<&'a str>,
    pub translated_text: Option<&'a str>,
    pub semantic_role: Option<Cow<'a, str>>,
    pub renderer: Option<Cow<'a, str>>,
}

impl<'a> Default for TriadRawPart<'a> {
    fn default() -> Self {
        Self {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            custom_kind: None,
            text: None,
            command: None,
            language: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            signal: None,
            tool_name: None,
            file_path: None,
            metadata_json: None,
            translated_text: None,
            semantic_role: None,
            renderer: None,
        }
    }
}

fn extract_part_custom_fields<'a>(
    content_card: Option<
        &'a crate::backend::domain::conversations::ConversationContentCardDescriptor,
    >,
    metadata_json: Option<&'a str>,
) -> (
    Option<Cow<'a, str>>,
    Option<Cow<'a, str>>,
    Option<Cow<'a, str>>,
) {
    let mut custom_kind = content_card.map(|c| Cow::Borrowed(c.kind.as_str()));
    let mut semantic_role =
        content_card.and_then(|c| c.semantic_role.as_deref().map(Cow::Borrowed));
    let mut renderer = content_card.and_then(|c| c.renderer.as_deref().map(Cow::Borrowed));

    if (custom_kind.is_none() || semantic_role.is_none() || renderer.is_none())
        && metadata_json.is_some()
    {
        if let Some(meta) = metadata_json {
            if let Ok(serde_json::Value::Object(map)) =
                serde_json::from_str::<serde_json::Value>(meta)
            {
                if let Some(card) = map
                    .get("content_card")
                    .or_else(|| map.get("contentCard"))
                    .and_then(|c| c.as_object())
                {
                    if custom_kind.is_none() {
                        if let Some(k) = card
                            .get("kind")
                            .or_else(|| card.get("type"))
                            .and_then(|v| v.as_str())
                        {
                            custom_kind = Some(Cow::Owned(k.to_string()));
                        }
                    }
                    if semantic_role.is_none() {
                        if let Some(r) = card
                            .get("semantic_role")
                            .or_else(|| card.get("semanticRole"))
                            .and_then(|v| v.as_str())
                        {
                            semantic_role = Some(Cow::Owned(r.to_string()));
                        }
                    }
                    if renderer.is_none() {
                        if let Some(rn) = card.get("renderer").and_then(|v| v.as_str()) {
                            renderer = Some(Cow::Owned(rn.to_string()));
                        }
                    }
                }
            }
        }
    }
    (custom_kind, semantic_role, renderer)
}

impl<'a> From<&'a NormalizedConversationPart> for TriadRawPart<'a> {
    fn from(p: &'a NormalizedConversationPart) -> Self {
        let (custom_kind, semantic_role, renderer) =
            extract_part_custom_fields(p.content_card.as_ref(), p.metadata_json.as_deref());
        Self {
            role: p.role,
            kind: p.kind,
            custom_kind,
            text: p.text.as_deref(),
            command: p.command.as_deref(),
            language: p.language.as_deref(),
            cwd: p.cwd.as_deref(),
            status: p.status.as_deref(),
            exit_code: p.exit_code,
            command_label: p.command_label.as_deref(),
            source_execution_id: p.source_execution_id.as_deref(),
            signal: None,
            tool_name: None,
            file_path: None,
            metadata_json: p.metadata_json.as_deref(),
            translated_text: None,
            semantic_role,
            renderer,
        }
    }
}

impl<'a> From<&'a ConversationPart> for TriadRawPart<'a> {
    fn from(p: &'a ConversationPart) -> Self {
        let (custom_kind, semantic_role, renderer) =
            extract_part_custom_fields(p.content_card.as_ref(), p.metadata_json.as_deref());
        Self {
            role: p.role,
            kind: p.kind,
            custom_kind,
            text: p.text.as_deref(),
            command: p.command.as_deref(),
            language: p.language.as_deref(),
            cwd: p.cwd.as_deref(),
            status: p.status.as_deref(),
            exit_code: p.exit_code,
            command_label: p.command_label.as_deref(),
            source_execution_id: p.source_execution_id.as_deref(),
            signal: p.signal.as_deref(),
            tool_name: p.tool_name.as_deref(),
            file_path: p.file_path.as_deref(),
            metadata_json: p.metadata_json.as_deref(),
            translated_text: p.translated_text.as_deref(),
            semantic_role,
            renderer,
        }
    }
}

pub(crate) fn is_user_part(part: &TriadRawPart<'_>) -> bool {
    if part.role == ConversationPartRole::User {
        return true;
    }
    if let Some(kind) = part.custom_kind.as_deref() {
        if kind == "user" || kind.ends_with(".user") {
            return true;
        }
    }
    false
}

pub(crate) fn is_command_part(part: &TriadRawPart<'_>) -> bool {
    if part.kind == ConversationPartKind::Command {
        return true;
    }
    if let Some(kind) = part.custom_kind.as_deref() {
        if kind == "command" || kind.ends_with(".command") {
            return true;
        }
    }
    part.role == ConversationPartRole::Tool && part.command.is_some()
}

pub(crate) fn is_result_part(part: &TriadRawPart<'_>) -> bool {
    if let Some(kind) = part.custom_kind.as_deref() {
        if kind == "result" || kind.ends_with(".result") {
            return true;
        }
    }
    part.role == ConversationPartRole::Tool
        && part.command.is_none()
        && part.source_execution_id.is_some()
}

pub(crate) fn is_answer_candidate(adapter_id: &str, part: &TriadRawPart<'_>) -> bool {
    if is_user_part(part) || is_command_part(part) || is_result_part(part) {
        return false;
    }
    let is_web = adapter_id == "web" || adapter_id.ends_with("-web");
    if let Some(kind) = part.custom_kind.as_deref() {
        if kind == "answer" || kind.ends_with(".answer") {
            return true;
        }
        if is_web {
            if kind.ends_with(".reasoning")
                || kind.ends_with(".thought")
                || kind.ends_with(".result")
                || kind.ends_with(".tool")
                || kind.ends_with(".command")
            {
                return false;
            }
        } else {
            return false;
        }
    }
    if part.role == ConversationPartRole::Assistant
        && (part.kind == ConversationPartKind::Text || part.kind == ConversationPartKind::CodeBlock)
    {
        if let Some(text) = part.text {
            let trimmed = text.trim();
            // Web 端中间检索指令 JSON 不是回答
            if trimmed.starts_with(r#"{"queries":"#) || trimmed.starts_with(r#"{"query":"#) {
                return false;
            }
        }
        return true;
    }
    false
}

pub(crate) fn format_code_block(text: Option<&str>, language: Option<&str>) -> String {
    let lang = language.map(str::trim).unwrap_or("");
    if !lang.is_empty() {
        format!("```{lang}\n{}\n```", text.unwrap_or_default())
    } else {
        format!("```\n{}\n```", text.unwrap_or_default())
    }
}

pub(crate) fn is_successful_status(status: &str) -> bool {
    let lower = status.trim().to_ascii_lowercase();
    matches!(lower.as_str(), "completed" | "success" | "ok" | "done")
}

pub(crate) fn extract_auxiliary_from_metadata<'a>(
    metadata_json: Option<&'a str>,
    existing_signal: Option<&'a str>,
    existing_tool: Option<&'a str>,
    existing_file: Option<&'a str>,
) -> (Option<String>, Option<String>, Option<String>) {
    let mut signal = existing_signal.map(|s| s.to_string());
    let mut tool = existing_tool.map(|s| s.to_string());
    let mut file = existing_file.map(|s| s.to_string());
    if let Some(raw) = metadata_json {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) {
            if signal.is_none() {
                if let Some(s) = val.get("signal").and_then(|v| v.as_str()) {
                    if !s.is_empty() {
                        signal = Some(s.to_string());
                    }
                } else if val.get("completed").and_then(|v| v.as_bool()) == Some(true)
                    || val.get("status").and_then(|v| v.as_str()) == Some("completed")
                {
                    signal = Some("completed".to_string());
                }
            }
            if tool.is_none() {
                if let Some(t) = val.get("tool_name").and_then(|v| v.as_str()) {
                    if !t.is_empty() {
                        tool = Some(t.to_string());
                    }
                }
            }
            if file.is_none() {
                if let Some(f) = val.get("file_path").and_then(|v| v.as_str()) {
                    if !f.is_empty() {
                        file = Some(f.to_string());
                    }
                }
            }
        }
    }
    (signal, tool, file)
}
