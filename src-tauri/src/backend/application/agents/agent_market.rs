use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};

use crate::backend::application::agents::lifecycle::AgentLifecycleCoordinator;
use crate::backend::application::agents::migration::migrate_legacy_assignments;
use crate::backend::domain::agents::{
    market::CatalogCapabilities, market::Verification, AgentInstallation, AgentMarketProtocol,
    CatalogItem, Distribution, DistributionCandidate, DistributionSelectionContext,
    DistributionSelector, DistributionType, InstallationStatus, Ownership, ProtocolStatus,
    RuntimeStatus, SystemObservation,
};
use crate::backend::infrastructure::agent_market::{
    default_runtime_root, is_safe_managed_install_path, AgentInstallPreviewRequest,
    AgentInstallStartRequest, AgentInstallationView, AgentMarketError, AgentMarketErrorView,
    AgentMarketListRequest, AgentUninstallStartRequest, CatalogCache, CatalogRefreshOutcome,
    InstallContext, Installer, LifecycleTaskPhase, SystemInstaller,
};
use crate::backend::infrastructure::extensions::TrustGate;
use crate::backend::infrastructure::runtime::AppRuntime;
use crate::backend::store::system::AgentInstallationRepository;
use std::path::Path;

pub(crate) use super::agent_market_lifecycle::*;
pub(crate) use super::agent_market_types::*;

impl AppService {
    pub(crate) fn refresh_agent_market_catalog(&self) -> AppResult<AgentMarketRefreshResult> {
        let result = CatalogCache::refresh_default()?;
        let (status, catalog, etag) = match result {
            CatalogRefreshOutcome::Updated { catalog, etag } => ("updated", catalog, etag),
            CatalogRefreshOutcome::NotModified { catalog, etag } => ("not_modified", catalog, etag),
        };
        let active_catalog_version = CatalogCache::best_available()?
            .catalog()
            .catalog_version
            .clone();
        Ok(AgentMarketRefreshResult {
            status: status.to_string(),
            catalog_version: catalog.catalog_version.clone(),
            active_catalog_version,
            downloaded_catalog_version: catalog.catalog_version,
            item_count: catalog.items.len(),
            source: "remote_curated".to_string(),
            etag,
        })
    }

    pub(crate) async fn list_agent_market(
        &self,
        request: AgentMarketListRequest,
    ) -> AppResult<Vec<AgentMarketItemView>> {
        let catalog = CatalogCache::best_available()?;
        let installations = self.list_agent_installations().await?;
        let context = host_distribution_context();
        let query = request
            .query
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let mut views = Vec::new();
        for item in catalog
            .catalog()
            .items
            .iter()
            .filter(|item| {
                request
                    .protocol
                    .as_ref()
                    .is_none_or(|protocol| protocol == &item.protocol)
            })
            .filter(|item| {
                query.is_empty()
                    || format!("{} {} {}", item.id, item.display_name, item.description)
                        .to_ascii_lowercase()
                        .contains(&query)
            })
            .filter(|item| {
                !request.installed_only
                    || installations
                        .iter()
                        .any(|installation| installation.agent_id == item.id)
            })
        {
            let mut item_context = context.clone();
            probe_item_system_distributions(item, &mut item_context).await;
            let candidates = DistributionSelector::select(item, &item_context, None)?;
            let installed = installations
                .iter()
                .find(|installation| installation.agent_id == item.id)
                .map(installation_view);
            let recommended_distribution_id = candidates
                .iter()
                .find(|candidate| candidate.recommended)
                .map(|candidate| candidate.distribution_id.clone());
            views.push(AgentMarketItemView {
                id: item.id.clone(),
                catalog_version: catalog.catalog().catalog_version.clone(),
                display_name: item.display_name.clone(),
                description: item.description.clone(),
                protocol: item.protocol.clone(),
                version: item.version.clone(),
                installability: installability(item, &candidates),
                capabilities: item.capabilities.clone(),
                verification: item.verification.clone(),
                distributions: candidates,
                recommended_distribution_id,
                update_available: installed.as_ref().is_some_and(|installation| {
                    installation.installation_status != "incompatible"
                        && installation.protocol == item.protocol
                        && installation.version != item.version
                }),
                installed,
            });
        }
        Ok(views)
    }

    pub(crate) async fn list_agent_installations(&self) -> AppResult<Vec<AgentInstallation>> {
        let repository = AgentInstallationRepository::new(self.db.pool().clone());
        Ok(repository.list().await?)
    }

    pub(crate) async fn list_installed_agents(&self) -> AppResult<Vec<AgentInstallationView>> {
        Ok(self
            .list_agent_installations()
            .await?
            .iter()
            .map(installation_view)
            .collect())
    }

