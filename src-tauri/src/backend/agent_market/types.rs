use std::{fmt, path::PathBuf, time::Duration};

use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

const MAX_ID_BYTES: usize = 64;
const MAX_TEXT_BYTES: usize = 500;

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AgentMarketProtocol {
    Acp,
    Native,
}

impl AgentMarketProtocol {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Acp => "acp",
            Self::Native => "native",
        }
    }
}

/// Agent-domain manifest projection. The manifest stays ACP/native-specific,
/// while its shared process and probe seams are owned by Extension Kernel.
#[derive(Clone, Debug)]
pub(crate) struct AgentPackageManifest {
    pub(crate) identity: crate::backend::extension_kernel::PackageIdentity,
    pub(crate) compatibility: crate::backend::extension_kernel::Compatibility,
    pub(crate) invocation: crate::backend::extension_kernel::ProcessInvocation,
    pub(crate) availability_probe: crate::backend::extension_kernel::ProbeSpec,
    pub(crate) model_discovery_probe: Option<crate::backend::extension_kernel::ProbeSpec>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DistributionType {
    System,
    Binary,
    Npx,
    Uvx,
}

impl DistributionType {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Binary => "binary",
            Self::Npx => "npx",
            Self::Uvx => "uvx",
        }
    }

    pub(crate) fn ownership(&self) -> Ownership {
        match self {
            Self::System => Ownership::System,
            Self::Binary | Self::Npx | Self::Uvx => Ownership::Managed,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Ownership {
    System,
    Managed,
}

impl Ownership {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Managed => "managed",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VerificationStatus {
    Tested,
    Experimental,
}

impl crate::backend::extension_kernel::TrustGate for VerificationStatus {
    #[cfg(test)]
    fn can_enable(&self) -> bool {
        true
    }

    fn needs_confirmation(&self) -> bool {
        matches!(self, Self::Experimental)
    }

    #[cfg(test)]
    fn integrity_changed(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CoreCompatibility {
    pub(crate) min: String,
    pub(crate) max_exclusive: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CatalogCapabilities {
    #[serde(default)]
    pub(crate) purposes: Vec<String>,
    #[serde(default)]
    pub(crate) text_prompt: bool,
    #[serde(default)]
    pub(crate) model_discovery: bool,
    /// The Agent can attach to a previously-created execution context.
    #[serde(default)]
    pub(crate) resume: bool,
    /// The Agent can return prior session content without creating a live turn.
    #[serde(default)]
    pub(crate) history_replay: bool,
    /// The Agent can publish live Session Events for the current turn.
    #[serde(default)]
    pub(crate) live_events: bool,
    /// The Agent can replay thought/tool history with provider fidelity.
    #[serde(default)]
    pub(crate) rich_history_replay: bool,
    /// The Agent can receive the restricted Team tool surface.
    #[serde(default)]
    pub(crate) team_tools: bool,
    /// Native resume arguments. `{session_id}` is replaced without shell parsing.
    #[serde(default)]
    pub(crate) resume_args: Option<Vec<String>>,
}

impl CatalogCapabilities {
    pub(crate) fn fallback_for_protocol(protocol: &AgentMarketProtocol) -> Self {
        match protocol {
            AgentMarketProtocol::Acp => Self {
                text_prompt: true,
                resume: true,
                history_replay: true,
                live_events: true,
                ..Self::default()
            },
            AgentMarketProtocol::Native => Self::default(),
        }
    }

    pub(crate) fn to_declared_agent_capabilities(
        &self,
        protocol: &AgentMarketProtocol,
    ) -> crate::backend::agents::types::DeclaredAgentCapabilities {
        crate::backend::agents::types::DeclaredAgentCapabilities {
            text_prompt: self.text_prompt,
            resume: self.resume,
            history_replay: self.history_replay,
            live_events: self.live_events,
            rich_history_replay: self.rich_history_replay,
            team_tools: self.team_tools,
            resume_args: matches!(protocol, AgentMarketProtocol::Native)
                .then(|| self.resume_args.clone())
                .flatten(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Verification {
    pub(crate) status: VerificationStatus,
    pub(crate) tested_at: String,
    #[serde(default)]
    pub(crate) evidence_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpstreamSource {
    pub(crate) registry_id: String,
    pub(crate) homepage: String,
    pub(crate) license: String,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Target {
    pub(crate) os: String,
    pub(crate) arch: String,
}

#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub(crate) enum Distribution {
    #[serde(rename = "system")]
    System {
        id: String,
        priority: u32,
        command_candidates: Vec<String>,
        version_args: Vec<String>,
        #[serde(default)]
        version_range: String,
        launch_args: Vec<String>,
        #[serde(default)]
        model_discovery_args: Option<Vec<String>>,
        #[serde(default)]
        session_cleanup_args: Option<Vec<String>>,
        #[serde(default)]
        session_cleanup_not_found_markers: Vec<String>,
    },
    #[serde(rename = "binary")]
    Binary {
        id: String,
        priority: u32,
        target: Target,
        archive: String,
        url: String,
        sha256: String,
        #[serde(default)]
        size: Option<u64>,
        executable: String,
        launch_args: Vec<String>,
        #[serde(default)]
        model_discovery_args: Option<Vec<String>>,
        #[serde(default)]
        session_cleanup_args: Option<Vec<String>>,
        #[serde(default)]
        session_cleanup_not_found_markers: Vec<String>,
    },
    #[serde(rename = "npx")]
    Npx {
        id: String,
        priority: u32,
        package: String,
        version: String,
        bin: String,
        launch_args: Vec<String>,
        #[serde(default)]
        node_range: Option<String>,
        #[serde(default)]
        model_discovery_args: Option<Vec<String>>,
        #[serde(default)]
        session_cleanup_args: Option<Vec<String>>,
        #[serde(default)]
        session_cleanup_not_found_markers: Vec<String>,
    },
    #[serde(rename = "uvx")]
    Uvx {
        id: String,
        priority: u32,
        package: String,
        version: String,
        command: String,
        launch_args: Vec<String>,
        #[serde(default)]
        python_range: Option<String>,
        #[serde(default)]
        model_discovery_args: Option<Vec<String>>,
        #[serde(default)]
        session_cleanup_args: Option<Vec<String>>,
        #[serde(default)]
        session_cleanup_not_found_markers: Vec<String>,
    },
}

impl<'de> Deserialize<'de> for Distribution {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let kind = value
            .get("type")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| serde::de::Error::custom("distribution type is required"))?;
        match kind {
            "system" => {
                let fields: SystemDistributionFields =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::System {
                    id: fields.id,
                    priority: fields.priority,
                    command_candidates: fields.command_candidates,
                    version_args: fields.version_args,
                    version_range: fields.version_range,
                    launch_args: fields.launch_args,
                    model_discovery_args: fields.model_discovery_args,
                    session_cleanup_args: fields.session_cleanup_args,
                    session_cleanup_not_found_markers: fields.session_cleanup_not_found_markers,
                })
            }
            "binary" => {
                let fields: BinaryDistributionFields =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::Binary {
                    id: fields.id,
                    priority: fields.priority,
                    target: fields.target,
                    archive: fields.archive,
                    url: fields.url,
                    sha256: fields.sha256,
                    size: fields.size,
                    executable: fields.executable,
                    launch_args: fields.launch_args,
                    model_discovery_args: fields.model_discovery_args,
                    session_cleanup_args: fields.session_cleanup_args,
                    session_cleanup_not_found_markers: fields.session_cleanup_not_found_markers,
                })
            }
            "npx" => {
                let fields: NpxDistributionFields =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::Npx {
                    id: fields.id,
                    priority: fields.priority,
                    package: fields.package,
                    version: fields.version,
                    bin: fields.bin,
                    launch_args: fields.launch_args,
                    node_range: fields.node_range,
                    model_discovery_args: fields.model_discovery_args,
                    session_cleanup_args: fields.session_cleanup_args,
                    session_cleanup_not_found_markers: fields.session_cleanup_not_found_markers,
                })
            }
            "uvx" => {
                let fields: UvxDistributionFields =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::Uvx {
                    id: fields.id,
                    priority: fields.priority,
                    package: fields.package,
                    version: fields.version,
                    command: fields.command,
                    launch_args: fields.launch_args,
                    python_range: fields.python_range,
                    model_discovery_args: fields.model_discovery_args,
                    session_cleanup_args: fields.session_cleanup_args,
                    session_cleanup_not_found_markers: fields.session_cleanup_not_found_markers,
                })
            }
            other => Err(serde::de::Error::custom(format!(
                "unsupported distribution type: {other}"
            ))),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SystemDistributionFields {
    id: String,
    priority: u32,
    command_candidates: Vec<String>,
    version_args: Vec<String>,
    version_range: String,
    launch_args: Vec<String>,
    #[serde(default)]
    model_discovery_args: Option<Vec<String>>,
    #[serde(default)]
    session_cleanup_args: Option<Vec<String>>,
    #[serde(default)]
    session_cleanup_not_found_markers: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinaryDistributionFields {
    id: String,
    priority: u32,
    target: Target,
    archive: String,
    url: String,
    sha256: String,
    #[serde(default)]
    size: Option<u64>,
    executable: String,
    launch_args: Vec<String>,
    #[serde(default)]
    model_discovery_args: Option<Vec<String>>,
    #[serde(default)]
    session_cleanup_args: Option<Vec<String>>,
    #[serde(default)]
    session_cleanup_not_found_markers: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NpxDistributionFields {
    id: String,
    priority: u32,
    package: String,
    version: String,
    bin: String,
    launch_args: Vec<String>,
    #[serde(default)]
    node_range: Option<String>,
    #[serde(default)]
    model_discovery_args: Option<Vec<String>>,
    #[serde(default)]
    session_cleanup_args: Option<Vec<String>>,
    #[serde(default)]
    session_cleanup_not_found_markers: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UvxDistributionFields {
    id: String,
    priority: u32,
    package: String,
    version: String,
    command: String,
    launch_args: Vec<String>,
    #[serde(default)]
    python_range: Option<String>,
    #[serde(default)]
    model_discovery_args: Option<Vec<String>>,
    #[serde(default)]
    session_cleanup_args: Option<Vec<String>>,
    #[serde(default)]
    session_cleanup_not_found_markers: Vec<String>,
}

impl Distribution {
    pub(crate) fn id(&self) -> &str {
        match self {
            Self::System { id, .. }
            | Self::Binary { id, .. }
            | Self::Npx { id, .. }
            | Self::Uvx { id, .. } => id,
        }
    }

    pub(crate) fn distribution_type(&self) -> DistributionType {
        match self {
            Self::System { .. } => DistributionType::System,
            Self::Binary { .. } => DistributionType::Binary,
            Self::Npx { .. } => DistributionType::Npx,
            Self::Uvx { .. } => DistributionType::Uvx,
        }
    }

    pub(crate) fn launch_args(&self) -> &[String] {
        match self {
            Self::System { launch_args, .. }
            | Self::Binary { launch_args, .. }
            | Self::Npx { launch_args, .. }
            | Self::Uvx { launch_args, .. } => launch_args,
        }
    }

    pub(crate) fn session_cleanup_args(&self) -> Option<&[String]> {
        match self {
            Self::System {
                session_cleanup_args,
                ..
            }
            | Self::Binary {
                session_cleanup_args,
                ..
            }
            | Self::Npx {
                session_cleanup_args,
                ..
            }
            | Self::Uvx {
                session_cleanup_args,
                ..
            } => session_cleanup_args.as_deref(),
        }
    }

    pub(crate) fn session_cleanup_not_found_markers(&self) -> &[String] {
        match self {
            Self::System {
                session_cleanup_not_found_markers,
                ..
            }
            | Self::Binary {
                session_cleanup_not_found_markers,
                ..
            }
            | Self::Npx {
                session_cleanup_not_found_markers,
                ..
            }
            | Self::Uvx {
                session_cleanup_not_found_markers,
                ..
            } => session_cleanup_not_found_markers,
        }
    }

    #[cfg(test)]
    pub(crate) fn ownership(&self) -> Ownership {
        self.distribution_type().ownership()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CatalogItem {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) description: String,
    pub(crate) protocol: AgentMarketProtocol,
    pub(crate) version: String,
    #[serde(default)]
    pub(crate) core_compatibility: CoreCompatibility,
    pub(crate) capabilities: CatalogCapabilities,
    pub(crate) verification: Verification,
    pub(crate) upstream: UpstreamSource,
    pub(crate) distributions: Vec<Distribution>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CatalogSource {
    pub(crate) kind: String,
    pub(crate) upstream: String,
    pub(crate) upstream_revision: String,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub(crate) struct Catalog {
    pub(crate) schema: String,
    #[serde(rename = "catalogVersion")]
    pub(crate) catalog_version: String,
    #[serde(rename = "generatedAt")]
    pub(crate) generated_at: String,
    pub(crate) source: CatalogSource,
    pub(crate) items: Vec<CatalogItem>,
}

#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DistributionCandidate {
    pub(crate) distribution_id: String,
    pub(crate) distribution_type: DistributionType,
    pub(crate) selectable: bool,
    pub(crate) recommended: bool,
    pub(crate) ownership: Ownership,
    pub(crate) reason_code: Option<String>,
    pub(crate) required_runtime: Option<String>,
    pub(crate) resolved_version: Option<String>,
    pub(crate) download_size: Option<u64>,
    pub(crate) target_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MaterializedRuntime {
    pub(crate) installation_id: String,
    pub(crate) ownership: Ownership,
    pub(crate) install_dir: Option<PathBuf>,
    pub(crate) resolved_program: PathBuf,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<(String, String)>,
    pub(crate) integrity: Option<serde_json::Value>,
    pub(crate) version: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum InstallationStatus {
    Ready,
    Incompatible,
    Broken,
}

impl InstallationStatus {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Incompatible => "incompatible",
            Self::Broken => "broken",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RuntimeStatus {
    Unchecked,
    Ready,
    RuntimeMissing,
    EntryMissing,
    Failed,
}

impl RuntimeStatus {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Unchecked => "unchecked",
            Self::Ready => "ready",
            Self::RuntimeMissing => "runtime_missing",
            Self::EntryMissing => "entry_missing",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ProtocolStatus {
    Unchecked,
    Ready,
    AuthRequired,
    Failed,
    Unsupported,
}

impl ProtocolStatus {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Unchecked => "unchecked",
            Self::Ready => "ready",
            Self::AuthRequired => "auth_required",
            Self::Failed => "failed",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentInstallation {
    pub(crate) agent_id: String,
    pub(crate) installation_id: String,
    pub(crate) display_name: String,
    pub(crate) catalog_item_version: String,
    pub(crate) agent_version: String,
    pub(crate) protocol: AgentMarketProtocol,
    pub(crate) distribution_id: String,
    pub(crate) distribution_type: DistributionType,
    pub(crate) ownership: Ownership,
    pub(crate) install_dir: Option<PathBuf>,
    pub(crate) resolved_program: PathBuf,
    pub(crate) args: Vec<String>,
    pub(crate) definition_json: serde_json::Value,
    pub(crate) integrity_json: Option<serde_json::Value>,
    pub(crate) source_registry: String,
    pub(crate) catalog_version: String,
    pub(crate) enabled: bool,
    pub(crate) installation_status: InstallationStatus,
    pub(crate) runtime_status: RuntimeStatus,
    pub(crate) runtime_error_code: Option<String>,
    pub(crate) runtime_error_message: Option<String>,
    pub(crate) runtime_checked_at: Option<String>,
    pub(crate) protocol_status: ProtocolStatus,
    pub(crate) protocol_error_code: Option<String>,
    pub(crate) protocol_error_message: Option<String>,
    pub(crate) protocol_checked_at: Option<String>,
    pub(crate) model_status: Option<String>,
    pub(crate) model_error_code: Option<String>,
    pub(crate) model_checked_at: Option<String>,
    pub(crate) installed_at: String,
    pub(crate) updated_at: String,
}

impl AgentInstallation {
    pub(crate) fn catalog_capabilities(&self) -> CatalogCapabilities {
        self.definition_json
            .get("capabilities")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_else(|| CatalogCapabilities::fallback_for_protocol(&self.protocol))
    }

    pub(crate) fn package_identity(
        &self,
    ) -> Result<crate::backend::extension_kernel::PackageIdentity, String> {
        // PackageIdentity currently uses semver for every extension kind, but
        // ACP Agent versions are opaque observations. Keep lifecycle identity
        // stable and retain the real value only on AgentInstallation.
        let version = Version::new(0, 0, 0);
        Ok(crate::backend::extension_kernel::PackageIdentity {
            kind: crate::backend::extension_kernel::PackageKind::Agent,
            package_id: self.agent_id.clone(),
            version,
        })
    }

    pub(crate) fn process_invocation(&self) -> crate::backend::extension_kernel::ProcessInvocation {
        let env = self
            .definition_json
            .get("env")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                Some(crate::backend::extension_kernel::EnvEntry {
                    key: entry.get("name")?.as_str()?.to_string(),
                    value: entry.get("value")?.as_str()?.to_string(),
                })
            })
            .collect();
        crate::backend::extension_kernel::ProcessInvocation {
            kind: crate::backend::extension_kernel::RuntimeProgramKind::Executable,
            entry: self.resolved_program.to_string_lossy().to_string(),
            args: self.args.clone(),
            env,
            working_dir: self.install_dir.clone(),
            // ACP runtime versions are observed for diagnostics only. Protocol
            // conformance, not semantic-version equality, determines readiness.
            version_req: None,
            immutable_install_dir: self.install_dir.clone().unwrap_or_else(|| {
                self.resolved_program
                    .parent()
                    .unwrap_or(std::path::Path::new("."))
                    .to_path_buf()
            }),
        }
    }

    pub(crate) fn package_manifest(&self) -> Result<AgentPackageManifest, String> {
        let identity = self.package_identity()?;
        let invocation = self.process_invocation();
        let availability_probe = crate::backend::extension_kernel::ProbeSpec {
            program: Some(invocation.entry.clone()),
            args: vec!["--version".to_string()],
            env: invocation.env.clone(),
            timeout: Duration::from_secs(8),
            output_limit: 1024 * 1024,
            kind: crate::backend::extension_kernel::ProbeKind::Availability,
        };
        let model_discovery_probe = self
            .definition_json
            .get("model_discovery_args")
            .or_else(|| self.definition_json.get("modelDiscoveryArgs"))
            .and_then(serde_json::Value::as_array)
            .map(|args| crate::backend::extension_kernel::ProbeSpec {
                program: Some(invocation.entry.clone()),
                args: args
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_string)
                    .collect(),
                env: invocation.env.clone(),
                timeout: Duration::from_secs(8),
                output_limit: 1024 * 1024,
                kind: crate::backend::extension_kernel::ProbeKind::ModelDiscovery,
            });
        Ok(AgentPackageManifest {
            identity,
            compatibility: crate::backend::extension_kernel::Compatibility {
                protocol_version: 1,
                core_requirement: None,
            },
            invocation,
            availability_probe,
            model_discovery_probe,
        })
    }

    pub(crate) fn installed(&self) -> bool {
        true
    }

    pub(crate) fn connected(&self) -> bool {
        self.protocol_status == ProtocolStatus::Ready
            && (self.protocol != AgentMarketProtocol::Acp
                || self.model_status.as_deref() == Some("ready"))
    }

    pub(crate) fn execution_ready(&self) -> bool {
        self.installed()
            && self.enabled
            && self.installation_status == InstallationStatus::Ready
            && self.runtime_status == RuntimeStatus::Ready
            && self.protocol_status == ProtocolStatus::Ready
            && (self.protocol != AgentMarketProtocol::Acp
                || self.model_status.as_deref() == Some("ready"))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentMarketError {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) agent_id: Option<String>,
    pub(crate) phase: Option<String>,
    pub(crate) retryable: bool,
    pub(crate) action: Option<String>,
    pub(crate) details: Option<Value>,
}

impl AgentMarketError {
    pub(crate) fn new(code: &str, message: &str, retryable: bool) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            agent_id: None,
            phase: None,
            retryable,
            action: None,
            details: None,
        }
    }
}

impl fmt::Display for AgentMarketError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AgentMarketError {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LifecycleTaskState {
    Queued,
    Running,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
}

impl LifecycleTaskState {
    pub(crate) fn is_terminal(&self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LifecycleTaskPhase {
    Queued,
    Preparing,
    ProbingRuntime,
    Downloading,
    Installing,
    ValidatingIntegrity,
    ValidatingLayout,
    ProbingProtocol,
    ActivatingDatabase,
    ReloadingRegistry,
    CleaningUp,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProgressSnapshot {
    pub(crate) completed_units: u64,
    pub(crate) total_units: Option<u64>,
    pub(crate) downloaded_bytes: Option<u64>,
    pub(crate) total_bytes: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentLifecycleTaskSnapshot {
    pub(crate) id: String,
    pub(crate) agent_id: String,
    pub(crate) action: String,
    pub(crate) state: LifecycleTaskState,
    pub(crate) phase: LifecycleTaskPhase,
    pub(crate) catalog_version: Option<String>,
    pub(crate) agent_version: Option<String>,
    pub(crate) distribution_id: Option<String>,
    pub(crate) distribution_type: Option<DistributionType>,
    pub(crate) ownership: Option<Ownership>,
    pub(crate) progress: ProgressSnapshot,
    pub(crate) cancellable: bool,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<serde_json::Value>,
    pub(crate) error: Option<AgentMarketErrorView>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentMarketErrorView {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) agent_id: Option<String>,
    pub(crate) phase: Option<String>,
    pub(crate) retryable: bool,
    pub(crate) action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) details: Option<Value>,
}

impl From<&AgentMarketError> for AgentMarketErrorView {
    fn from(value: &AgentMarketError) -> Self {
        Self {
            code: value.code.clone(),
            message: crate::backend::runtime::sanitize_public_message(&value.message),
            agent_id: value.agent_id.clone(),
            phase: value.phase.clone(),
            retryable: value.retryable,
            action: value.action.clone(),
            details: value
                .details
                .as_ref()
                .and_then(crate::backend::runtime::sanitize_details),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentMarketListRequest {
    pub(crate) query: Option<String>,
    pub(crate) protocol: Option<AgentMarketProtocol>,
    #[serde(default)]
    pub(crate) installed_only: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentInstallPreviewRequest {
    pub(crate) agent_id: String,
    pub(crate) catalog_version: Option<String>,
    pub(crate) agent_version: Option<String>,
    pub(crate) distribution_id: Option<String>,
    pub(crate) action: String,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentInstallStartRequest {
    pub(crate) agent_id: String,
    pub(crate) action: String,
    pub(crate) catalog_version: String,
    pub(crate) agent_version: String,
    pub(crate) distribution_id: String,
    pub(crate) preview_token: String,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentUninstallStartRequest {
    pub(crate) agent_id: String,
    #[serde(default)]
    pub(crate) clear_capability_assignments: Vec<String>,
    pub(crate) preview_token: String,
}

#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentInstallationView {
    pub(crate) agent_id: String,
    pub(crate) display_name: String,
    pub(crate) version: String,
    pub(crate) protocol: AgentMarketProtocol,
    pub(crate) distribution_id: String,
    pub(crate) distribution_type: DistributionType,
    pub(crate) ownership: Ownership,
    pub(crate) capabilities: CatalogCapabilities,
    pub(crate) display_install_path: Option<String>,
    pub(crate) enabled: bool,
    pub(crate) installed: bool,
    pub(crate) installation_status: String,
    pub(crate) runtime_status: String,
    pub(crate) protocol_status: String,
    pub(crate) connected: bool,
    pub(crate) execution_ready: bool,
    pub(crate) health_stale: bool,
    pub(crate) selected_model_id: Option<String>,
    pub(crate) model_status: Option<String>,
    pub(crate) update_available: bool,
    pub(crate) operation: Option<String>,
    pub(crate) last_checked_at: Option<String>,
    pub(crate) error: Option<AgentMarketErrorView>,
    pub(crate) warnings: Vec<String>,
}

impl CatalogItem {
    pub(crate) fn validate_basic(&self) -> Result<(), String> {
        if !is_valid_id(&self.id) {
            return Err(format!("invalid catalog item id: {}", self.id));
        }
        if self.display_name.trim().is_empty() || self.display_name.len() > 120 {
            return Err(format!("invalid display name for {}", self.id));
        }
        if self.description.trim().is_empty() || self.description.len() > MAX_TEXT_BYTES {
            return Err(format!("invalid description for {}", self.id));
        }
        if self.version.trim().is_empty() || self.version.len() > 120 || self.version.contains('\0')
        {
            return Err(format!("invalid observed version for {}", self.id));
        }
        if self.distributions.is_empty() {
            return Err(format!("catalog item has no distributions: {}", self.id));
        }
        let mut ids = std::collections::HashSet::new();
        for distribution in &self.distributions {
            if !ids.insert(distribution.id()) {
                return Err(format!("duplicate distribution id: {}", distribution.id()));
            }
            if distribution.id().is_empty()
                || distribution
                    .launch_args()
                    .iter()
                    .any(|arg| arg.contains('\0'))
            {
                return Err(format!("invalid distribution: {}", distribution.id()));
            }
            if let Some(args) = distribution.session_cleanup_args() {
                let placeholder_count = args
                    .iter()
                    .filter(|arg| arg.as_str() == "{session_id}")
                    .count();
                if placeholder_count != 1
                    || args.iter().any(|arg| {
                        arg.contains('\0')
                            || (arg.as_str() != "{session_id}" && arg.contains(['{', '}']))
                    })
                {
                    return Err(format!(
                        "invalid session cleanup arguments: {}",
                        distribution.id()
                    ));
                }
            }
            if distribution
                .session_cleanup_not_found_markers()
                .iter()
                .any(|marker| marker.is_empty() || marker.contains('\0'))
            {
                return Err(format!(
                    "invalid session cleanup not-found marker: {}",
                    distribution.id()
                ));
            }
            if let Distribution::System {
                command_candidates, ..
            } = distribution
            {
                if command_candidates
                    .iter()
                    .any(|command| !is_safe_command_candidate(command))
                {
                    return Err(format!(
                        "invalid system distribution: {}",
                        distribution.id()
                    ));
                }
            }
            if let Distribution::Binary {
                url,
                sha256,
                executable,
                ..
            } = distribution
            {
                if !is_safe_artifact_url(url)
                    || sha256.len() != 64
                    || !sha256
                        .chars()
                        .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
                {
                    return Err(format!(
                        "invalid binary integrity metadata: {}",
                        distribution.id()
                    ));
                }
                if !is_safe_relative_path(executable) {
                    return Err(format!("invalid binary executable path: {}", executable));
                }
            }
            if matches!(distribution, Distribution::Npx { package, version, bin, .. } if !is_valid_npm_package(package) || !is_fixed_version(version) || !is_safe_relative_path(bin))
            {
                return Err(format!("invalid npx distribution: {}", distribution.id()));
            }
            if matches!(distribution, Distribution::Uvx { package, version, command, .. } if !is_valid_python_project(package) || !is_fixed_version(version) || !is_safe_relative_path(command))
            {
                return Err(format!("invalid uvx distribution: {}", distribution.id()));
            }
        }
        Ok(())
    }
}

pub(crate) fn is_valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value.bytes().enumerate().all(|(index, byte)| {
            (index == 0 && byte.is_ascii_lowercase())
                || (index > 0
                    && (byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'))
        })
}

pub(crate) fn is_safe_relative_path(value: &str) -> bool {
    let path = std::path::Path::new(value);
    !value.is_empty()
        && !path.is_absolute()
        && !value.contains('\0')
        && path.components().all(|component| {
            !matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
}

fn is_valid_npm_package(value: &str) -> bool {
    let mut parts = value.split('/');
    let first = parts.next().unwrap_or_default();
    let name = if first.starts_with('@') {
        let Some(second) = parts.next() else {
            return false;
        };
        if parts.next().is_some() {
            return false;
        }
        format!("{first}/{second}")
    } else {
        if parts.next().is_some() {
            return false;
        }
        first.to_string()
    };
    !name.is_empty() && !name.contains([':', '\\', ' ', '\0']) && !name.ends_with('.')
}

fn is_fixed_version(value: &str) -> bool {
    Version::parse(value).is_ok()
}

pub(crate) fn is_safe_artifact_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    if url.scheme() != "https"
        || url.host_str().is_none()
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port().is_some_and(|port| port != 443)
    {
        return false;
    }
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if matches!(host.as_str(), "localhost" | "localhost.localdomain")
        || host.ends_with(".localhost")
        || host.ends_with(".local")
    {
        return false;
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return false;
    }
    true
}

pub(crate) fn is_safe_command_candidate(value: &str) -> bool {
    !value.is_empty()
        && !value.contains(['/', '\\', '\0', ';', '|', '&', '\n', '\r'])
        && value.chars().all(|character| !character.is_whitespace())
}

fn is_valid_python_project(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn distribution() -> Distribution {
        Distribution::Npx {
            id: "npx-test".to_string(),
            priority: 30,
            package: "@scope/agent".to_string(),
            version: "1.2.3".to_string(),
            bin: "agent".to_string(),
            launch_args: vec!["acp".to_string()],
            node_range: Some(">=20".to_string()),
            model_discovery_args: None,
            session_cleanup_args: None,
            session_cleanup_not_found_markers: Vec::new(),
        }
    }

    fn item() -> CatalogItem {
        CatalogItem {
            id: "agent".to_string(),
            display_name: "Agent".to_string(),
            description: "A curated agent".to_string(),
            protocol: AgentMarketProtocol::Acp,
            version: "1.2.3".to_string(),
            core_compatibility: CoreCompatibility {
                min: "0.5.0".to_string(),
                max_exclusive: "0.6.0".to_string(),
            },
            capabilities: CatalogCapabilities {
                purposes: vec!["card_translation".to_string()],
                text_prompt: true,
                model_discovery: false,
                ..CatalogCapabilities::default()
            },
            verification: Verification {
                status: VerificationStatus::Tested,
                tested_at: "2026-08-16T00:00:00Z".to_string(),
                evidence_id: Some("fixture".to_string()),
            },
            upstream: UpstreamSource {
                registry_id: "agent".to_string(),
                homepage: "https://example.com".to_string(),
                license: "MIT".to_string(),
            },
            distributions: vec![distribution()],
        }
    }

    #[test]
    fn protocol_and_distribution_are_independent_and_fixed() {
        let item = item();
        item.validate_basic().expect("valid catalog item");
        assert_eq!(item.protocol, AgentMarketProtocol::Acp);
        assert_eq!(
            item.distributions[0].distribution_type(),
            DistributionType::Npx
        );
        assert_eq!(item.distributions[0].ownership(), Ownership::Managed);
    }

    #[test]
    fn catalog_capabilities_project_to_runtime_without_losing_richness() {
        let capabilities = CatalogCapabilities {
            text_prompt: true,
            resume: true,
            history_replay: true,
            live_events: true,
            rich_history_replay: true,
            team_tools: true,
            resume_args: Some(vec![
                "--conversation".to_string(),
                "{session_id}".to_string(),
            ]),
            ..CatalogCapabilities::default()
        };

        let declared = capabilities.to_declared_agent_capabilities(&AgentMarketProtocol::Native);

        assert!(declared.resume);
        assert!(declared.history_replay);
        assert!(declared.live_events);
        assert!(declared.rich_history_replay);
        assert_eq!(declared.resume_args, capabilities.resume_args);
    }

    #[test]
    fn verification_status_uses_the_shared_trust_gate_without_collapsing_domain_states() {
        use crate::backend::extension_kernel::TrustGate;

        assert!(VerificationStatus::Tested.can_enable());
        assert!(!VerificationStatus::Tested.needs_confirmation());
        assert!(VerificationStatus::Experimental.can_enable());
        assert!(VerificationStatus::Experimental.needs_confirmation());
        assert!(!VerificationStatus::Experimental.integrity_changed());
    }

    #[test]
    fn catalog_accepts_observed_versions_but_rejects_unsafe_paths() {
        let mut item = item();
        item.version = "release-2026.08-current".to_string();
        item.validate_basic()
            .expect("Agent version metadata is observational");
        item.distributions = vec![Distribution::Binary {
            id: "binary".to_string(),
            priority: 20,
            target: Target {
                os: "darwin".to_string(),
                arch: "aarch64".to_string(),
            },
            archive: "none".to_string(),
            url: "https://example.com/agent".to_string(),
            sha256: "a".repeat(64),
            size: None,
            executable: "../agent".to_string(),
            launch_args: Vec::new(),
            model_discovery_args: None,
            session_cleanup_args: None,
            session_cleanup_not_found_markers: Vec::new(),
        }];
        assert!(item.validate_basic().is_err());
    }

    #[test]
    fn readiness_separates_installed_connected_and_execution_ready() {
        let mut installation = AgentInstallation {
            agent_id: "agent".to_string(),
            installation_id: "installation".to_string(),
            display_name: "Agent".to_string(),
            catalog_item_version: "1.0.0".to_string(),
            agent_version: "1.0.0".to_string(),
            protocol: AgentMarketProtocol::Acp,
            distribution_id: "system".to_string(),
            distribution_type: DistributionType::System,
            ownership: Ownership::System,
            install_dir: None,
            resolved_program: PathBuf::from("/usr/bin/agent"),
            args: Vec::new(),
            definition_json: serde_json::json!({}),
            integrity_json: None,
            source_registry: "agent".to_string(),
            catalog_version: "2026.08.16.1".to_string(),
            enabled: true,
            installation_status: InstallationStatus::Ready,
            runtime_status: RuntimeStatus::Ready,
            runtime_error_code: None,
            runtime_error_message: None,
            runtime_checked_at: None,
            protocol_status: ProtocolStatus::Failed,
            protocol_error_code: Some("acp_probe_failed".to_string()),
            protocol_error_message: None,
            protocol_checked_at: None,
            model_status: None,
            model_error_code: None,
            model_checked_at: None,
            installed_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        assert!(installation.installed());
        assert!(!installation.connected());
        assert!(!installation.execution_ready());
        installation.protocol_status = ProtocolStatus::Ready;
        assert!(!installation.connected());
        assert!(!installation.execution_ready());
        installation.model_status = Some("ready".to_string());
        assert!(installation.connected());
        assert!(installation.execution_ready());
    }

    #[test]
    fn package_manifest_projects_agent_invocation_and_probe_contracts() {
        let installation = AgentInstallation {
            agent_id: "agent".to_string(),
            installation_id: "installation".to_string(),
            display_name: "Agent".to_string(),
            catalog_item_version: "1.2.3".to_string(),
            agent_version: "release-2026.08-current".to_string(),
            protocol: AgentMarketProtocol::Acp,
            distribution_id: "binary".to_string(),
            distribution_type: DistributionType::Binary,
            ownership: Ownership::Managed,
            install_dir: Some(PathBuf::from("/tmp/agent-install")),
            resolved_program: PathBuf::from("/tmp/agent-install/bin/agent"),
            args: vec!["acp".to_string()],
            definition_json: serde_json::json!({
                "env": [{ "name": "TOKEN", "value": "fixture" }],
                "modelDiscoveryArgs": ["--models"],
                "capabilities": {
                    "textPrompt": true,
                    "resume": true,
                    "historyReplay": true,
                    "liveEvents": true,
                    "richHistoryReplay": true,
                    "teamTools": true
                }
            }),
            integrity_json: Some(serde_json::json!({ "sha256": "fixture" })),
            source_registry: "fixture".to_string(),
            catalog_version: "catalog-v1".to_string(),
            enabled: true,
            installation_status: InstallationStatus::Ready,
            runtime_status: RuntimeStatus::Ready,
            runtime_error_code: None,
            runtime_error_message: None,
            runtime_checked_at: None,
            protocol_status: ProtocolStatus::Ready,
            protocol_error_code: None,
            protocol_error_message: None,
            protocol_checked_at: None,
            model_status: None,
            model_error_code: None,
            model_checked_at: None,
            installed_at: "now".to_string(),
            updated_at: "now".to_string(),
        };

        let manifest = installation
            .package_manifest()
            .expect("agent package manifest");
        assert_eq!(manifest.identity.package_id, "agent");
        assert_eq!(manifest.identity.version, Version::new(0, 0, 0));
        assert_eq!(manifest.invocation.args, vec!["acp"]);
        assert_eq!(manifest.invocation.version_req, None);
        assert_eq!(manifest.invocation.env[0].key, "TOKEN");
        assert_eq!(manifest.availability_probe.args, vec!["--version"]);
        assert!(installation.catalog_capabilities().rich_history_replay);
        assert_eq!(
            manifest
                .model_discovery_probe
                .as_ref()
                .expect("model probe")
                .args,
            vec!["--models"]
        );
    }

    #[test]
    fn agent_market_error_view_redacts_infrastructure_diagnostics() {
        let mut error = AgentMarketError::new(
            "uninstall_failed",
            "failed to remove /Users/util6/private-agent token=secret",
            true,
        );
        error.details = Some(serde_json::json!({
            "path": "/Users/util6/private-agent",
            "token": "secret",
            "phase": "cleaning_up",
        }));

        let view = AgentMarketErrorView::from(&error);
        let serialized = serde_json::to_string(&view).unwrap();

        assert_eq!(view.message, "The operation failed.");
        assert_eq!(view.details.as_ref().unwrap()["path"], "<redacted>");
        assert!(view.details.as_ref().unwrap().get("token").is_none());
        assert_eq!(view.details.as_ref().unwrap()["phase"], "cleaning_up");
        assert!(!serialized.contains("/Users/util6"));
        assert!(!serialized.contains("secret"));
    }
}
