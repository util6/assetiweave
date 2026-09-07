use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use super::prelude::*;
use crate::backend::runtime::{AppError, AppResult};

use crate::backend::agent_market::types::{
    AgentInstallPreviewRequest, AgentInstallation, AgentInstallationView, AgentMarketError,
    AgentMarketErrorView, AgentMarketListRequest, AgentMarketProtocol, CatalogItem, Distribution,
    DistributionCandidate, DistributionType, InstallationStatus, Ownership, ProtocolStatus,
    RuntimeStatus,
};
use crate::backend::agent_market::{
    default_runtime_root, is_safe_managed_install_path, AgentLifecycleService, CatalogCache,
    DistributionSelectionContext, DistributionSelector, SystemObservation,
};
use crate::backend::extension_kernel::TrustGate;

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
    pub(crate) capabilities: crate::backend::agent_market::types::CatalogCapabilities,
    pub(crate) verification: crate::backend::agent_market::types::Verification,
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

impl AppService {
    pub(crate) fn refresh_agent_market_catalog(&self) -> AppResult<AgentMarketRefreshResult> {
        let result = CatalogCache::refresh_default()?;
        let (status, catalog, etag) = match result {
            crate::backend::agent_market::CatalogRefreshOutcome::Updated { catalog, etag } => {
                ("updated", catalog, etag)
            }
            crate::backend::agent_market::CatalogRefreshOutcome::NotModified { catalog, etag } => {
                ("not_modified", catalog, etag)
            }
        };
        let active_catalog_version = CatalogCache::best_available()?
            .catalog()
            .catalog_version
            .clone();
        Ok(AgentMarketRefreshResult {
            status: status.to_string(),
            catalog_version: catalog.catalog_version.clone(),
            active_catalog_version,
            downloaded_catalog_version: catalog.catalog_version,
            item_count: catalog.items.len(),
            source: "remote_curated".to_string(),
            etag,
        })
    }

    pub(crate) async fn list_agent_market(
        &self,
        request: AgentMarketListRequest,
    ) -> AppResult<Vec<AgentMarketItemView>> {
        let catalog = CatalogCache::best_available()?;
        let installations = self.list_agent_installations().await?;
        let context = host_distribution_context();
        let query = request
            .query
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let mut views = Vec::new();
        for item in catalog
            .catalog()
            .items
            .iter()
            .filter(|item| {
                request
                    .protocol
                    .as_ref()
                    .is_none_or(|protocol| protocol == &item.protocol)
            })
            .filter(|item| {
                query.is_empty()
                    || format!("{} {} {}", item.id, item.display_name, item.description)
                        .to_ascii_lowercase()
                        .contains(&query)
            })
            .filter(|item| {
                !request.installed_only
                    || installations
                        .iter()
                        .any(|installation| installation.agent_id == item.id)
            })
        {
            let mut item_context = context.clone();
            probe_item_system_distributions(item, &mut item_context).await;
            let candidates = DistributionSelector::select(item, &item_context, None)?;
            let installed = installations
                .iter()
                .find(|installation| installation.agent_id == item.id)
                .map(installation_view);
            let recommended_distribution_id = candidates
                .iter()
                .find(|candidate| candidate.recommended)
                .map(|candidate| candidate.distribution_id.clone());
            views.push(AgentMarketItemView {
                id: item.id.clone(),
                catalog_version: catalog.catalog().catalog_version.clone(),
                display_name: item.display_name.clone(),
                description: item.description.clone(),
                protocol: item.protocol.clone(),
                version: item.version.clone(),
                installability: installability(item, &candidates),
                capabilities: item.capabilities.clone(),
                verification: item.verification.clone(),
                distributions: candidates,
                recommended_distribution_id,
                update_available: installed.as_ref().is_some_and(|installation| {
                    installation.installation_status != "incompatible"
                        && installation.protocol == item.protocol
                        && installation.version != item.version
                }),
                installed,
            });
        }
        Ok(views)
    }

    pub(crate) async fn list_agent_installations(&self) -> AppResult<Vec<AgentInstallation>> {
        let repository =
            crate::backend::agent_market::AgentInstallationRepository::new(self.db.pool().clone());
        Ok(repository.list().await?)
    }

