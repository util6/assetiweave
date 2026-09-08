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
            .filter(|installation| installation.enabled)
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
mod tests {
    use super::*;
    use crate::backend::agent_market::types::{
        DistributionType, InstallationStatus, Ownership, ProtocolStatus, RuntimeStatus,
    };
    use crate::backend::agents::types::AgentModelOption;

    #[test]
    fn failed_publish_preserves_the_previous_complete_snapshot() {
        let registry = AgentRuntimeRegistry::default();
        let definition = AgentDefinition {
            id: AgentId::parse("agent").unwrap(),
            installation_id: None,
            display_name: "Agent".to_string(),
            protocol: AgentProtocol::Acp,
            command: "/tmp/agent".to_string(),
            args: Vec::new(),
            env: Vec::new(),
            declared_capabilities: DeclaredAgentCapabilities::acp_text(),
            availability_probe: None,
            model_discovery: None,
            session_cleanup: None,
            session_cleanup_not_found_markers: Vec::new(),
        };
        registry.publish(vec![definition]).unwrap();
        let generation = registry.generation();
        let invalid = AgentDefinition {
            id: AgentId::parse("bad").unwrap(),
            installation_id: None,
            display_name: String::new(),
            protocol: AgentProtocol::Acp,
            command: "/tmp/bad".to_string(),
            args: Vec::new(),
            env: Vec::new(),
            declared_capabilities: DeclaredAgentCapabilities::acp_text(),
            availability_probe: None,
            model_discovery: None,
            session_cleanup: None,
            session_cleanup_not_found_markers: Vec::new(),
        };
        assert!(registry.publish(vec![invalid]).is_err());
        assert_eq!(registry.generation(), generation);
        assert!(registry
            .snapshot()
            .get(&AgentId::parse("agent").unwrap())
            .is_some());
    }

