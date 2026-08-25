//! Agent installation lifecycle orchestration.
//!
//! The lifecycle owns staging, conformance, activation and cleanup.  Adapters
//! only call these methods; they do not perform package-manager or filesystem
//! work themselves.

mod install;
mod uninstall;

use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};

use super::{
    catalog::CatalogService,
    repository::AgentInstallationRepository,
    runtime::AgentRuntimeManager,
    types::{
        AgentInstallStartRequest, AgentInstallation, AgentMarketError, AgentUninstallStartRequest,
        LifecycleTaskPhase,
    },
};

pub(crate) use install::InstallOutcome;

#[derive(Clone)]
pub(crate) struct AgentLifecycleService {
    pub(crate) catalog: CatalogService,
    pub(crate) repository: AgentInstallationRepository,
    pub(crate) runtime_manager: Arc<AgentRuntimeManager>,
    pub(crate) runtime_root: PathBuf,
}

impl AgentLifecycleService {
    pub(crate) fn new(
        pool: sqlx::SqlitePool,
        runtime_manager: Arc<AgentRuntimeManager>,
        runtime_root: PathBuf,
    ) -> Result<Self, AgentMarketError> {
        let catalog = crate::backend::agent_market::CatalogCache::best_available()
            .map_err(|error| market_error("catalog_unavailable", error, true))?;
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

    pub(crate) async fn install_with_cancellation_and_progress(
        &self,
        tenant_id: &str,
        request: AgentInstallStartRequest,
        cancellation: Option<Arc<AtomicBool>>,
        phase_sink: Option<Arc<dyn Fn(LifecycleTaskPhase) + Send + Sync>>,
    ) -> Result<InstallOutcome, AgentMarketError> {
        install::run(self, tenant_id, request, cancellation, phase_sink).await
    }

    #[cfg(test)]
    pub(crate) async fn install(
        &self,
        tenant_id: &str,
        request: AgentInstallStartRequest,
    ) -> Result<InstallOutcome, AgentMarketError> {
        self.install_with_cancellation_and_progress(tenant_id, request, None, None)
            .await
    }

    pub(crate) async fn uninstall_with_cancellation_and_progress(
        &self,
        tenant_id: &str,
        request: AgentUninstallStartRequest,
        cancellation: Option<Arc<AtomicBool>>,
        phase_sink: Option<Arc<dyn Fn(LifecycleTaskPhase) + Send + Sync>>,
    ) -> Result<AgentInstallation, AgentMarketError> {
        uninstall::run(self, tenant_id, request, cancellation, phase_sink).await
    }

    pub(crate) async fn set_enabled(
        &self,
        tenant_id: &str,
        agent_id: &str,
        enabled: bool,
    ) -> Result<AgentInstallation, AgentMarketError> {
        let mutation_gate = self.runtime_manager.mutation_gate(agent_id);
        let _mutation_lease = mutation_gate.write().await;
        let installation = self
            .repository
            .get(tenant_id, agent_id)
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
            .update_enabled(tenant_id, agent_id, enabled, &updated_at)
            .await
            .map_err(|error| market_error("storage_failed", error, true))?;
        if self.runtime_manager.reload(tenant_id).await.is_err() {
            let _ = self
                .repository
                .update_enabled(tenant_id, agent_id, installation.enabled, &updated_at)
                .await;
            return Err(market_error(
                "registry_reload_failed",
                "The Agent runtime registry could not be reloaded.",
                true,
            ));
        }
        self.repository
            .get(tenant_id, agent_id)
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

pub(crate) fn default_runtime_root() -> Result<PathBuf, AgentMarketError> {
    let home = dirs::home_dir().ok_or_else(|| {
        market_error(
            "runtime_root_unavailable",
            "The user runtime directory is unavailable.",
            true,
        )
    })?;
    Ok(home.join(".assetiweave").join("agent-runtimes"))
}

pub(crate) fn ensure_runtime_root(path: &Path) -> Result<(), AgentMarketError> {
    std::fs::create_dir_all(path)
        .map_err(|error| market_error("runtime_root_unavailable", error.to_string(), true))
}

pub(crate) fn is_safe_managed_install_path(
    runtime_root: &Path,
    installation_id: &str,
    install_dir: &Path,
) -> bool {
    if uuid::Uuid::parse_str(installation_id).is_err() {
        return false;
    }
    let expected = runtime_root.join("active").join(installation_id);
    if install_dir != expected || install_dir == runtime_root {
        return false;
    }
    if std::fs::symlink_metadata(install_dir)
        .ok()
        .is_some_and(|metadata| metadata.file_type().is_symlink())
    {
        return false;
    }
    if !install_dir.exists() {
        return true;
    }
    let Ok(root) = runtime_root.canonicalize() else {
        return false;
    };
    let Ok(active) = root.join("active").canonicalize() else {
        return false;
    };
    let Ok(canonical_install_dir) = install_dir.canonicalize() else {
        return false;
    };
    canonical_install_dir.starts_with(&active)
        && canonical_install_dir != root
        && canonical_install_dir == active.join(installation_id)
}

pub(crate) fn market_error(
    code: &str,
    message: impl Into<String>,
    retryable: bool,
) -> AgentMarketError {
    AgentMarketError::new(code, &message.into(), retryable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{
        agent_market::{
            lifecycle::install::register_test_artifact,
            types::{
                AgentInstallStartRequest, AgentMarketProtocol, Catalog, CatalogCapabilities,
                CatalogItem, CatalogSource, CoreCompatibility, Distribution, ProtocolStatus,
                Target, UpstreamSource, Verification, VerificationStatus,
            },
        },
        agents::types::AgentId,
        store::Database,
    };
    use sha2::{Digest, Sha256};
    use std::{path::Path, sync::Arc};

    const AGENT_ID: &str = "fixture-agent";

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn agent_market_lifecycle_e2e_install_update_failure_recovery_and_cancel() {
        let database_path = std::env::temp_dir().join(format!(
            "assetiweave-agent-market-e2e-{}.db",
            uuid::Uuid::new_v4()
        ));
        let runtime_root = std::env::temp_dir().join(format!(
            "assetiweave-agent-market-runtime-{}",
            uuid::Uuid::new_v4()
        ));
        let workspace_root = std::env::temp_dir().join(format!(
            "assetiweave-agent-market-workspace-{}",
            uuid::Uuid::new_v4()
        ));
        let pool = std::thread::spawn({
            let database_path = database_path.clone();
            move || {
                Database::open_initialized(&database_path)
                    .expect("database")
                    .pool()
                    .clone()
            }
        })
        .join()
        .expect("database thread");
        let fixture_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/fake-acp-agent.mjs");

        let good_v1 = fixture_agent_script(&fixture_path, "happy");
        let good_v2 = fixture_agent_script(&fixture_path, "happy");
        let failed_v3 = fixture_agent_script(&fixture_path, "initialize_error");
        let url_v1 = format!("https://fixture.invalid/{}/v1", uuid::Uuid::new_v4());
        let url_v2 = format!("https://fixture.invalid/{}/v2", uuid::Uuid::new_v4());
        let url_v3 = format!("https://fixture.invalid/{}/v3", uuid::Uuid::new_v4());
        register_test_artifact(&url_v1, good_v1.clone());
        register_test_artifact(&url_v2, good_v2.clone());
        register_test_artifact(&url_v3, failed_v3.clone());

        let manager = Arc::new(AgentRuntimeManager::new(
            pool.clone(),
            workspace_root.clone(),
        ));
        let service_v1 = AgentLifecycleService::new_with_catalog(
            pool.clone(),
            manager.clone(),
            runtime_root.clone(),
            CatalogService::from_catalog(fixture_catalog("1.0.0", &url_v1, &good_v1)),
        );
        let mut install_request = request_for(&service_v1, "install", "1.0.0");
        install_request.catalog_version = "observed-old-catalog".to_string();
        install_request.agent_version = "0.0.1".to_string();
        let installed = service_v1
            .install("default", install_request)
            .await
            .expect("observational request versions must not block install");
        assert_eq!(
            installed.installation.protocol_status,
            ProtocolStatus::Ready
        );
        assert_eq!(installed.installation.catalog_item_version, "1.0.0");
        let first_install_dir = installed
            .installation
            .install_dir
            .clone()
            .expect("managed install directory");
        assert!(first_install_dir.is_dir());

        let service_v2 = AgentLifecycleService::new_with_catalog(
            pool.clone(),
            manager.clone(),
            runtime_root.clone(),
            CatalogService::from_catalog(fixture_catalog("1.1.0", &url_v2, &good_v2)),
        );
        let updated = service_v2
            .install("default", request_for(&service_v2, "update", "1.1.0"))
            .await
            .expect("fixture update");
        assert_eq!(updated.installation.catalog_item_version, "1.1.0");
        let second_install_dir = updated
            .installation
            .install_dir
            .clone()
            .expect("updated managed install directory");
        assert_ne!(first_install_dir, second_install_dir);
        assert!(!first_install_dir.exists());
        assert!(second_install_dir.is_dir());

        let service_v3 = AgentLifecycleService::new_with_catalog(
            pool.clone(),
            manager.clone(),
            runtime_root.clone(),
            CatalogService::from_catalog(fixture_catalog("1.2.0", &url_v3, &failed_v3)),
        );
        let failed = service_v3
            .install("default", request_for(&service_v3, "update", "1.2.0"))
            .await
            .expect_err("failed fixture update");
        assert_eq!(failed.code, "acp_connection_failed");
        let current = service_v3
            .repository
            .get("default", AGENT_ID)
            .await
            .expect("current installation")
            .expect("previous installation remains active");
        assert_eq!(current.catalog_item_version, "1.1.0");
        assert_eq!(current.install_dir, Some(second_install_dir.clone()));
        assert!(second_install_dir.is_dir());
        assert_eq!(count_directories(&runtime_root.join("active")), 1);
        assert_eq!(count_directories(&runtime_root.join(".staging")), 0);

        let recovered_manager = Arc::new(AgentRuntimeManager::new(
            pool.clone(),
            workspace_root.clone(),
        ));
        let warnings = recovered_manager
            .recover_startup("default", &runtime_root)
            .await
            .expect("restart recovery");
        assert!(
            warnings.is_empty(),
            "unexpected recovery warnings: {warnings:?}"
        );
        assert!(recovered_manager
            .registry()
            .get(&AgentId::parse(AGENT_ID).expect("agent id"))
            .is_some());

        let cancellation = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let cancelled = service_v3
            .install_with_cancellation_and_progress(
                "default",
                request_for(&service_v3, "reinstall", "1.2.0"),
                Some(cancellation),
                None,
            )
            .await
            .expect_err("cancelled reinstall");
        assert_eq!(cancelled.code, "cancelled");
        let after_cancel = service_v3
            .repository
            .get("default", AGENT_ID)
            .await
            .expect("installation after cancellation")
            .expect("installation preserved after cancellation");
        assert_eq!(after_cancel.install_dir, Some(second_install_dir));

        drop(pool);
        let _ = std::fs::remove_file(database_path);
        let _ = std::fs::remove_dir_all(runtime_root);
        let _ = std::fs::remove_dir_all(workspace_root);
    }

    fn request_for(
        service: &AgentLifecycleService,
        action: &str,
        agent_version: &str,
    ) -> AgentInstallStartRequest {
        let item = service
            .catalog
            .item(AGENT_ID)
            .expect("fixture catalog item");
        let distribution_id = item.distributions[0].id().to_string();
        AgentInstallStartRequest {
            agent_id: AGENT_ID.to_string(),
            action: action.to_string(),
            catalog_version: service.catalog.catalog().catalog_version.clone(),
            agent_version: agent_version.to_string(),
            distribution_id: distribution_id.clone(),
            preview_token: service
                .catalog
                .preview_token(item, &distribution_id, action),
        }
    }

    fn fixture_catalog(version: &str, url: &str, bytes: &[u8]) -> Catalog {
        Catalog {
            schema: "assetiweave.agent-market/v1".to_string(),
            catalog_version: format!("2099.01.01.{}", version.replace('.', "")),
            generated_at: "2026-08-20T00:00:00Z".to_string(),
            source: CatalogSource {
                kind: "test".to_string(),
                upstream: "local fixture".to_string(),
                upstream_revision: version.to_string(),
            },
            items: vec![CatalogItem {
                id: AGENT_ID.to_string(),
                display_name: "Fixture ACP Agent".to_string(),
                description: "Local ACP lifecycle fixture".to_string(),
                protocol: AgentMarketProtocol::Acp,
                version: version.to_string(),
                core_compatibility: CoreCompatibility {
                    min: "0.0.0".to_string(),
                    max_exclusive: "99.0.0".to_string(),
                },
                capabilities: CatalogCapabilities {
                    purposes: vec!["text_prompt".to_string()],
                    text_prompt: true,
                    model_discovery: false,
                },
                verification: Verification {
                    status: VerificationStatus::Tested,
                    tested_at: "2026-08-20T00:00:00Z".to_string(),
                    evidence_id: Some("fixture-evidence".to_string()),
                },
                upstream: UpstreamSource {
                    registry_id: "fixture".to_string(),
                    homepage: "https://fixture.invalid/agent".to_string(),
                    license: "MIT".to_string(),
                },
                distributions: vec![Distribution::Binary {
                    id: format!("fixture-{version}"),
                    priority: 1,
                    target: Target {
                        os: std::env::consts::OS.to_string(),
                        arch: std::env::consts::ARCH.to_string(),
                    },
                    archive: "none".to_string(),
                    url: url.to_string(),
                    sha256: sha256(bytes),
                    size: Some(bytes.len() as u64),
                    executable: "bin/agent".to_string(),
                    launch_args: Vec::new(),
                    model_discovery_args: None,
                }],
            }],
        }
    }

    fn fixture_agent_script(path: &Path, mode: &str) -> Vec<u8> {
        format!(
            "#!/bin/sh\nexec env ASSETIWEAVE_FAKE_ACP_MODE={mode} node '{}'\n",
            path.display()
        )
        .into_bytes()
    }

    fn sha256(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn count_directories(path: &Path) -> usize {
        std::fs::read_dir(path)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_dir()))
                    .count()
            })
            .unwrap_or_default()
    }
}