    pub(crate) async fn list_installed_agents(&self) -> AppResult<Vec<AgentInstallationView>> {
        Ok(self
            .list_agent_installations()
            .await?
            .iter()
            .map(installation_view)
            .collect())
    }

    pub(crate) async fn get_installed_agent(
        &self,
        agent_id: String,
    ) -> AppResult<AgentInstallationView> {
        self.list_agent_installations()
            .await?
            .into_iter()
            .find(|installation| installation.agent_id == agent_id)
            .map(|installation| installation_view(&installation))
            .ok_or_else(|| {
                AppError::from(AgentMarketError::new(
                    "agent_not_installed",
                    "The Agent is not installed.",
                    false,
                ))
            })
    }

    pub(crate) async fn check_agent_runtime(
        &self,
        agent_id: String,
    ) -> AppResult<AgentInstallationView> {
        let repository =
            crate::backend::agent_market::AgentInstallationRepository::new(self.db.pool().clone());
        let mut installation = repository.get(&agent_id).await?.ok_or_else(|| {
            AppError::from(AgentMarketError::new(
                "agent_not_installed",
                "The Agent is not installed.",
                false,
            ))
        })?;
        let now = chrono::Utc::now().to_rfc3339();
        let probe = if installation.resolved_program.is_file() {
            let spec = crate::backend::host_process::HostCommandSpec {
                program: installation.resolved_program.clone(),
                args: vec!["--version".to_string()],
                env: Vec::new(),
                working_dir: None,
                stdin: crate::backend::host_process::HostInput::Null,
                timeout: Duration::from_secs(8),
                stdout_limit: 1024 * 1024,
                stderr_limit: 256 * 1024,
            };
            Some(crate::backend::host_process::run_host_command_async(spec, None).await)
        } else {
            None
        };
        if !installation.resolved_program.is_file() {
            installation.runtime_status = if installation.ownership
                == crate::backend::agent_market::types::Ownership::Managed
            {
                RuntimeStatus::EntryMissing
            } else {
                RuntimeStatus::RuntimeMissing
            };
            installation.runtime_error_code = Some("agent_entry_missing".to_string());
            installation.runtime_error_message =
                Some("The resolved Agent entry is missing.".to_string());
            installation.installation_status = InstallationStatus::Broken;
        } else if let Some(result) = probe {
            match result {
                Ok(output)
                    if output.status.success()
                        && !output.stdout_truncated
                        && !output.stderr_truncated =>
                {
                    installation.runtime_status = RuntimeStatus::Ready;
                    installation.runtime_error_code = None;
                    installation.runtime_error_message = None;
                    if installation.installation_status == InstallationStatus::Broken {
                        installation.installation_status = InstallationStatus::Ready;
                    }
                }
                Ok(output) => {
                    installation.runtime_status = RuntimeStatus::Failed;
                    installation.runtime_error_code = Some(
                        if output.stdout_truncated || output.stderr_truncated {
                            "runtime_probe_output_limit"
                        } else {
                            "runtime_probe_failed"
                        }
                        .to_string(),
                    );
                    installation.runtime_error_message = Some(
                        "The Agent runtime version probe did not complete successfully."
                            .to_string(),
                    );
                    installation.installation_status = InstallationStatus::Broken;
                }
                Err(error) => {
                    installation.runtime_status = RuntimeStatus::Failed;
                    installation.runtime_error_code = Some(
                        match error {
                            crate::backend::host_process::HostProcessError::Timeout { .. } => {
                                "runtime_probe_timeout"
                            }
                            _ => "runtime_probe_failed",
                        }
                        .to_string(),
                    );
                    installation.runtime_error_message =
                        Some("The Agent runtime version probe failed.".to_string());
                    installation.installation_status = InstallationStatus::Broken;
                }
            }
        } else {
            installation.runtime_status = RuntimeStatus::Ready;
            installation.runtime_error_code = None;
            installation.runtime_error_message = None;
            if installation.installation_status == InstallationStatus::Broken {
                installation.installation_status = InstallationStatus::Ready;
            }
        }
        installation.runtime_checked_at = Some(now.clone());
        installation.updated_at = now;
        repository.update_health(&installation).await?;
        self.agent_runtime_manager.reload().await?;
        Ok(installation_view(&installation))
    }

