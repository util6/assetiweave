use std::sync::{atomic::AtomicBool, Arc};

use super::{is_safe_managed_install_path, market_error, AgentLifecycleService};
use crate::backend::agent_market::types::{
    AgentInstallation, AgentMarketError, AgentUninstallStartRequest, LifecycleTaskPhase, Ownership,
};
use crate::backend::{
    agent_market::runtime::AgentPackageSystem,
    extension_kernel::{DomainPackageSystem, PackageKind},
};

pub(crate) async fn run(
    service: &AgentLifecycleService,
    tenant_id: &str,
    request: AgentUninstallStartRequest,
    cancellation: Option<Arc<AtomicBool>>,
    phase_sink: Option<Arc<dyn Fn(LifecycleTaskPhase) + Send + Sync>>,
) -> Result<AgentInstallation, AgentMarketError> {
    if let Some(sink) = phase_sink.as_ref() {
        sink(LifecycleTaskPhase::Preparing);
    }
    let mutation_gate = service.runtime_manager.mutation_gate(&request.agent_id);
    let _mutation_lease = mutation_gate.write().await;
    let installation = service
        .repository
        .get(tenant_id, &request.agent_id)
        .await
        .map_err(|error| market_error("storage_failed", error, true))?
        .ok_or_else(|| market_error("agent_not_installed", "The Agent is not installed.", false))?;
    let item = service.catalog.item(&request.agent_id).ok_or_else(|| {
        market_error(
            "agent_not_found",
            "The installed Agent is no longer in the curated catalog.",
            false,
        )
    })?;
    let expected = service
        .catalog
        .preview_token(item, &installation.distribution_id, "uninstall");
    if expected != request.preview_token {
        return Err(market_error(
            "preview_stale",
            "The uninstall preview is stale; preview the operation again.",
            true,
        ));
    }
    if service.runtime_manager.agent_in_use(&request.agent_id) {
        return Err(market_error(
            "agent_in_use",
            "The Agent has an active execution.",
            true,
        ));
    }
    if cancellation
        .as_ref()
        .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::SeqCst))
    {
        return Err(market_error(
            "cancelled",
            "Agent uninstall was cancelled.",
            true,
        ));
    }
    if installation.ownership == Ownership::Managed
        && !installation.install_dir.as_ref().is_some_and(|path| {
            is_safe_managed_install_path(&service.runtime_root, &installation.installation_id, path)
        })
    {
        return Err(market_error(
            "unsafe_install_path",
            "The managed Agent path is outside the owned runtime layout.",
            false,
        ));
    }
    let package_system = AgentPackageSystem::from_installation(&installation)
        .map_err(|error| market_error("uninstall_failed", error, false))?;
    if package_system.kind() != PackageKind::Agent {
        return Err(market_error(
            "uninstall_failed",
            "The Agent package system returned the wrong package kind.",
            false,
        ));
    }
    // Capability assignment cleanup is deliberately explicit at the adapter
    // boundary. This service only removes a row after the caller has supplied
    // the requested references; it never invents a replacement assignment.
    if let Some(sink) = phase_sink.as_ref() {
        sink(LifecycleTaskPhase::ActivatingDatabase);
    }
    service
        .repository
        .delete(tenant_id, &request.agent_id)
        .await
        .map_err(|error| market_error("uninstall_failed", error, true))?;
    if let Some(sink) = phase_sink.as_ref() {
        sink(LifecycleTaskPhase::ReloadingRegistry);
    }
    if let Err(error) = service.runtime_manager.reload(tenant_id).await {
        let _ = service.repository.upsert_active(&installation).await;
        return Err(market_error("registry_reload_failed", error, true));
    }
    if installation.ownership == Ownership::Managed {
        if let Some(sink) = phase_sink.as_ref() {
            sink(LifecycleTaskPhase::CleaningUp);
        }
        if let Some(path) = installation.install_dir.as_ref() {
            let _ = std::fs::remove_dir_all(path);
        }
    }
    Ok(installation)
}
