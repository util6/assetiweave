use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::agents::{
    market::CatalogCapabilities, market::Verification, AgentInstallation, AgentMarketProtocol,
    CatalogItem, Distribution, DistributionCandidate, DistributionSelectionContext,
    DistributionSelector, DistributionType, InstallationStatus, Ownership, ProtocolStatus,
    RuntimeStatus, SystemObservation,
};
use crate::backend::infrastructure::agent_market::{
    AgentInstallationView, AgentMarketError, AgentMarketErrorView, CatalogCache,
    CatalogRefreshOutcome, InstallContext, Installer, SystemInstaller,
};
use serde_json::Value;

impl From<crate::backend::domain::agents::market::distribution::DistributionError> for AppError {
    fn from(
        error: crate::backend::domain::agents::market::distribution::DistributionError,
    ) -> Self {
        use crate::backend::domain::agents::market::distribution::DistributionError;
        match error {
            DistributionError::Unsupported {
                code,
                message,
                agent_id,
                distribution_id,
            } => Self::Domain {
                code,
                message,
                retryable: false,
                details: Some(serde_json::json!({
                    "agentId": agent_id,
                    "distributionId": distribution_id,
                })),
            },
            DistributionError::RuntimeMissing {
                agent_id,
                distribution_id,
            } => Self::Domain {
                code: "runtime_missing".to_string(),
                message:
                    "The selected Agent distribution requires a runtime that is not installed."
                        .to_string(),
                retryable: false,
                details: Some(serde_json::json!({
                    "agentId": agent_id,
                    "distributionId": distribution_id,
                })),
            },
            DistributionError::SystemVersionIncompatible {
                agent_id,
                distribution_id,
            } => Self::Domain {
                code: "system_version_incompatible".to_string(),
                message: "The selected system Agent runtime could not be used.".to_string(),
                retryable: false,
                details: Some(serde_json::json!({
                    "agentId": agent_id,
                    "distributionId": distribution_id,
                })),
            },
        }
    }
}

impl From<AgentMarketError> for AppError {
    fn from(error: AgentMarketError) -> Self {
        match error {
            AgentMarketError::Database(err) => Self::Db(err),
            AgentMarketError::Catalog(err) => Self::Domain {
                code: "invalid_catalog".to_string(),
                message: err.to_string(),
                retryable: false,
                details: None,
            },
            AgentMarketError::CatalogValidation {
                message,
                agent_id,
                field,
                details,
            } => {
                let structured = details.or_else(|| {
                    if agent_id.is_some() || field.is_some() {
                        Some(serde_json::json!({
                            "agentId": agent_id,
                            "field": field,
                        }))
                    } else {
                        None
                    }
                });
                Self::Domain {
                    code: "catalog_validation_failed".to_string(),
                    message,
                    retryable: false,
                    details: structured,
                }
            }
            AgentMarketError::InstallationNotFound { agent_id } => Self::Domain {
                code: "agent_not_installed".to_string(),
                message: format!("Agent installation '{agent_id}' not found"),
                retryable: false,
                details: Some(serde_json::json!({ "agentId": agent_id })),
            },
            AgentMarketError::Distribution {
                code,
                message,
                agent_id,
                distribution_id,
                details,
            } => {
                let retryable = matches!(
                    code.as_str(),
                    "runtime_missing" | "system_version_incompatible"
                );
                let structured = details.or_else(|| {
                    if agent_id.is_some() || distribution_id.is_some() {
                        Some(serde_json::json!({
                            "agentId": agent_id,
                            "distributionId": distribution_id,
                        }))
                    } else {
                        None
                    }
                });
                Self::Domain {
                    code,
                    message,
                    retryable,
                    details: structured,
                }
            }
            AgentMarketError::Process(err) => err.into(),
            AgentMarketError::Timeout { message, agent_id } => Self::Domain {
                code: "timeout".to_string(),
                message,
                retryable: true,
                details: agent_id.map(|id| serde_json::json!({ "agentId": id })),
            },
            AgentMarketError::Lifecycle {
                code,
                message,
                agent_id,
                phase,
                retryable,
                action,
                details,
            } => {
                let structured = details.or_else(|| {
                    if agent_id.is_some() || phase.is_some() || action.is_some() {
                        Some(serde_json::json!({
                            "agentId": agent_id,
                            "phase": phase,
                            "action": action,
                        }))
                    } else {
                        None
                    }
                });
                Self::Domain {
                    code,
                    message,
                    retryable,
                    details: structured,
                }
            }
            AgentMarketError::Io(err) => Self::Io(err),
            AgentMarketError::Serialization(err) => Self::Storage(err.to_string()),
        }
    }
}