    pub(crate) async fn inspect_agent_market_item(
        &self,
        agent_id: String,
    ) -> AppResult<AgentMarketItemView> {
        self.list_agent_market(AgentMarketListRequest {
            query: Some(agent_id.clone()),
            protocol: None,
            installed_only: false,
        })
        .await?
        .into_iter()
        .find(|item| item.id == agent_id)
        .ok_or_else(|| {
            AppError::from(AgentMarketError::new(
                "agent_not_found",
                "The selected Agent is not in the curated catalog.",
                false,
            ))
        })
    }

    pub(crate) async fn preview_agent_installation(
        &self,
        request: AgentInstallPreviewRequest,
    ) -> AppResult<AgentInstallPreview> {
        let catalog = CatalogCache::best_available()?;
        let item = catalog.item(&request.agent_id).ok_or_else(|| {
            AppError::from(AgentMarketError::new(
                "agent_not_found",
                "The selected Agent is not in the curated catalog.",
                false,
            ))
        })?;
        if !matches!(request.action.as_str(), "install" | "update" | "reinstall") {
            return Err(AppError::from(AgentMarketError::new(
                "invalid_action",
                "Unsupported Agent installation action.",
                false,
            )));
        }
        let mut context = host_distribution_context();
        probe_item_system_distributions(item, &mut context).await;
        let candidates =
            DistributionSelector::select(item, &context, request.distribution_id.as_deref())?;
        let selected = candidates
            .iter()
            .find(|candidate| {
                candidate.recommended
                    || request.distribution_id.as_deref()
                        == Some(candidate.distribution_id.as_str())
            })
            .cloned()
            .ok_or_else(|| {
                AppError::from(AgentMarketError::Distribution {
                    code: "distribution_unsupported".to_string(),
                    message: "The selected Agent distribution is unavailable on this platform."
                        .to_string(),
                    agent_id: Some(item.id.clone()),
                    distribution_id: request.distribution_id.clone(),
                    details: None,
                })
            })?;
        let current = self
            .list_agent_installations()
            .await?
            .into_iter()
            .find(|installation| installation.agent_id == request.agent_id);
        match request.action.as_str() {
            "install" if current.is_some() => {
                return Err(AppError::from(AgentMarketError::new(
                    "agent_already_installed",
                    "The Agent is already installed; choose update or reinstall.",
                    false,
                )))
            }
            "update" => {
                let Some(current_inst) = current.as_ref() else {
                    return Err(AppError::from(AgentMarketError::new(
                        "agent_not_installed",
                        "The Agent is not installed; choose install.",
                        false,
                    )));
                };
                if current_inst.installation_status == InstallationStatus::Incompatible
                    || current_inst.protocol != item.protocol
                {
                    return Err(AppError::from(AgentMarketError::new(
                        "agent_reinstall_required",
                        "The installed Agent is incompatible with the active catalog definition; choose reinstall instead of update.",
                        false,
                    )));
                }
            }
            "reinstall" if current.is_none() => {
                return Err(AppError::from(AgentMarketError::new(
                    "agent_not_installed",
                    "The Agent is not installed; choose install.",
                    false,
                )))
            }
            _ => {}
        }
        let mut conflicts = Vec::new();
        if self.agent_runtime_manager.agent_in_use(&request.agent_id) {
            conflicts.push("agent_in_use".to_string());
        }
        let mut warnings = Vec::new();
        if item.verification.status.needs_confirmation() {
            warnings.push("experimental_verification".to_string());
        }
        let preview_token = catalog.preview_token(item, &selected.distribution_id, &request.action);
        let target_path = selected.target_path.as_ref().map(|path| {
            crate::backend::path_utils::display_path_or_original(&path.to_string_lossy())
        });
        Ok(AgentInstallPreview {
            agent_id: item.id.clone(),
            catalog_version: catalog.catalog().catalog_version.clone(),
            action: request.action,
            selected_distribution: selected.clone(),
            alternatives: candidates
                .into_iter()
                .filter(|candidate| candidate.distribution_id != selected.distribution_id)
                .collect(),
            current_installation: current.as_ref().map(installation_view),
            target_version: item.version.clone(),
            ownership: selected.ownership.clone(),
            target_path,
            download_size: selected.download_size,
            runtime_requirements: selected.required_runtime.into_iter().collect(),
            conflicts,
            warnings,
            confirmation_required: true,
            preview_token,
        })
    }

