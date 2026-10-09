use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::error::AgentMarketErrorView;
use crate::backend::domain::agents::{
    market::CatalogCapabilities, AgentMarketProtocol, DistributionType, Ownership,
};

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMarketListRequest {
    pub query: Option<String>,
    pub protocol: Option<AgentMarketProtocol>,
    #[serde(default)]
    pub installed_only: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInstallPreviewRequest {
    pub agent_id: String,
    pub catalog_version: Option<String>,
    pub agent_version: Option<String>,
    pub distribution_id: Option<String>,
    pub action: String,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInstallStartRequest {
    pub agent_id: String,
    pub action: String,
    pub catalog_version: String,
    pub agent_version: String,
    pub distribution_id: String,
    pub preview_token: String,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentUninstallStartRequest {
    pub agent_id: String,
    #[serde(default)]
    pub clear_capability_assignments: Vec<String>,
    pub preview_token: String,
}

#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInstallationView {
    pub agent_id: String,
    pub display_name: String,
    pub version: String,
    pub protocol: AgentMarketProtocol,
    pub distribution_id: String,
    pub distribution_type: DistributionType,
    pub ownership: Ownership,
    pub capabilities: CatalogCapabilities,
    pub display_install_path: Option<String>,
    pub enabled: bool,
    pub installed: bool,
    pub installation_status: String,
    pub runtime_status: String,
    pub protocol_status: String,
    pub connected: bool,
    pub execution_ready: bool,
    pub health_stale: bool,
    pub selected_model_id: Option<String>,
    pub model_status: Option<String>,
    pub update_available: bool,
    pub operation: Option<String>,
    pub last_checked_at: Option<String>,
    pub error: Option<AgentMarketErrorView>,
    pub warnings: Vec<String>,
}