#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentMarketItemView {
    pub(crate) id: String,
    pub(crate) catalog_version: String,
    pub(crate) display_name: String,
    pub(crate) description: String,
    pub(crate) protocol: AgentMarketProtocol,
    pub(crate) version: String,
    pub(crate) installability: String,
    pub(crate) capabilities: CatalogCapabilities,
    pub(crate) verification: Verification,
    pub(crate) distributions: Vec<DistributionCandidate>,
    pub(crate) recommended_distribution_id: Option<String>,
    pub(crate) installed: Option<AgentInstallationView>,
    pub(crate) update_available: bool,
}

#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentInstallPreview {
    pub(crate) agent_id: String,
    pub(crate) catalog_version: String,
    pub(crate) action: String,
    pub(crate) selected_distribution: DistributionCandidate,
    pub(crate) alternatives: Vec<DistributionCandidate>,
    pub(crate) current_installation: Option<AgentInstallationView>,
    pub(crate) target_version: String,
    pub(crate) ownership: Ownership,
    pub(crate) target_path: Option<String>,
    pub(crate) download_size: Option<u64>,
    pub(crate) runtime_requirements: Vec<String>,
    pub(crate) conflicts: Vec<String>,
    pub(crate) warnings: Vec<String>,
    pub(crate) confirmation_required: bool,
    pub(crate) preview_token: String,
}

#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentInstallResult {
    pub(crate) installation: AgentInstallationView,
    pub(crate) warnings: Vec<String>,
}

#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentUninstallPreview {
    pub(crate) agent_id: String,
    pub(crate) current_installation: AgentInstallationView,
    pub(crate) ownership: Ownership,
    pub(crate) target_path: Option<String>,
    pub(crate) capability_assignments: Vec<String>,
    pub(crate) conflicts: Vec<String>,
    pub(crate) warnings: Vec<String>,
    pub(crate) confirmation_required: bool,
    pub(crate) preview_token: String,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentMarketRefreshResult {
    pub(crate) status: String,
    pub(crate) catalog_version: String,
    pub(crate) active_catalog_version: String,
    pub(crate) downloaded_catalog_version: String,
    pub(crate) item_count: usize,
    pub(crate) source: String,
    pub(crate) etag: Option<String>,
}

pub(crate) fn host_distribution_context() -> DistributionSelectionContext {
    let mut context = DistributionSelectionContext::default();
    context.node_available =
        crate::backend::infrastructure::host_process::resolve_host_executable("node").is_some();
    context.npm_available =
        crate::backend::infrastructure::host_process::resolve_host_executable("npm").is_some();
    context.uv_available =
        crate::backend::infrastructure::host_process::resolve_host_executable("uv").is_some();
    context
}

pub(crate) fn installability(_item: &CatalogItem, candidates: &[DistributionCandidate]) -> String {
    if candidates.iter().any(|candidate| candidate.selectable) {
        return "installable".to_string();
    }
    if candidates
        .iter()
        .any(|candidate| candidate.reason_code.as_deref() == Some("runtime_missing"))
    {
        return "runtime-required".to_string();
    }
    "unsupported".to_string()
}

