use super::prelude::*;

pub(super) const EXTERNAL_ADAPTER_PROTOCOL_VERSION: u32 = 1;
pub(super) const DEFAULT_PROBE_TIMEOUT_MS: u64 = 10_000;
pub(super) const DEFAULT_LIST_TIMEOUT_MS: u64 = 30_000;
pub(super) const DEFAULT_READ_TIMEOUT_MS: u64 = 120_000;
pub(super) const DEFAULT_PROJECT_TIMEOUT_MS: u64 = 30_000;
pub(super) const DEFAULT_MAX_CONTROL_LINE_BYTES: usize = 8 * 1024 * 1024;
pub(super) const DEFAULT_MAX_ITEM_LINE_BYTES: usize = 64 * 1024 * 1024;
pub(super) const DEFAULT_MAX_TOTAL_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConversationAdapterManifest {
    #[serde(alias = "schemaVersion")]
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(alias = "protocolVersion")]
    pub protocol_version: u32,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub runtime: Option<ConversationAdapterRuntime>,
    pub capabilities: Vec<String>,
    #[serde(alias = "inputKinds")]
    pub input_kinds: Vec<ConversationSourceKind>,
    #[serde(default, alias = "cardContractVersion")]
    pub card_contract_version: Option<u32>,
    #[serde(default, alias = "cardKinds")]
    pub card_kinds: Vec<ConversationCardKindDefinition>,
    #[serde(default, alias = "projectionVersion")]
    pub projection_version: Option<u32>,
}

impl ConversationAdapterManifest {
    pub fn package_identity(
        &self,
    ) -> Result<
        crate::backend::infrastructure::extensions::PackageIdentity,
        crate::backend::infrastructure::extensions::ExtensionError,
    > {
        let version = semver::Version::parse(&self.version).map_err(|error| {
            crate::backend::infrastructure::extensions::ExtensionError::ManifestInvalid {
                package_id: self.id.clone(),
                reason: format!("invalid semantic version: {error}"),
            }
        })?;
        Ok(
            crate::backend::infrastructure::extensions::PackageIdentity {
                kind: crate::backend::infrastructure::extensions::PackageKind::ConversationAdapter,
                package_id: self.id.clone(),
                version,
            },
        )
    }

    pub fn compatibility(&self) -> crate::backend::infrastructure::extensions::Compatibility {
        crate::backend::infrastructure::extensions::Compatibility {
            protocol_version: self.protocol_version,
            core_requirement: None,
        }
    }

    pub fn process_invocation(
        &self,
        install_dir: &std::path::Path,
    ) -> crate::backend::infrastructure::extensions::ProcessInvocation {
        let (kind, entry, args, version_req) = match self.runtime.as_ref() {
            Some(runtime) => (
                match runtime.kind {
                    ConversationAdapterRuntimeKind::Node => {
                        crate::backend::infrastructure::extensions::RuntimeProgramKind::Node
                    }
                    ConversationAdapterRuntimeKind::Python => {
                        crate::backend::infrastructure::extensions::RuntimeProgramKind::Python
                    }
                    ConversationAdapterRuntimeKind::Bash => {
                        crate::backend::infrastructure::extensions::RuntimeProgramKind::Bash
                    }
                    ConversationAdapterRuntimeKind::Executable => {
                        crate::backend::infrastructure::extensions::RuntimeProgramKind::Executable
                    }
                },
                runtime.entry.clone(),
                runtime.args.clone(),
                runtime.version.clone(),
            ),
            None => (
                crate::backend::infrastructure::extensions::RuntimeProgramKind::Executable,
                self.command.first().cloned().unwrap_or_default(),
                self.command.iter().skip(1).cloned().collect(),
                None,
            ),
        };
        crate::backend::infrastructure::extensions::ProcessInvocation {
            kind,
            entry,
            args,
            env: Vec::new(),
            working_dir: Some(install_dir.to_path_buf()),
            version_req,
            immutable_install_dir: install_dir.to_path_buf(),
        }
    }

