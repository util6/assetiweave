use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime},
};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::backend::{
    agents::{
        registry::{AgentRegistry, AgentRegistryHandle},
        types::{
            AgentCommandDefinition, AgentConnectionResult, AgentDefinition, AgentEnvEntry, AgentId,
            AgentModelsResult, AgentProtocol, DeclaredAgentCapabilities,
        },
    },
    ai_execution::{
        backends::{
            acp::{
                AcpConnectionStage, AcpExecutionBackend, AcpModelDiscoveryOutcome, AcpProbeReport,
                AcpProtocolConnectionOutcome,
            },
            native::NativeExecutionBackend,
        },
        executor::AgentExecutor,
        AgentExecutionRuntime, AiExecutionCancellation, AiExecutionError,
    },
    extension_kernel::DomainPackageSystem,
};

use super::{
    repository::AgentInstallationRepository,
    types::{
        AgentInstallation, AgentMarketError, AgentMarketProtocol, CatalogCapabilities,
        InstallationStatus, Ownership, ProtocolStatus, RuntimeStatus,
    },
};

const STAGING_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

pub(crate) type AgentRuntimeRegistry = AgentRegistryHandle;

/// Agent Market's domain seam over the installation record. ACP/native
/// manifest details stay here; the kernel only receives the normalized
/// identity and compatibility projection.
pub(crate) struct AgentPackageSystem {
    manifest: super::types::AgentPackageManifest,
}

impl AgentPackageSystem {
    pub(crate) fn from_installation(
        installation: &AgentInstallation,
    ) -> Result<Self, AgentMarketError> {
        Ok(Self {
            manifest: installation.package_manifest()?,
        })
    }
}

impl crate::backend::extension_kernel::DomainPackageSystem for AgentPackageSystem {
    fn kind(&self) -> crate::backend::extension_kernel::PackageKind {
        crate::backend::extension_kernel::PackageKind::Agent
    }

    fn inspect(
        &self,
        dir: &Path,
    ) -> Result<
        crate::backend::extension_kernel::InspectedPackage,
        crate::backend::extension_kernel::ExtensionError,
    > {
        if !dir.exists() {
            return Err(
                crate::backend::extension_kernel::ExtensionError::ManifestInvalid {
                    package_id: self.manifest.identity.package_id.clone(),
                    reason: format!("Agent install directory does not exist: {}", dir.display()),
                },
            );
        }
        Ok(crate::backend::extension_kernel::InspectedPackage {
            identity: self.manifest.identity.clone(),
            compatibility: self.manifest.compatibility.clone(),
            invocation: self.manifest.invocation.clone(),
            availability_probe: self.manifest.availability_probe.clone(),
            model_discovery_probe: self.manifest.model_discovery_probe.clone(),
            install_dir: dir.to_path_buf(),
        })
    }
}

const MODEL_CACHE_CAPACITY: usize = 64;
const MODEL_CACHE_TTL: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct AgentProbeIdentity {
    agent_id: String,
    installation_id: String,
    definition_digest: String,
    enabled: bool,
    executable_present: bool,
}

#[derive(Clone, Debug)]
struct CachedModelProbe {
    identity: AgentProbeIdentity,
    timestamp: Instant,
    last_accessed: Instant,
    result: AgentModelsResult,
}

#[derive(Clone, Debug)]
pub(crate) struct BoundedModelCache {
    entries: HashMap<String, CachedModelProbe>,
    capacity: usize,
}

impl BoundedModelCache {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            capacity,
        }
    }

    pub(crate) fn get(
        &mut self,
        agent_id: &str,
        identity: &AgentProbeIdentity,
    ) -> Option<AgentModelsResult> {
        if let Some(entry) = self.entries.get_mut(agent_id) {
            if entry.identity == *identity && entry.timestamp.elapsed() < MODEL_CACHE_TTL {
                entry.last_accessed = Instant::now();
                return Some(entry.result.clone());
            } else if entry.timestamp.elapsed() >= MODEL_CACHE_TTL || entry.identity != *identity {
                self.entries.remove(agent_id);
            }
        }
        None
    }

    pub(crate) fn insert(
        &mut self,
        agent_id: String,
        identity: AgentProbeIdentity,
        result: AgentModelsResult,
    ) {
        let now = Instant::now();
        if self.entries.len() >= self.capacity && !self.entries.contains_key(&agent_id) {
            if let Some(lru_key) = self
                .entries
                .iter()
                .min_by_key(|(_, v)| v.last_accessed)
                .map(|(k, _)| k.clone())
            {
                self.entries.remove(&lru_key);
            }
        }
        self.entries.insert(
            agent_id,
            CachedModelProbe {
                identity,
                timestamp: now,
                last_accessed: now,
                result,
            },
        );
    }

    pub(crate) fn remove(&mut self, agent_id: &str) {
        self.entries.remove(agent_id);
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Clone, Debug)]
struct SharedProbeError {
    code: String,
    message: String,
    retryable: bool,
    agent_id: String,
}

impl SharedProbeError {
    fn from_error(agent_id: &str, error: &AgentMarketError) -> Self {
        Self {
            code: error.code(),
            message: error.message(),
            retryable: error.retryable(),
            agent_id: agent_id.to_string(),
        }
    }

