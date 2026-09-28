use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

mod adapter_path;
pub mod events;
pub mod fingerprint;
pub mod grouping;
pub(crate) mod pricing;
pub mod projection;
pub mod read_models;
mod search;
mod serde_utils;
pub mod usage;

pub(crate) use adapter_path::{
    validate_conversation_adapter_entry_path, ConversationAdapterEntryPathError,
};
pub use events::*;
pub use fingerprint::*;
pub use grouping::*;
pub use projection::*;
pub use read_models::*;
pub(crate) use search::*;
use serde_utils::deserialize_optional_metadata_json;
pub use usage::*;

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

impl ConversationPartRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
            Self::System => "system",
        }
    }
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

impl ConversationPartKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::CodeBlock => "code",
            Self::Command => "command",
            Self::Tool => "tool",
            Self::FileChange => "file_change",
            Self::Subagent => "subagent",
            Self::Metadata => "metadata",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationGroupingOrigin {
    Imported,
    AutoMerged,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationSyncStatus {
    Running,
    #[default]
    Completed,
    PartialSuccess,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionSyncFailure {
    pub session_external_id: String,
    pub stage: String,
    pub error_code: String,
    pub error_message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionSyncWarning {
    pub session_external_id: Option<String>,
    pub code: String,
    pub message: String,
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
    #[serde(default = "default_execution_origin")]
    pub execution_origin: String,
    #[serde(default)]
    pub execution_purpose: Option<String>,
    #[serde(default = "default_user_visible")]
    pub user_visible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationSessionObservation {
    pub external_id: String,
    pub updated_at: Option<String>,
    pub source_locator: Option<String>,
    pub version_token: String,
}

fn default_execution_origin() -> String {
    "user".to_string()
}

fn default_user_visible() -> bool {
    true
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

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct NormalizedConversationSession {
    pub external_id: String,
    pub title: Option<String>,
    pub project_path: Option<String>,
    pub started_at: Option<String>,
    pub updated_at: Option<String>,
    pub source_locator: Option<String>,
    pub source_fingerprint: Option<String>,
    pub turns: Vec<NormalizedConversationTurn>,
    #[serde(default)]
    pub execution_origin: Option<String>,
    #[serde(default)]
    pub execution_purpose: Option<String>,
    #[serde(default)]
    pub user_visible: Option<bool>,
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

#[cfg(test)]
#[path = "conversation_tests.rs"]
mod tests;
