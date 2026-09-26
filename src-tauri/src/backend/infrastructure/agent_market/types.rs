use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::error::AgentMarketErrorView;
use crate::backend::domain::agents::{
    market::CatalogCapabilities, AgentMarketProtocol, DistributionType, Ownership,
};

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
