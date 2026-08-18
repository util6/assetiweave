use super::prelude::*;

pub(super) const EXTERNAL_ADAPTER_PROTOCOL_VERSION: u32 = 1;
pub(super) const DEFAULT_PROBE_TIMEOUT_MS: u64 = 10_000;
pub(super) const DEFAULT_LIST_TIMEOUT_MS: u64 = 30_000;
pub(super) const DEFAULT_READ_TIMEOUT_MS: u64 = 120_000;
pub(super) const DEFAULT_MAX_CONTROL_LINE_BYTES: usize = 8 * 1024 * 1024;
pub(super) const DEFAULT_MAX_ITEM_LINE_BYTES: usize = 64 * 1024 * 1024;
pub(super) const DEFAULT_MAX_TOTAL_BYTES: usize = 256 * 1024 * 1024;

/// Immutable domain catalog published through the shared kernel snapshot.
/// Package manifests remain domain-owned; this catalog only provides the
/// complete adapter registration view consumed by conversation workflows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ConversationAdapterCatalog {
    pub(crate) adapters: Vec<ConversationAdapter>,
}

impl ConversationAdapterCatalog {
    pub(crate) fn new(adapters: Vec<ConversationAdapter>) -> Self {
        Self { adapters }
    }

    pub(crate) fn get(&self, adapter_id: &str) -> Option<&ConversationAdapter> {
        self.adapters
            .iter()
            .find(|adapter| adapter.id == adapter_id)
    }
}

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
    #[serde(default, alias = "cardContractVersion")]
    pub(crate) card_contract_version: Option<u32>,
    #[serde(default, alias = "cardKinds")]
    pub(crate) card_kinds: Vec<ConversationCardKindDefinition>,
}

impl ConversationAdapterManifest {
    pub(crate) fn package_identity(
        &self,
    ) -> Result<
        crate::backend::extension_kernel::PackageIdentity,
        crate::backend::extension_kernel::ExtensionError,
    > {
        let version = semver::Version::parse(&self.version).map_err(|error| {
            crate::backend::extension_kernel::ExtensionError::ManifestInvalid {
                package_id: self.id.clone(),
                reason: format!("invalid semantic version: {error}"),
            }
        })?;
        Ok(crate::backend::extension_kernel::PackageIdentity {
            kind: crate::backend::extension_kernel::PackageKind::ConversationAdapter,
            package_id: self.id.clone(),
            version,
        })
    }

    pub(crate) fn compatibility(&self) -> crate::backend::extension_kernel::Compatibility {
        crate::backend::extension_kernel::Compatibility {
            protocol_version: self.protocol_version,
            core_requirement: None,
        }
    }

    pub(crate) fn process_invocation(
        &self,
        install_dir: &std::path::Path,
    ) -> crate::backend::extension_kernel::ProcessInvocation {
        let (kind, entry, args, version_req) = match self.runtime.as_ref() {
            Some(runtime) => (
                match runtime.kind {
                    ConversationAdapterRuntimeKind::Node => {
                        crate::backend::extension_kernel::RuntimeProgramKind::Node
                    }
                    ConversationAdapterRuntimeKind::Python => {
                        crate::backend::extension_kernel::RuntimeProgramKind::Python
                    }
                    ConversationAdapterRuntimeKind::Bash => {
                        crate::backend::extension_kernel::RuntimeProgramKind::Bash
                    }
                    ConversationAdapterRuntimeKind::Executable => {
                        crate::backend::extension_kernel::RuntimeProgramKind::Executable
                    }
                },
                runtime.entry.clone(),
                runtime.args.clone(),
                runtime.version.clone(),
            ),
            None => (
                crate::backend::extension_kernel::RuntimeProgramKind::Executable,
                self.command.first().cloned().unwrap_or_default(),
                self.command.iter().skip(1).cloned().collect(),
                None,
            ),
        };
        crate::backend::extension_kernel::ProcessInvocation {
            kind,
            entry,
            args,
            env: Vec::new(),
            working_dir: Some(install_dir.to_path_buf()),
            version_req,
            immutable_install_dir: install_dir.to_path_buf(),
        }
    }

    pub(crate) fn availability_probe(&self) -> crate::backend::extension_kernel::ProbeSpec {
        crate::backend::extension_kernel::ProbeSpec {
            program: None,
            args: vec!["--version".to_string()],
            env: Vec::new(),
            timeout: std::time::Duration::from_millis(DEFAULT_PROBE_TIMEOUT_MS),
            output_limit: DEFAULT_MAX_CONTROL_LINE_BYTES,
            kind: crate::backend::extension_kernel::ProbeKind::Availability,
        }
    }
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
    pub(crate) content_hash: String,
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
    pub(crate) legacy_cards_upgraded: usize,
    pub(crate) session_descriptors: Vec<ConversationSessionDescriptor>,
    pub(crate) snapshot_complete: bool,
    pub(crate) sessions: Vec<NormalizedConversationSession>,
    pub(crate) markdown_export: Option<ExternalMarkdownExport>,
    pub(crate) warnings: Vec<String>,
    pub(crate) stderr: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct ConversationSessionDescriptor {
    pub(crate) external_id: String,
    pub(crate) updated_at: Option<String>,
    pub(crate) source_locator: Option<String>,
    pub(crate) version_token: String,
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