    pub(crate) async fn preview_agent_uninstall(
        &self,
        agent_id: String,
    ) -> AppResult<AgentUninstallPreview> {
        let installation = self
            .list_agent_installations()
            .await?
            .into_iter()
            .find(|installation| installation.agent_id == agent_id)
            .ok_or_else(|| {
                AppError::from(AgentMarketError::new(
                    "agent_not_installed",
                    "The Agent is not installed.",
                    false,
                ))
            })?;
        let catalog = CatalogCache::best_available()?;
        let item = catalog.item(&agent_id).ok_or_else(|| {
            AppError::from(AgentMarketError::new(
                "agent_not_found",
                "The installed Agent is no longer in the curated catalog.",
                false,
            ))
        })?;
        let capability_assignments =
            agent_assignment_refs_from_settings(&self.app_settings_value(), &agent_id);
        let mut conflicts = capability_assignments
            .iter()
            .map(|assignment| format!("assignment:{assignment}"))
            .collect::<Vec<_>>();
        if self.agent_runtime_manager.agent_in_use(&agent_id) {
            conflicts.push("agent_in_use".to_string());
        }
        if installation.ownership == Ownership::Managed
            && !installation.install_dir.as_ref().is_some_and(|path| {
                default_runtime_root().ok().is_some_and(|runtime_root| {
                    is_safe_managed_install_path(&runtime_root, &installation.installation_id, path)
                })
            })
        {
            conflicts.push("unsafe_install_path".to_string());
        }
        Ok(AgentUninstallPreview {
            agent_id: agent_id.clone(),
            current_installation: installation_view(&installation),
            ownership: installation.ownership.clone(),
            target_path: installation.install_dir.as_ref().map(|path| {
                crate::backend::path_utils::display_path_or_original(&path.to_string_lossy())
            }),
            capability_assignments,
            conflicts,
            warnings: Vec::new(),
            confirmation_required: true,
            preview_token: catalog.preview_token(item, &installation.distribution_id, "uninstall"),
        })
    }

    pub(crate) async fn install_agent(
        &self,
        request: crate::backend::agent_market::types::AgentInstallStartRequest,
    ) -> AppResult<AgentInstallResult> {
        self.install_agent_with_cancellation(request, None).await
    }

    pub(crate) async fn install_agent_with_cancellation(
        &self,
        request: crate::backend::agent_market::types::AgentInstallStartRequest,
        cancellation: Option<Arc<AtomicBool>>,
    ) -> AppResult<AgentInstallResult> {
        self.install_agent_with_cancellation_and_progress(request, cancellation, None)
            .await
    }

    pub(crate) async fn install_agent_with_cancellation_and_progress(
        &self,
        request: crate::backend::agent_market::types::AgentInstallStartRequest,
        cancellation: Option<Arc<AtomicBool>>,
        phase_sink: Option<
            Arc<dyn Fn(crate::backend::agent_market::types::LifecycleTaskPhase) + Send + Sync>,
        >,
    ) -> AppResult<AgentInstallResult> {
        let lifecycle = self.agent_lifecycle()?;
        lifecycle
            .install_with_cancellation_and_progress(request, cancellation, phase_sink)
            .await
            .map(|outcome| AgentInstallResult {
                installation: installation_view(&outcome.installation),
                warnings: outcome.warnings,
            })
            .map_err(AppError::from)
    }

    pub(crate) async fn uninstall_agent(
        &self,
        request: crate::backend::agent_market::types::AgentUninstallStartRequest,
    ) -> AppResult<AgentInstallationView> {
        self.uninstall_agent_with_cancellation(request, None).await
    }

    pub(crate) async fn uninstall_agent_with_cancellation(
        &self,
        request: crate::backend::agent_market::types::AgentUninstallStartRequest,
        cancellation: Option<Arc<AtomicBool>>,
    ) -> AppResult<AgentInstallationView> {
        self.uninstall_agent_with_cancellation_and_progress(request, cancellation, None)
            .await
    }

