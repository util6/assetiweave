use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};

use crate::backend::application::agents::lifecycle::AgentLifecycleCoordinator;
use crate::backend::application::agents::migration::migrate_legacy_assignments;
use crate::backend::domain::agents::market::catalog::CatalogService;
use crate::backend::domain::agents::{
    market::CatalogCapabilities, market::Verification, AgentInstallation, AgentMarketProtocol,
    CatalogItem, Distribution, DistributionCandidate, DistributionSelectionContext,
    DistributionSelector, DistributionType, InstallationStatus, Ownership, ProtocolStatus,
    RuntimeStatus,
};
use crate::backend::infrastructure::agent_market::{
    default_runtime_root, is_safe_managed_install_path, AgentInstallPreviewRequest,
    AgentInstallStartRequest, AgentInstallationView, AgentMarketError, AgentMarketErrorView,
    AgentMarketListRequest, AgentUninstallStartRequest, CatalogCache, CatalogRefreshOutcome,
    InstallContext, Installer, LifecycleTaskPhase,
};
use crate::backend::infrastructure::extensions::TrustGate;
use crate::backend::infrastructure::runtime::AppRuntime;
use crate::backend::store::system::{
    AgentCatalogReleaseMeta, AgentInstallationRepository, ExtensionCatalogRelease,
    ExtensionCatalogReleaseRepository,
};
use std::path::Path;

pub use super::agent_market_lifecycle::*;
pub use super::agent_market_types::*;

impl AppService {
    pub async fn ensure_agent_catalog_releases_seeded(&self) -> AppResult<()> {
        let repo = ExtensionCatalogReleaseRepository::new(self.db.pool().clone());
        repo.ensure_agent_seeded().await?;
        Ok(())
    }

    pub async fn refresh_agent_market_catalog(&self) -> AppResult<AgentMarketRefreshResult> {
        self.ensure_agent_catalog_releases_seeded().await?;
        let repo = ExtensionCatalogReleaseRepository::new(self.db.pool().clone());
        let bundled =
            CatalogService::bundled().map_err(|e| AppError::from(AgentMarketError::Catalog(e)))?;
        let catalog = (*bundled.catalog()).clone();
        let cached_etag = repo
            .get_latest_etag("assetiweave-curated")
            .await
            .ok()
            .flatten();

        let mut upstream_versions = std::collections::HashMap::new();
        let items = catalog.items.clone();
        if let Ok(probed) = tokio::task::spawn_blocking(move || {
            if let Ok(client) = crate::backend::infrastructure::http_client::shared_http_client() {
                crate::backend::infrastructure::agent_market::UpstreamProbe::probe_catalog_items(
                    &client, &items,
                )
            } else {
                std::collections::HashMap::new()
            }
        })
        .await
        {
            upstream_versions = probed;
        }

        let now = chrono::Utc::now().to_rfc3339();
        let mut releases = Vec::new();
        for item in &catalog.items {
            let upstream = upstream_versions.get(&item.id);
            if let Ok(release) = ExtensionCatalogRelease::from_agent_item(
                "assetiweave-curated",
                &catalog.catalog_version,
                item,
                upstream,
                cached_etag.as_deref(),
                &now,
            ) {
                releases.push(release);
            }
        }
        if !releases.is_empty() {
            repo.batch_upsert(&releases).await?;
        }

        Ok(AgentMarketRefreshResult {
            status: "updated".to_string(),
            catalog_version: catalog.catalog_version.clone(),
            active_catalog_version: catalog.catalog_version.clone(),
            downloaded_catalog_version: catalog.catalog_version.clone(),
            item_count: catalog.items.len(),
            source: "remote_curated".to_string(),
            etag: cached_etag,
        })
    }