    pub(crate) async fn get_installed_agent(
        &self,
        agent_id: String,
    ) -> AppResult<AgentInstallationView> {
        self.list_agent_installations()
            .await?
            .into_iter()
            .find(|installation| installation.agent_id == agent_id)
            .map(|installation| installation_view(&installation))
            .ok_or_else(|| {
                AppError::from(AgentMarketError::new(
                    "agent_not_installed",
                    "The Agent is not installed.",
                    false,
                ))
            })
    }

    pub(crate) async fn check_agent_runtime(
        &self,
        agent_id: String,
    ) -> AppResult<AgentInstallationView> {
        let repository = AgentInstallationRepository::new(self.db.pool().clone());
        let mut installation = repository.get(&agent_id).await?.ok_or_else(|| {
            AppError::from(AgentMarketError::new(
                "agent_not_installed",
                "The Agent is not installed.",
                false,
            ))
        })?;
        let now = chrono::Utc::now().to_rfc3339();
        let probe = if installation.resolved_program.is_file() {
            let spec = crate::backend::infrastructure::host_process::HostCommandSpec {
                program: installation.resolved_program.clone(),
                args: vec!["--version".to_string()],
                env: Vec::new(),
                working_dir: None,
                stdin: crate::backend::infrastructure::host_process::HostInput::Null,
                timeout: Duration::from_secs(8),
                stdout_limit: 1024 * 1024,
                stderr_limit: 256 * 1024,
            };
            Some(
                crate::backend::infrastructure::host_process::run_host_command_async(spec, None)
                    .await,
            )
        } else {
            None
        };
        if !installation.resolved_program.is_file() {
            installation.runtime_status = if installation.ownership == Ownership::Managed {
                RuntimeStatus::EntryMissing
            } else {
                RuntimeStatus::RuntimeMissing
            };
            installation.runtime_error_code = Some("agent_entry_missing".to_string());
            installation.runtime_error_message =
                Some("The resolved Agent entry is missing.".to_string());
            installation.installation_status = InstallationStatus::Broken;
        } else if let Some(result) = probe {
            match result {
                Ok(output)
                    if output.status.success()
                        && !output.stdout_truncated
                        && !output.stderr_truncated =>
                {
                    installation.runtime_status = RuntimeStatus::Ready;
                    installation.runtime_error_code = None;
                    installation.runtime_error_message = None;
                    if installation.installation_status == InstallationStatus::Broken {
                        installation.installation_status = InstallationStatus::Ready;
                    }
                }
                Ok(output) => {
                    installation.runtime_status = RuntimeStatus::Failed;
                    installation.runtime_error_code = Some(
                        if output.stdout_truncated || output.stderr_truncated {
                            "runtime_probe_output_limit"
                        } else {
                            "runtime_probe_failed"
                        }
                        .to_string(),
                    );
                    installation.runtime_error_message = Some(
                        "The Agent runtime version probe did not complete successfully."
                            .to_string(),
                    );
                    installation.installation_status = InstallationStatus::Broken;
                }
                Err(error) => {
                    installation.runtime_status = RuntimeStatus::Failed;
                    installation.runtime_error_code = Some(
                        match error {
                            crate::backend::infrastructure::host_process::HostProcessError::Timeout { .. } => {
                                "runtime_probe_timeout"
                            }
                            _ => "runtime_probe_failed",
                        }
                        .to_string(),
                    );
                    installation.runtime_error_message =
                        Some("The Agent runtime version probe failed.".to_string());
                    installation.installation_status = InstallationStatus::Broken;
                }
            }
        } else {
            installation.runtime_status = RuntimeStatus::Ready;
            installation.runtime_error_code = None;
            installation.runtime_error_message = None;
            if installation.installation_status == InstallationStatus::Broken {
                installation.installation_status = InstallationStatus::Ready;
            }
        }
        installation.runtime_checked_at = Some(now.clone());
        installation.updated_at = now;
        repository.update_health(&installation).await?;
        self.agent_runtime_manager.reload().await?;
        Ok(installation_view(&installation))
    }

    pub(crate) async fn inspect_agent_market_item(
        &self,
        agent_id: String,
    ) -> AppResult<AgentMarketItemView> {
        self.list_agent_market(AgentMarketListRequest {
            query: Some(agent_id.clone()),
            protocol: None,
            installed_only: false,
        })
        .await?
        .into_iter()
        .find(|item| item.id == agent_id)
        .ok_or_else(|| {
            AppError::from(AgentMarketError::new(
                "agent_not_found",
                "The selected Agent is not in the curated catalog.",
                false,
            ))
        })
    }
}

#[cfg(test)]
#[path = "agent_market_tests.rs"]
mod tests;