    pub(crate) async fn uninstall_agent_with_cancellation_and_progress(
        &self,
        request: crate::backend::agent_market::types::AgentUninstallStartRequest,
        cancellation: Option<Arc<AtomicBool>>,
        phase_sink: Option<
            Arc<dyn Fn(crate::backend::agent_market::types::LifecycleTaskPhase) + Send + Sync>,
        >,
    ) -> AppResult<AgentInstallationView> {
        let settings_before = self.app_settings_value();
        let assignment_refs =
            agent_assignment_refs_from_settings(&settings_before, &request.agent_id);
        if assignment_refs.iter().any(|assignment| {
            !request
                .clear_capability_assignments
                .iter()
                .any(|selected| selected == assignment)
        }) {
            return Err(AppError::from(AgentMarketError::new(
                "assignment_conflict",
                "The Agent is still assigned to one or more capabilities.",
                false,
            )));
        }
        // Remove assignments before deleting the installation so a crash cannot
        // leave settings pointing at a nonexistent Agent. If lifecycle work
        // fails, restore the exact prior settings snapshot for recovery.
        let cleared_settings =
            settings_without_agent_assignments(settings_before.clone(), &assignment_refs);
        let assignments_changed = cleared_settings != settings_before;
        let pool = self.db.pool().clone();
        let runtime = self.runtime.clone();
        let lifecycle = self.agent_lifecycle()?;
        if assignments_changed {
            let saved =
                crate::backend::app_settings::save_app_settings_sqlx(&pool, cleared_settings)
                    .await
                    .map_err(|e| AgentMarketError::new("settings_save_failed", &e.code(), false))?;
            runtime.update_app_settings_value(saved.settings);
        }
        let res = lifecycle
            .uninstall_with_cancellation_and_progress(request, cancellation, phase_sink)
            .await;
        if res.is_err() && assignments_changed {
            if let Ok(restored) =
                crate::backend::app_settings::save_app_settings_sqlx(&pool, settings_before).await
            {
                runtime.update_app_settings_value(restored.settings);
            }
        }
        res.map(|installation| installation_view(&installation))
            .map_err(AppError::from)
    }

    pub(crate) async fn set_agent_enabled(
        &self,
        agent_id: String,
        enabled: bool,
    ) -> AppResult<AgentInstallationView> {
        let lifecycle = self.agent_lifecycle()?;
        lifecycle
            .set_enabled(&agent_id, enabled)
            .await
            .map(|installation| installation_view(&installation))
            .map_err(AppError::from)
    }

    fn agent_lifecycle(&self) -> AppResult<AgentLifecycleService> {
        Ok(AgentLifecycleService::new(
            self.db.pool().clone(),
            self.agent_runtime_manager.clone(),
            default_runtime_root()?,
        )?)
    }
}

fn host_distribution_context() -> DistributionSelectionContext {
    let mut context = DistributionSelectionContext::default();
    context.node_available =
        crate::backend::host_process::resolve_host_executable("node").is_some();
    context.npm_available = crate::backend::host_process::resolve_host_executable("npm").is_some();
    context.uv_available = crate::backend::host_process::resolve_host_executable("uv").is_some();
    context
}

