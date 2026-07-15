use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationAdapterKind {
    #[serde(
        rename = "external",
        alias = "codex",
        alias = "claude_code",
        alias = "opencode",
        alias = "open_code"
    )]
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
pub enum ConversationAdapterPackageRecordKind {
    Session,
    Web,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationAdapterPackageOrigin {
    BuiltIn,
    ManagedRelease,
    LocalDirectory,
    GitRef,
    LegacyExternal,
    DevOverride,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationAdapterRuntimeGateStatus {
    Ready,
    RuntimeMissing,
    HashMismatch,
    ManifestInvalid,
    CoreIncompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationPackageUpdatePolicy {
    Manual,
    FollowStable,
    FollowBeta,
    PinExact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationAdapterPackageChangeAction {
    Register,
    Unregister,
    Install,
    Update,
    Uninstall,
    Revalidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationAdapterPackageChangeRisk {
    ReadOnly,
    Write,
    HighRiskWrite,
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
pub struct ConversationAdapterPackage {
    pub package_id: String,
    pub adapter_id: String,
    pub name: String,
    pub version: String,
    pub record_kind: ConversationAdapterPackageRecordKind,
    pub install_dir: String,
    pub manifest_path: String,
    pub adapter_manifest_path: String,
    pub runtime_protocol: String,
    pub runtime_ready: bool,
    pub origin: ConversationAdapterPackageOrigin,
    pub source_url: Option<String>,
    pub git_ref: Option<String>,
    pub git_commit: Option<String>,
    pub catalog_url: Option<String>,
    pub update_policy: ConversationPackageUpdatePolicy,
    pub latest_version: Option<String>,
    pub last_checked_at: Option<String>,
    pub runtime_gate_status: ConversationAdapterRuntimeGateStatus,
    pub runtime_validated_at: Option<String>,
    pub installed_content_hash: Option<String>,
    pub trusted_package_hash: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationAdapterPackageVersion {
    pub package_id: String,
    pub version: String,
    pub install_dir: String,
    pub artifact_hash: Option<String>,
    pub content_hash: String,
    pub runtime_gate_status: ConversationAdapterRuntimeGateStatus,
    pub installed_at: String,
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
    pub translated_text: Option<String>,
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
    #[serde(default, deserialize_with = "deserialize_optional_metadata_json")]
    pub metadata_json: Option<String>,
}

fn deserialize_optional_metadata_json<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    let Some(value) = value else {
        return Ok(None);
    };
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(text))
            }
        }
        other => serde_json::to_string(&other)
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationGroupSeed {
    pub turn_ids: Vec<String>,
    pub origin: ConversationGroupingOrigin,
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
        if let Some(value) = &part.language {
            hasher.update(value.as_bytes());
        }
        if let Some(value) = &part.command {
            hasher.update(value.as_bytes());
        }
        if let Some(value) = &part.cwd {
            hasher.update(value.as_bytes());
        }
        if let Some(value) = &part.status {
            hasher.update(value.as_bytes());
        }
        if let Some(value) = part.exit_code {
            hasher.update(value.to_string().as_bytes());
        }
        if let Some(value) = &part.metadata_json {
            hasher.update(value.as_bytes());
        }
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

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
