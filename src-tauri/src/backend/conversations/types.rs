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
    pub(crate) command: Vec<String>,
    pub(crate) capabilities: Vec<String>,
    #[serde(alias = "inputKinds")]
    pub(crate) input_kinds: Vec<ConversationSourceKind>,
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
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExternalAdapterRunResult {
    pub(crate) method: String,
    pub(crate) item_count: usize,
    pub(crate) warning_count: usize,
    pub(crate) sessions: Vec<NormalizedConversationSession>,
    pub(crate) warnings: Vec<String>,
    pub(crate) stderr: String,
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