    #[test]
    fn installation_definition_is_local_and_does_not_contain_package_manager_invocation() {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-agent-runtime-{}",
            uuid::Uuid::new_v4()
        ));
        let program_path = root.join("bin").join("agent");
        std::fs::create_dir_all(program_path.parent().unwrap()).unwrap();
        std::fs::write(&program_path, b"#!/bin/sh\nexit 0\n").unwrap();
        let mut resolved_definition = definition_json(
            "agent",
            "Agent",
            &AgentMarketProtocol::Acp,
            &program_path,
            &["acp".to_string()],
        );
        resolved_definition["capabilities"] = serde_json::json!({
            "textPrompt": true,
            "resume": true,
            "historyReplay": true,
            "liveEvents": true,
            "richHistoryReplay": true,
            "teamTools": true,
        });
        resolved_definition["sessionCleanupArgs"] =
            serde_json::json!(["session", "delete", "{session_id}"]);
        resolved_definition["sessionCleanupNotFoundMarkers"] =
            serde_json::json!(["Session not found:"]);
        let installation = AgentInstallation {
            agent_id: "agent".to_string(),
            installation_id: "id".to_string(),
            display_name: "Agent".to_string(),
            catalog_item_version: "1.0.0".to_string(),
            agent_version: "1.0.0".to_string(),
            protocol: AgentMarketProtocol::Acp,
            distribution_id: "npx".to_string(),
            distribution_type: DistributionType::Npx,
            ownership: Ownership::Managed,
            install_dir: Some(root.clone()),
            resolved_program: program_path.clone(),
            args: vec!["acp".to_string()],
            definition_json: resolved_definition,
            integrity_json: None,
            source_registry: "agent".to_string(),
            catalog_version: "catalog".to_string(),
            enabled: true,
            installation_status: InstallationStatus::Ready,
            runtime_status: RuntimeStatus::Ready,
            runtime_error_code: None,
            runtime_error_message: None,
            runtime_checked_at: None,
            protocol_status: ProtocolStatus::Ready,
            protocol_error_code: None,
            protocol_error_message: None,
            protocol_checked_at: None,
            model_status: None,
            model_error_code: None,
            model_checked_at: None,
            installed_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        let definition = definition_from_installation(&installation).unwrap();
        assert_eq!(definition.command, program_path.to_string_lossy());
        assert!(definition.declared_capabilities.resume);
        assert!(definition.declared_capabilities.history_replay);
        assert!(definition.declared_capabilities.live_events);
        assert!(definition.declared_capabilities.rich_history_replay);
        assert_eq!(
            definition
                .session_cleanup
                .as_ref()
                .map(|cleanup| &cleanup.args),
            Some(&vec![
                "session".to_string(),
                "delete".to_string(),
                "{session_id}".to_string()
            ])
        );
        assert_eq!(
            definition.session_cleanup_not_found_markers,
            ["Session not found:"]
        );
        assert!(!definition
            .args
            .iter()
            .any(|arg| arg == "-y" || arg == "npx" || arg == "uvx"));
        let package_system = AgentPackageSystem::from_installation(&installation).unwrap();
        let inspected = package_system.inspect(&root).unwrap();
        assert_eq!(inspected.identity, installation.package_identity().unwrap());
        assert_eq!(inspected.invocation.entry, program_path.to_string_lossy());
        assert_eq!(
            inspected.availability_probe.args,
            vec!["--version".to_string()]
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn startup_cleanup_preserves_active_installations_owned_by_another_database() {
        let runtime_root = std::env::temp_dir().join(format!(
            "assetiweave-agent-runtime-shared-{}",
            uuid::Uuid::new_v4()
        ));
        let installation_id = uuid::Uuid::new_v4().to_string();
        let active_install = runtime_root.join("active").join(&installation_id);
        std::fs::create_dir_all(&active_install).expect("create shared active installation");
        std::fs::write(active_install.join("agent"), b"persisted")
            .expect("write shared active installation");

        let warnings = cleanup_runtime_directories(&runtime_root);

        assert!(warnings.is_empty());
        assert!(active_install.join("agent").is_file());
        let _ = std::fs::remove_dir_all(runtime_root);
    }

    async fn acp_test_fixture_with_id(
        agent_id: &str,
        mode: &str,
    ) -> (
        AgentRuntimeManager,
        AgentInstallationRepository,
        std::path::PathBuf,
    ) {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-acp-{}-{}",
            agent_id,
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let db_path = root.join("app.db");
        let db = crate::backend::store::Database::open_initialized_async(&db_path)
            .await
            .expect("open db");
        let pool = db.pool().clone();
        let repository = AgentInstallationRepository::new(pool.clone());
        let program =
            crate::backend::host_process::resolve_host_executable("node").expect("node executable");
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-fixtures/fake-acp-agent.mjs");
        let record = root.join("record.log");
        let args = vec![
            fixture.to_string_lossy().to_string(),
            format!("--mode={mode}"),
            format!("--record={}", record.to_string_lossy()),
        ];
        let now = chrono::Utc::now().to_rfc3339();
        let installation = AgentInstallation {
            agent_id: agent_id.to_string(),
            installation_id: uuid::Uuid::new_v4().to_string(),
            display_name: format!("{agent_id} Display"),
            catalog_item_version: "1.0.0".to_string(),
            agent_version: "1.0.0".to_string(),
            protocol: AgentMarketProtocol::Acp,
            distribution_id: "test-distribution".to_string(),
            distribution_type: DistributionType::System,
            ownership: Ownership::System,
            install_dir: None,
            resolved_program: program.clone(),
            args: args.clone(),
            definition_json: serde_json::json!({
                "id": agent_id,
                "display_name": format!("{agent_id} Display"),
                "protocol": "acp",
                "program": program.to_string_lossy(),
                "args": args,
                "env": [
                    { "name": "ASSETIWEAVE_FAKE_ACP_MODE", "value": mode },
                    { "name": "ASSETIWEAVE_FAKE_ACP_RECORD_PATH", "value": record.to_string_lossy() }
                ],
            }),
            integrity_json: None,
            source_registry: "test".to_string(),
            catalog_version: "1.0".to_string(),
            enabled: true,
            installation_status: InstallationStatus::Ready,
            runtime_status: RuntimeStatus::Ready,
            runtime_error_code: None,
            runtime_error_message: None,
            runtime_checked_at: Some(now.clone()),
            protocol_status: ProtocolStatus::Ready,
            protocol_error_code: None,
            protocol_error_message: None,
            protocol_checked_at: Some(now.clone()),
            model_status: None,
            model_error_code: None,
            model_checked_at: None,
            installed_at: now.clone(),
            updated_at: now,
        };
        repository.upsert_active(&installation).await.unwrap();
        let manager = AgentRuntimeManager::new(pool, root.join("workspaces"));
        (manager, repository, root)
    }

    async fn acp_test_fixture(
        mode: &str,
    ) -> (
        AgentRuntimeManager,
        AgentInstallationRepository,
        std::path::PathBuf,
    ) {
        acp_test_fixture_with_id("test-agent", mode).await
    }

    fn test_request(
        agent_id: &str,
        model: Option<&str>,
    ) -> crate::backend::ai_execution::AiExecutionRequest {
        use crate::backend::ai_execution::*;
        AiExecutionRequest {
            execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
            agent_id: AgentId::parse(agent_id).expect("valid agent id"),
            purpose: AiExecutionPurpose::Translation,
            session_mode: AgentSessionMode::OneShot,
            prompt: "Please translate: Hello world".to_string(),
            model: model.map(|m| m.to_string()),
            limits: AiExecutionLimits {
                total_timeout: Duration::from_secs(10),
                spawn_timeout: Duration::from_secs(10),
                initialize_timeout: Duration::from_secs(5),
                config_rpc_timeout: Duration::from_secs(5),
                cancel_grace: Duration::from_secs(2),
                close_timeout: Duration::from_secs(2),
                cleanup_timeout: Duration::from_secs(10),
                text_bytes: 1024 * 1024,
                stderr_bytes: 64 * 1024,
            },
            cancellation: AiExecutionCancellation::default(),
            progress: None,
            tenant_id: None,
            execution_context_key: None,
            binding: None,
            replay: false,
            restore_only: false,
            team_tools: None,
            recall_tools: None,
        }
    }

    fn read_record_events(record_file: &Path) -> Vec<serde_json::Value> {
        if !record_file.is_file() {
            return Vec::new();
        }
        let content = std::fs::read_to_string(record_file).unwrap_or_default();
        content
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .collect()
    }

    fn assert_clean_workspaces(workspaces_dir: &Path) {
        if workspaces_dir.exists() {
            let entries = std::fs::read_dir(workspaces_dir)
                .unwrap()
                .filter_map(|e| e.ok())
                .collect::<Vec<_>>();
            assert!(
                entries.is_empty(),
                "expected clean workspace directory, found: {:?}",
                entries.iter().map(|e| e.path()).collect::<Vec<_>>()
            );
        }
    }

    #[tokio::test]
    async fn acp_model_requests_share_one_probe_and_force_refresh_bypasses_cache() {
        let (manager, repository, root) = acp_test_fixture("happy").await;

        let (first, second) = tokio::join!(
            manager.get_or_refresh_acp_models("test-agent"),
            manager.get_or_refresh_acp_models("test-agent"),
        );
        assert!(first.unwrap().available);
        assert!(second.unwrap().available);

        let events = read_record_events(&root.join("record.log"));
        assert_eq!(
            events
                .iter()
                .filter(|event| event["event"] == "initialize")
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| event["event"] == "new")
                .count(),
            1
        );

        let refreshed = manager
            .probe_acp_health("test-agent")
            .await
            .expect("forced ACP health refresh");
        assert!(refreshed.available);
        let events = read_record_events(&root.join("record.log"));
        assert_eq!(
            events
                .iter()
                .filter(|event| event["event"] == "initialize")
                .count(),
            2
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| event["event"] == "new")
                .count(),
            2
        );

        repository
            .update_enabled("test-agent", false, &chrono::Utc::now().to_rfc3339())
            .await
            .expect("disable fixture Agent");
        let disabled = manager
            .get_or_refresh_acp_models("test-agent")
            .await
            .expect("disabled Agent model result");
        assert!(!disabled.available);
        assert_eq!(disabled.error_code.as_deref(), Some("agent_disabled"));

        repository
            .delete("test-agent")
            .await
            .expect("uninstall fixture Agent");
        let error = manager
            .get_or_refresh_acp_models("test-agent")
            .await
            .expect_err("uninstalled Agent must not use the model cache");
        assert_eq!(error.code(), "agent_not_installed");

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn cancelling_acp_probe_reaps_the_process_without_marking_health_failed() {
        let (manager, repository, root) = acp_test_fixture("initialize_timeout").await;
        let manager = Arc::new(manager);
        let task_manager = Arc::clone(&manager);
        let task =
            tokio::spawn(async move { task_manager.get_or_refresh_acp_models("test-agent").await });

        for _ in 0..50 {
            if !manager.probe_flights.lock().await.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(manager.release_acp_probe_caller("test-agent").await);
        let result = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("cancelled probe should finish")
            .expect("probe task should not panic");
        let error = result.expect_err("cancelled probe should return an error");
        assert_eq!(error.code(), "cancelled");

        let installation = repository
            .get("test-agent")
            .await
            .expect("load installation")
            .expect("installation exists");
        assert_eq!(installation.protocol_status, ProtocolStatus::Ready);
        assert_clean_workspaces(&root.join("workspaces"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn acp_health_probe_succeeds_when_models_empty_and_leaves_protocol_ready() {
        let (manager, repository, root) = acp_test_fixture("no_models").await;

        let models = manager
            .probe_acp_health("test-agent")
            .await
            .expect("probe ACP health");
        assert!(models.available);
        assert!(models.models.is_empty());
        assert_eq!(models.error_code.as_deref(), Some("model_list_empty"));

        let installation = repository
            .get("test-agent")
            .await
            .unwrap()
            .expect("installation exists");
        assert_eq!(installation.protocol_status, ProtocolStatus::Ready);
        assert_eq!(installation.model_status.as_deref(), Some("unsupported"));
        assert!(installation.connected());
        assert!(installation.execution_ready());

        let candidates = repository.list_registry_candidates().await.unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].agent_id, "test-agent");

        let reloaded = manager.reload().await.expect("reload candidates");
        assert_eq!(reloaded, 1);

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn acp_health_probe_marks_auth_required_when_auth_error() {
        let (manager, repository, root) = acp_test_fixture("auth_error").await;

        let models = manager
            .probe_acp_health("test-agent")
            .await
            .expect("probe ACP health");
        assert!(!models.available);
        assert_eq!(models.error_code.as_deref(), Some("auth_required"));

        let installation = repository
            .get("test-agent")
            .await
            .unwrap()
            .expect("installation exists");
        assert_eq!(installation.installation_status, InstallationStatus::Ready);
        assert_eq!(installation.protocol_status, ProtocolStatus::AuthRequired);
        assert_eq!(
            installation.protocol_error_code.as_deref(),
            Some("auth_required")
        );
        assert!(!installation.connected());
        assert!(!installation.execution_ready());

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn acp_health_probe_marks_failed_when_connection_fails() {
        let (manager, repository, root) = acp_test_fixture("new_error").await;

        let models = manager
            .probe_acp_health("test-agent")
            .await
            .expect("probe ACP health");
        assert!(!models.available);
        assert_eq!(models.error_code.as_deref(), Some("connection_failed"));

        let installation = repository
            .get("test-agent")
            .await
            .unwrap()
            .expect("installation exists");
        assert_eq!(installation.protocol_status, ProtocolStatus::Failed);
        assert_eq!(
            installation.protocol_error_code.as_deref(),
            Some("connection_failed")
        );
        assert_eq!(installation.model_status.as_deref(), Some("failed"));
        assert!(!installation.connected());
        assert!(!installation.execution_ready());

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn startup_reconciliation_marks_mismatched_legacy_native_installation_incompatible_and_excludes_from_registry(
    ) {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-legacy-reconcile-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let db_path = root.join("app.db");
        let db = crate::backend::store::Database::open_initialized_async(&db_path)
            .await
            .expect("open db");
        let pool = db.pool().clone();
        let repository = AgentInstallationRepository::new(pool.clone());
        let fake_agy = root.join("bin").join("agy");
        std::fs::create_dir_all(fake_agy.parent().unwrap()).unwrap();
        std::fs::write(&fake_agy, b"#!/bin/sh\necho 1.0.0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake_agy, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let now = chrono::Utc::now().to_rfc3339();
        let legacy_installation = AgentInstallation {
            agent_id: "antigravity".to_string(),
            installation_id: uuid::Uuid::new_v4().to_string(),
            display_name: "Google Antigravity".to_string(),
            catalog_item_version: "1.0.0".to_string(),
            agent_version: "1.0.0".to_string(),
            protocol: AgentMarketProtocol::Native,
            distribution_id: "system-antigravity".to_string(),
            distribution_type: DistributionType::System,
            ownership: Ownership::System,
            install_dir: None,
            resolved_program: fake_agy.clone(),
            args: vec![],
            definition_json: definition_json(
                "antigravity",
                "Google Antigravity",
                &AgentMarketProtocol::Native,
                &fake_agy,
                &[],
            ),
            integrity_json: None,
            source_registry: "builtin".to_string(),
            catalog_version: "2026.03.01.1".to_string(),
            enabled: true,
            installation_status: InstallationStatus::Ready,
            runtime_status: RuntimeStatus::Ready,
            runtime_error_code: None,
            runtime_error_message: None,
            runtime_checked_at: Some(now.clone()),
            protocol_status: ProtocolStatus::Ready,
            protocol_error_code: None,
            protocol_error_message: None,
            protocol_checked_at: Some(now.clone()),
            model_status: None,
            model_error_code: None,
            model_checked_at: None,
            installed_at: now.clone(),
            updated_at: now.clone(),
        };
        repository
            .upsert_active(&legacy_installation)
            .await
            .unwrap();

        let manager = AgentRuntimeManager::new(pool.clone(), root.join("workspaces"));

        // 证明旧代码在未按 catalog 协调时仍会把 Native definition 发布到 Registry
        let candidates = repository.list_registry_candidates().await.unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].agent_id, "antigravity");
        assert_eq!(candidates[0].protocol, AgentMarketProtocol::Native);

        let reloaded = manager.reload().await.unwrap();
        assert_eq!(reloaded, 1);
        let snapshot = manager.registry().snapshot();
        let active_def = snapshot.get(&AgentId::parse("antigravity").unwrap());
        assert!(active_def.is_some());
        assert_eq!(active_def.unwrap().protocol, AgentProtocol::Native);

        // 构造活动 catalog fixture：protocol 为 ACP，分发仅为 Binary
        let active_catalog = crate::backend::agent_market::catalog::CatalogService::from_catalog(
            crate::backend::agent_market::types::Catalog {
                schema: "assetiweave.agent-market/v1".to_string(),
                catalog_version: "2026.03.07.1".to_string(),
                generated_at: now.clone(),
                source: crate::backend::agent_market::types::CatalogSource {
                    kind: "official".to_string(),
                    upstream: "https://github.com/google/antigravity".to_string(),
                    upstream_revision: "v1.1.1".to_string(),
                },
                items: vec![crate::backend::agent_market::types::CatalogItem {
                    id: "antigravity".to_string(),
                    display_name: "Google Antigravity".to_string(),
                    description: "Google Antigravity ACP Agent".to_string(),
                    protocol: AgentMarketProtocol::Acp,
                    version: "1.1.1".to_string(),
                    core_compatibility: Default::default(),
                    capabilities: crate::backend::agent_market::types::CatalogCapabilities::fallback_for_protocol(&AgentMarketProtocol::Acp),
                    verification: crate::backend::agent_market::types::Verification {
                        status: crate::backend::agent_market::types::VerificationStatus::Experimental,
                        tested_at: now.clone(),
                        evidence_id: None,
                    },
                    upstream: crate::backend::agent_market::types::UpstreamSource {
                        registry_id: "antigravity-acp".to_string(),
                        homepage: "https://github.com/google/antigravity".to_string(),
                        license: "Apache-2.0".to_string(),
                    },
                    distributions: vec![crate::backend::agent_market::types::Distribution::Binary {
                        id: "binary-antigravity-darwin-arm64".to_string(),
                        priority: 100,
                        target: crate::backend::agent_market::types::Target {
                            os: "darwin".to_string(),
                            arch: "arm64".to_string(),
                        },
                        archive: "antigravity-darwin-arm64.tar.gz".to_string(),
                        url: "https://example.com/antigravity.tar.gz".to_string(),
                        sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
                        size: Some(1024),
                        executable: "antigravity.par".to_string(),
                        launch_args: vec![],
                        model_discovery_args: None,
                        session_cleanup_args: None,
                        session_cleanup_not_found_markers: vec![],
                    }],
                }],
            }
        );

        // 执行启动协调（传入 active catalog）
        let warnings = manager
            .recover_startup_with_catalog(&root.join("runtime"), Some(&active_catalog))
            .await
            .unwrap();
        assert!(!warnings.is_empty());

        // 验证 SQLite 记录状态
        let updated = repository
            .get("antigravity")
            .await
            .unwrap()
            .expect("record preserved");
        assert_eq!(
            updated.installation_status,
            InstallationStatus::Incompatible
        );
        assert_eq!(
            updated.runtime_error_code.as_deref(),
            Some("catalog_distribution_incompatible")
        );
        assert!(!updated.connected());
        assert!(!updated.execution_ready());

        // 外部 agy 未被删除，原记录的 program / protocol 未被静默篡改
        assert!(fake_agy.is_file());
        assert_eq!(updated.resolved_program, fake_agy);
        assert_eq!(updated.protocol, AgentMarketProtocol::Native);

        // Registry 中无该 definition
        let final_snapshot = manager.registry().snapshot();
        let registered = final_snapshot.get(&AgentId::parse("antigravity").unwrap());
        assert!(registered.is_none());

        let candidates_after = repository.list_registry_candidates().await.unwrap();
        assert!(candidates_after.is_empty());

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn agacp_05_scene_1_normal_models_and_prompt() {
        let (manager, repository, root) = acp_test_fixture_with_id("test-agent-s1", "happy").await;

        // 1. Health Probe
        let models = manager
            .probe_acp_health("test-agent-s1")
            .await
            .expect("probe ACP health");
        assert!(models.available);
        assert_eq!(models.models.len(), 2);
        assert_eq!(
            models.current_model_id.as_deref(),
            Some("fixture/model-fast")
        );

        // 2. Candidate & Reload to Registry
        let candidates = repository.list_registry_candidates().await.unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].agent_id, "test-agent-s1");

        let reloaded = manager.reload().await.expect("reload candidates");
        assert_eq!(reloaded, 1);
        let snapshot = manager.registry().snapshot();
        let def = snapshot
            .get(&AgentId::parse("test-agent-s1").unwrap())
            .expect("registered");
        assert_eq!(def.protocol, AgentProtocol::Acp);

        // 3. SQLite State Verification
        let installation = repository
            .get("test-agent-s1")
            .await
            .unwrap()
            .expect("installation exists");
        assert_eq!(installation.protocol_status, ProtocolStatus::Ready);
        assert_eq!(installation.model_status.as_deref(), Some("ready"));
        assert!(installation.connected());
        assert!(installation.execution_ready());

        // 4. Execution with specific model
        let req = test_request("test-agent-s1", Some("fixture/model-accurate"));
        let result = manager
            .runtime()
            .execute(req)
            .await
            .expect("execution succeeds");
        assert_eq!(result.text, "translated");
        assert_eq!(result.protocol, AgentProtocol::Acp);
        assert_eq!(
            result.requested_model.as_deref(),
            Some("fixture/model-accurate")
        );

        // 5. Check Record Events: initialize, new, model(accurate), prompt, close, delete
        let events = read_record_events(&root.join("record.log"));
        assert!(events.iter().any(|e| e["event"] == "initialize"));
        assert!(events.iter().any(|e| e["event"] == "new"));
        let model_event = events.iter().find(|e| e["event"] == "model");
        assert!(model_event.is_some());
        assert_eq!(model_event.unwrap()["value"], "fixture/model-accurate");
        assert!(events.iter().any(|e| e["event"] == "prompt"));
        assert!(events.iter().any(|e| e["event"] == "close"));
        assert!(events.iter().any(|e| e["event"] == "delete"));

        // 6. Clean workspace assertion
        assert_clean_workspaces(&root.join("workspaces"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn agacp_05_scene_2_empty_models_and_default_model_prompt() {
        let (manager, repository, root) =
            acp_test_fixture_with_id("test-agent-s2", "no_models").await;

        // 1. Health Probe with empty models
        let models = manager
            .probe_acp_health("test-agent-s2")
            .await
            .expect("probe ACP health");
        assert!(models.available);
        assert!(models.models.is_empty());
        assert_eq!(models.error_code.as_deref(), Some("model_list_empty"));

        // 2. SQLite State: Protocol is Ready, Model is unsupported, but execution_ready is true!
        let installation = repository
            .get("test-agent-s2")
            .await
            .unwrap()
            .expect("installation exists");
        assert_eq!(installation.protocol_status, ProtocolStatus::Ready);
        assert_eq!(installation.model_status.as_deref(), Some("unsupported"));
        assert!(installation.connected());
        assert!(installation.execution_ready());

        // 3. Reload
        let reloaded = manager.reload().await.expect("reload candidates");
        assert_eq!(reloaded, 1);

        // 4. Execute with default model (model: None)
        let req = test_request("test-agent-s2", None);
        let result = manager
            .runtime()
            .execute(req)
            .await
            .expect("execution succeeds with default model");
        assert_eq!(result.text, "translated");
        assert_eq!(result.requested_model, None);

        // 5. Verify record: no "model" set_config_option event was called!
        let events = read_record_events(&root.join("record.log"));
        assert!(!events.iter().any(|e| e["event"] == "model"));
        assert!(events.iter().any(|e| e["event"] == "prompt"));
        assert!(events.iter().any(|e| e["event"] == "close"));
        assert!(events.iter().any(|e| e["event"] == "delete"));

        assert_clean_workspaces(&root.join("workspaces"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn agacp_05_scene_3_model_discovery_error_leaves_protocol_ready() {
        let (manager, repository, root) =
            acp_test_fixture_with_id("test-agent-s3", "model_discovery_error").await;

        // 1. Probe ACP health: Stage 1 (check_connection) passes, Stage 2 (discover_models) fails
        let models = manager
            .probe_acp_health("test-agent-s3")
            .await
            .expect("probe ACP health succeeds at manager level");
        assert!(models.available);
        assert!(models.models.is_empty());
        assert_eq!(models.error_code.as_deref(), Some("model_catalog_invalid"));

        // 2. SQLite State: Protocol is Ready, Model is failed, connected/execution_ready are true!
        let installation = repository
            .get("test-agent-s3")
            .await
            .unwrap()
            .expect("installation exists");
        assert_eq!(installation.protocol_status, ProtocolStatus::Ready);
        assert_eq!(installation.model_status.as_deref(), Some("failed"));
        assert_eq!(
            installation.model_error_code.as_deref(),
            Some("model_catalog_invalid")
        );
        assert!(installation.connected());
        assert!(installation.execution_ready());

        // 3. Reloads into Registry
        let reloaded = manager.reload().await.expect("reload candidates");
        assert_eq!(reloaded, 1);

        // 4. Default model execution succeeds
        let req_default = test_request("test-agent-s3", None);
        let result = manager
            .runtime()
            .execute(req_default)
            .await
            .expect("default model execution succeeds");
        assert_eq!(result.text, "translated");

        // 5. Explicit model selection fails (rejected by agent)
        let req_model = test_request("test-agent-s3", Some("fixture/model-fast"));
        let err = manager
            .runtime()
            .execute(req_model)
            .await
            .expect_err("explicit model selection should fail");
        assert!(matches!(err, AiExecutionError::ModelSelectionFailed { .. }));

        assert_clean_workspaces(&root.join("workspaces"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn agacp_05_scene_4_auth_error_cleans_workspace_and_no_fallback() {
        let (manager, repository, root) =
            acp_test_fixture_with_id("test-agent-s4", "auth_error").await;

        // 1. Probe ACP health fails with auth_required
        let models = manager
            .probe_acp_health("test-agent-s4")
            .await
            .expect("probe completes");
        assert!(!models.available);
        assert_eq!(models.error_code.as_deref(), Some("auth_required"));

        // 2. SQLite State: Protocol is AuthRequired, disconnected
        let installation = repository
            .get("test-agent-s4")
            .await
            .unwrap()
            .expect("installation exists");
        assert_eq!(installation.protocol_status, ProtocolStatus::AuthRequired);
        assert_eq!(
            installation.protocol_error_code.as_deref(),
            Some("auth_required")
        );
        assert!(!installation.connected());
        assert!(!installation.execution_ready());

        // 3. Excluded from Registry candidates
        let candidates = repository.list_registry_candidates().await.unwrap();
        assert!(candidates.is_empty());

        // 4. Directly load definition to verify execution phase error, clean workspace, and no fallback
        let definition = definition_from_installation(&installation).unwrap();
        manager
            .registry_snapshot
            .replace(AgentRegistry::from_definitions(vec![definition]).unwrap());

        let err = manager
            .runtime()
            .execute(test_request("test-agent-s4", None))
            .await
            .expect_err("execution fails on auth error");
        match err {
            AiExecutionError::ProtocolDetail { operation, detail } => {
                assert_eq!(operation, "session_new");
                assert!(detail
                    .to_ascii_lowercase()
                    .contains("authentication required"));
            }
            AiExecutionError::Protocol { operation } => {
                assert_eq!(operation, "session_new");
            }
            other => panic!("unexpected error on auth_error: {:?}", other),
        }

        // 5. Zero prompt events in fake ACP
        let events = read_record_events(&root.join("record.log"));
        assert!(!events.iter().any(|e| e["event"] == "prompt"));

        // 6. Workspace completely cleaned up
        assert_clean_workspaces(&root.join("workspaces"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn agacp_05_scene_5_prompt_error_timeout_cancel_disconnect() {
        // 5a. Prompt Error
        {
            let (manager, _, root) =
                acp_test_fixture_with_id("test-agent-5a", "prompt_error").await;
            let reloaded = manager.reload().await.unwrap();
            assert_eq!(reloaded, 1);
            let err = manager
                .runtime()
                .execute(test_request("test-agent-5a", None))
                .await
                .expect_err("prompt error expected");
            match err {
                AiExecutionError::ProtocolDetail { operation, detail } => {
                    assert_eq!(operation, "prompt");
                    assert!(detail.contains("Internal prompt execution error"));
                }
                AiExecutionError::Protocol { operation } => {
                    assert_eq!(operation, "prompt");
                }
                other => panic!("unexpected error on prompt_error: {:?}", other),
            }
            assert_clean_workspaces(&root.join("workspaces"));
            let _ = std::fs::remove_dir_all(root);
        }

        // 5b. Timeout
        {
            let (manager, _, root) = acp_test_fixture_with_id("test-agent-5b", "cancel_wait").await;
            let reloaded = manager.reload().await.unwrap();
            assert_eq!(reloaded, 1);
            let mut req = test_request("test-agent-5b", None);
            req.limits.total_timeout = Duration::from_millis(300);
            let err = manager
                .runtime()
                .execute(req)
                .await
                .expect_err("timeout expected");
            assert!(matches!(err, AiExecutionError::Timeout { .. }));
            assert_clean_workspaces(&root.join("workspaces"));
            let _ = std::fs::remove_dir_all(root);
        }

        // 5c. Cancel
        {
            let (manager, _, root) = acp_test_fixture_with_id("test-agent-5c", "cancel_wait").await;
            let reloaded = manager.reload().await.unwrap();
            assert_eq!(reloaded, 1);
            let req = test_request("test-agent-5c", None);
            let cancellation = req.cancellation.clone();
            let runtime = manager.runtime();
            let handle = tokio::spawn(async move { runtime.execute(req).await });

            // Wait until the agent has actually received and started the prompt
            let record_file = root.join("record.log");
            let wait_start = std::time::Instant::now();
            while wait_start.elapsed() < Duration::from_secs(3) {
                let events = read_record_events(&record_file);
                if events.iter().any(|e| e["event"] == "prompt") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }

            cancellation.cancel();
            let err = handle.await.unwrap().expect_err("cancellation expected");
            assert!(matches!(err, AiExecutionError::Cancelled { .. }));
            let events = read_record_events(&root.join("record.log"));
            assert!(events.iter().any(|e| e["event"] == "cancel"));
            assert_clean_workspaces(&root.join("workspaces"));
            let _ = std::fs::remove_dir_all(root);
        }

        // 5d. Disconnect
        {
            let (manager, _, root) = acp_test_fixture_with_id("test-agent-5d", "disconnect").await;
            let reloaded = manager.reload().await.unwrap();
            assert_eq!(reloaded, 1);
            let err = manager
                .runtime()
                .execute(test_request("test-agent-5d", None))
                .await
                .expect_err("disconnect expected");
            assert!(matches!(
                err,
                AiExecutionError::Protocol { .. }
                    | AiExecutionError::ProtocolDetail { .. }
                    | AiExecutionError::AgentExited { .. }
            ));
            assert_clean_workspaces(&root.join("workspaces"));
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[tokio::test]
    async fn agacp_05_scene_6_cleanup_close_and_delete_supported_and_unsupported() {
        // 6a. Supported close and delete
        {
            let (manager, _, root) = acp_test_fixture_with_id("test-agent-6a", "happy").await;
            let reloaded = manager.reload().await.unwrap();
            assert_eq!(reloaded, 1);
            let res = manager
                .runtime()
                .execute(test_request("test-agent-6a", None))
                .await
                .expect("execution succeeds");
            assert_eq!(res.text, "translated");
            let events = read_record_events(&root.join("record.log"));
            assert!(events.iter().any(|e| e["event"] == "close"));
            assert!(events.iter().any(|e| e["event"] == "delete"));
            assert_clean_workspaces(&root.join("workspaces"));
            let _ = std::fs::remove_dir_all(root);
        }

        // 6b. Unsupported close and delete without fallback -> CleanupFailed with delete_unsupported, but process and workspace cleaned cleanly
        {
            let (manager, _, root) =
                acp_test_fixture_with_id("test-agent-6b", "no_close_no_delete").await;
            let reloaded = manager.reload().await.unwrap();
            assert_eq!(reloaded, 1);
            let err = manager
                .runtime()
                .execute(test_request("test-agent-6b", None))
                .await
                .expect_err("without fallback, unsupported delete reports cleanup failure");
            match err {
                AiExecutionError::CleanupFailed { failures } => {
                    assert!(
                        failures.iter().any(|f| f == "delete_unsupported"),
                        "failures must include delete_unsupported: {failures:?}"
                    );
                }
                other => panic!("expected CleanupFailed error, got: {other:?}"),
            }
            let events = read_record_events(&root.join("record.log"));
            assert!(!events.iter().any(|e| e["event"] == "close"));
            assert!(!events.iter().any(|e| e["event"] == "delete"));
            assert!(events
                .iter()
                .any(|e| e["event"] == "stdin_closed" || e["event"] == "sigterm"));
            assert_clean_workspaces(&root.join("workspaces"));
            let _ = std::fs::remove_dir_all(root);
        }
    }

    async fn acp_test_fixture_with_extra_args(
        agent_id: &str,
        mode: &str,
        extra_args: &[String],
    ) -> (
        AgentRuntimeManager,
        AgentInstallationRepository,
        std::path::PathBuf,
    ) {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-acp-{}-{}",
            agent_id,
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let db_path = root.join("app.db");
        let db = crate::backend::store::Database::open_initialized_async(&db_path)
            .await
            .expect("open db");
        let pool = db.pool().clone();
        let repository = AgentInstallationRepository::new(pool.clone());
        let program =
            crate::backend::host_process::resolve_host_executable("node").expect("node executable");
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-fixtures/fake-acp-agent.mjs");
        let record = root.join("record.log");
        let mut args = vec![
            fixture.to_string_lossy().to_string(),
            format!("--mode={mode}"),
            format!("--record={}", record.to_string_lossy()),
        ];
        args.extend(extra_args.iter().cloned());
        let now = chrono::Utc::now().to_rfc3339();
        let installation = AgentInstallation {
            agent_id: agent_id.to_string(),
            installation_id: uuid::Uuid::new_v4().to_string(),
            display_name: format!("{agent_id} Display"),
            catalog_item_version: "1.0.0".to_string(),
            agent_version: "1.0.0".to_string(),
            protocol: AgentMarketProtocol::Acp,
            distribution_id: "test-distribution".to_string(),
            distribution_type: DistributionType::System,
            ownership: Ownership::System,
            install_dir: None,
            resolved_program: program.clone(),
            args: args.clone(),
            definition_json: serde_json::json!({
                "id": agent_id,
                "display_name": format!("{agent_id} Display"),
                "protocol": "acp",
                "program": program.to_string_lossy(),
                "args": args,
                "env": [
                    { "name": "ASSETIWEAVE_FAKE_ACP_MODE", "value": mode },
                    { "name": "ASSETIWEAVE_FAKE_ACP_RECORD_PATH", "value": record.to_string_lossy() }
                ],
            }),
            integrity_json: None,
            source_registry: "test".to_string(),
            catalog_version: "1.0".to_string(),
            enabled: true,
            installation_status: InstallationStatus::Ready,
            runtime_status: RuntimeStatus::Ready,
            runtime_error_code: None,
            runtime_error_message: None,
            runtime_checked_at: Some(now.clone()),
            protocol_status: ProtocolStatus::Ready,
            protocol_error_code: None,
            protocol_error_message: None,
            protocol_checked_at: Some(now.clone()),
            model_status: None,
            model_error_code: None,
            model_checked_at: None,
            installed_at: now.clone(),
            updated_at: now,
        };
        repository.upsert_active(&installation).await.unwrap();
        let manager = AgentRuntimeManager::new(pool, root.join("workspaces"));
        (manager, repository, root)
    }

    fn read_spawn_count(path: &std::path::Path) -> usize {
        if !path.exists() {
            return 0;
        }
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count()
    }

    #[test]
    fn test_bounded_model_cache_unit() {
        let mut cache = BoundedModelCache::new(3);
        let make_id = |id: &str| AgentProbeIdentity {
            agent_id: id.to_string(),
            installation_id: format!("inst-{id}"),
            definition_digest: "digest".to_string(),
            enabled: true,
            executable_present: true,
        };
        let make_res = |id: &str| AgentModelsResult {
            agent_id: id.to_string(),
            available: true,
            current_model_id: None,
            models: vec![AgentModelOption {
                id: format!("m-{id}"),
                label: id.to_string(),
                description: None,
            }],
            error_code: None,
            error: None,
        };

        cache.insert("a".to_string(), make_id("a"), make_res("a"));
        cache.insert("b".to_string(), make_id("b"), make_res("b"));
        cache.insert("c".to_string(), make_id("c"), make_res("c"));
        assert_eq!(cache.len(), 3);

        // Access "a" so it becomes most recently used
        assert!(cache.get("a", &make_id("a")).is_some());

        // Insert "d", capacity is 3 -> "b" should be evicted (least recently used)
        cache.insert("d".to_string(), make_id("d"), make_res("d"));
        assert_eq!(cache.len(), 3);
        assert!(cache.get("a", &make_id("a")).is_some());
        assert!(cache.get("b", &make_id("b")).is_none());
        assert!(cache.get("c", &make_id("c")).is_some());
        assert!(cache.get("d", &make_id("d")).is_some());

        // Identity mismatch evicts entry
        let wrong_id = AgentProbeIdentity {
            agent_id: "a".to_string(),
            installation_id: "wrong".to_string(),
            definition_digest: "digest".to_string(),
            enabled: true,
            executable_present: true,
        };
        assert!(cache.get("a", &wrong_id).is_none());
        assert_eq!(cache.len(), 2);

        // Remove and clear
        cache.remove("c");
        assert_eq!(cache.len(), 1);
        cache.clear();
        assert_eq!(cache.len(), 0);
    }

    #[tokio::test]
    async fn test_issue_29_decision_1_and_14_concurrent_success_single_cold_start() {
        let agent_id = "test-agent-c1";
        let root_dir = std::env::temp_dir().join(format!("counter-c1-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_dir).unwrap();
        let counter_path = root_dir.join("spawn.count");
        let extra_args = vec![
            format!("--counter-path={}", counter_path.display()),
            "--new-delay-ms=50".to_string(),
        ];

        let (manager, _, root) =
            acp_test_fixture_with_extra_args(agent_id, "happy", &extra_args).await;
        let manager = Arc::new(manager);

        let mut handles = Vec::new();
        for _ in 0..5 {
            let m = manager.clone();
            let aid = agent_id.to_string();
            handles.push(tokio::spawn(async move {
                m.get_or_refresh_acp_models(&aid).await
            }));
        }

        for handle in handles {
            let result = handle.await.unwrap().expect("probe succeeded");
            assert!(result.available);
            assert_eq!(result.models.len(), 2);
        }

        let cold_starts = read_spawn_count(&counter_path);
        assert_eq!(
            cold_starts, 1,
            "concurrent success requests must share exactly 1 cold start"
        );

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(root_dir);
    }

    #[tokio::test]
    async fn test_issue_29_decision_2_concurrent_failure_coalescing() {
        let agent_id = "test-agent-c2";
        let root_dir = std::env::temp_dir().join(format!("counter-c2-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_dir).unwrap();
        let counter_path = root_dir.join("spawn.count");
        let extra_args = vec![
            format!("--counter-path={}", counter_path.display()),
            "--start-delay-ms=50".to_string(),
        ];

        let (manager, _, root) =
            acp_test_fixture_with_extra_args(agent_id, "exit_on_init", &extra_args).await;
        let manager = Arc::new(manager);

        let mut handles = Vec::new();
        for _ in 0..5 {
            let m = manager.clone();
            let aid = agent_id.to_string();
            handles.push(tokio::spawn(async move {
                m.get_or_refresh_acp_models(&aid).await
            }));
        }

        for handle in handles {
            let result = handle.await.unwrap().expect("probe returns models result");
            assert!(!result.available);
            assert!(result.models.is_empty());
        }

        let cold_starts = read_spawn_count(&counter_path);
        assert_eq!(
            cold_starts, 1,
            "concurrent failure requests must share exactly 1 cold start"
        );

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(root_dir);
    }

    #[tokio::test]
    async fn test_issue_29_decision_3_and_5_cache_hit_and_force_refresh() {
        let agent_id = "test-agent-c3";
        let root_dir = std::env::temp_dir().join(format!("counter-c3-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_dir).unwrap();
        let counter_path = root_dir.join("spawn.count");
        let extra_args = vec![format!("--counter-path={}", counter_path.display())];

        let (manager, _, root) =
            acp_test_fixture_with_extra_args(agent_id, "happy", &extra_args).await;

        // 1. Initial call: cold start 1, caches models
        let res1 = manager.get_or_refresh_acp_models(agent_id).await.unwrap();
        assert!(res1.available);
        assert_eq!(read_spawn_count(&counter_path), 1);

        // 2. Second call: cache hit, no new process spawned
        let res2 = manager.get_or_refresh_acp_models(agent_id).await.unwrap();
        assert!(res2.available);
        assert_eq!(read_spawn_count(&counter_path), 1);

        // 3. Explicit connection test: force-refresh bypasses model cache and runs probe
        let conn = manager.refresh_acp_connection(agent_id).await.unwrap();
        assert!(conn.available);
        assert!(conn.connected);
        assert_eq!(
            read_spawn_count(&counter_path),
            2,
            "force-refresh must bypass cached models"
        );

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(root_dir);
    }

    #[tokio::test]
    async fn test_issue_29_decision_6_health_refresh_verifies_connectivity() {
        let (manager_good, _, root_good) =
            acp_test_fixture_with_id("test-agent-good", "happy").await;
        let (manager_bad, _, root_bad) =
            acp_test_fixture_with_id("test-agent-bad", "exit_nonzero").await;

        let summary_good = manager_good.refresh_installed_agent_health().await.unwrap();
        assert_eq!(summary_good.checked, 1);
        assert_eq!(summary_good.available, 1);
        assert_eq!(summary_good.unavailable, 0);

        let summary_bad = manager_bad.refresh_installed_agent_health().await.unwrap();
        assert_eq!(summary_bad.checked, 1);
        assert_eq!(summary_bad.available, 0);
        assert_eq!(summary_bad.unavailable, 1);

        let _ = std::fs::remove_dir_all(root_good);
        let _ = std::fs::remove_dir_all(root_bad);
    }

    #[tokio::test]
    async fn test_issue_29_decision_7_prerequisite_short_circuit() {
        let (manager, repository, root) = acp_test_fixture_with_id("test-agent-c7", "happy").await;

        // Case A: Agent is disabled
        let now = chrono::Utc::now().to_rfc3339();
        repository
            .update_enabled("test-agent-c7", false, &now)
            .await
            .unwrap();
        let models = manager
            .get_or_refresh_acp_models("test-agent-c7")
            .await
            .unwrap();
        assert!(!models.available);
        assert_eq!(models.error_code.as_deref(), Some("agent_disabled"));

        let conn = manager
            .refresh_acp_connection("test-agent-c7")
            .await
            .unwrap();
        assert!(!conn.available);
        assert_eq!(conn.error_code.as_deref(), Some("agent_disabled"));

        // Case B: Resolved program missing
        repository
            .update_enabled("test-agent-c7", true, &now)
            .await
            .unwrap();
        let mut inst = repository.get("test-agent-c7").await.unwrap().unwrap();
        inst.resolved_program = std::path::PathBuf::from("/non/existent/path/never_spawn");
        repository.upsert_active(&inst).await.unwrap();

        let models_missing = manager
            .get_or_refresh_acp_models("test-agent-c7")
            .await
            .unwrap();
        assert!(!models_missing.available);
        assert_eq!(
            models_missing.error_code.as_deref(),
            Some("agent_entry_missing")
        );

        let conn_missing = manager
            .refresh_acp_connection("test-agent-c7")
            .await
            .unwrap();
        assert!(!conn_missing.available);
        assert_eq!(
            conn_missing.error_code.as_deref(),
            Some("agent_entry_missing")
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn test_issue_29_decision_8_and_10_lifecycle_invalidation_and_race_protection() {
        let agent_id = "test-agent-c8";
        let root_dir = std::env::temp_dir().join(format!("counter-c8-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_dir).unwrap();
        let counter_path = root_dir.join("spawn.count");
        let extra_args = vec![
            format!("--counter-path={}", counter_path.display()),
            "--new-delay-ms=200".to_string(),
        ];

        let (manager, repository, root) =
            acp_test_fixture_with_extra_args(agent_id, "happy", &extra_args).await;

        // 1. Prime cache
        let (fast_manager, _, fast_root) = acp_test_fixture_with_id("fast-agent", "happy").await;
        let _ = fast_manager
            .get_or_refresh_acp_models("fast-agent")
            .await
            .unwrap();
        assert!(fast_manager.models_cache.read().await.len() > 0);

        // Invalidate state clears model cache
        fast_manager.invalidate_agent_state("fast-agent").await;
        assert_eq!(fast_manager.models_cache.read().await.len(), 0);
        let _ = std::fs::remove_dir_all(fast_root);

        // 2. Race protection: start probe, and before it finishes, change installation in SQLite
        let m = Arc::new(manager);
        let aid = agent_id.to_string();
        let m_clone = m.clone();
        let probe_task = tokio::spawn(async move { m_clone.refresh_acp_connection(&aid).await });

        // Give probe time to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Simulate concurrent lifecycle action (reinstall with new version)
        let mut inst = repository.get(agent_id).await.unwrap().unwrap();
        inst.agent_version = "2.0.0".to_string();
        inst.protocol_status = ProtocolStatus::Unchecked;
        repository.upsert_active(&inst).await.unwrap();
        m.invalidate_agent_state(agent_id).await;

        // Wait for old probe to complete or be cancelled
        let _ = probe_task.await;

        // Verify SQLite was NOT overwritten by the old probe
        let current_inst = repository.get(agent_id).await.unwrap().unwrap();
        assert_eq!(current_inst.agent_version, "2.0.0");
        assert_eq!(current_inst.protocol_status, ProtocolStatus::Unchecked);

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(root_dir);
    }

    #[tokio::test]
    async fn test_issue_29_decision_9_cancellation_and_caller_release() {
        let agent_id = "test-agent-c9";
        let extra_args = vec!["--new-delay-ms=500".to_string()];
        let (manager, _, root) =
            acp_test_fixture_with_extra_args(agent_id, "happy", &extra_args).await;

        let m = Arc::new(manager);
        let aid = agent_id.to_string();
        let m_clone = m.clone();
        let handle = tokio::spawn(async move { m_clone.get_or_refresh_acp_models(&aid).await });

        tokio::time::sleep(Duration::from_millis(50)).await;

        // Single caller releases -> triggers probe cancellation
        let cancelled = m.release_acp_probe_caller(agent_id).await;
        assert!(cancelled, "releasing only caller must trigger cancellation");

        let res = handle.await.unwrap();
        assert!(res.is_err(), "probe must fail with cancellation");
        assert_eq!(res.unwrap_err().code(), "cancelled");

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn test_issue_29_decision_11_and_12_state_decoupling_empty_corrupt_unsupported() {
        // 1. Empty models
        {
            let (manager, repository, root) =
                acp_test_fixture_with_id("test-decouple-empty", "empty_model_options").await;
            let conn = manager
                .refresh_acp_connection("test-decouple-empty")
                .await
                .unwrap();
            assert!(
                conn.available,
                "empty models must NOT degrade protocol connection available"
            );
            assert!(conn.connected, "protocol connection must remain connected");
            assert_eq!(conn.error_code, None);

            let models = manager
                .get_or_refresh_acp_models("test-decouple-empty")
                .await
                .unwrap();
            assert!(models.available);
            assert!(models.models.is_empty());
            assert_eq!(models.error_code.as_deref(), Some("model_list_empty"));

            let inst = repository
                .get("test-decouple-empty")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(inst.protocol_status, ProtocolStatus::Ready);
            assert_eq!(inst.model_status.as_deref(), Some("unsupported"));
            assert_eq!(inst.model_error_code.as_deref(), Some("model_list_empty"));
            let _ = std::fs::remove_dir_all(root);
        }

        // 2. Corrupt catalog
        {
            let (manager, repository, root) =
                acp_test_fixture_with_id("test-decouple-corrupt", "corrupt_catalog").await;
            let conn = manager
                .refresh_acp_connection("test-decouple-corrupt")
                .await
                .unwrap();
            assert!(
                conn.available,
                "corrupt catalog must NOT degrade protocol connection"
            );
            assert!(conn.connected);
            assert_eq!(conn.error_code, None);

            let models = manager
                .get_or_refresh_acp_models("test-decouple-corrupt")
                .await
                .unwrap();
            assert!(models.available);
            assert!(models.models.is_empty());
            assert_eq!(models.error_code.as_deref(), Some("model_catalog_invalid"));

            let inst = repository
                .get("test-decouple-corrupt")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(inst.protocol_status, ProtocolStatus::Ready);
            assert_eq!(inst.model_status.as_deref(), Some("failed"));
            assert_eq!(
                inst.model_error_code.as_deref(),
                Some("model_catalog_invalid")
            );
            let _ = std::fs::remove_dir_all(root);
        }

        // 3. Unsupported models
        {
            let (manager, repository, root) =
                acp_test_fixture_with_id("test-decouple-unsupported", "unsupported_models").await;
            let conn = manager
                .refresh_acp_connection("test-decouple-unsupported")
                .await
                .unwrap();
            assert!(
                conn.available,
                "unsupported models must NOT degrade protocol connection"
            );
            assert!(conn.connected);
            assert_eq!(conn.error_code, None);

            let models = manager
                .get_or_refresh_acp_models("test-decouple-unsupported")
                .await
                .unwrap();
            assert!(models.available);
            assert!(models.models.is_empty());
            assert_eq!(models.error_code.as_deref(), Some("unsupported"));

            let inst = repository
                .get("test-decouple-unsupported")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(inst.protocol_status, ProtocolStatus::Ready);
            assert_eq!(inst.model_status.as_deref(), Some("unsupported"));
            assert_eq!(inst.model_error_code.as_deref(), Some("unsupported"));
            let _ = std::fs::remove_dir_all(root);
        }
    }
}
