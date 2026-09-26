mod install;
mod uninstall;

use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

use crate::backend::{
    domain::agents::{
        market::catalog::CatalogService, AgentInstallation, InstallationStatus, ProtocolStatus,
        RuntimeStatus,
    },
    infrastructure::agent_market::{
        cache::CatalogCache,
        error::{AgentMarketError, LifecycleTaskPhase},
        runtime::{definition::definition_from_installation, AgentRuntimeManager},
        types::{AgentInstallStartRequest, AgentUninstallStartRequest},
    },
    store::system::AgentInstallationRepository,
};

pub(crate) use install::InstallOutcome;

#[derive(Clone)]
pub(crate) struct AgentLifecycleCoordinator {
    pub(crate) catalog: CatalogService,
    pub(crate) repository: AgentInstallationRepository,
    pub(crate) runtime_manager: Arc<AgentRuntimeManager>,
    pub(crate) runtime_root: PathBuf,
}

impl AgentLifecycleCoordinator {
    pub(crate) fn new(
        pool: sqlx::SqlitePool,
        runtime_manager: Arc<AgentRuntimeManager>,
        runtime_root: PathBuf,
    ) -> Result<Self, AgentMarketError> {
        let catalog = CatalogCache::best_available()?;
        Ok(Self::new_with_catalog(
            pool,
            runtime_manager,
            runtime_root,
            catalog,
        ))
    }

    pub(crate) fn new_with_catalog(
        pool: sqlx::SqlitePool,
        runtime_manager: Arc<AgentRuntimeManager>,
        runtime_root: PathBuf,
        catalog: CatalogService,
    ) -> Self {
        Self {
            catalog,
            repository: AgentInstallationRepository::new(pool),
            runtime_manager,
            runtime_root,
        }
    }

    pub(crate) async fn recover_startup(&self) -> Result<Vec<String>, AgentMarketError> {
        let installations = self.repository.list().await?;
        let mut warnings = self
            .runtime_manager
            .cleanup_runtime_storage(&self.runtime_root);
        for installation in installations {
            if installation.installation_status != InstallationStatus::Ready {
                continue;
            }
            if let Some(item) = self.catalog.item(&installation.agent_id) {
                let is_compatible = installation.protocol == item.protocol
                    && item.distributions.iter().any(|dist| {
                        dist.id() == installation.distribution_id
                            && dist.distribution_type() == installation.distribution_type
                    });
                if !is_compatible {
                    let now = chrono::Utc::now().to_rfc3339();
                    self.repository
                        .mark_incompatible(
                            &installation.agent_id,
                            "catalog_distribution_incompatible",
                            "The installed Agent distribution or protocol is incompatible with the active catalog.",
                            &now,
                        )
                        .await?;
                    warnings.push(format!(
                        "{} marked incompatible: catalog_distribution_incompatible",
                        installation.agent_id
                    ));
                    continue;
                }
            }
            let entry_error = self
                .runtime_manager
                .probe_installation_entry(&self.runtime_root, &installation);
            let definition_error = if entry_error.is_none()
                && installation.enabled
                && installation.runtime_status == RuntimeStatus::Ready
                && installation.protocol_status == ProtocolStatus::Ready
            {
                definition_from_installation(&installation).err()
            } else {
                None
            };
            if let Some((runtime_status, code, message)) = entry_error {
                self.repository
                    .mark_broken(
                        &installation.agent_id,
                        runtime_status,
                        code,
                        message,
                        &chrono::Utc::now().to_rfc3339(),
                    )
                    .await?;
                warnings.push(format!("{} marked broken: {code}", installation.agent_id));
            } else if let Some(error) = definition_error {
                self.repository
                    .mark_broken(
                        &installation.agent_id,
                        RuntimeStatus::Failed,
                        "definition_invalid",
                        &error.to_string(),
                        &chrono::Utc::now().to_rfc3339(),
                    )
                    .await?;
                warnings.push(format!(
                    "{} marked broken: definition_invalid",
                    installation.agent_id
                ));
            }
        }
        self.runtime_manager.reload().await?;
        Ok(warnings)
    }

    #[cfg(test)]
    pub(crate) async fn install(
        &self,
        request: AgentInstallStartRequest,
    ) -> Result<InstallOutcome, AgentMarketError> {
        self.install_with_cancellation_and_progress(request, None, None)
            .await
    }

    pub(crate) async fn install_with_cancellation_and_progress(
        &self,
        request: AgentInstallStartRequest,
        cancellation: Option<Arc<AtomicBool>>,
        phase_sink: Option<Arc<dyn Fn(LifecycleTaskPhase) + Send + Sync>>,
    ) -> Result<InstallOutcome, AgentMarketError> {
        install::run(self, request, cancellation, phase_sink).await
    }

    pub(crate) async fn uninstall_with_cancellation_and_progress(
        &self,
        request: AgentUninstallStartRequest,
        cancellation: Option<Arc<AtomicBool>>,
        phase_sink: Option<Arc<dyn Fn(LifecycleTaskPhase) + Send + Sync>>,
    ) -> Result<AgentInstallation, AgentMarketError> {
        uninstall::run(self, request, cancellation, phase_sink).await
    }

    pub(crate) async fn set_enabled(
        &self,
        agent_id: &str,
        enabled: bool,
    ) -> Result<AgentInstallation, AgentMarketError> {
        self.runtime_manager.invalidate_agent_state(agent_id).await;
        let mutation_gate = self.runtime_manager.mutation_gate(agent_id);
        let _mutation_lease = mutation_gate.write().await;

        let installation = self
            .repository
            .get(agent_id)
            .await
            .map_err(|error| market_error("storage_failed", error, true))?
            .ok_or_else(|| {
                market_error("agent_not_installed", "The Agent is not installed.", false)
            })?;

        if self.runtime_manager.agent_in_use(agent_id) {
            return Err(market_error(
                "agent_in_use",
                "The Agent has an active execution.",
                true,
            ));
        }

        let updated_at = chrono::Utc::now().to_rfc3339();
        self.repository
            .update_enabled(agent_id, enabled, &updated_at)
            .await
            .map_err(|error| market_error("storage_failed", error, true))?;

        if self.runtime_manager.reload().await.is_err() {
            let _ = self
                .repository
                .update_enabled(agent_id, installation.enabled, &updated_at)
                .await;
            return Err(market_error(
                "registry_reload_failed",
                "The Agent runtime registry could not be reloaded.",
                true,
            ));
        }

        self.repository
            .get(agent_id)
            .await
            .map_err(|error| market_error("storage_failed", error, true))?
            .ok_or_else(|| {
                market_error(
                    "agent_not_installed",
                    "The Agent installation disappeared.",
                    true,
                )
            })
    }
}

pub(crate) fn market_error(
    code: &str,
    message: impl std::fmt::Display,
    retryable: bool,
) -> AgentMarketError {
    let msg = format!("{message}");
    AgentMarketError::new(code, &msg, retryable)
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
