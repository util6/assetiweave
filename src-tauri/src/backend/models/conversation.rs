use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationAdapterKind {
    Codex,
    ClaudeCode,
    #[serde(rename = "opencode", alias = "open_code")]
    OpenCode,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationSourceKind {
    Live,
    File,
    Directory,
    Sqlite,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationAdapterTrustState {
    BuiltIn,
    Trusted,
    Changed,
    Untrusted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationPartRole {
    User,
    Assistant,
    Tool,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationPartKind {
    Text,
    CodeBlock,
    Command,
    Tool,
    FileChange,
    Subagent,
    Metadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationGroupingOrigin {
    Imported,
    AutoMerged,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationSyncStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationAdapter {
    pub id: String,
    pub name: String,
    pub kind: ConversationAdapterKind,
    pub version: String,
    pub enabled: bool,
    pub manifest_path: Option<String>,
    pub executable_path: Option<String>,
    pub content_hash: Option<String>,
    pub trusted_hash: Option<String>,
    pub trust_state: ConversationAdapterTrustState,
    pub protocol_version: Option<u32>,
    pub capabilities: Vec<String>,
    pub input_kinds: Vec<ConversationSourceKind>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationSource {
    pub id: String,
    pub adapter_id: String,
    pub name: String,
    pub kind: ConversationSourceKind,
    pub location: String,
    pub config_json: Option<String>,
    pub enabled: bool,
    pub last_synced_at: Option<String>,
    pub last_sync_status: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationSession {
    pub id: String,
    pub source_id: String,
    pub adapter_id: String,
    pub external_id: String,
    pub title: String,
    pub project_path: Option<String>,
    pub started_at: Option<String>,
    pub updated_at: Option<String>,
    pub source_locator: Option<String>,
    pub source_fingerprint: Option<String>,
    pub missing: bool,
    pub created_at: String,
    pub imported_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationTurn {
    pub id: String,
    pub session_id: String,
    pub external_id: String,
    pub turn_index: i64,
    pub user_text: String,
    pub title: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub fingerprint: String,
    pub missing: bool,
    pub imported_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationPart {
    pub id: String,
    pub turn_id: String,
    pub part_index: i64,
    pub role: ConversationPartRole,
    pub kind: ConversationPartKind,
    pub text: Option<String>,
    pub language: Option<String>,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub status: Option<String>,
    pub exit_code: Option<i32>,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationQuestion {
    pub id: String,
    pub session_id: String,
    pub question_index: i64,
    pub title: Option<String>,
    pub question_text: String,
    pub answer_text: String,
    pub code_text: String,
    pub command_text: String,
    pub grouping_origin: ConversationGroupingOrigin,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationSyncRun {
    pub id: String,
    pub source_id: Option<String>,
    pub adapter_id: Option<String>,
    pub status: ConversationSyncStatus,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub session_count: i64,
    pub turn_count: i64,
    pub warning_count: i64,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NormalizedConversationSession {
    pub external_id: String,
    pub title: Option<String>,
    pub project_path: Option<String>,
    pub started_at: Option<String>,
    pub updated_at: Option<String>,
    pub source_locator: Option<String>,
    pub source_fingerprint: Option<String>,
    pub turns: Vec<NormalizedConversationTurn>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NormalizedConversationTurn {
    pub external_id: String,
    pub turn_index: i64,
    pub user_text: String,
    pub title: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub parts: Vec<NormalizedConversationPart>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NormalizedConversationPart {
    pub role: ConversationPartRole,
    pub kind: ConversationPartKind,
    pub text: Option<String>,
    pub language: Option<String>,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub status: Option<String>,
    pub exit_code: Option<i32>,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationGroupSeed {
    pub turn_ids: Vec<String>,
    pub origin: ConversationGroupingOrigin,
}

pub fn split_markdown_text_parts(
    role: ConversationPartRole,
    text: &str,
) -> Vec<NormalizedConversationPart> {
    let mut parts = Vec::new();
    let mut remaining = text;

    while let Some(start) = remaining.find("```") {
        let before = &remaining[..start];
        push_text_part(&mut parts, role, before);

        let fence_body = &remaining[start + 3..];
        let Some(end) = fence_body.find("```") else {
            push_text_part(&mut parts, role, &remaining[start..]);
            return parts;
        };

        let fenced = &fence_body[..end];
        let (language, code) = split_fenced_block(fenced);
        let trimmed_code = code.trim_matches('\n').to_string();
        if !trimmed_code.trim().is_empty() {
            parts.push(NormalizedConversationPart {
                role,
                kind: ConversationPartKind::CodeBlock,
                text: Some(trimmed_code),
                language,
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
                metadata_json: None,
            });
        }

        remaining = &fence_body[end + 3..];
    }

    push_text_part(&mut parts, role, remaining);
    parts
}

pub fn should_auto_merge_acknowledgement(user_text: &str) -> bool {
    let normalized = user_text.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.contains('\n')
        || normalized.contains("```")
        || normalized.contains('?')
        || normalized.contains('？')
    {
        return false;
    }

    matches!(
        normalized.as_str(),
        "ok" | "okay"
            | "yes"
            | "y"
            | "no"
            | "n"
            | "continue"
            | "go ahead"
            | "proceed"
            | "确认"
            | "可以"
            | "好的"
            | "好"
            | "继续"
            | "继续吧"
            | "是"
            | "否"
            | "不用"
            | "不需要"
    )
}

pub fn group_turn_ids_by_question<I>(turns: I) -> Vec<ConversationGroupSeed>
where
    I: IntoIterator<Item = (String, String)>,
{
    let mut groups: Vec<ConversationGroupSeed> = Vec::new();
    for (turn_id, user_text) in turns {
        if should_auto_merge_acknowledgement(&user_text) {
            if let Some(previous) = groups.last_mut() {
                previous.turn_ids.push(turn_id);
                if previous.origin == ConversationGroupingOrigin::Imported {
                    previous.origin = ConversationGroupingOrigin::AutoMerged;
                }
                continue;
            }
        }
        groups.push(ConversationGroupSeed {
            turn_ids: vec![turn_id],
            origin: ConversationGroupingOrigin::Imported,
        });
    }
    groups
}

pub fn conversation_turn_fingerprint(turn: &NormalizedConversationTurn) -> String {
    let mut hasher = Sha256::new();
    hasher.update(turn.external_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(turn.user_text.as_bytes());
    for part in &turn.parts {
        hasher.update(b"\0");
        hasher.update(format!("{:?}:{:?}", part.role, part.kind).as_bytes());
        if let Some(value) = &part.text {
            hasher.update(value.as_bytes());
        }
        if let Some(value) = &part.command {
            hasher.update(value.as_bytes());
        }
        if let Some(value) = &part.metadata_json {
            hasher.update(value.as_bytes());
        }
    }
    format!("{:x}", hasher.finalize())
}

fn push_text_part(
    parts: &mut Vec<NormalizedConversationPart>,
    role: ConversationPartRole,
    text: &str,
) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    parts.push(NormalizedConversationPart {
        role,
        kind: ConversationPartKind::Text,
        text: Some(trimmed.to_string()),
        language: None,
        command: None,
        cwd: None,
        status: None,
        exit_code: None,
        metadata_json: None,
    });
}

fn split_fenced_block(fenced: &str) -> (Option<String>, &str) {
    let Some(first_newline) = fenced.find('\n') else {
        return (None, fenced);
    };
    let first_line = fenced[..first_newline].trim();
    let code = &fenced[first_newline + 1..];
    let language = if first_line.is_empty() {
        None
    } else {
        Some(first_line.to_string())
    };
    (language, code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_markdown_text_and_code_parts_in_order() {
        let parts = split_markdown_text_parts(
            ConversationPartRole::Assistant,
            "Use this:\n```ts\nconst x = 1;\n```\nThen run it.",
        );

        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].kind, ConversationPartKind::Text);
        assert_eq!(parts[0].text.as_deref(), Some("Use this:"));
        assert_eq!(parts[1].kind, ConversationPartKind::CodeBlock);
        assert_eq!(parts[1].language.as_deref(), Some("ts"));
        assert_eq!(parts[1].text.as_deref(), Some("const x = 1;"));
        assert_eq!(parts[2].text.as_deref(), Some("Then run it."));
    }

    #[test]
    fn keeps_unclosed_fence_as_text() {
        let parts =
            split_markdown_text_parts(ConversationPartRole::Assistant, "Broken:\n```sh\necho nope");

        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1].kind, ConversationPartKind::Text);
        assert_eq!(parts[1].text.as_deref(), Some("```sh\necho nope"));
    }

    #[test]
    fn only_merges_exact_simple_acknowledgements() {
        assert!(should_auto_merge_acknowledgement("继续"));
        assert!(should_auto_merge_acknowledgement("OK"));
        assert!(!should_auto_merge_acknowledgement("继续解释一下原因"));
        assert!(!should_auto_merge_acknowledgement("ok?"));
        assert!(!should_auto_merge_acknowledgement("ok\nnow add tests"));
    }

    #[test]
    fn groups_acknowledgement_turns_with_previous_question() {
        let groups = group_turn_ids_by_question(vec![
            ("t1".to_string(), "How does sync work?".to_string()),
            ("t2".to_string(), "继续".to_string()),
            ("t3".to_string(), "Now export it".to_string()),
        ]);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].turn_ids, vec!["t1", "t2"]);
        assert_eq!(groups[0].origin, ConversationGroupingOrigin::AutoMerged);
        assert_eq!(groups[1].turn_ids, vec!["t3"]);
    }
}
