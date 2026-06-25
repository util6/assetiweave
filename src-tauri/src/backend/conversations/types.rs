use super::prelude::*;

pub(super) const EXTERNAL_ADAPTER_PROTOCOL_VERSION: u32 = 1;
pub(super) const DEFAULT_PROBE_TIMEOUT_MS: u64 = 10_000;
pub(super) const DEFAULT_LIST_TIMEOUT_MS: u64 = 30_000;
pub(super) const DEFAULT_READ_TIMEOUT_MS: u64 = 120_000;
pub(super) const DEFAULT_MAX_LINE_BYTES: usize = 8 * 1024 * 1024;
pub(super) const DEFAULT_MAX_TOTAL_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ConversationAdapterManifest {
    #[serde(alias = "schemaVersion")]
    pub(crate) schema_version: u32,
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    #[serde(alias = "protocolVersion")]
    pub(crate) protocol_version: u32,
    #[serde(default)]
    pub(crate) command: Vec<String>,
    #[serde(default)]
    pub(crate) runtime: Option<ConversationAdapterRuntime>,
    pub(crate) capabilities: Vec<String>,
    #[serde(alias = "inputKinds")]
    pub(crate) input_kinds: Vec<ConversationSourceKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ConversationAdapterRuntime {
    #[serde(rename = "type")]
    pub(crate) kind: ConversationAdapterRuntimeKind,
    pub(crate) entry: String,
    #[serde(default)]
    pub(crate) args: Vec<String>,
    #[serde(default)]
    pub(crate) version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConversationAdapterRuntimeKind {
    Node,
    Python,
    Bash,
    Executable,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ConversationAdapterRuntimeStatus {
    pub(crate) kind: ConversationAdapterRuntimeKind,
    pub(crate) program: String,
    pub(crate) available: bool,
    pub(crate) version: Option<String>,
    pub(crate) required_version: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ExternalAdapterRegisterParams {
    #[serde(alias = "manifestPath")]
    pub(crate) manifest_path: String,
    #[serde(default)]
    pub(crate) yes: bool,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ExternalAdapterScaffoldParams {
    pub(crate) directory: String,
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(default, alias = "runtimeType")]
    pub(crate) runtime_type: Option<ConversationAdapterRuntimeKind>,
    #[serde(default, alias = "runtimeEntry")]
    pub(crate) runtime_entry: Option<String>,
    #[serde(default, alias = "runtimeVersion")]
    pub(crate) runtime_version: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ExternalAdapterValidateParams {
    #[serde(alias = "manifestPath")]
    pub(crate) manifest_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ExternalAdapterTryRunParams {
    #[serde(alias = "manifestPath")]
    pub(crate) manifest_path: String,
    pub(crate) method: String,
    pub(crate) location: Option<String>,
    #[serde(default, alias = "sessionId")]
    pub(crate) session_id: Option<String>,
    #[serde(default)]
    pub(crate) yes: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExternalAdapterValidationResult {
    pub(crate) valid: bool,
    pub(crate) manifest_path: String,
    pub(crate) manifest_hash: String,
    pub(crate) executable_path: String,
    pub(crate) executable_hash: Option<String>,
    pub(crate) manifest: ConversationAdapterManifest,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExternalAdapterScaffoldResult {
    pub(crate) dry_run: bool,
    pub(crate) manifest_path: String,
    pub(crate) request_fixture_path: String,
    pub(crate) response_fixture_path: String,
    pub(crate) export_request_fixture_path: String,
    pub(crate) export_response_fixture_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExternalAdapterRunResult {
    pub(crate) method: String,
    pub(crate) item_count: usize,
    pub(crate) warning_count: usize,
    pub(crate) sessions: Vec<NormalizedConversationSession>,
    pub(crate) markdown_export: Option<ExternalMarkdownExport>,
    pub(crate) warnings: Vec<String>,
    pub(crate) stderr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ExternalMarkdownExport {
    pub(crate) content: String,
    #[serde(alias = "relativePath")]
    pub(crate) relative_path: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ExternalAdapterLine {
    #[serde(rename = "type")]
    pub(super) kind: String,
    #[serde(default)]
    pub(super) item: Option<Value>,
    #[serde(default)]
    pub(super) message: Option<String>,
    #[serde(default)]
    pub(super) error: Option<Value>,
}
