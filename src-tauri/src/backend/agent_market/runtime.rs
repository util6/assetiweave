use std::{
    future::Future,
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
            AgentCommandDefinition, AgentDefinition, AgentEnvEntry, AgentId, AgentModelsResult,
            AgentProtocol, DeclaredAgentCapabilities,
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
        AgentInstallation, AgentMarketProtocol, InstallationStatus, Ownership, ProtocolStatus,
        RuntimeStatus,
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
    pub(crate) fn from_installation(installation: &AgentInstallation) -> Result<Self, String> {
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
        let executor = Arc::new(AgentExecutor::with_registry_handle(
            registry.clone(),
            Arc::new(AcpExecutionBackend::new(workspace_root.clone())),
            Arc::new(NativeExecutionBackend::new(workspace_root.clone())),
            2,
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

    pub(crate) async fn reload(&self, tenant_id: &str) -> Result<u64, String> {
        let installations = self.repository.list_registry_candidates(tenant_id).await?;
        let definitions = installations
            .iter()
            .map(|installation| {
                let package_system = AgentPackageSystem::from_installation(installation)
                    .map_err(|error| error.to_string())?;
                if package_system.kind() != crate::backend::extension_kernel::PackageKind::Agent {
                    return Err("Agent package system returned the wrong package kind".to_string());
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
                    .ok_or_else(|| "Agent installation has no runtime directory".to_string())?;
                let inspected = package_system
                    .inspect(&install_dir)
                    .map_err(|error| error.to_string())?;
                let _ = inspected;
                definition_from_installation(installation)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let next =
            AgentRegistry::from_definitions(definitions).map_err(|error| error.to_string())?;
        self.registry_snapshot.replace(next);
        Ok(self.registry.bump_generation())
    }

    /// Recover only state that can be proven to be owned by the Agent Market.
    /// This performs no network or protocol probes and is safe to call on each
    /// process start before the first registry publication.
    pub(crate) async fn recover_startup(
        &self,
        tenant_id: &str,
        runtime_root: &Path,
    ) -> Result<Vec<String>, String> {
        let installations = self.repository.list(tenant_id).await?;
        let mut warnings = cleanup_runtime_directories(runtime_root);
        for installation in installations {
            if installation.installation_status != InstallationStatus::Ready {
                continue;
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
                        tenant_id,
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
                        tenant_id,
                        &installation.agent_id,
                        RuntimeStatus::Failed,
                        "definition_invalid",
                        &error,
                        &chrono::Utc::now().to_rfc3339(),
                    )
                    .await?;
                warnings.push(format!(
                    "{} marked broken: definition_invalid",
                    installation.agent_id
                ));
            }
        }
        self.reload(tenant_id).await?;
        Ok(warnings)
    }

    pub(crate) async fn prepare_startup_health_refresh(
        &self,
        tenant_id: &str,
    ) -> Result<u64, String> {
        let changed = self
            .repository
            .mark_acp_health_unchecked(tenant_id, &chrono::Utc::now().to_rfc3339())
            .await?;
        self.reload(tenant_id).await?;
        Ok(changed)
    }

    pub(crate) async fn refresh_installed_acp_health(
        &self,
        tenant_id: &str,
    ) -> Result<AgentHealthRefreshSummary, String> {
        let agent_ids = self
            .repository
            .list(tenant_id)
            .await?
            .into_iter()
            .filter(|installation| {
                installation.enabled && installation.protocol == AgentMarketProtocol::Acp
            })
            .map(|installation| installation.agent_id)
            .collect::<Vec<_>>();
        let mut summary = AgentHealthRefreshSummary::default();
        for agent_id in agent_ids {
            let result = match self.probe_acp_health(tenant_id, &agent_id).await {
                Ok(result) => result,
                Err(error) => {
                    self.reload(tenant_id).await?;
                    return Err(error);
                }
            };
            summary.checked += 1;
            if result.available {
                summary.available += 1;
            } else {
                summary.unavailable += 1;
            }
        }
        self.reload(tenant_id).await?;
        Ok(summary)
    }

    pub(crate) fn refresh_installed_acp_health_blocking(
        self: Arc<Self>,
        tenant_id: String,
    ) -> Result<AgentHealthRefreshSummary, String> {
        run_process_capable_runtime(
            "aiw-acp-startup-health",
            "The ACP startup health refresh did not complete.",
            async move { self.refresh_installed_acp_health(&tenant_id).await },
        )
    }

    pub(crate) async fn refresh_acp_health(
        &self,
        tenant_id: &str,
        agent_id: &str,
    ) -> Result<AgentModelsResult, String> {
        let result = self.probe_acp_health(tenant_id, agent_id).await?;
        self.reload(tenant_id).await?;
        Ok(result)
    }

    pub(crate) fn refresh_acp_health_blocking(
        self: Arc<Self>,
        tenant_id: String,
        agent_id: String,
    ) -> Result<AgentModelsResult, String> {
        run_process_capable_runtime(
            "aiw-acp-health",
            "The ACP health refresh did not complete.",
            async move { self.refresh_acp_health(&tenant_id, &agent_id).await },
        )
    }

    async fn probe_acp_health(
        &self,
        tenant_id: &str,
        agent_id: &str,
    ) -> Result<AgentModelsResult, String> {
        let mutation_gate = self.mutation_gate(agent_id);
        let _mutation_lease = mutation_gate.write().await;
        let mut installation = self
            .repository
            .get(tenant_id, agent_id)
            .await?
            .ok_or_else(|| "The Agent is not installed.".to_string())?;
        if installation.protocol != AgentMarketProtocol::Acp {
            return Err("The installed Agent does not use ACP.".to_string());
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
                installation.runtime_error_message = Some(error);
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
        let discovery = AcpExecutionBackend::new(self.workspace_root.clone())
            .discover_models(&definition)
            .await;
        let result = match discovery {
            Ok((models, current_model_id)) => {
                installation.protocol_status = ProtocolStatus::Ready;
                installation.protocol_error_code = None;
                installation.protocol_error_message = None;
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
            Err(error) => {
                let message = model_discovery_error_message(&error);
                if matches!(
                    error,
                    AiExecutionError::RuntimeUnavailable { .. } | AiExecutionError::Spawn { .. }
                ) {
                    installation.installation_status = InstallationStatus::Broken;
                    installation.runtime_status = RuntimeStatus::Failed;
                    installation.runtime_error_code = Some("runtime_probe_failed".to_string());
                    installation.runtime_error_message = Some(message.clone());
                }
                installation.protocol_status = ProtocolStatus::Failed;
                installation.protocol_error_code = Some("model_list_unavailable".to_string());
                installation.protocol_error_message = Some(message.clone());
                installation.model_status = Some("failed".to_string());
                installation.model_error_code = Some("model_list_unavailable".to_string());
                unavailable_models(agent_id, "model_list_unavailable", &message)
            }
        };
        installation.protocol_checked_at = Some(now.clone());
        installation.model_checked_at = Some(now.clone());
        installation.updated_at = now;
        self.repository.update_health(&installation).await?;
        Ok(result)
    }
}

fn run_process_capable_runtime<T, F>(
    thread_name: &str,
    join_error: &str,
    future: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: Future<Output = Result<T, String>> + Send + 'static,
{
    let handle = std::thread::Builder::new()
        .name(thread_name.to_string())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())?;
            runtime.block_on(future)
        })
        .map_err(|error| error.to_string())?;
    handle.join().map_err(|_| join_error.to_string())?
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
) -> Result<AgentDefinition, String> {
    let package_manifest = installation.package_manifest()?;
    let resolved: ResolvedDefinition = serde_json::from_value(installation.definition_json.clone())
        .map_err(|error| error.to_string())?;
    if package_manifest.identity.package_id != resolved.id {
        return Err("resolved definition id does not match package identity".to_string());
    }
    if package_manifest.compatibility.protocol_version != 1 {
        return Err("unsupported Agent package protocol version".to_string());
    }
    let id = AgentId::parse(resolved.id).map_err(|error| error.to_string())?;
    let protocol = match resolved.protocol.as_str() {
        "acp" => AgentProtocol::Acp,
        "native" => AgentProtocol::Native,
        other => return Err(format!("unsupported agent protocol: {other}")),
    };
    let invocation = package_manifest.invocation;
    let availability_probe = package_manifest.availability_probe;
    let model_discovery_probe = package_manifest.model_discovery_probe;
    let program = invocation.entry.clone();
    let expected_program = installation.resolved_program.to_string_lossy();
    if program != expected_program {
        return Err("resolved definition program does not match installation record".to_string());
    }
    let program_path = std::path::PathBuf::from(&program);
    if !program_path.is_file() {
        return Err("resolved Agent program is missing".to_string());
    }
    if installation.ownership == super::types::Ownership::Managed {
        let install_dir = installation
            .install_dir
            .as_ref()
            .ok_or_else(|| "managed Agent is missing install directory".to_string())?;
        if !program_path.starts_with(install_dir) {
            return Err("resolved Agent program escapes its managed installation".to_string());
        }
    } else if installation.install_dir.is_some() {
        return Err("system Agent must not have a managed installation directory".to_string());
    }
    if invocation
        .args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-y" | "npx" | "uvx"))
        || program_path
            .file_name()
            .is_some_and(|name| matches!(name.to_string_lossy().as_ref(), "npx" | "uvx"))
    {
        return Err("runtime definition may not invoke a package manager".to_string());
    }
    let mut definition = AgentDefinition {
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
        declared_capabilities: DeclaredAgentCapabilities::acp_text(),
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
    if installation.protocol == AgentMarketProtocol::Acp {
        definition.declared_capabilities = DeclaredAgentCapabilities::acp_text();
    }
    definition.validate().map_err(|error| error.to_string())?;
    Ok(definition)
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
        resolved_definition["sessionCleanupArgs"] =
            serde_json::json!(["session", "delete", "{session_id}"]);
        resolved_definition["sessionCleanupNotFoundMarkers"] =
            serde_json::json!(["Session not found:"]);
        let installation = AgentInstallation {
            tenant_id: "tenant".to_string(),
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
}