pub(crate) async fn probe_item_system_distributions(
    item: &CatalogItem,
    context: &mut DistributionSelectionContext,
) {
    for distribution in &item.distributions {
        let Distribution::System {
            command_candidates, ..
        } = distribution
        else {
            continue;
        };
        for command in command_candidates {
            let Some(program) =
                crate::backend::infrastructure::host_process::resolve_host_executable(&command)
            else {
                continue;
            };
            let install_context = InstallContext::new(
                std::env::temp_dir().join("assetiweave-agent-market-preview"),
                item.version.clone(),
            );
            let result = SystemInstaller {
                resolver: Some(program.clone()),
            };
            let observation =
                match Installer::materialize(&result, &distribution, &install_context).await {
                    Ok(runtime) => SystemObservation {
                        resolved_program: Some(runtime.resolved_program),
                        version: Some(runtime.version),
                        error_code: None,
                    },
                    Err(error) => SystemObservation {
                        resolved_program: Some(program),
                        version: None,
                        error_code: Some(error.to_string()),
                    },
                };
            context.system.insert(command.clone(), observation);
        }
    }
}

pub(crate) fn installation_view(installation: &AgentInstallation) -> AgentInstallationView {
    let last_checked_at = installation
        .protocol_checked_at
        .clone()
        .or_else(|| installation.runtime_checked_at.clone());
    let health_stale = installation.protocol_status == ProtocolStatus::Unchecked
        || installation.model_status.as_deref() == Some("unchecked")
        || last_checked_at.as_deref().is_none_or(|value| {
            chrono::DateTime::parse_from_rfc3339(value)
                .map(|checked| {
                    chrono::Utc::now() - checked.with_timezone(&chrono::Utc)
                        > chrono::Duration::minutes(30)
                })
                .unwrap_or(true)
        });
    AgentInstallationView {
        agent_id: installation.agent_id.clone(),
        display_name: installation.display_name.clone(),
        version: installation.agent_version.clone(),
        protocol: installation.protocol.clone(),
        distribution_id: installation.distribution_id.clone(),
        distribution_type: installation.distribution_type.clone(),
        ownership: installation.ownership.clone(),
        capabilities: installation.catalog_capabilities(),
        display_install_path: installation.install_dir.as_ref().map(|path| {
            crate::backend::infrastructure::path_utils::display_path_or_original(
                &path.to_string_lossy(),
            )
        }),
        enabled: installation.enabled,
        installed: installation.installed(),
        installation_status: if installation.enabled {
            installation.installation_status.as_str().to_string()
        } else {
            "disabled".to_string()
        },
        runtime_status: installation.runtime_status.as_str().to_string(),
        protocol_status: installation.protocol_status.as_str().to_string(),
        connected: installation.connected(),
        execution_ready: installation.execution_ready(),
        health_stale,
        selected_model_id: None,
        model_status: installation.model_status.clone(),
        update_available: false,
        operation: None,
        last_checked_at,
        error: installation
            .protocol_error_code
            .as_ref()
            .map(|code| {
                AgentMarketErrorView::from(&AgentMarketError::new(
                    code,
                    installation
                        .protocol_error_message
                        .as_deref()
                        .unwrap_or("Agent protocol health check failed."),
                    true,
                ))
            })
            .or_else(|| {
                installation.runtime_error_code.as_ref().map(|code| {
                    AgentMarketErrorView::from(&AgentMarketError::new(
                        code,
                        installation
                            .runtime_error_message
                            .as_deref()
                            .unwrap_or("Agent runtime health check failed."),
                        true,
                    ))
                })
            }),
        warnings: Vec::new(),
    }
}

pub(crate) fn agent_assignment_refs_from_settings(settings: &Value, agent_id: &str) -> Vec<String> {
    settings
        .get("agentAssignments")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|assignments| assignments.iter())
        .filter(|(_, value)| value.get("agentId").and_then(Value::as_str) == Some(agent_id))
        .map(|(key, _)| key.clone())
        .collect()
}

pub(crate) fn settings_without_agent_assignments(
    mut settings: Value,
    assignments: &[String],
) -> Value {
    if let Some(values) = settings
        .get_mut("agentAssignments")
        .and_then(Value::as_object_mut)
    {
        for assignment in assignments {
            values.remove(assignment);
        }
    }
    settings
}

#[allow(dead_code)]
fn _keep_domain_types_linked(
    _item: &CatalogItem,
    _kind: &DistributionType,
    _status: &InstallationStatus,
    _runtime: &RuntimeStatus,
    _protocol: &ProtocolStatus,
) {
}