fn installability(_item: &CatalogItem, candidates: &[DistributionCandidate]) -> String {
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

async fn probe_item_system_distributions(
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
            let Some(program) = crate::backend::host_process::resolve_host_executable(command)
            else {
                continue;
            };
            let install_context = crate::backend::agent_market::InstallContext::new(
                std::env::temp_dir().join("assetiweave-agent-market-preview"),
                item.version.clone(),
            );
            let result = crate::backend::agent_market::SystemInstaller {
                resolver: Some(program.clone()),
            };
            let observation = match crate::backend::agent_market::Installer::materialize(
                &result,
                distribution,
                &install_context,
            )
            .await
            {
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

fn installation_view(installation: &AgentInstallation) -> AgentInstallationView {
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
            crate::backend::path_utils::display_path_or_original(&path.to_string_lossy())
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

fn agent_assignment_refs_from_settings(settings: &Value, agent_id: &str) -> Vec<String> {
    settings
        .get("agentAssignments")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|assignments| assignments.iter())
        .filter(|(_, value)| value.get("agentId").and_then(Value::as_str) == Some(agent_id))
        .map(|(key, _)| key.clone())
        .collect()
}

fn settings_without_agent_assignments(mut settings: Value, assignments: &[String]) -> Value {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::agent_market::types::{
        AgentMarketProtocol, DistributionType, InstallationStatus, Ownership, ProtocolStatus,
        RuntimeStatus,
    };
    use crate::backend::agent_market::AgentInstallationRepository;
    use std::fs;
    use uuid::Uuid;

    #[tokio::test]
    async fn incompatible_installation_denies_update_and_requires_reinstall() {
        let root = std::env::temp_dir().join(format!("assetiweave-market-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create test root");
        let db_path = root.join("app.db");
        let service = AppService::open_with_db_path(db_path)
            .await
            .expect("open application service");

        let repo = AgentInstallationRepository::new(service.db.pool().clone());
        let now = chrono::Utc::now().to_rfc3339();

        let fake_bin = root.join("fake_agy");
        fs::write(&fake_bin, b"#!/bin/sh\necho 1.0.0\n").expect("write fake bin");

        let installation = AgentInstallation {
            agent_id: "antigravity".to_string(),
            installation_id: "inst-test-incompatible".to_string(),
            display_name: "Antigravity".to_string(),
            catalog_item_version: "2026.03.1".to_string(),
            agent_version: "1.0.0".to_string(),
            protocol: AgentMarketProtocol::Native,
            distribution_id: "system-antigravity".to_string(),
            distribution_type: DistributionType::System,
            ownership: Ownership::System,
            install_dir: None,
            resolved_program: fake_bin,
            args: vec![],
            definition_json: serde_json::json!({
                "id": "antigravity",
                "version": "1.0.0",
                "protocol": "native"
            }),
            integrity_json: None,
            source_registry: "curated".to_string(),
            catalog_version: "2026.03.1".to_string(),
            enabled: true,
            installation_status: InstallationStatus::Incompatible,
            runtime_status: RuntimeStatus::Ready,
            runtime_error_code: Some("catalog_distribution_incompatible".to_string()),
            runtime_error_message: Some("catalog distribution incompatible".to_string()),
            runtime_checked_at: Some(now.clone()),
            protocol_status: ProtocolStatus::Failed,
            protocol_error_code: Some("catalog_distribution_incompatible".to_string()),
            protocol_error_message: Some("catalog distribution incompatible".to_string()),
            protocol_checked_at: Some(now.clone()),
            model_status: None,
            model_error_code: None,
            model_checked_at: None,
            installed_at: now.clone(),
            updated_at: now.clone(),
        };
        repo.upsert_active(&installation)
            .await
            .expect("insert installation");

        // 1. list_agent_market 中的 update_available 必须为 false
        let items = service
            .list_agent_market(AgentMarketListRequest {
                query: Some("antigravity".to_string()),
                protocol: None,
                installed_only: false,
            })
            .await
            .expect("list agent market");
        let item = items
            .iter()
            .find(|i| i.id == "antigravity")
            .expect("antigravity in market");
        assert!(
            !item.update_available,
            "incompatible installation must have update_available == false"
        );
        assert!(item.installed.is_some());

        // 2. preview action == "update" 必须报错 agent_reinstall_required
        let update_err = service
            .preview_agent_installation(AgentInstallPreviewRequest {
                agent_id: "antigravity".to_string(),
                catalog_version: None,
                agent_version: None,
                distribution_id: None,
                action: "update".to_string(),
            })
            .await
            .expect_err("update preview must fail");
        let err_desc = format!("{update_err:?}");
        assert!(
            err_desc.contains("agent_reinstall_required"),
            "expected agent_reinstall_required, got: {err_desc}"
        );

        // 3. preview action == "reinstall" 必须成功
        let reinstall_preview = service
            .preview_agent_installation(AgentInstallPreviewRequest {
                agent_id: "antigravity".to_string(),
                catalog_version: None,
                agent_version: None,
                distribution_id: None,
                action: "reinstall".to_string(),
            })
            .await
            .expect("reinstall preview should succeed");
        assert_eq!(reinstall_preview.action, "reinstall");
        assert_eq!(reinstall_preview.agent_id, "antigravity");
        assert!(reinstall_preview.current_installation.is_some());

        let _ = fs::remove_dir_all(root);
    }
}