    pub fn availability_probe(&self) -> crate::backend::infrastructure::extensions::ProbeSpec {
        crate::backend::infrastructure::extensions::ProbeSpec {
            program: None,
            args: vec!["--version".to_string()],
            env: Vec::new(),
            timeout: std::time::Duration::from_millis(DEFAULT_PROBE_TIMEOUT_MS),
            output_limit: DEFAULT_MAX_CONTROL_LINE_BYTES,
            kind: crate::backend::infrastructure::extensions::ProbeKind::Availability,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConversationAdapterRuntime {
    #[serde(rename = "type")]
    pub kind: ConversationAdapterRuntimeKind,
    pub entry: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationAdapterRuntimeKind {
    Node,
    Python,
    Bash,
    Executable,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ConversationAdapterRuntimeStatus {
    pub kind: ConversationAdapterRuntimeKind,
    pub program: String,
    pub available: bool,
    pub version: Option<String>,
    pub required_version: Option<String>,
    pub error: Option<String>,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExternalAdapterRegisterParams {
    #[serde(alias = "manifestPath")]
    pub manifest_path: String,
    #[serde(default)]
    pub yes: bool,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExternalAdapterScaffoldParams {
    pub directory: String,
    pub id: String,
    pub name: String,
    #[serde(default, alias = "runtimeType")]
    pub runtime_type: Option<ConversationAdapterRuntimeKind>,
    #[serde(default, alias = "runtimeEntry")]
    pub runtime_entry: Option<String>,
    #[serde(default, alias = "runtimeVersion")]
    pub runtime_version: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExternalAdapterValidateParams {
    #[serde(alias = "manifestPath")]
    pub manifest_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExternalAdapterTryRunParams {
    #[serde(alias = "manifestPath")]
    pub manifest_path: String,
    pub method: String,
    pub location: Option<String>,
    #[serde(default, alias = "sessionId")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub yes: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternalAdapterValidationResult {
    pub valid: bool,
    pub manifest_path: String,
    pub content_hash: String,
    pub manifest_hash: String,
    pub executable_path: String,
    pub executable_hash: Option<String>,
    pub manifest: ConversationAdapterManifest,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternalAdapterScaffoldResult {
    pub dry_run: bool,
    pub manifest_path: String,
    pub request_fixture_path: String,
    pub response_fixture_path: String,
    pub export_request_fixture_path: String,
    pub export_response_fixture_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternalAdapterRunResult {
    pub method: String,
    pub item_count: usize,
    pub warning_count: usize,
    pub legacy_cards_upgraded: usize,
    pub session_descriptors: Vec<ConversationSessionDescriptor>,
    pub snapshot_complete: bool,
    pub sessions: Vec<NormalizedConversationSession>,
    pub command_projections: Vec<ConversationCommandProjection>,
    pub markdown_export: Option<ExternalMarkdownExport>,
    pub usage_events: Vec<crate::backend::store::conversations::usage_repo::RawUsageEventInput>,
    pub next_cursor: Option<String>,
    pub decoder_profile: Option<String>,
    pub diagnostics: Vec<String>,
    pub warnings: Vec<String>,
    pub stderr: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationCommandProjection {
    pub part_id: String,
    pub schema_version: u32,
    pub projector_version: String,
    pub nodes: Vec<ConversationCommandProjectionNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationCommandProjectionParams {
    pub adapter_id: String,
    pub parts: Vec<ConversationCommandProjectionPart>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationCommandProjectionPart {
    pub part_id: String,
    pub command: String,
    #[serde(default)]
    pub command_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationCommandProjectionNode {
    pub display_order: usize,
    pub command: String,
    #[serde(default)]
    pub command_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationSessionDescriptor {
    pub external_id: String,
    pub updated_at: Option<String>,
    pub source_locator: Option<String>,
    pub version_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalMarkdownExport {
    pub content: String,
    #[serde(alias = "relativePath")]
    pub relative_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalAdapterProgress {
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub current: Option<u64>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub worker: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExternalAdapterLine {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub item: Option<Value>,
    #[serde(default)]
    pub event: Option<Value>,
    #[serde(default)]
    pub usage_event: Option<Value>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub error: Option<Value>,
    #[serde(default)]
    pub progress: Option<Value>,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub current: Option<u64>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub worker: Option<String>,
}
