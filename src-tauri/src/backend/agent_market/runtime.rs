use std::{
    collections::HashSet,
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
            AgentCommandDefinition, AgentDefinition, AgentEnvEntry, AgentId, AgentProtocol,
            DeclaredAgentCapabilities,
        },
    },
    ai_execution::{
        backends::{acp::AcpExecutionBackend, native::NativeExecutionBackend},
        executor::AgentExecutor,
        AgentExecutionRuntime,
    },
    extension_kernel::DomainPackageSystem,
};

use super::{
    repository::AgentInstallationRepository,
    types::{AgentInstallation, AgentMarketProtocol, InstallationStatus, Ownership, RuntimeStatus},
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
            Arc::new(NativeExecutionBackend::new(workspace_root)),
            2,
        ));
        Self {
            repository: AgentInstallationRepository::new(pool),
            registry,
            registry_snapshot,
            executor,
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
        let referenced_install_dirs = installations
            .iter()
            .filter_map(|installation| installation.install_dir.as_ref())
            .cloned()
            .collect::<HashSet<_>>();
        let mut warnings = cleanup_runtime_directories(runtime_root, &referenced_install_dirs);
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
}

fn cleanup_runtime_directories(
    runtime_root: &Path,
    referenced_install_dirs: &HashSet<PathBuf>,
) -> Vec<String> {
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
    let active = runtime_root.join("active");
    if let Ok(entries) = std::fs::read_dir(&active) {
        for entry in entries.flatten() {
            let path = entry.path();
            let owned_identity = entry
                .file_name()
                .to_str()
                .and_then(|value| uuid::Uuid::parse_str(value).ok())
                .is_some();
            let referenced = referenced_install_dirs.contains(&path)
                || path.canonicalize().ok().is_some_and(|canonical| {
                    referenced_install_dirs
                        .iter()
                        .any(|reference| reference.canonicalize().ok().as_ref() == Some(&canonical))
                });
            if owned_identity
                && !referenced
                && entry
                    .file_type()
                    .map(|file_type| file_type.is_dir() && !file_type.is_symlink())
                    .unwrap_or(false)
            {
                if let Err(error) = std::fs::remove_dir_all(&path) {
                    warnings.push(format!("orphan active cleanup pending: {error}"));
                }
            }
        }
    }
    warnings
}

#[derive(Debug, Deserialize)]
struct ResolvedDefinition {
    id: String,
    display_name: String,
    protocol: String,
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
            definition_json: definition_json(
                "agent",
                "Agent",
                &AgentMarketProtocol::Acp,
                &program_path,
                &["acp".to_string()],
            ),
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
}
