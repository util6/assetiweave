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
    pub custom_kind: Option<&'a str>,
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
        }
    }
}

impl<'a> From<&'a NormalizedConversationPart> for TriadRawPart<'a> {
    fn from(p: &'a NormalizedConversationPart) -> Self {
        Self {
            role: p.role,
            kind: p.kind,
            custom_kind: p.content_card.as_ref().map(|c| c.kind.as_str()),
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
        }
    }
}

impl<'a> From<&'a ConversationPart> for TriadRawPart<'a> {
    fn from(p: &'a ConversationPart) -> Self {
        Self {
            role: p.role,
            kind: p.kind,
            custom_kind: p.content_card.as_ref().map(|c| c.kind.as_str()),
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
        }
    }
}
