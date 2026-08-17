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
        Ok(Self {
            catalog,
            repository: AgentInstallationRepository::new(pool),
            runtime_manager,
            runtime_root,
        })
    }

    pub(crate) async fn install(
        &self,
        tenant_id: &str,
        request: AgentInstallStartRequest,
    ) -> Result<InstallOutcome, AgentMarketError> {
        self.install_with_cancellation(tenant_id, request, None)
            .await
    }

    pub(crate) async fn install_with_cancellation(
        &self,
        tenant_id: &str,
        request: AgentInstallStartRequest,
        cancellation: Option<Arc<AtomicBool>>,
    ) -> Result<InstallOutcome, AgentMarketError> {
        self.install_with_cancellation_and_progress(tenant_id, request, cancellation, None)
            .await
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

    pub(crate) async fn uninstall(
        &self,
        tenant_id: &str,
        request: AgentUninstallStartRequest,
    ) -> Result<AgentInstallation, AgentMarketError> {
        self.uninstall_with_cancellation(tenant_id, request, None)
            .await
    }

    pub(crate) async fn uninstall_with_cancellation(
        &self,
        tenant_id: &str,
        request: AgentUninstallStartRequest,
        cancellation: Option<Arc<AtomicBool>>,
    ) -> Result<AgentInstallation, AgentMarketError> {
        self.uninstall_with_cancellation_and_progress(tenant_id, request, cancellation, None)
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
