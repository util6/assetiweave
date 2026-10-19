use std::sync::{atomic::AtomicBool, Arc};

use super::{market_error, AgentLifecycleCoordinator};
use crate::backend::{
    domain::agents::{
        AgentInstallation, Distribution, InstallationStatus, Ownership, ProtocolStatus,
        RuntimeStatus,
    },
    infrastructure::agent_market::{
        error::{AgentMarketError, LifecycleTaskPhase},
        layout::{ensure_runtime_root, is_safe_managed_install_path},
        materialize::materialize_and_activate,
        types::AgentInstallStartRequest,
        CatalogCache,
    },
};

#[derive(Clone, Debug)]
pub struct InstallOutcome {
    pub installation: AgentInstallation,
    pub warnings: Vec<String>,
}

pub async fn run(
    coordinator: &AgentLifecycleCoordinator,
    request: AgentInstallStartRequest,
    cancellation: Option<Arc<AtomicBool>>,
    phase_sink: Option<Arc<dyn Fn(LifecycleTaskPhase) + Send + Sync>>,
    progress_sink: Option<Arc<dyn Fn(u64, Option<u64>) + Send + Sync>>,
) -> Result<InstallOutcome, AgentMarketError> {
    coordinator
        .runtime_manager
        .invalidate_agent_state(&request.agent_id)
        .await;
    let mutation_gate = coordinator.runtime_manager.mutation_gate(&request.agent_id);
    let _mutation_lease = mutation_gate.write().await;

    let item = coordinator.catalog.item(&request.agent_id).ok_or_else(|| {
        market_error(
            "agent_not_found",
            "The selected Agent is not in the curated catalog.",
            false,
        )
    })?;

    if !matches!(request.action.as_str(), "install" | "update" | "reinstall") {
        return Err(market_error(
            "invalid_action",
            "Unsupported Agent installation action.",
            false,
        ));
    }

    let distribution = item
        .distributions
        .iter()
        .find(|distribution| distribution.id() == request.distribution_id)
        .ok_or_else(|| {
            market_error(
                "distribution_unsupported",
                "The selected distribution is not in the catalog item.",
                false,
            )
        })?;

    if coordinator
        .catalog
        .preview_token(item, &request.distribution_id, &request.action)
        != request.preview_token
    {
        return Err(market_error(
            "preview_stale",
            "The installation preview is stale; preview the operation again.",
            true,
        ));
    }

    let current = coordinator
        .repository
        .get(&request.agent_id)
        .await
        .map_err(|error| market_error("storage_failed", error, true))?;

    match request.action.as_str() {
        "install" if current.is_some() => {
            return Err(market_error(
                "agent_already_installed",
                "The Agent is already installed; choose update or reinstall.",
                false,
            ));
        }
        "update" if current.is_none() => {
            return Err(market_error(
                "agent_not_installed",
                "The Agent is not installed; choose install.",
                false,
            ));
        }
        "reinstall" if current.is_none() => {
            return Err(market_error(
                "agent_not_installed",
                "The Agent is not installed; choose install.",
                false,
            ));
        }
        _ => {}
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
            "Agent installation was cancelled.",
            true,
        ));
    }

    let target_version = if request.action == "update" {
        CatalogCache::in_app_cache()
            .and_then(|c| c.get_upstream_version(&item.id))
            .filter(|up_ver| {
                if let Some(cur) = current.as_ref() {
                    crate::backend::infrastructure::agent_market::UpstreamProbe::semver_is_newer(
                        up_ver,
                        &cur.agent_version,
                    )
                } else {
                    false
                }
            })
            .unwrap_or_else(|| item.version.clone())
    } else {
        item.version.clone()
    };

    let mut item_effective = item.clone();
    let mut distribution_effective = distribution.clone();
    if target_version != item.version {
        item_effective.version = target_version.clone();
        match &mut distribution_effective {
            Distribution::Npx { version, .. } => {
                *version = target_version.clone();
            }
            Distribution::Uvx { version, .. } => {
                *version = target_version.clone();
            }
            Distribution::Binary {
                url,
                size,
                sha256,
                target,
                executable,
                launch_args,
                ..
            } => {
                let upstream_info =
                    CatalogCache::in_app_cache().and_then(|c| c.get_upstream_info(&item.id));
                let matched_target = upstream_info
                    .as_ref()
                    .and_then(|info| info.find_binary_target(&target.os, &target.arch));

                if let Some(upstream_target) = matched_target {
                    *url = upstream_target.archive.clone();
                    if let Some(cmd) = &upstream_target.cmd {
                        let clean_cmd = cmd.trim_start_matches("./");
                        *executable = clean_cmd.to_string();
                    }
                    if let Some(args) = &upstream_target.args {
                        *launch_args = args.clone();
                    }
                    if let Some(hash) = &upstream_target.sha256 {
                        *sha256 = hash.clone();
                    } else {
                        *sha256 = String::new();
                    }
                    *size = None;
                } else {
                    let mut new_url = url.clone();
                    if new_url.contains(&item.version) {
                        new_url = new_url.replace(&item.version, &target_version);
                    }
                    if new_url.contains("agy-acp-server-agy_acp_server_") {
                        new_url =
                            new_url.replace("agy-acp-server-agy_acp_server_", "agy-acp-server-");
                    }
                    *url = new_url;
                    *size = None;
                    *sha256 = String::new();
                }
            }
            _ => {}
        }
    }

    ensure_runtime_root(&coordinator.runtime_root)?;
    let task_id = uuid::Uuid::new_v4().to_string();
    let staging = coordinator.runtime_root.join(".staging").join(&task_id);
    std::fs::create_dir_all(&staging)
        .map_err(|error| market_error("staging_unavailable", error, true))?;

    let materialize_res = materialize_and_activate(
        &item_effective,
        &distribution_effective,
        &staging,
        &coordinator.runtime_root,
        cancellation,
        phase_sink.clone(),
        progress_sink.clone(),
    )
    .await;

    let outcome = match materialize_res {
        Ok(outcome) => outcome,
        Err(err) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(err);
        }
    };

    if let Some(sink) = phase_sink.as_ref() {
        sink(LifecycleTaskPhase::ActivatingDatabase);
    }

    let now = chrono::Utc::now().to_rfc3339();
    let installation = AgentInstallation {
        agent_id: item.id.clone(),
        installation_id: outcome.installation_id.clone(),
        display_name: item.display_name.clone(),
        catalog_item_version: target_version.clone(),
        agent_version: outcome.version.clone(),
        protocol: item.protocol.clone(),
        distribution_id: distribution.id().to_string(),
        distribution_type: distribution.distribution_type(),
        ownership: outcome.ownership.clone(),
        install_dir: outcome.active_dir.clone(),
        resolved_program: outcome.active_program.clone(),
        args: outcome.args.clone(),
        definition_json: outcome.definition_json,
        integrity_json: outcome.integrity_json,
        source_registry: item.upstream.registry_id.clone(),
        catalog_version: coordinator.catalog.catalog().catalog_version.clone(),
        enabled: true,
        installation_status: InstallationStatus::Ready,
        runtime_status: RuntimeStatus::Ready,
        runtime_error_code: None,
        runtime_error_message: None,
        runtime_checked_at: Some(now.clone()),
        protocol_status: outcome.protocol_status.clone(),
        protocol_error_code: outcome.protocol_error_code,
        protocol_error_message: outcome.protocol_error_message,
        protocol_checked_at: Some(now.clone()),
        model_status: Some(if outcome.protocol_status == ProtocolStatus::Ready {
            "ready".to_string()
        } else {
            "failed".to_string()
        }),
        model_error_code: None,
        model_checked_at: Some(now.clone()),
        installed_at: now.clone(),
        updated_at: now,
    };

    coordinator
        .repository
        .upsert_active(&installation)
        .await
        .map_err(|error| {
            if let Some(path) = installation.install_dir.as_ref() {
                if is_safe_managed_install_path(
                    &coordinator.runtime_root,
                    &installation.installation_id,
                    path,
                ) {
                    let _ = std::fs::remove_dir_all(path);
                }
            }
            market_error("activation_failed", error, true)
        })?;

    if let Some(sink) = phase_sink.as_ref() {
        sink(LifecycleTaskPhase::ReloadingRegistry);
    }

    if let Err(error) = coordinator.runtime_manager.reload().await {
        let restore_result = match current.as_ref() {
            Some(previous) => coordinator.repository.upsert_active(previous).await,
            None => coordinator.repository.delete(&installation.agent_id).await,
        };
        let _ = coordinator.runtime_manager.reload().await;
        if let Some(path) = installation.install_dir.as_ref() {
            if is_safe_managed_install_path(
                &coordinator.runtime_root,
                &installation.installation_id,
                path,
            ) {
                let _ = std::fs::remove_dir_all(path);
            }
        }
        if let Err(restore_error) = restore_result {
            return Err(market_error(
                "activation_rollback_failed",
                restore_error,
                true,
            ));
        }
        return Err(market_error("registry_reload_failed", error, true));
    }

    if let Some(sink) = phase_sink.as_ref() {
        sink(LifecycleTaskPhase::CleaningUp);
    }

    let mut warnings = outcome.warnings;
    if let Some(previous) = current.as_ref() {
        if previous.ownership == Ownership::Managed
            && previous.install_dir != installation.install_dir
        {
            if let Some(path) = previous.install_dir.as_ref() {
                if is_safe_managed_install_path(
                    &coordinator.runtime_root,
                    &previous.installation_id,
                    path,
                ) {
                    if let Err(error) = std::fs::remove_dir_all(path) {
                        warnings.push(format!("old installation cleanup pending: {error}"));
                    }
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(coordinator.runtime_root.join(".staging").join(task_id));

    Ok(InstallOutcome {
        installation,
        warnings,
    })
}
