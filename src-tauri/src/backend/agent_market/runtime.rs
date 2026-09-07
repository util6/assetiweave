use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

use serde::Deserialize;
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
        backends::{acp::AcpExecutionBackend, native::NativeExecutionBackend},
        executor::AgentExecutor,
        AgentExecutionRuntime, AiExecutionError,
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

#[derive(Clone)]
pub(crate) struct AgentRuntimeManager {
    repository: AgentInstallationRepository,
    registry: AgentRuntimeRegistry,
    registry_snapshot: Arc<crate::backend::extension_kernel::RegistrySnapshot<AgentRegistry>>,
    executor: Arc<AgentExecutor>,
    workspace_root: PathBuf,
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
                AgentMarketProtocol::Acp => match self.probe_acp_health(&agent_id).await {
                    Ok(result) => result.available,
                    Err(error) => {
                        self.reload().await?;
                        return Err(error);
                    }
                },
                AgentMarketProtocol::Native => match self.probe_native_health(&agent_id).await {
                    Ok(result) => result.available,
                    Err(error) => {
                        self.reload().await?;
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
        self.reload().await?;
        Ok(summary)
    }

    pub(crate) async fn refresh_native_health(
        &self,
        agent_id: &str,
    ) -> Result<AgentConnectionResult, AgentMarketError> {
        let result = self.probe_native_health(agent_id).await?;
        self.reload().await?;
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
        self.reload().await?;
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

    pub(crate) async fn refresh_acp_health(
        &self,
        agent_id: &str,
    ) -> Result<AgentModelsResult, AgentMarketError> {
        let result = self.probe_acp_health(agent_id).await?;
        self.reload().await?;
        Ok(result)
    }

    async fn probe_acp_health(
        &self,
        agent_id: &str,
    ) -> Result<AgentModelsResult, AgentMarketError> {
        let mutation_gate = self.mutation_gate(agent_id);
        let _mutation_lease = mutation_gate.write().await;
        let mut installation = self.repository.get(agent_id).await?.ok_or_else(|| {
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

        let now = chrono::Utc::now().to_rfc3339();
        if !installation.enabled {
            return Ok(unavailable_models(
                agent_id,
                "agent_disabled",
                "The ACP Agent is disabled.",
            ));
        }
        if !installation.resolved_program.is_file() {
            installation.installation_status = InstallationStatus::Broken;
            installation.runtime_status = if installation.ownership == Ownership::Managed {
                RuntimeStatus::EntryMissing
            } else {
                RuntimeStatus::RuntimeMissing
            };
            installation.runtime_error_code = Some("agent_entry_missing".to_string());
            installation.runtime_error_message =
                Some("The resolved Agent entry is missing.".to_string());
            installation.runtime_checked_at = Some(now.clone());
            installation.protocol_status = ProtocolStatus::Failed;
            installation.protocol_error_code = Some("agent_entry_missing".to_string());
            installation.protocol_error_message =
                Some("The resolved Agent entry is missing.".to_string());
            installation.protocol_checked_at = Some(now.clone());
            installation.model_status = Some("failed".to_string());
            installation.model_error_code = Some("agent_entry_missing".to_string());
            installation.model_checked_at = Some(now.clone());
            installation.updated_at = now;
            self.repository.update_health(&installation).await?;
            return Ok(unavailable_models(
                agent_id,
                "agent_entry_missing",
                "The resolved Agent entry is missing.",
            ));
        }

        let definition = match definition_from_installation(&installation) {
            Ok(definition) => definition,
            Err(error) => {
                installation.installation_status = InstallationStatus::Broken;
                installation.runtime_status = RuntimeStatus::Failed;
                installation.runtime_error_code = Some("definition_invalid".to_string());
                installation.runtime_error_message = Some(error.to_string());
                installation.runtime_checked_at = Some(now.clone());
                installation.protocol_status = ProtocolStatus::Failed;
                installation.protocol_error_code = Some("definition_invalid".to_string());
                installation.protocol_error_message =
                    Some("The persisted ACP definition is invalid.".to_string());
                installation.protocol_checked_at = Some(now.clone());
                installation.model_status = Some("failed".to_string());
                installation.model_error_code = Some("definition_invalid".to_string());
                installation.model_checked_at = Some(now.clone());
                installation.updated_at = now;
                self.repository.update_health(&installation).await?;
                return Ok(unavailable_models(
                    agent_id,
                    "definition_invalid",
                    "The persisted ACP definition is invalid.",
                ));
            }
        };

        installation.installation_status = InstallationStatus::Ready;
        installation.runtime_status = RuntimeStatus::Ready;
        installation.runtime_error_code = None;
        installation.runtime_error_message = None;
        installation.runtime_checked_at = Some(now.clone());

        let backend = AcpExecutionBackend::new(self.workspace_root.clone());

        // Stage 1: Connection Probe (initialize + session/new)
        let connection_result = backend.check_connection(&definition).await;
        installation.protocol_checked_at = Some(now.clone());

        if let Err(error) = connection_result {
            let message = model_discovery_error_message(&error);
            if matches!(
                error,
                AiExecutionError::RuntimeUnavailable { .. } | AiExecutionError::Spawn { .. }
            ) {
                installation.installation_status = InstallationStatus::Broken;
                installation.runtime_status = RuntimeStatus::Failed;
                installation.runtime_error_code = Some("runtime_probe_failed".to_string());
                installation.runtime_error_message = Some(message.clone());
                installation.protocol_status = ProtocolStatus::Failed;
                installation.protocol_error_code = Some("runtime_probe_failed".to_string());
                installation.protocol_error_message = Some(message.clone());
            } else if is_auth_error(&error) {
                installation.protocol_status = ProtocolStatus::AuthRequired;
                installation.protocol_error_code = Some("auth_required".to_string());
                installation.protocol_error_message = Some(message.clone());
            } else {
                installation.protocol_status = ProtocolStatus::Failed;
                installation.protocol_error_code = Some("connection_failed".to_string());
                installation.protocol_error_message = Some(message.clone());
            }
            installation.model_status = Some("failed".to_string());
            installation.model_error_code = installation.protocol_error_code.clone();
            installation.model_checked_at = Some(now.clone());
            installation.updated_at = now;
            self.repository.update_health(&installation).await?;
            let code = installation
                .protocol_error_code
                .as_deref()
                .unwrap_or("connection_failed");
            return Ok(unavailable_models(agent_id, code, &message));
        }

        // Connection probe succeeded: Protocol is Ready
        installation.protocol_status = ProtocolStatus::Ready;
        installation.protocol_error_code = None;
        installation.protocol_error_message = None;

        // Stage 2: Model Discovery
        let discovery = backend.discover_models(&definition).await;
        installation.model_checked_at = Some(now.clone());
        let result = match discovery {
            Ok((models, current_model_id)) => {
                if models.is_empty() {
                    installation.model_status = Some("unsupported".to_string());
                    installation.model_error_code = Some("model_list_empty".to_string());
                    AgentModelsResult {
                        agent_id: agent_id.to_string(),
                        available: true,
                        current_model_id: None,
                        models: Vec::new(),
                        error_code: Some("model_list_empty".to_string()),
                        error: Some("No models advertised by ACP session".to_string()),
                    }
                } else {
                    installation.model_status = Some("ready".to_string());
                    installation.model_error_code = None;
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
            }
            Err(error) => {
                let message = model_discovery_error_message(&error);
                let is_empty = matches!(
                    error,
                    AiExecutionError::Protocol {
                        operation: "session_model_catalog_empty"
                    }
                );
                if is_empty {
                    installation.model_status = Some("unsupported".to_string());
                    installation.model_error_code = Some("model_list_empty".to_string());
                    AgentModelsResult {
                        agent_id: agent_id.to_string(),
                        available: true,
                        current_model_id: None,
                        models: Vec::new(),
                        error_code: Some("model_list_empty".to_string()),
                        error: Some(message),
                    }
                } else {
                    installation.model_status = Some("failed".to_string());
                    installation.model_error_code = Some("model_discovery_failed".to_string());
                    AgentModelsResult {
                        agent_id: agent_id.to_string(),
                        available: true,
                        current_model_id: None,
                        models: Vec::new(),
                        error_code: Some("model_discovery_failed".to_string()),
                        error: Some(message),
                    }
                }
            }
        };
        installation.updated_at = now;
        self.repository.update_health(&installation).await?;
        Ok(result)
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

    async fn acp_test_fixture(
        mode: &str,
    ) -> (
        AgentRuntimeManager,
        AgentInstallationRepository,
        std::path::PathBuf,
    ) {
        let root =
            std::env::temp_dir().join(format!("assetiweave-acp-health-{}", uuid::Uuid::new_v4()));
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
            agent_id: "test-agent".to_string(),
            installation_id: uuid::Uuid::new_v4().to_string(),
            display_name: "Test Agent".to_string(),
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
                "id": "test-agent",
                "display_name": "Test Agent",
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
}
