use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::validation::*;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMarketProtocol {
    Acp,
    Native,
}

impl AgentMarketProtocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Acp => "acp",
            Self::Native => "native",
        }
    }
}

impl From<AgentMarketProtocol> for crate::backend::domain::agents::definition::AgentProtocol {
    fn from(p: AgentMarketProtocol) -> Self {
        match p {
            AgentMarketProtocol::Acp => Self::Acp,
            AgentMarketProtocol::Native => Self::Native,
        }
    }
}

impl From<crate::backend::domain::agents::definition::AgentProtocol> for AgentMarketProtocol {
    fn from(p: crate::backend::domain::agents::definition::AgentProtocol) -> Self {
        match p {
            crate::backend::domain::agents::definition::AgentProtocol::Acp => Self::Acp,
            crate::backend::domain::agents::definition::AgentProtocol::Native => Self::Native,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DistributionType {
    System,
    Binary,
    Npx,
    Uvx,
}

impl DistributionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Binary => "binary",
            Self::Npx => "npx",
            Self::Uvx => "uvx",
        }
    }

    pub fn ownership(&self) -> Ownership {
        match self {
            Self::System => Ownership::System,
            Self::Binary | Self::Npx | Self::Uvx => Ownership::Managed,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ownership {
    System,
    Managed,
}

impl Ownership {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Managed => "managed",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Tested,
    Experimental,
}

impl VerificationStatus {
    pub fn needs_confirmation(&self) -> bool {
        matches!(self, Self::Experimental)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreCompatibility {
    pub min: String,
    pub max_exclusive: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogCapabilities {
    #[serde(default)]
    pub purposes: Vec<String>,
    #[serde(default)]
    pub text_prompt: bool,
    #[serde(default)]
    pub model_discovery: bool,
    #[serde(default)]
    pub resume: bool,
    #[serde(default)]
    pub history_replay: bool,
    #[serde(default)]
    pub live_events: bool,
    #[serde(default)]
    pub rich_history_replay: bool,
    #[serde(default)]
    pub resume_args: Option<Vec<String>>,
}

impl CatalogCapabilities {
    pub fn fallback_for_protocol(protocol: &AgentMarketProtocol) -> Self {
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

    pub fn to_declared_agent_capabilities(
        &self,
        protocol: &AgentMarketProtocol,
    ) -> crate::backend::domain::agents::DeclaredAgentCapabilities {
        crate::backend::domain::agents::DeclaredAgentCapabilities {
            text_prompt: self.text_prompt,
            resume: self.resume,
            history_replay: self.history_replay,
            live_events: self.live_events,
            rich_history_replay: self.rich_history_replay,
            resume_args: matches!(protocol, AgentMarketProtocol::Native)
                .then(|| self.resume_args.clone())
                .flatten(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verification {
    pub status: VerificationStatus,
    pub tested_at: String,
    #[serde(default)]
    pub evidence_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamSource {
    pub registry_id: String,
    pub homepage: String,
    pub license: String,
}

pub use super::catalog_item::{Catalog, CatalogItem};
pub use super::distribution_spec::{Distribution, Target};

pub(crate) fn validate_non_blank(value: &str) -> Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("non_blank"));
    }
    Ok(())
}

pub(crate) fn validate_max_120_bytes(value: &str) -> Result<(), validator::ValidationError> {
    if value.len() > 120 {
        return Err(validator::ValidationError::new("max_120_bytes"));
    }
    Ok(())
}

pub(crate) fn validate_max_500_bytes(value: &str) -> Result<(), validator::ValidationError> {
    if value.len() > 500 {
        return Err(validator::ValidationError::new("max_500_bytes"));
    }
    Ok(())
}

pub(crate) fn validate_no_null_bytes(value: &str) -> Result<(), validator::ValidationError> {
    if value.contains('\0') {
        return Err(validator::ValidationError::new("no_null_bytes"));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DistributionCandidate {
    pub distribution_id: String,
    pub distribution_type: DistributionType,
    pub selectable: bool,
    pub recommended: bool,
    pub ownership: Ownership,
    pub reason_code: Option<String>,
    pub required_runtime: Option<String>,
    pub resolved_version: Option<String>,
    pub download_size: Option<u64>,
    pub target_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializedRuntime {
    pub installation_id: String,
    pub ownership: Ownership,
    pub install_dir: Option<PathBuf>,
    pub resolved_program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub integrity: Option<serde_json::Value>,
    pub version: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallationStatus {
    Ready,
    Incompatible,
    Broken,
}

impl InstallationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Incompatible => "incompatible",
            Self::Broken => "broken",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeStatus {
    Unchecked,
    Ready,
    RuntimeMissing,
    EntryMissing,
    Failed,
}

impl RuntimeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unchecked => "unchecked",
            Self::Ready => "ready",
            Self::RuntimeMissing => "runtime_missing",
            Self::EntryMissing => "entry_missing",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolStatus {
    Unchecked,
    Ready,
    AuthRequired,
    Failed,
    Unsupported,
}

impl ProtocolStatus {
    pub fn as_str(&self) -> &'static str {
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
pub struct AgentInstallation {
    pub agent_id: String,
    pub installation_id: String,
    pub display_name: String,
    pub catalog_item_version: String,
    pub agent_version: String,
    pub protocol: AgentMarketProtocol,
    pub distribution_id: String,
    pub distribution_type: DistributionType,
    pub ownership: Ownership,
    pub install_dir: Option<PathBuf>,
    pub resolved_program: PathBuf,
    pub args: Vec<String>,
    pub definition_json: serde_json::Value,
    pub integrity_json: Option<serde_json::Value>,
    pub source_registry: String,
    pub catalog_version: String,
    pub enabled: bool,
    pub installation_status: InstallationStatus,
    pub runtime_status: RuntimeStatus,
    pub runtime_error_code: Option<String>,
    pub runtime_error_message: Option<String>,
    pub runtime_checked_at: Option<String>,
    pub protocol_status: ProtocolStatus,
    pub protocol_error_code: Option<String>,
    pub protocol_error_message: Option<String>,
    pub protocol_checked_at: Option<String>,
    pub model_status: Option<String>,
    pub model_error_code: Option<String>,
    pub model_checked_at: Option<String>,
    pub installed_at: String,
    pub updated_at: String,
}

impl AgentInstallation {
    pub fn catalog_capabilities(&self) -> CatalogCapabilities {
        self.definition_json
            .get("capabilities")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_else(|| CatalogCapabilities::fallback_for_protocol(&self.protocol))
    }

    pub fn installed(&self) -> bool {
        true
    }

    pub fn connected(&self) -> bool {
        self.enabled
            && self.installation_status == InstallationStatus::Ready
            && self.protocol_status == ProtocolStatus::Ready
    }

    pub fn execution_ready(&self) -> bool {
        self.installed()
            && self.enabled
            && self.installation_status == InstallationStatus::Ready
            && self.runtime_status == RuntimeStatus::Ready
            && self.protocol_status == ProtocolStatus::Ready
    }
}