    fn into_error(self) -> AgentMarketError {
        AgentMarketError::new(&self.code, &self.message, self.retryable)
            .with_agent_id(self.agent_id)
    }
}

type SharedProbeResult = Result<Arc<AcpProbeReport>, SharedProbeError>;

struct ProbeFlight {
    result: tokio::sync::Mutex<Option<SharedProbeResult>>,
    notify: tokio::sync::Notify,
    cancellation: AiExecutionCancellation,
    callers: AtomicUsize,
    persist_health: std::sync::atomic::AtomicBool,
}

#[derive(Clone)]
pub(crate) struct AgentRuntimeManager {
    repository: AgentInstallationRepository,
    registry: AgentRuntimeRegistry,
    registry_snapshot: Arc<crate::backend::extension_kernel::RegistrySnapshot<AgentRegistry>>,
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
        let registry_snapshot = Arc::new(crate::backend::extension_kernel::RegistrySnapshot::new(
            AgentRegistry::from_definitions(Vec::<AgentDefinition>::new())
                .expect("empty agent registry is valid"),
        ));
        let registry = AgentRuntimeRegistry::from_snapshot(registry_snapshot.clone());
        let executor = Arc::new(AgentExecutor::with_registry_handle_and_bindings(
            registry.clone(),
            Arc::new(AcpExecutionBackend::new(workspace_root.clone())),
            Arc::new(NativeExecutionBackend::new(workspace_root.clone())),
            2,
            Arc::new(crate::backend::ai_execution::PersistentBindingStore::new(
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

    async fn reload_registry(&self) -> Result<u64, AgentMarketError> {
        let installations = self.repository.list_registry_candidates().await?;
        let definitions = installations
            .iter()
            .map(|installation| {
                let package_system = AgentPackageSystem::from_installation(installation)?;
                if package_system.kind() != crate::backend::extension_kernel::PackageKind::Agent {
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

    /// Recover only state that can be proven to be owned by the Agent Market.
    /// This performs no network or protocol probes and is safe to call on each
    /// process start before the first registry publication.
    pub(crate) async fn recover_startup(
        &self,
        runtime_root: &Path,
    ) -> Result<Vec<String>, AgentMarketError> {
        let catalog = crate::backend::agent_market::CatalogCache::best_available().ok();
        self.recover_startup_with_catalog(runtime_root, catalog.as_ref())
            .await
    }

    pub(crate) async fn recover_startup_with_catalog(
        &self,
        runtime_root: &Path,
        catalog: Option<&crate::backend::agent_market::catalog::CatalogService>,
    ) -> Result<Vec<String>, AgentMarketError> {
        let installations = self.repository.list().await?;
        let mut warnings = cleanup_runtime_directories(runtime_root);
        for installation in installations {
            if installation.installation_status != InstallationStatus::Ready {
                continue;
            }
            if let Some(catalog) = catalog {
                if let Some(item) = catalog.item(&installation.agent_id) {
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
            }
            let entry_error = if installation.ownership == Ownership::Managed
                && !installation.install_dir.as_ref().is_some_and(|path| {
                    super::is_safe_managed_install_path(
                        runtime_root,
                        &installation.installation_id,
                        path,
                    )
                }) {
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
            };
            let definition_error = if entry_error.is_none()
                && installation.enabled
                && installation.runtime_status == RuntimeStatus::Ready
                && installation.protocol_status == super::types::ProtocolStatus::Ready
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
        self.reload().await?;
        Ok(warnings)
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

    pub(crate) async fn refresh_native_health(
        &self,
        agent_id: &str,
    ) -> Result<AgentConnectionResult, AgentMarketError> {
        let result = self.probe_native_health(agent_id).await?;
        self.reload_registry().await?;
        Ok(result)
    }

    pub(crate) async fn refresh_native_models(
        &self,
        agent_id: &str,
    ) -> Result<AgentModelsResult, AgentMarketError> {
        let health = self.refresh_native_health(agent_id).await?;
        if !health.available {
            return Ok(unavailable_models(
                agent_id,
                health
                    .error_code
                    .as_deref()
                    .unwrap_or("native_connection_failed"),
                health
                    .error
                    .as_deref()
                    .unwrap_or("The native Agent is unavailable."),
            ));
        }

        let mutation_gate = self.mutation_gate(agent_id);
        let _mutation_lease = mutation_gate.write().await;
        let mut installation = self.repository.get(agent_id).await?.ok_or_else(|| {
            AgentMarketError::InstallationNotFound {
                agent_id: agent_id.to_string(),
            }
        })?;
        let now = chrono::Utc::now().to_rfc3339();
        let definition = definition_from_installation(&installation)?;
        let discovery = NativeExecutionBackend::new(self.workspace_root.clone())
            .discover_models(&definition)
            .await;
        let result = match discovery {
            Ok((models, current_model_id)) => {
                installation.model_status = Some("ready".to_string());
                installation.model_error_code = None;
                installation.protocol_status = super::types::ProtocolStatus::Ready;
                installation.protocol_error_code = None;
                installation.protocol_error_message = None;
                AgentModelsResult {
                    agent_id: agent_id.to_string(),
                    available: true,
                    current_model_id: current_model_id
                        .or_else(|| models.first().map(|model| model.id.clone())),
                    models,
                    error_code: None,
                    error: None,
                }
            }
            Err(error) => {
                let message = error.to_view().message;
                installation.model_status = Some("failed".to_string());
                installation.model_error_code = Some("model_discovery_failed".to_string());
                unavailable_models(agent_id, "model_discovery_failed", &message)
            }
        };
        installation.model_checked_at = Some(now.clone());
        installation.protocol_checked_at = Some(now.clone());
        installation.updated_at = now;
        self.repository.update_health(&installation).await?;
        self.reload_registry().await?;
        Ok(result)
    }

    async fn probe_native_health(
        &self,
        agent_id: &str,
    ) -> Result<AgentConnectionResult, AgentMarketError> {
        let mutation_gate = self.mutation_gate(agent_id);
        let _mutation_lease = mutation_gate.write().await;
        let mut installation = self.repository.get(agent_id).await?.ok_or_else(|| {
            AgentMarketError::InstallationNotFound {
                agent_id: agent_id.to_string(),
            }
        })?;
        if installation.protocol != AgentMarketProtocol::Native {
            return Err(AgentMarketError::new(
                "protocol_mismatch",
                "The installed Agent does not use the native runtime.",
                false,
            ));
        }

        let now = chrono::Utc::now().to_rfc3339();
        if !installation.enabled {
            return Ok(unavailable_native_connection(
                agent_id,
                "agent_disabled",
                "The native Agent is disabled.",
            ));
        }
        if !installation.resolved_program.is_file() {
            let runtime_status = if installation.ownership == Ownership::Managed {
                RuntimeStatus::EntryMissing
            } else {
                RuntimeStatus::RuntimeMissing
            };
            mark_native_health_failed(
                &mut installation,
                &now,
                runtime_status,
                "agent_entry_missing",
                "The resolved Agent entry is missing.",
            );
            self.repository.update_health(&installation).await?;
            return Ok(unavailable_native_connection(
                agent_id,
                "agent_entry_missing",
                "The resolved Agent entry is missing.",
            ));
        }

        let definition = match definition_from_installation(&installation) {
            Ok(definition) => definition,
            Err(error) => {
                mark_native_health_failed(
                    &mut installation,
                    &now,
                    RuntimeStatus::Failed,
                    "definition_invalid",
                    &error.to_string(),
                );
                self.repository.update_health(&installation).await?;
                return Ok(unavailable_native_connection(
                    agent_id,
                    "definition_invalid",
                    "The persisted native Agent definition is invalid.",
                ));
            }
        };
        installation.installation_status = InstallationStatus::Ready;
        installation.runtime_status = RuntimeStatus::Ready;
        installation.runtime_error_code = None;
        installation.runtime_error_message = None;
        installation.runtime_checked_at = Some(now.clone());
        let result = match NativeExecutionBackend::new(self.workspace_root.clone())
            .check_connection(&definition)
            .await
        {
            Ok(()) => {
                installation.protocol_status = super::types::ProtocolStatus::Ready;
                installation.protocol_error_code = None;
                installation.protocol_error_message = None;
                installation.model_status = Some("ready".to_string());
                installation.model_error_code = None;
                AgentConnectionResult {
                    agent_id: agent_id.to_string(),
                    available: true,
                    installed: true,
                    connected: true,
                    version: Some(installation.agent_version.clone()),
                    connection_method: Some("native".to_string()),
                    error_code: None,
                    error: None,
                    installation_status: None,
                    runtime_status: None,
                    protocol_status: None,
                    execution_ready: false,
                    health_stale: false,
                }
            }
            Err(error) => {
                let message = error.to_view().message;
                installation.protocol_status = super::types::ProtocolStatus::Failed;
                installation.protocol_error_code = Some("native_connection_failed".to_string());
                installation.protocol_error_message = Some(message.clone());
                installation.model_status = Some("failed".to_string());
                installation.model_error_code = Some("native_connection_failed".to_string());
                unavailable_native_connection(agent_id, "native_connection_failed", &message)
            }
        };
        installation.protocol_checked_at = Some(now.clone());
        installation.model_checked_at = Some(now.clone());
        installation.updated_at = now;
        self.repository.update_health(&installation).await?;
        Ok(result)
    }

    pub(crate) async fn refresh_acp_connection(
        &self,
        agent_id: &str,
    ) -> Result<AgentConnectionResult, AgentMarketError> {
        let installation = self.current_acp_installation(agent_id).await?;
        if !installation.enabled {
            return Ok(unavailable_acp_connection(
                agent_id,
                "agent_disabled",
                "The ACP Agent is disabled.",
            ));
        }
        if !installation.resolved_program.is_file() {
            return Ok(unavailable_acp_connection(
                agent_id,
                "agent_entry_missing",
                "The resolved Agent entry is missing.",
            ));
        }
        if let Err(error) = definition_from_installation(&installation) {
            return Ok(unavailable_acp_connection(
                agent_id,
                "definition_invalid",
                &error.to_string(),
            ));
        }
        let identity = probe_identity(&installation);
        let report = self.run_acp_probe(agent_id, identity, true, true).await?;
        self.reload_registry().await?;
        let current_inst = self.repository.get(agent_id).await?.unwrap_or(installation);
        Ok(report.to_connection_result(
            agent_id,
            Some(&current_inst.agent_version),
            Some(current_inst.installation_status.as_str()),
            Some(current_inst.runtime_status.as_str()),
            Some(current_inst.protocol_status.as_str()),
        ))
    }

    pub(crate) async fn refresh_acp_health(
        &self,
        agent_id: &str,
    ) -> Result<AgentModelsResult, AgentMarketError> {
        let installation = self.current_acp_installation(agent_id).await?;
        let identity = probe_identity(&installation);
        let report = self.run_acp_probe(agent_id, identity, true, true).await?;
        self.reload_registry().await?;
        Ok(report.to_models_result(agent_id))
    }

    #[cfg(test)]
    pub(crate) async fn probe_acp_health(
        &self,
        agent_id: &str,
    ) -> Result<AgentModelsResult, AgentMarketError> {
        let installation = self.current_acp_installation(agent_id).await?;
        let identity = probe_identity(&installation);
        let report = self.run_acp_probe(agent_id, identity, true, true).await?;
        Ok(report.to_models_result(agent_id))
    }

    pub(crate) async fn get_or_refresh_acp_models(
        &self,
        agent_id: &str,
    ) -> Result<AgentModelsResult, AgentMarketError> {
        let installation = self.current_acp_installation(agent_id).await?;
        if !installation.enabled {
            return Ok(unavailable_models(
                agent_id,
                "agent_disabled",
                "The ACP Agent is disabled.",
            ));
        }
        if !installation.resolved_program.is_file() {
            return Ok(unavailable_models(
                agent_id,
                "agent_entry_missing",
                "The resolved Agent entry is missing.",
            ));
        }
        if let Err(error) = definition_from_installation(&installation) {
            return Ok(unavailable_models(
                agent_id,
                "definition_invalid",
                &error.to_string(),
            ));
        }
        let identity = probe_identity(&installation);
        if let Some(result) = self.cached_acp_models(agent_id, &identity).await {
            return Ok(result);
        }
        let report = self.run_acp_probe(agent_id, identity, false, false).await?;
        Ok(report.to_models_result(agent_id))
    }

    pub(crate) async fn invalidate_agent_state(&self, agent_id: &str) {
        self.models_cache.write().await.remove(agent_id);
        let flights = self.probe_flights.lock().await;
        for (identity, flight) in flights.iter() {
            if identity.agent_id == agent_id {
                flight.cancellation.cancel();
            }
        }
    }

    pub(crate) async fn cancel_acp_probe(&self, agent_id: &str) -> bool {
        let flights = self.probe_flights.lock().await;
        let mut cancelled = false;
        for (identity, flight) in flights.iter() {
            if identity.agent_id == agent_id {
                flight.cancellation.cancel();
                cancelled = true;
            }
        }
        drop(flights);
        self.models_cache.write().await.remove(agent_id);
        cancelled
    }

    pub(crate) async fn release_acp_probe_caller(&self, agent_id: &str) -> bool {
        let flights = self.probe_flights.lock().await;
        let mut cancelled = false;
        for (identity, flight) in flights.iter() {
            if identity.agent_id != agent_id {
                continue;
            }
            let previous = flight.callers.load(Ordering::Acquire);
            if previous > 0 && flight.callers.fetch_sub(1, Ordering::AcqRel) == 1 {
                flight.cancellation.cancel();
                cancelled = true;
            }
        }
        cancelled
    }

    async fn invalidate_all_acp_probe_state(&self) {
        self.models_cache.write().await.clear();
        let flights = self.probe_flights.lock().await;
        for flight in flights.values() {
            flight.cancellation.cancel();
        }
    }

    async fn current_acp_installation(
        &self,
        agent_id: &str,
    ) -> Result<AgentInstallation, AgentMarketError> {
        let installation = self.repository.get(agent_id).await?.ok_or_else(|| {
            AgentMarketError::InstallationNotFound {
                agent_id: agent_id.to_string(),
            }
        })?;
        if installation.protocol != AgentMarketProtocol::Acp {
            return Err(AgentMarketError::new(
                "protocol_mismatch",
                "The installed Agent does not use ACP.",
                false,
            ));
        }
        Ok(installation)
    }

    async fn cached_acp_models(
        &self,
        agent_id: &str,
        identity: &AgentProbeIdentity,
    ) -> Option<AgentModelsResult> {
        let mut cache = self.models_cache.write().await;
        cache.get(agent_id, identity)
    }

    async fn run_acp_probe(
        &self,
        agent_id: &str,
        identity: AgentProbeIdentity,
        _force_refresh: bool,
        persist_health: bool,
    ) -> Result<Arc<AcpProbeReport>, AgentMarketError> {
        let (flight, leader) = {
            let mut flights = self.probe_flights.lock().await;
            if let Some(flight) = flights.get(&identity) {
                flight.callers.fetch_add(1, Ordering::AcqRel);
                if persist_health {
                    flight.persist_health.store(true, Ordering::Release);
                }
                (Arc::clone(flight), false)
            } else {
                let flight = Arc::new(ProbeFlight {
                    result: tokio::sync::Mutex::new(None),
                    notify: tokio::sync::Notify::new(),
                    cancellation: AiExecutionCancellation::default(),
                    callers: AtomicUsize::new(1),
                    persist_health: std::sync::atomic::AtomicBool::new(persist_health),
                });
                flights.insert(identity.clone(), Arc::clone(&flight));
                (flight, true)
            }
        };

        if leader {
            let raw_outcome = self
                .probe_acp_health_uncached(agent_id, identity.clone(), flight.cancellation.clone())
                .await;

            let outcome: Result<Arc<AcpProbeReport>, AgentMarketError> = match raw_outcome {
                Ok(report) => {
                    if matches!(
                        report.protocol_connection,
                        AcpProtocolConnectionOutcome::Cancelled
                    ) {
                        Err(AgentMarketError::new(
                            "cancelled",
                            "The ACP probe was cancelled.",
                            true,
                        )
                        .with_agent_id(agent_id))
                    } else {
                        if flight.persist_health.load(Ordering::Acquire) {
                            self.persist_acp_probe_health(agent_id, &identity, &report)
                                .await;
                        }
                        if let AcpModelDiscoveryOutcome::Success { .. } = &report.model_discovery {
                            let models_result = report.to_models_result(agent_id);
                            self.models_cache.write().await.insert(
                                agent_id.to_string(),
                                identity.clone(),
                                models_result,
                            );
                        }
                        Ok(Arc::new(report))
                    }
                }
                Err(error) => Err(error),
            };

            let shared = outcome
                .as_ref()
                .map(|report| Arc::clone(report))
                .map_err(|error| SharedProbeError::from_error(agent_id, error));
            *flight.result.lock().await = Some(shared);
            flight.notify.notify_waiters();
            self.probe_flights.lock().await.remove(&identity);
            outcome
        } else {
            loop {
                if let Some(result) = flight.result.lock().await.clone() {
                    return result.map_err(SharedProbeError::into_error);
                }
                flight.notify.notified().await;
            }
        }
    }

    async fn probe_acp_health_uncached(
        &self,
        agent_id: &str,
        expected_identity: AgentProbeIdentity,
        cancellation: AiExecutionCancellation,
    ) -> Result<AcpProbeReport, AgentMarketError> {
        if cancellation.is_cancelled() {
            return Err(AgentMarketError::new(
                "cancelled",
                "The ACP probe was cancelled.",
                true,
            ));
        }

        let installation = self.repository.get(agent_id).await?.ok_or_else(|| {
            AgentMarketError::InstallationNotFound {
                agent_id: agent_id.to_string(),
            }
        })?;
        if installation.protocol != AgentMarketProtocol::Acp {
            return Err(AgentMarketError::new(
                "protocol_mismatch",
                "The installed Agent does not use ACP.",
                false,
            ));
        }

        if !installation.enabled {
            return Ok(AcpProbeReport {
                protocol_connection: AcpProtocolConnectionOutcome::Failed {
                    stage: AcpConnectionStage::Spawn,
                    error_code: "agent_disabled".to_string(),
                    error_message: "The ACP Agent is disabled.".to_string(),
                },
                model_discovery: AcpModelDiscoveryOutcome::Skipped,
                cleanup: crate::backend::ai_execution::backends::acp::AcpCleanupOutcome {
                    process_reaped: true,
                    workspace_removed: true,
                    timed_out: false,
                    failures: Vec::new(),
                },
                timings: crate::backend::ai_execution::backends::acp::AcpProbeTimings::default(),
            });
        }

        if !installation.resolved_program.is_file() {
            return Ok(AcpProbeReport {
                protocol_connection: AcpProtocolConnectionOutcome::Failed {
                    stage: AcpConnectionStage::Spawn,
                    error_code: "agent_entry_missing".to_string(),
                    error_message: "The resolved Agent entry is missing.".to_string(),
                },
                model_discovery: AcpModelDiscoveryOutcome::Skipped,
                cleanup: crate::backend::ai_execution::backends::acp::AcpCleanupOutcome {
                    process_reaped: true,
                    workspace_removed: true,
                    timed_out: false,
                    failures: Vec::new(),
                },
                timings: crate::backend::ai_execution::backends::acp::AcpProbeTimings::default(),
            });
        }

        let definition = match definition_from_installation(&installation) {
            Ok(definition) => definition,
            Err(error) => {
                return Ok(AcpProbeReport {
                    protocol_connection: AcpProtocolConnectionOutcome::Failed {
                        stage: AcpConnectionStage::Spawn,
                        error_code: "definition_invalid".to_string(),
                        error_message: error.to_string(),
                    },
                    model_discovery: AcpModelDiscoveryOutcome::Skipped,
                    cleanup: crate::backend::ai_execution::backends::acp::AcpCleanupOutcome {
                        process_reaped: true,
                        workspace_removed: true,
                        timed_out: false,
                        failures: Vec::new(),
                    },
                    timings: crate::backend::ai_execution::backends::acp::AcpProbeTimings::default(
                    ),
                });
            }
        };

        let identity = probe_identity(&installation);
        if identity != expected_identity {
            tracing::debug!(
                action = "agent_market.acp_probe.identity_changed",
                agent_id,
                "ACP probe identity changed before the process started"
            );
        }

        let backend = AcpExecutionBackend::new(self.workspace_root.clone());
        let report = backend
            .probe_connection_and_models(&definition, cancellation)
            .await;
        Ok(report)
    }

    async fn persist_acp_probe_health(
        &self,
        agent_id: &str,
        expected_identity: &AgentProbeIdentity,
        report: &AcpProbeReport,
    ) {
        if matches!(
            report.protocol_connection,
            AcpProtocolConnectionOutcome::Cancelled
        ) {
            return;
        }

        let mut installation = match self.repository.get(agent_id).await {
            Ok(Some(inst)) => inst,
            _ => return,
        };

        let current_identity = probe_identity(&installation);
        if current_identity != *expected_identity {
            tracing::warn!(
                agent_id,
                "ACP probe finished but installation identity changed; skipped health persistence"
            );
            return;
        }

        let now = chrono::Utc::now().to_rfc3339();
        installation.runtime_checked_at = Some(now.clone());
        installation.protocol_checked_at = Some(now.clone());
        installation.model_checked_at = Some(now.clone());
        installation.updated_at = now.clone();

        match &report.protocol_connection {
            AcpProtocolConnectionOutcome::Connected => {
                installation.installation_status = InstallationStatus::Ready;
                installation.runtime_status = RuntimeStatus::Ready;
                installation.runtime_error_code = None;
                installation.runtime_error_message = None;
                installation.protocol_status = ProtocolStatus::Ready;
                installation.protocol_error_code = None;
                installation.protocol_error_message = None;

                match &report.model_discovery {
                    AcpModelDiscoveryOutcome::Success { .. } => {
                        installation.model_status = Some("ready".to_string());
                        installation.model_error_code = None;
                    }
                    AcpModelDiscoveryOutcome::Empty => {
                        installation.model_status = Some("unsupported".to_string());
                        installation.model_error_code = Some("model_list_empty".to_string());
                    }
                    AcpModelDiscoveryOutcome::Invalid { error_code, .. } => {
                        installation.model_status = Some("failed".to_string());
                        installation.model_error_code = Some(error_code.clone());
                    }
                    AcpModelDiscoveryOutcome::Timeout => {
                        installation.model_status = Some("failed".to_string());
                        installation.model_error_code = Some("model_discovery_timeout".to_string());
                    }
                    AcpModelDiscoveryOutcome::Unsupported => {
                        installation.model_status = Some("unsupported".to_string());
                        installation.model_error_code = Some("unsupported".to_string());
                    }
                    AcpModelDiscoveryOutcome::Failed { error_code, .. } => {
                        installation.model_status = Some("failed".to_string());
                        installation.model_error_code = Some(error_code.clone());
                    }
                    AcpModelDiscoveryOutcome::Skipped => {}
                }
            }
            AcpProtocolConnectionOutcome::Failed {
                stage,
                error_code,
                error_message,
            } => {
                if matches!(stage, AcpConnectionStage::Spawn) {
                    installation.installation_status = InstallationStatus::Broken;
                    installation.runtime_status = RuntimeStatus::Failed;
                    installation.runtime_error_code = Some(error_code.clone());
                    installation.runtime_error_message = Some(error_message.clone());
                }
                if error_code == "auth_required" {
                    installation.protocol_status = ProtocolStatus::AuthRequired;
                } else {
                    installation.protocol_status = ProtocolStatus::Failed;
                }
                installation.protocol_error_code = Some(error_code.clone());
                installation.protocol_error_message = Some(error_message.clone());
                installation.model_status = Some("failed".to_string());
                installation.model_error_code = Some(error_code.clone());
            }
            AcpProtocolConnectionOutcome::Cancelled => {}
        }

        if let Err(error) = self.repository.update_health(&installation).await {
            tracing::error!(
                agent_id,
                error = %error,
                "Failed to persist ACP health to repository"
            );
        }
    }
}

fn unavailable_acp_connection(agent_id: &str, code: &str, message: &str) -> AgentConnectionResult {
    AgentConnectionResult {
        agent_id: agent_id.to_string(),
        available: false,
        installed: true,
        connected: false,
        version: None,
        connection_method: Some("acp".to_string()),
        error_code: Some(code.to_string()),
        error: Some(message.to_string()),
        installation_status: None,
        runtime_status: None,
        protocol_status: None,
        execution_ready: false,
        health_stale: false,
    }
}

fn probe_identity(installation: &AgentInstallation) -> AgentProbeIdentity {
    let mut digest = Sha256::new();
    digest.update(installation.agent_id.as_bytes());
    digest.update([0]);
    digest.update(installation.installation_id.as_bytes());
    digest.update([0]);
    digest.update(installation.catalog_item_version.as_bytes());
    digest.update([0]);
    digest.update(installation.agent_version.as_bytes());
    digest.update([0]);
    digest.update(installation.distribution_id.as_bytes());
    digest.update([0]);
    digest.update(installation.definition_json.to_string().as_bytes());
    digest.update([0]);
    digest.update(installation.resolved_program.to_string_lossy().as_bytes());
    digest.update([0]);
    for arg in &installation.args {
        digest.update(arg.as_bytes());
        digest.update([0]);
    }
    digest.update([installation.enabled as u8]);
    digest.update([installation.resolved_program.is_file() as u8]);
    AgentProbeIdentity {
        agent_id: installation.agent_id.clone(),
        installation_id: installation.installation_id.clone(),
        definition_digest: format!("{:x}", digest.finalize()),
        enabled: installation.enabled,
        executable_present: installation.resolved_program.is_file(),
    }
}

fn model_error_code(error: &AiExecutionError) -> &'static str {
    match error {
        AiExecutionError::Protocol {
            operation: "session_model_catalog_empty",
        } => "model_list_empty",
        AiExecutionError::Protocol {
            operation: "session_model_catalog_invalid",
        } => "model_catalog_invalid",
        AiExecutionError::Timeout { .. }
        | AiExecutionError::Protocol {
            operation: "session_new_timeout" | "spawn_timeout",
        } => "model_discovery_timeout",
        _ => "model_discovery_failed",
    }
}

fn is_auth_error(error: &AiExecutionError) -> bool {
    match error {
        AiExecutionError::ProtocolDetail { detail, .. } => is_auth_message(detail),
        AiExecutionError::Output { message } => is_auth_message(message),
        _ => false,
    }
}

fn is_auth_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("auth")
        || lower.contains("login")
        || lower.contains("sign in")
        || lower.contains("unauthorized")
        || lower.contains("unauthenticated")
        || lower.contains("credential")
}

fn unavailable_models(agent_id: &str, code: &str, message: &str) -> AgentModelsResult {
    AgentModelsResult {
        agent_id: agent_id.to_string(),
        available: false,
        models: Vec::new(),
        current_model_id: None,
        error_code: Some(code.to_string()),
        error: Some(message.to_string()),
    }
}

fn unavailable_native_connection(
    agent_id: &str,
    code: &str,
    message: &str,
) -> AgentConnectionResult {
    AgentConnectionResult {
        agent_id: agent_id.to_string(),
        available: false,
        installed: true,
        connected: false,
        version: None,
        connection_method: Some("native".to_string()),
        error_code: Some(code.to_string()),
        error: Some(message.to_string()),
        installation_status: None,
        runtime_status: None,
        protocol_status: None,
        execution_ready: false,
        health_stale: false,
    }
}

fn mark_native_health_failed(
    installation: &mut AgentInstallation,
    checked_at: &str,
    runtime_status: RuntimeStatus,
    error_code: &str,
    message: &str,
) {
    installation.installation_status = InstallationStatus::Broken;
    installation.runtime_status = runtime_status;
    installation.runtime_error_code = Some(error_code.to_string());
    installation.runtime_error_message = Some(message.to_string());
    installation.runtime_checked_at = Some(checked_at.to_string());
    installation.protocol_status = ProtocolStatus::Failed;
    installation.protocol_error_code = Some(error_code.to_string());
    installation.protocol_error_message = Some(message.to_string());
    installation.protocol_checked_at = Some(checked_at.to_string());
    installation.model_status = Some("failed".to_string());
    installation.model_error_code = Some(error_code.to_string());
    installation.model_checked_at = Some(checked_at.to_string());
    installation.updated_at = checked_at.to_string();
}

fn model_discovery_error_message(error: &AiExecutionError) -> String {
    if matches!(
        error,
        AiExecutionError::Protocol {
            operation: "session_model_catalog_empty"
        }
    ) {
        "The ACP Agent did not return a usable model list.".to_string()
    } else {
        error.to_view().message
    }
}

fn cleanup_runtime_directories(runtime_root: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    let now = SystemTime::now();
    let staging = runtime_root.join(".staging");
    if let Ok(entries) = std::fs::read_dir(&staging) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_old = entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age > STAGING_RETENTION);
            if is_old
                && entry
                    .file_type()
                    .map(|file_type| file_type.is_dir() && !file_type.is_symlink())
                    .unwrap_or(false)
            {
                if let Err(error) = std::fs::remove_dir_all(&path) {
                    warnings.push(format!("stale staging cleanup pending: {error}"));
                }
            }
        }
    }
    // The runtime root is shared by desktop, CLI, tests, and selectable database
    // paths. An installation absent from this database may still be owned by a
    // different database, so startup cleanup must never infer that an active
    // directory is orphaned. Active installations are removed only by explicit
    // lifecycle rollback, update, and uninstall operations with an exact ID.
    warnings
}

#[derive(Debug, Deserialize)]
struct ResolvedDefinition {
    id: String,
    display_name: String,
    protocol: String,
    #[serde(default)]
    capabilities: Option<CatalogCapabilities>,
    #[serde(default, alias = "sessionCleanupArgs")]
    session_cleanup_args: Option<Vec<String>>,
    #[serde(default, alias = "sessionCleanupNotFoundMarkers")]
    session_cleanup_not_found_markers: Vec<String>,
}

#[cfg(test)]
pub(crate) fn definition_json(
    agent_id: &str,
    display_name: &str,
    protocol: &AgentMarketProtocol,
    program: &std::path::Path,
    args: &[String],
) -> serde_json::Value {
    serde_json::json!({
        "id": agent_id,
        "display_name": display_name,
        "protocol": protocol.as_str(),
        "program": program.to_string_lossy(),
        "args": args,
        "env": [],
    })
}

pub(crate) fn definition_from_installation(
    installation: &AgentInstallation,
) -> Result<AgentDefinition, AgentMarketError> {
    let package_manifest = installation.package_manifest()?;
    let resolved: ResolvedDefinition = serde_json::from_value(installation.definition_json.clone())
        .map_err(AgentMarketError::Serialization)?;
    if package_manifest.identity.package_id != resolved.id {
        return Err(AgentMarketError::new(
            "definition_id_mismatch",
            "resolved definition id does not match package identity",
            false,
        ));
    }
    if package_manifest.compatibility.protocol_version != 1 {
        return Err(AgentMarketError::new(
            "unsupported_protocol_version",
            "unsupported Agent package protocol version",
            false,
        ));
    }
    let id = AgentId::parse(resolved.id)
        .map_err(|error| AgentMarketError::new("invalid_agent_id", &error.to_string(), false))?;
    let protocol = match resolved.protocol.as_str() {
        "acp" => AgentProtocol::Acp,
        "native" => AgentProtocol::Native,
        other => {
            return Err(AgentMarketError::new(
                "unsupported_agent_protocol",
                &format!("unsupported agent protocol: {other}"),
                false,
            ))
        }
    };
    let invocation = package_manifest.invocation;
    let availability_probe = package_manifest.availability_probe;
    let model_discovery_probe = package_manifest.model_discovery_probe;
    let program = invocation.entry.clone();
    let expected_program = installation.resolved_program.to_string_lossy();
    if program != expected_program {
        return Err(AgentMarketError::new(
            "definition_program_mismatch",
            "resolved definition program does not match installation record",
            false,
        ));
    }
    let program_path = std::path::PathBuf::from(&program);
    if !program_path.is_file() {
        return Err(AgentMarketError::new(
            "agent_program_missing",
            "resolved Agent program is missing",
            false,
        ));
    }
    if installation.ownership == super::types::Ownership::Managed {
        let install_dir = installation.install_dir.as_ref().ok_or_else(|| {
            AgentMarketError::new(
                "missing_install_dir",
                "managed Agent is missing install directory",
                false,
            )
        })?;
        if !program_path.starts_with(install_dir) {
            return Err(AgentMarketError::new(
                "program_escapes_install_dir",
                "resolved Agent program escapes its managed installation",
                false,
            ));
        }
    } else if installation.install_dir.is_some() {
        return Err(AgentMarketError::new(
            "invalid_system_install_dir",
            "system Agent must not have a managed installation directory",
            false,
        ));
    }
    if invocation
        .args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-y" | "npx" | "uvx"))
        || program_path
            .file_name()
            .is_some_and(|name| matches!(name.to_string_lossy().as_ref(), "npx" | "uvx"))
    {
        return Err(AgentMarketError::new(
            "package_manager_invocation_forbidden",
            "runtime definition may not invoke a package manager",
            false,
        ));
    }
    let definition = AgentDefinition {
        id,
        installation_id: Some(installation.installation_id.clone()),
        display_name: resolved.display_name,
        protocol,
        command: program.clone(),
        args: invocation.args,
        env: invocation
            .env
            .into_iter()
            .map(|entry| AgentEnvEntry::new(entry.key, entry.value))
            .collect(),
        declared_capabilities: declared_capabilities_from_catalog(
            resolved.capabilities.as_ref(),
            protocol,
        ),
        availability_probe: Some(AgentCommandDefinition {
            command: availability_probe.program,
            args: availability_probe.args,
        }),
        model_discovery: model_discovery_probe.map(|probe| AgentCommandDefinition {
            command: probe.program,
            args: probe.args,
        }),
        session_cleanup: resolved
            .session_cleanup_args
            .map(AgentCommandDefinition::new),
        session_cleanup_not_found_markers: resolved.session_cleanup_not_found_markers,
    };
    definition
        .validate()
        .map_err(|error| AgentMarketError::new("definition_invalid", &error.to_string(), false))?;
    Ok(definition)
}

fn declared_capabilities_from_catalog(
    capabilities: Option<&CatalogCapabilities>,
    protocol: AgentProtocol,
) -> DeclaredAgentCapabilities {
    let market_protocol = match protocol {
        AgentProtocol::Acp => AgentMarketProtocol::Acp,
        AgentProtocol::Native => AgentMarketProtocol::Native,
    };
    capabilities
        .map(|value| value.to_declared_agent_capabilities(&market_protocol))
        .unwrap_or_else(|| {
            CatalogCapabilities::fallback_for_protocol(&market_protocol)
                .to_declared_agent_capabilities(&market_protocol)
        })
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
