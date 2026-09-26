use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use sqlx::SqlitePool;

use crate::backend::{
    domain::agents::{
        definition::{AgentDefinition, AgentId},
        market::catalog::CatalogService,
        AgentInstallation, AgentMarketProtocol, InstallationStatus, Ownership, ProtocolStatus,
        RuntimeStatus,
    },
    infrastructure::{
        agent_execution::{
            backends::{acp::AcpExecutionBackend, native::NativeExecutionBackend},
            executor::AgentExecutor,
            registry::{AgentRegistry, AgentRegistryHandle},
            AgentExecutionRuntime,
        },
        agent_market::{cache::CatalogCache, error::AgentMarketError},
        extensions::DomainPackageSystem,
    },
    store::system::AgentInstallationRepository,
};

pub(crate) mod definition;
pub(crate) mod model_cache;
pub(crate) mod package_system;
pub(crate) mod probes_acp;
pub(crate) mod probes_acp_health;
pub(crate) mod probes_native;

pub(crate) use definition::*;
pub(crate) use model_cache::*;
pub(crate) use package_system::*;

pub(crate) const STAGING_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

pub(crate) type AgentRuntimeRegistry = AgentRegistryHandle;

#[derive(Clone)]
pub(crate) struct AgentRuntimeManager {
    pub(crate) repository: AgentInstallationRepository,
    registry: AgentRuntimeRegistry,
    registry_snapshot:
        Arc<crate::backend::infrastructure::extensions::RegistrySnapshot<AgentRegistry>>,
    executor: Arc<AgentExecutor>,
    workspace_root: PathBuf,
    models_cache: Arc<tokio::sync::RwLock<BoundedModelCache>>,
    probe_flights: Arc<tokio::sync::Mutex<HashMap<AgentProbeIdentity, Arc<ProbeFlight>>>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct AgentHealthRefreshSummary {
    pub(crate) checked: usize,
    pub(crate) available: usize,
    pub(crate) unavailable: usize,
}

impl AgentRuntimeManager {
    pub(crate) fn new(pool: SqlitePool, workspace_root: PathBuf) -> Self {
        let registry_snapshot = Arc::new(
            crate::backend::infrastructure::extensions::RegistrySnapshot::new(
                AgentRegistry::from_definitions(Vec::<AgentDefinition>::new())
                    .expect("empty agent registry is valid"),
            ),
        );
        let registry = AgentRuntimeRegistry::from_snapshot(registry_snapshot.clone());
        let executor = Arc::new(AgentExecutor::with_registry_handle_and_bindings(
            registry.clone(),
            Arc::new(AcpExecutionBackend::new(workspace_root.clone())),
            Arc::new(NativeExecutionBackend::new(workspace_root.clone())),
            2,
            Arc::new(crate::backend::store::system::PersistentBindingStore::new(
                pool.clone(),
            )),
        ));
        Self {
            repository: AgentInstallationRepository::new(pool),
            registry,
            registry_snapshot,
            executor,
            workspace_root,
            models_cache: Arc::new(tokio::sync::RwLock::new(BoundedModelCache::new(
                MODEL_CACHE_CAPACITY,
            ))),
            probe_flights: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    #[cfg(test)]
    pub(crate) fn registry(&self) -> AgentRuntimeRegistry {
        self.registry.clone()
    }

    pub(crate) fn runtime(&self) -> Arc<dyn AgentExecutionRuntime> {
        self.executor.clone()
    }

    pub(crate) fn agent_in_use(&self, agent_id: &str) -> bool {
        self.executor.agent_in_use(agent_id)
    }

    pub(crate) fn mutation_gate(&self, agent_id: &str) -> Arc<tokio::sync::RwLock<()>> {
        self.executor.mutation_gate(agent_id)
    }

    pub(crate) async fn reload(&self) -> Result<u64, AgentMarketError> {
        self.invalidate_all_acp_probe_state().await;
        self.reload_registry().await
    }

    pub(crate) async fn reload_registry(&self) -> Result<u64, AgentMarketError> {
        let installations = self.repository.list_registry_candidates().await?;
        let definitions = installations
            .iter()
            .map(|installation| {
                let package_system = AgentPackageSystem::from_installation(installation)?;
                if package_system.kind()
                    != crate::backend::infrastructure::extensions::PackageKind::Agent
                {
                    return Err(AgentMarketError::new(
                        "package_kind_invalid",
                        "Agent package system returned the wrong package kind",
                        false,
                    ));
                }
                let install_dir = installation
                    .install_dir
                    .clone()
                    .or_else(|| {
                        installation
                            .resolved_program
                            .parent()
                            .map(Path::to_path_buf)
                    })
                    .ok_or_else(|| {
                        AgentMarketError::new(
                            "missing_runtime_directory",
                            "Agent installation has no runtime directory",
                            false,
                        )
                    })?;
                let inspected = package_system.inspect(&install_dir).map_err(|error| {
                    AgentMarketError::new("package_inspect_failed", &error.to_string(), false)
                })?;
                let _ = inspected;
                definition_from_installation(installation)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let next = AgentRegistry::from_definitions(definitions).map_err(|error| {
            AgentMarketError::new("registry_init_failed", &error.to_string(), false)
        })?;
        self.registry_snapshot.replace(next);
        Ok(self.registry.bump_generation())
    }

    pub(crate) fn cleanup_runtime_storage(&self, runtime_root: &Path) -> Vec<String> {
        probes_native::cleanup_runtime_directories(runtime_root)
    }

    pub(crate) fn probe_installation_entry(
        &self,
        runtime_root: &Path,
        installation: &AgentInstallation,
    ) -> Option<(RuntimeStatus, &'static str, &'static str)> {
        if installation.ownership == Ownership::Managed
            && !installation.install_dir.as_ref().is_some_and(|path| {
                super::layout::is_safe_managed_install_path(
                    runtime_root,
                    &installation.installation_id,
                    path,
                )
            })
        {
            Some((
                RuntimeStatus::Failed,
                "managed_path_invalid",
                "The managed Agent path is outside the runtime root.",
            ))
        } else if !installation.resolved_program.is_file() {
            Some((
                if installation.ownership == Ownership::Managed {
                    RuntimeStatus::EntryMissing
                } else {
                    RuntimeStatus::RuntimeMissing
                },
                "agent_entry_missing",
                "The resolved Agent entry is missing.",
            ))
        } else {
            None
        }
    }

    pub(crate) async fn prepare_startup_health_refresh(&self) -> Result<u64, AgentMarketError> {
        let changed = self
            .repository
            .mark_health_unchecked(&chrono::Utc::now().to_rfc3339())
            .await?;
        self.reload().await?;
        Ok(changed)
    }

    pub(crate) async fn refresh_installed_agent_health(
        &self,
    ) -> Result<AgentHealthRefreshSummary, AgentMarketError> {
        let agent_ids = self
            .repository
            .list()
            .await?
            .into_iter()
            .filter(|installation| {
                installation.enabled
                    && installation.installation_status != InstallationStatus::Incompatible
            })
            .map(|installation| (installation.agent_id, installation.protocol))
            .collect::<Vec<_>>();
        let mut summary = AgentHealthRefreshSummary::default();
        for (agent_id, protocol) in agent_ids {
            let available = match protocol {
                AgentMarketProtocol::Acp => match self.refresh_acp_connection(&agent_id).await {
                    Ok(result) => result.available,
                    Err(error) => {
                        self.reload_registry().await?;
                        return Err(error);
                    }
                },
                AgentMarketProtocol::Native => match self.probe_native_health(&agent_id).await {
                    Ok(result) => result.available,
                    Err(error) => {
                        self.reload_registry().await?;
                        return Err(error);
                    }
                },
            };
            summary.checked += 1;
            if available {
                summary.available += 1;
            } else {
                summary.unavailable += 1;
            }
        }
        self.reload_registry().await?;
        Ok(summary)
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