    pub async fn list_agent_market(
        &self,
        request: AgentMarketListRequest,
    ) -> AppResult<Vec<AgentMarketItemView>> {
        self.ensure_agent_catalog_releases_seeded().await?;
        let repo = ExtensionCatalogReleaseRepository::new(self.db.pool().clone());
        let releases = repo.list_by_capability("agent").await?;
        let installations = self.list_agent_installations().await?;
        let context = host_distribution_context();
        let query = request
            .query
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();

        let mut seen = std::collections::HashSet::new();
        let mut metas = Vec::new();
        for release in releases {
            if seen.insert(release.extension_id.clone()) {
                if let Ok(meta) = release.parse_agent_meta() {
                    metas.push(meta);
                }
            }
        }
        if metas.is_empty() {
            if let Ok(bundled) = CatalogService::bundled() {
                for item in &bundled.catalog().items {
                    metas.push(AgentCatalogReleaseMeta {
                        catalog_version: bundled.catalog().catalog_version.clone(),
                        item: item.clone(),
                        upstream_info: None,
                        etag: None,
                    });
                }
            }
        }

        let mut views = Vec::new();
        for meta in metas
            .iter()
            .filter(|meta| {
                request
                    .protocol
                    .as_ref()
                    .is_none_or(|protocol| protocol == &meta.item.protocol)
            })
            .filter(|meta| {
                query.is_empty()
                    || format!(
                        "{} {} {}",
                        meta.item.id, meta.item.display_name, meta.item.description
                    )
                    .to_ascii_lowercase()
                    .contains(&query)
            })
            .filter(|meta| {
                !request.installed_only
                    || installations
                        .iter()
                        .any(|installation| installation.agent_id == meta.item.id)
            })
        {
            let item = &meta.item;
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
            let update_available = installed.as_ref().is_some_and(|installation| {
                if installation.installation_status == "incompatible"
                    || installation.protocol != item.protocol
                {
                    return false;
                }
                let upstream_newer = meta
                    .upstream_info
                    .as_ref()
                    .map(|up| {
                        crate::backend::infrastructure::agent_market::UpstreamProbe::semver_is_newer(
                            &up.version,
                            &installation.version,
                        )
                    })
                    .unwrap_or(false);
                let catalog_newer =
                    crate::backend::infrastructure::agent_market::UpstreamProbe::semver_is_newer(
                        &item.version,
                        &installation.version,
                    );
                upstream_newer || catalog_newer
            });
            let mut installed_view = installed;
            if let Some(ref mut inst_view) = installed_view {
                inst_view.update_available = update_available;
            }
            views.push(AgentMarketItemView {
                id: item.id.clone(),
                catalog_version: meta.catalog_version.clone(),
                display_name: item.display_name.clone(),
                description: item.description.clone(),
                protocol: item.protocol.clone(),
                version: item.version.clone(),
                installability: installability(item, &candidates),
                capabilities: item.capabilities.clone(),
                verification: item.verification.clone(),
                distributions: candidates,
                recommended_distribution_id,
                update_available,
                installed: installed_view,
            });
        }
        Ok(views)
    }

    pub async fn list_agent_installations(&self) -> AppResult<Vec<AgentInstallation>> {
        let repository = AgentInstallationRepository::new(self.db.pool().clone());
        Ok(repository.list().await?)
    }

    pub async fn list_installed_agents(&self) -> AppResult<Vec<AgentInstallationView>> {
        self.ensure_agent_catalog_releases_seeded().await?;
        let repo = ExtensionCatalogReleaseRepository::new(self.db.pool().clone());
        let releases = repo.list_by_capability("agent").await.unwrap_or_default();
        let mut metas: std::collections::HashMap<String, AgentCatalogReleaseMeta> =
            std::collections::HashMap::new();
        for release in releases {
            if !metas.contains_key(&release.extension_id) {
                if let Ok(meta) = release.parse_agent_meta() {
                    metas.insert(release.extension_id.clone(), meta);
                }
            }
        }
        Ok(self
            .list_agent_installations()
            .await?
            .iter()
            .map(|inst| {
                let mut view = installation_view(inst);
                if let Some(meta) = metas.get(&inst.agent_id) {
                    let item = &meta.item;
                    let is_compatible = view.installation_status != "incompatible"
                        && view.protocol == item.protocol;
                    if is_compatible {
                        let upstream_newer = meta
                            .upstream_info
                            .as_ref()
                            .map(|up| {
                                crate::backend::infrastructure::agent_market::UpstreamProbe::semver_is_newer(
                                    &up.version,
                                    &view.version,
                                )
                            })
                            .unwrap_or(false);
                        let catalog_newer =
                            crate::backend::infrastructure::agent_market::UpstreamProbe::semver_is_newer(
                                &item.version,
                                &view.version,
                            );
                        view.update_available = upstream_newer || catalog_newer;
                    }
                }
                view
            })
            .collect())
    }

    pub async fn get_installed_agent(&self, agent_id: String) -> AppResult<AgentInstallationView> {
        let repo = ExtensionCatalogReleaseRepository::new(self.db.pool().clone());
        let meta = repo.get_latest_agent_meta(&agent_id).await.ok().flatten();
        self.list_agent_installations()
            .await?
            .into_iter()
            .find(|installation| installation.agent_id == agent_id)
            .map(|installation| {
                let mut view = installation_view(&installation);
                if let Some((_, meta)) = meta {
                    let item = meta.item;
                    let is_compatible = view.installation_status != "incompatible"
                        && view.protocol == item.protocol;
                    if is_compatible {
                        let upstream_newer = meta
                            .upstream_info
                            .as_ref()
                            .map(|up| {
                                crate::backend::infrastructure::agent_market::UpstreamProbe::semver_is_newer(
                                    &up.version,
                                    &view.version,
                                )
                            })
                            .unwrap_or(false);
                        let catalog_newer =
                            crate::backend::infrastructure::agent_market::UpstreamProbe::semver_is_newer(
                                &item.version,
                                &view.version,
                            );
                        view.update_available = upstream_newer || catalog_newer;
                    }
                }
                view
            })
            .ok_or_else(|| {
                AppError::from(AgentMarketError::new(
                    "agent_not_installed",
                    "The Agent is not installed.",
                    false,
                ))
            })
    }

    pub async fn check_agent_runtime(&self, agent_id: String) -> AppResult<AgentInstallationView> {
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

    pub async fn inspect_agent_market_item(
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
