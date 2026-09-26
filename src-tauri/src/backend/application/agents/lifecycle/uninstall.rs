use std::sync::{atomic::AtomicBool, Arc};

use super::{market_error, AgentLifecycleCoordinator};
use crate::backend::{
    domain::agents::{AgentInstallation, Ownership},
    infrastructure::{
        agent_market::{
            error::{AgentMarketError, LifecycleTaskPhase},
            layout::is_safe_managed_install_path,
            runtime::AgentPackageSystem,
            types::AgentUninstallStartRequest,
        },
        extensions::{DomainPackageSystem, PackageKind},
    },
};

pub(crate) async fn run(
    coordinator: &AgentLifecycleCoordinator,
    request: AgentUninstallStartRequest,
    cancellation: Option<Arc<AtomicBool>>,
    phase_sink: Option<Arc<dyn Fn(LifecycleTaskPhase) + Send + Sync>>,
) -> Result<AgentInstallation, AgentMarketError> {
    coordinator
        .runtime_manager
        .invalidate_agent_state(&request.agent_id)
        .await;
    if let Some(sink) = phase_sink.as_ref() {
        sink(LifecycleTaskPhase::Preparing);
    }
    let mutation_gate = coordinator.runtime_manager.mutation_gate(&request.agent_id);
    let _mutation_lease = mutation_gate.write().await;

    let installation = coordinator
        .repository
        .get(&request.agent_id)
        .await
        .map_err(|error| market_error("storage_failed", error, true))?
        .ok_or_else(|| market_error("agent_not_installed", "The Agent is not installed.", false))?;

    let item = coordinator.catalog.item(&request.agent_id).ok_or_else(|| {
        market_error(
            "agent_not_found",
            "The installed Agent is no longer in the curated catalog.",
            false,
        )
    })?;

    let expected =
        coordinator
            .catalog
            .preview_token(item, &installation.distribution_id, "uninstall");
    if expected != request.preview_token {
        return Err(market_error(
            "preview_stale",
            "The uninstall preview is stale; preview the operation again.",
            true,
        ));
    }

    if coordinator.runtime_manager.agent_in_use(&request.agent_id) {
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
            is_safe_managed_install_path(
                &coordinator.runtime_root,
                &installation.installation_id,
                path,
            )
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

    if let Some(sink) = phase_sink.as_ref() {
        sink(LifecycleTaskPhase::ActivatingDatabase);
    }

    coordinator
        .repository
        .delete(&request.agent_id)
        .await
        .map_err(|error| market_error("uninstall_failed", error, true))?;

    if let Some(sink) = phase_sink.as_ref() {
        sink(LifecycleTaskPhase::ReloadingRegistry);
    }

    if let Err(error) = coordinator.runtime_manager.reload().await {
        let _ = coordinator.repository.upsert_active(&installation).await;
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
