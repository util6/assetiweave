use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

pub(crate) fn parse_conversation_timestamp(value: &str) -> Option<DateTime<Utc>> {
    let value = value.trim();
    if let Ok(timestamp) = DateTime::parse_from_rfc3339(value) {
        return Some(timestamp.with_timezone(&Utc));
    }
    let raw = value.parse::<i64>().ok()?;
    let (seconds, nanoseconds) = if raw.unsigned_abs() >= 100_000_000_000 {
        (
            raw.div_euclid(1_000),
            u32::try_from(raw.rem_euclid(1_000)).ok()? * 1_000_000,
        )
    } else {
        (raw, 0)
    };
    DateTime::from_timestamp(seconds, nanoseconds)
}

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

impl crate::backend::extension_kernel::TrustGate for ConversationAdapterTrustState {
    #[cfg(test)]
    fn can_enable(&self) -> bool {
        matches!(self, Self::BuiltIn | Self::Trusted)
    }

    fn needs_confirmation(&self) -> bool {
        matches!(self, Self::Changed)
    }

    #[cfg(test)]
    fn integrity_changed(&self) -> bool {
        matches!(self, Self::Changed | Self::Untrusted)
    }
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
    SwitchVersion,
    Rollback,
    DeleteVersion,
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
    pub card_contract_version: Option<u32>,
    pub card_kinds: Vec<ConversationCardKindDefinition>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConversationCardKindDefinition {
    pub id: String,
    #[serde(default, alias = "semanticRole")]
    pub semantic_role: Option<String>,
    pub label: String,
    #[serde(alias = "defaultRenderer")]
    pub default_renderer: String,
    #[serde(alias = "allowedRenderers")]
    pub allowed_renderers: Vec<String>,
    #[serde(default, alias = "iconHint")]
    pub icon_hint: Option<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationAdapterReleaseChannel {
    Stable,
    Beta,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationAdapterCatalogRelease {
    pub catalog_url: String,
    pub package_id: String,
    pub adapter_id: String,
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub channel: ConversationAdapterReleaseChannel,
    pub released_at: Option<String>,
    pub core_compatibility: String,
    pub artifact_url: String,
    pub artifact_size: Option<i64>,
    pub artifact_sha256: String,
    pub changelog_markdown: String,
    pub breaking_change: bool,
    pub runtime_protocol: String,
    pub record_kind: ConversationAdapterPackageRecordKind,
    pub package_manifest_file: String,
    pub adapter_manifest_file: String,
    pub adapter_manifest_json: Option<String>,
    pub source_json: Option<String>,
    pub etag: Option<String>,
    pub fetched_at: String,
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
    pub command_label: Option<String>,
    pub source_execution_id: Option<String>,
    pub content_card: Option<ConversationContentCardDescriptor>,
    pub metadata_json: Option<String>,
    pub translated_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationQuestion {
    pub id: String,
    pub session_id: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationQuestionTurn {
    pub question_id: String,
    pub turn_id: String,
    pub turn_order: i64,
    pub assignment_origin: ConversationGroupingOrigin,
    pub assigned_at: String,
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
    #[serde(default)]
    pub command_label: Option<String>,
    #[serde(default)]
    pub source_execution_id: Option<String>,
    #[serde(default, alias = "contentCard")]
    pub content_card: Option<ConversationContentCardDescriptor>,
    #[serde(default, deserialize_with = "deserialize_optional_metadata_json")]
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConversationContentCardDescriptor {
    #[serde(alias = "schemaVersion")]
    pub schema_version: u32,
    pub kind: String,
    #[serde(default)]
    pub renderer: Option<String>,
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

pub fn conversation_id_fragment(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    find_conversation_hash(&normalized)
        .or_else(|| find_legacy_conversation_hash(&normalized))
        .unwrap_or(normalized.as_str())
        .chars()
        .take(8)
        .collect()
}

pub fn conversation_id_search_term(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() == 8 && normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Some(normalized);
    }
    if is_conversation_domain_id(&normalized)
        && (find_conversation_hash(&normalized).is_some()
            || find_legacy_conversation_hash(&normalized).is_some())
    {
        Some(normalized)
    } else {
        None
    }
}

fn is_conversation_domain_id(value: &str) -> bool {
    [
        "conversation-session-",
        "conversation-question-",
        "conversation-turn-",
        "conversation-part-",
        "web-record-session-",
        "web-record-question-",
        "web-record-turn-",
        "web-record-part-",
    ]
    .iter()
    .any(|prefix| value.starts_with(prefix))
}

fn find_conversation_hash(value: &str) -> Option<&str> {
    find_hex_run(value, 64)
}

fn find_legacy_conversation_hash(value: &str) -> Option<&str> {
    find_hex_run_at_least(value, 12)
}

fn find_hex_run(value: &str, expected_length: usize) -> Option<&str> {
    let bytes = value.as_bytes();
    if bytes.len() < expected_length {
        return None;
    }
    let mut start = 0;
    while start < bytes.len() {
        if !bytes[start].is_ascii_hexdigit() {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < bytes.len() && bytes[end].is_ascii_hexdigit() {
            end += 1;
        }
        if end - start == expected_length {
            return value.get(start..end);
        }
        start = end;
    }
    None
}

fn find_hex_run_at_least(value: &str, minimum_length: usize) -> Option<&str> {
    let bytes = value.as_bytes();
    if bytes.len() < minimum_length {
        return None;
    }
    let mut start = 0;
    while start < bytes.len() {
        if !bytes[start].is_ascii_hexdigit() {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < bytes.len() && bytes[end].is_ascii_hexdigit() {
            end += 1;
        }
        if end - start >= minimum_length {
            return value.get(start..end);
        }
        start = end;
    }
    None
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

fn should_auto_merge_interruption_recovery(user_text: &str) -> bool {
    let normalized = user_text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.contains('?')
        || normalized.contains('？')
        || normalized.contains("```")
    {
        return false;
    }

    [
        "continue where we left off",
        "pick up where we left off",
        "resume where we left off",
        "continue the previous question",
        "继续上一个问题",
        "接着刚才",
        "从刚才继续",
        "恢复刚才",
        "回到刚才",
    ]
    .iter()
    .any(|prefix| normalized == *prefix || normalized.starts_with(&format!("{prefix} ")))
}

fn should_auto_merge_micro_follow_up(user_text: &str) -> bool {
    let normalized = user_text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.chars().count() > 48
        || normalized.contains('?')
        || normalized.contains('？')
        || normalized.contains("```")
    {
        return false;
    }

    [
        "change it to ",
        "change that to ",
        "make it ",
        "switch it to ",
        "rename it to ",
        "改成",
        "改为",
        "换成",
        "加上",
        "删掉",
        "去掉",
        "把它",
        "调整",
    ]
    .iter()
    .any(|prefix| normalized.starts_with(prefix))
}

pub fn group_turn_ids_by_question<I>(turns: I) -> Vec<ConversationGroupSeed>
where
    I: IntoIterator<Item = (String, String)>,
{
    let mut groups: Vec<ConversationGroupSeed> = Vec::new();
    for (turn_id, user_text) in turns {
        if should_auto_merge_acknowledgement(&user_text)
            || should_auto_merge_interruption_recovery(&user_text)
            || should_auto_merge_micro_follow_up(&user_text)
        {
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
        if let Some(value) = &part.source_execution_id {
            hasher.update(value.as_bytes());
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
    fn conversation_id_fragment_extracts_hashes_and_supports_legacy_ids() {
        assert_eq!(
            conversation_id_fragment(
                "conversation-session-ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
            ),
            "abcdef01"
        );
        assert_eq!(
            conversation_id_fragment(
                "conversation-part-1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef-answer"
            ),
            "12345678"
        );
        assert_eq!(conversation_id_fragment("  Legacy-Session  "), "legacy-s");
        assert_eq!(
            conversation_id_fragment("conversation-session-abcdef1234567890abcdef1234567890"),
            "abcdef12"
        );
        assert_eq!(conversation_id_fragment("   "), "");
    }

    #[test]
    fn conversation_id_search_term_only_accepts_display_fragments_or_full_hash_ids() {
        assert_eq!(
            conversation_id_search_term("ABCDEF01"),
            Some("abcdef01".to_string())
        );
        assert_eq!(conversation_id_search_term("dead"), None);
        assert_eq!(conversation_id_search_term("abcdef123456"), None);
        assert_eq!(conversation_id_search_term("session title"), None);
        assert_eq!(
            conversation_id_search_term(
                "conversation-session-abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
            ),
            Some(
                "conversation-session-abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
                    .to_string()
            )
        );
        assert_eq!(
            conversation_id_search_term(
                "unrelated-abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
            ),
            None
        );
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

    #[test]
    fn groups_interruption_recovery_and_micro_follow_up_with_previous_question() {
        let groups = group_turn_ids_by_question(vec![
            ("t1".to_string(), "Design the sync boundary".to_string()),
            ("t2".to_string(), "继续上一个问题".to_string()),
            ("t3".to_string(), "改成 Rust".to_string()),
            ("t4".to_string(), "Now export it".to_string()),
        ]);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].turn_ids, vec!["t1", "t2", "t3"]);
        assert_eq!(groups[0].origin, ConversationGroupingOrigin::AutoMerged);
        assert_eq!(groups[1].turn_ids, vec!["t4"]);
        assert_eq!(groups[1].origin, ConversationGroupingOrigin::Imported);
    }
}
