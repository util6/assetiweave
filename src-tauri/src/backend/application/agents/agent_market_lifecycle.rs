use super::agent_market_types::*;
use crate::backend::application::agents::lifecycle::AgentLifecycleCoordinator;
use crate::backend::application::agents::migration::migrate_legacy_assignments;
use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::agents::{
    AgentInstallation, Distribution, DistributionCandidate, DistributionSelector, DistributionType,
    InstallationStatus, Ownership, ProtocolStatus, RuntimeStatus, SystemObservation,
};
use crate::backend::infrastructure::agent_market::{
    default_runtime_root, is_safe_managed_install_path, AgentInstallPreviewRequest,
    AgentInstallStartRequest, AgentInstallationView, AgentMarketError, AgentMarketListRequest,
    AgentUninstallStartRequest, CatalogCache, InstallContext, Installer, LifecycleTaskPhase,
    SystemInstaller,
};
use crate::backend::infrastructure::runtime::AppRuntime;
use crate::backend::store::system::AgentInstallationRepository;
use std::path::Path;
use std::sync::{atomic::AtomicBool, Arc};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Recover persisted Agent Market state and prepare the runtime manager before
/// Application publishes the completed startup snapshot.
pub(crate) async fn prepare_startup_runtime(
    runtime: &AppRuntime,
    db_path: &Path,
    resident_host: bool,
) -> AppResult<()> {
    let manager = runtime.context().agent_runtime_manager.clone();
    let runtime_root = default_runtime_root().map_err(AppError::from)?;
    let coordinator =
        AgentLifecycleCoordinator::new(runtime.db().pool().clone(), manager.clone(), runtime_root)
            .map_err(AppError::from)?;
    coordinator
        .recover_startup()
        .await
        .map_err(AppError::from)?;

    let migration_scope = db_path.to_string_lossy().to_string();
    if let Err(error) = migrate_legacy_assignments(
        runtime.db().pool().clone(),
        manager.clone(),
        &migration_scope,
    )
    .await
    {
        tracing::warn!(
            action = "app.startup.agent_market_migration",
            error = %error,
            "agent market legacy migration deferred"
        );
    }
    manager.reload().await.map_err(AppError::from)?;
    if resident_host {
        if let Err(error) = manager.prepare_startup_health_refresh().await {
            tracing::warn!(
                action = "app.startup.agent_health_prepare",
                error = %error,
                "Agent startup health refresh could not be prepared"
            );
        }
    }
    Ok(())
}

impl AppService {
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
            crate::backend::infrastructure::path_utils::display_path_or_original(
                &path.to_string_lossy(),
            )
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
                crate::backend::infrastructure::path_utils::display_path_or_original(
                    &path.to_string_lossy(),
                )
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
        request: AgentInstallStartRequest,
    ) -> AppResult<AgentInstallResult> {
        self.install_agent_with_cancellation(request, None).await
    }

    pub(crate) async fn install_agent_with_cancellation(
        &self,
        request: AgentInstallStartRequest,
        cancellation: Option<Arc<AtomicBool>>,
    ) -> AppResult<AgentInstallResult> {
        self.install_agent_with_cancellation_and_progress(request, cancellation, None)
            .await
    }

    pub(crate) async fn install_agent_with_cancellation_and_progress(
        &self,
        request: AgentInstallStartRequest,
        cancellation: Option<Arc<AtomicBool>>,
        phase_sink: Option<Arc<dyn Fn(LifecycleTaskPhase) + Send + Sync>>,
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
        request: AgentUninstallStartRequest,
    ) -> AppResult<AgentInstallationView> {
        self.uninstall_agent_with_cancellation(request, None).await
    }

    pub(crate) async fn uninstall_agent_with_cancellation(
        &self,
        request: AgentUninstallStartRequest,
        cancellation: Option<Arc<AtomicBool>>,
    ) -> AppResult<AgentInstallationView> {
        self.uninstall_agent_with_cancellation_and_progress(request, cancellation, None)
            .await
    }

    pub(crate) async fn uninstall_agent_with_cancellation_and_progress(
        &self,
        request: AgentUninstallStartRequest,
        cancellation: Option<Arc<AtomicBool>>,
        phase_sink: Option<Arc<dyn Fn(LifecycleTaskPhase) + Send + Sync>>,
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
            let saved = crate::backend::infrastructure::app_settings::save_app_settings_sqlx(
                &pool,
                cleared_settings,
            )
            .await
            .map_err(|e| AgentMarketError::new("settings_save_failed", &e.code(), false))?;
            runtime.update_app_settings_value(saved.settings);
        }
        let res = lifecycle
            .uninstall_with_cancellation_and_progress(request, cancellation, phase_sink)
            .await;
        if res.is_err() && assignments_changed {
            if let Ok(restored) =
                crate::backend::infrastructure::app_settings::save_app_settings_sqlx(
                    &pool,
                    settings_before,
                )
                .await
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

    fn agent_lifecycle(&self) -> AppResult<AgentLifecycleCoordinator> {
        Ok(AgentLifecycleCoordinator::new(
            self.db.pool().clone(),
            self.agent_runtime_manager.clone(),
            default_runtime_root()?,
        )?)
    }
}
