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
        let details = error.details.or_else(|| {
            if error.agent_id.is_some() || error.phase.is_some() || error.action.is_some() {
                Some(serde_json::json!({
                    "agentId": error.agent_id,
                    "phase": error.phase,
                    "action": error.action,
                }))
            } else {
                None
            }
        });
        Self::Domain {
            code: error.code,
            message: error.message,
            retryable: error.retryable,
            details,
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
        let result = CatalogCache::refresh_default().map_err(AppError::external)?;
        let (status, catalog, etag) = match result {
            crate::backend::agent_market::CatalogRefreshOutcome::Updated { catalog, etag } => {
                ("updated", catalog, etag)
            }
            crate::backend::agent_market::CatalogRefreshOutcome::NotModified { catalog, etag } => {
                ("not_modified", catalog, etag)
            }
        };
        let active_catalog_version = CatalogCache::best_available()
            .map_err(AppError::external)?
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

    pub(crate) fn list_agent_market(
        &self,
        request: AgentMarketListRequest,
    ) -> AppResult<Vec<AgentMarketItemView>> {
        let catalog = CatalogCache::best_available().map_err(AppError::external)?;
        let installations = self.list_agent_installations()?;
        let context = host_distribution_context();
        let query = request
            .query
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        catalog
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
            .map(|item| {
                let mut item_context = context.clone();
                probe_item_system_distributions(item, &mut item_context);
                let candidates = DistributionSelector::select(item, &item_context, None)
                    .map_err(distribution_selection_error)?;
                let installed = installations
                    .iter()
                    .find(|installation| installation.agent_id == item.id)
                    .map(installation_view);
                let recommended_distribution_id = candidates
                    .iter()
                    .find(|candidate| candidate.recommended)
                    .map(|candidate| candidate.distribution_id.clone());
                Ok(AgentMarketItemView {
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
                    update_available: installed
                        .as_ref()
                        .is_some_and(|installation| installation.version != item.version),
                    installed,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()
    }

    pub(crate) fn list_agent_installations(&self) -> AppResult<Vec<AgentInstallation>> {
        let repository =
            crate::backend::agent_market::AgentInstallationRepository::new(self.db.pool().clone());
        Ok(self
            .db
            .block_on(repository.list())
            .map_err(AppError::external)?)
    }

    pub(crate) fn list_installed_agents(&self) -> AppResult<Vec<AgentInstallationView>> {
        Ok(self
            .list_agent_installations()?
            .iter()
            .map(installation_view)
            .collect())
    }

    pub(crate) fn get_installed_agent(&self, agent_id: String) -> AppResult<AgentInstallationView> {
        self.list_agent_installations()?
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

    pub(crate) fn check_agent_runtime(&self, agent_id: String) -> AppResult<AgentInstallationView> {
        let repository =
            crate::backend::agent_market::AgentInstallationRepository::new(self.db.pool().clone());
        let mut installation = self
            .db
            .block_on(repository.get(&agent_id))
            .map_err(AppError::external)?
            .ok_or_else(|| {
                AppError::from(AgentMarketError::new(
                    "agent_not_installed",
                    "The Agent is not installed.",
                    false,
                ))
            })?;
        let now = chrono::Utc::now().to_rfc3339();
        let probe = if installation.resolved_program.is_file() {
            Some(crate::backend::host_process::run_program_with_timeout(
                &installation.resolved_program,
                &["--version".to_string()],
                None,
                Duration::from_secs(8),
                1024 * 1024,
                256 * 1024,
            ))
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
        self.db
            .block_on(repository.update_health(&installation))
            .map_err(AppError::external)?;
        self.db
            .block_on(self.agent_runtime_manager.reload())
            .map_err(AppError::external)?;
        Ok(installation_view(&installation))
    }

    pub(crate) fn inspect_agent_market_item(
        &self,
        agent_id: String,
    ) -> AppResult<AgentMarketItemView> {
        self.list_agent_market(AgentMarketListRequest {
            query: Some(agent_id.clone()),
            protocol: None,
            installed_only: false,
        })?
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

    pub(crate) fn preview_agent_installation(
        &self,
        request: AgentInstallPreviewRequest,
    ) -> AppResult<AgentInstallPreview> {
        let catalog = CatalogCache::best_available().map_err(AppError::external)?;
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
        probe_item_system_distributions(item, &mut context);
        let candidates =
            DistributionSelector::select(item, &context, request.distribution_id.as_deref())
                .map_err(distribution_selection_error)?;
        let selected = candidates
            .iter()
            .find(|candidate| {
                candidate.recommended
                    || request.distribution_id.as_deref()
                        == Some(candidate.distribution_id.as_str())
            })
            .cloned()
            .ok_or_else(|| "distribution_unsupported".to_string())
            .map_err(AppError::external)?;
        let current = self
            .list_agent_installations()?
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
            "update" if current.is_none() => {
                return Err(AppError::from(AgentMarketError::new(
                    "agent_not_installed",
                    "The Agent is not installed; choose install.",
                    false,
                )))
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

    pub(crate) fn preview_agent_uninstall(
        &self,
        agent_id: String,
    ) -> AppResult<AgentUninstallPreview> {
        let installation = self
            .list_agent_installations()?
            .into_iter()
            .find(|installation| installation.agent_id == agent_id)
            .ok_or_else(|| {
                AppError::from(AgentMarketError::new(
                    "agent_not_installed",
                    "The Agent is not installed.",
                    false,
                ))
            })?;
        let catalog = CatalogCache::best_available().map_err(AppError::external)?;
        let item = catalog.item(&agent_id).ok_or_else(|| {
            AppError::from(AgentMarketError::new(
                "agent_not_found",
                "The installed Agent is no longer in the curated catalog.",
                false,
            ))
        })?;
        let capability_assignments = agent_assignment_refs(&self.db, &agent_id)?;
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

    pub(crate) fn install_agent(
        &self,
        request: crate::backend::agent_market::types::AgentInstallStartRequest,
    ) -> AppResult<AgentInstallResult> {
        self.install_agent_with_cancellation(request, None)
    }

    pub(crate) fn install_agent_with_cancellation(
        &self,
        request: crate::backend::agent_market::types::AgentInstallStartRequest,
        cancellation: Option<Arc<AtomicBool>>,
    ) -> AppResult<AgentInstallResult> {
        self.install_agent_with_cancellation_and_progress(request, cancellation, None)
    }

    pub(crate) fn install_agent_with_cancellation_and_progress(
        &self,
        request: crate::backend::agent_market::types::AgentInstallStartRequest,
        cancellation: Option<Arc<AtomicBool>>,
        phase_sink: Option<
            Arc<dyn Fn(crate::backend::agent_market::types::LifecycleTaskPhase) + Send + Sync>,
        >,
    ) -> AppResult<AgentInstallResult> {
        let lifecycle = self.agent_lifecycle()?;
        self.db
            .block_on(lifecycle.install_with_cancellation_and_progress(
                request,
                cancellation,
                phase_sink,
            ))
            .map(|outcome| AgentInstallResult {
                installation: installation_view(&outcome.installation),
                warnings: outcome.warnings,
            })
            .map_err(AppError::from)
    }

    pub(crate) fn uninstall_agent(
        &self,
        request: crate::backend::agent_market::types::AgentUninstallStartRequest,
    ) -> AppResult<AgentInstallationView> {
        self.uninstall_agent_with_cancellation(request, None)
    }

    pub(crate) fn uninstall_agent_with_cancellation(
        &self,
        request: crate::backend::agent_market::types::AgentUninstallStartRequest,
        cancellation: Option<Arc<AtomicBool>>,
    ) -> AppResult<AgentInstallationView> {
        self.uninstall_agent_with_cancellation_and_progress(request, cancellation, None)
    }

    pub(crate) fn uninstall_agent_with_cancellation_and_progress(
        &self,
        request: crate::backend::agent_market::types::AgentUninstallStartRequest,
        cancellation: Option<Arc<AtomicBool>>,
        phase_sink: Option<
            Arc<dyn Fn(crate::backend::agent_market::types::LifecycleTaskPhase) + Send + Sync>,
        >,
    ) -> AppResult<AgentInstallationView> {
        let settings_before =
            crate::backend::app_settings::read_app_settings_value_for_database(&self.db)?;
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
        if assignments_changed {
            crate::backend::app_settings::save_app_settings_for_database(
                &self.db,
                cleared_settings,
            )?;
        }

        let restore_assignments = || {
            crate::backend::app_settings::save_app_settings_for_database(
                &self.db,
                settings_before.clone(),
            )
            .map(|_| ())
        };
        let lifecycle = match self.agent_lifecycle() {
            Ok(lifecycle) => lifecycle,
            Err(error) => {
                if assignments_changed {
                    restore_assignments().map_err(|_recovery| {
                        AppError::from(AgentMarketError::new(
                            "assignment_recovery_failed",
                            "Agent uninstall failed and assignment recovery failed.",
                            true,
                        ))
                    })?;
                }
                return Err(error);
            }
        };
        let result = self
            .db
            .block_on(lifecycle.uninstall_with_cancellation_and_progress(
                request,
                cancellation,
                phase_sink,
            ))
            .map(|installation| installation_view(&installation));
        match result {
            Ok(result) => Ok(result),
            Err(error) => {
                if assignments_changed {
                    restore_assignments().map_err(|_recovery| {
                        AppError::from(AgentMarketError::new(
                            "assignment_recovery_failed",
                            "Agent uninstall failed and assignment recovery failed.",
                            true,
                        ))
                    })?;
                }
                Err(AppError::from(error))
            }
        }
    }

    pub(crate) fn set_agent_enabled(
        &self,
        agent_id: String,
        enabled: bool,
    ) -> AppResult<AgentInstallationView> {
        let lifecycle = self.agent_lifecycle()?;
        self.db
            .block_on(lifecycle.set_enabled(&agent_id, enabled))
            .map(|installation| installation_view(&installation))
            .map_err(AppError::from)
    }

    fn agent_lifecycle(&self) -> AppResult<AgentLifecycleService> {
        AgentLifecycleService::new(
            self.db.pool().clone(),
            self.agent_runtime_manager.clone(),
            default_runtime_root().map_err(AppError::from)?,
        )
        .map_err(AppError::from)
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

fn distribution_selection_error(error: String) -> AppError {
    let code = error
        .split_once(':')
        .map(|(code, _)| code)
        .unwrap_or(error.as_str())
        .trim();
    let (message, retryable) = match code {
        "runtime_missing" => (
            "The selected Agent distribution requires a runtime that is not installed.",
            true,
        ),
        "system_version_incompatible" => {
            ("The selected system Agent runtime could not be used.", true)
        }
        _ => (
            "The selected Agent distribution is unavailable on this platform.",
            false,
        ),
    };
    AppError::from(AgentMarketError::new(code, message, retryable))
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

fn probe_item_system_distributions(item: &CatalogItem, context: &mut DistributionSelectionContext) {
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
            ) {
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

fn agent_assignment_refs(
    database: &crate::backend::store::Database,
    agent_id: &str,
) -> AppResult<Vec<String>> {
    let settings = crate::backend::app_settings::read_app_settings_value_for_database(database)?;
    Ok(agent_assignment_refs_from_settings(&settings, agent_id))
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
