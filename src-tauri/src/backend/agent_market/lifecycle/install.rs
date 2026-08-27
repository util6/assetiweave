use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use crate::backend::{
    agents::{
        registry::AgentRegistry,
        types::{
            AgentCommandDefinition, AgentDefinition, AgentId, AgentProtocol,
            DeclaredAgentCapabilities,
        },
    },
    ai_execution::{check_agent_connection_blocking, executor::AgentExecutor},
};

use super::{
    ensure_runtime_root, is_safe_managed_install_path, market_error, AgentLifecycleService,
};
use crate::backend::agent_market::{
    installers::{
        binary::BinaryInstaller, npx::NpxInstaller, system::SystemInstaller, uvx::UvxInstaller,
        InstallContext, Installer, MAX_BINARY_BYTES,
    },
    types::{
        AgentInstallStartRequest, AgentInstallation, AgentMarketError, AgentMarketProtocol,
        Distribution, InstallationStatus, LifecycleTaskPhase, Ownership, ProtocolStatus,
        RuntimeStatus,
    },
};

#[derive(Clone, Debug)]
pub(crate) struct InstallOutcome {
    pub(crate) installation: AgentInstallation,
    pub(crate) warnings: Vec<String>,
}

pub(crate) async fn run(
    service: &AgentLifecycleService,
    tenant_id: &str,
    request: AgentInstallStartRequest,
    cancellation: Option<Arc<AtomicBool>>,
    phase_sink: Option<Arc<dyn Fn(LifecycleTaskPhase) + Send + Sync>>,
) -> Result<InstallOutcome, AgentMarketError> {
    let mutation_gate = service.runtime_manager.mutation_gate(&request.agent_id);
    let _mutation_lease = mutation_gate.write().await;
    let item = service.catalog.item(&request.agent_id).ok_or_else(|| {
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
    if service
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

    let current = service
        .repository
        .get(tenant_id, &request.agent_id)
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
            "Agent installation was cancelled.",
            true,
        ));
    }
    ensure_runtime_root(&service.runtime_root)?;
    let task_id = uuid::Uuid::new_v4().to_string();
    let staging = service.runtime_root.join(".staging").join(&task_id);
    std::fs::create_dir_all(&staging)
        .map_err(|error| market_error("staging_unavailable", error.to_string(), true))?;
    let result = materialize_and_activate(
        service,
        tenant_id,
        item,
        distribution,
        current.as_ref(),
        &task_id,
        staging.clone(),
        cancellation,
        phase_sink,
    )
    .await;
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

async fn materialize_and_activate(
    service: &AgentLifecycleService,
    tenant_id: &str,
    item: &crate::backend::agent_market::types::CatalogItem,
    distribution: &Distribution,
    current: Option<&AgentInstallation>,
    task_id: &str,
    staging: PathBuf,
    cancellation: Option<Arc<AtomicBool>>,
    phase_sink: Option<Arc<dyn Fn(LifecycleTaskPhase) + Send + Sync>>,
) -> Result<InstallOutcome, AgentMarketError> {
    let mut context = InstallContext::new(staging.clone(), item.version.clone());
    context.installation_id = uuid::Uuid::new_v4().to_string();
    context.timeout = Duration::from_secs(10 * 60);
    context.cancellation = cancellation;
    context.phase_sink = phase_sink;
    context.report_phase(LifecycleTaskPhase::Preparing);
    context.report_phase(match distribution {
        Distribution::System { .. } => LifecycleTaskPhase::ProbingRuntime,
        Distribution::Binary { .. } => LifecycleTaskPhase::Downloading,
        Distribution::Npx { .. } | Distribution::Uvx { .. } => LifecycleTaskPhase::Installing,
    });
    let materialized = match distribution {
        Distribution::System { .. } => {
            SystemInstaller::default().materialize(distribution, &context)
        }
        Distribution::Binary { url, size, .. } => {
            context.report_phase(LifecycleTaskPhase::ValidatingIntegrity);
            let bytes = download_artifact(url, *size, &context)?;
            BinaryInstaller.materialize_bytes(distribution, &context, &bytes)
        }
        Distribution::Npx { .. } => NpxInstaller::default().materialize(distribution, &context),
        Distribution::Uvx { .. } => UvxInstaller::default().materialize(distribution, &context),
    }
    .map_err(|error| install_error(error))?;
    context.report_phase(LifecycleTaskPhase::ValidatingLayout);

    let definition = definition_for(item, distribution, &materialized)?;
    context.report_phase(LifecycleTaskPhase::ProbingProtocol);
    let (protocol_status, protocol_error, mut warnings) =
        conformance(&definition, &service.runtime_root);
    if !matches!(protocol_status, ProtocolStatus::Ready)
        && matches!(materialized.ownership, Ownership::Managed)
    {
        return Err(protocol_error.clone().unwrap_or_else(|| {
            market_error(
                "protocol_failed",
                "Agent protocol conformance failed.",
                true,
            )
        }));
    }

    let mut active_program = materialized.resolved_program.clone();
    let mut active_dir = materialized.install_dir.clone();
    if matches!(materialized.ownership, Ownership::Managed) {
        let target = service
            .runtime_root
            .join("active")
            .join(&context.installation_id);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| market_error("activation_failed", error.to_string(), true))?;
        }
        let staging_root = staging.canonicalize().map_err(|error| {
            market_error("installation_layout_invalid", error.to_string(), false)
        })?;
        let relative = materialized
            .resolved_program
            .strip_prefix(&staging_root)
            .map_err(|_| {
                market_error(
                    "installation_layout_invalid",
                    "The resolved program is outside staging.",
                    false,
                )
            })?;
        std::fs::rename(&staging, &target)
            .map_err(|error| market_error("activation_failed", error.to_string(), true))?;
        active_program = target.join(relative);
        active_dir = Some(target);
    }

    context.report_phase(LifecycleTaskPhase::ActivatingDatabase);
    let now = chrono::Utc::now().to_rfc3339();
    let protocol_error_code = protocol_error.as_ref().map(|error| error.code.clone());
    let protocol_error_message = protocol_error.as_ref().map(|error| error.message.clone());
    let protocol_status_for_row = protocol_status.clone();
    let definition_json = serde_json::json!({
        "id": item.id,
        "display_name": item.display_name,
        "protocol": item.protocol.as_str(),
        "program": active_program.to_string_lossy(),
        "args": materialized.args,
        "env": [],
        "modelDiscoveryArgs": model_discovery_args(distribution),
    });
    let installation = AgentInstallation {
        tenant_id: tenant_id.to_string(),
        agent_id: item.id.clone(),
        installation_id: context.installation_id.clone(),
        display_name: item.display_name.clone(),
        catalog_item_version: item.version.clone(),
        agent_version: materialized.version.clone(),
        protocol: item.protocol.clone(),
        distribution_id: distribution.id().to_string(),
        distribution_type: distribution.distribution_type(),
        ownership: materialized.ownership.clone(),
        install_dir: active_dir,
        resolved_program: active_program,
        args: materialized.args.clone(),
        definition_json,
        integrity_json: materialized.integrity.clone(),
        source_registry: item.upstream.registry_id.clone(),
        catalog_version: service.catalog.catalog().catalog_version.clone(),
        enabled: true,
        installation_status: InstallationStatus::Ready,
        runtime_status: RuntimeStatus::Ready,
        runtime_error_code: None,
        runtime_error_message: None,
        runtime_checked_at: Some(now.clone()),
        protocol_status: protocol_status_for_row,
        protocol_error_code,
        protocol_error_message,
        protocol_checked_at: Some(now.clone()),
        model_status: Some(if protocol_status == ProtocolStatus::Ready {
            "ready".to_string()
        } else {
            "failed".to_string()
        }),
        model_error_code: None,
        model_checked_at: Some(now.clone()),
        installed_at: now.clone(),
        updated_at: now,
    };
    service
        .repository
        .upsert_active(&installation)
        .await
        .map_err(|error| {
            if let Some(path) = installation.install_dir.as_ref() {
                if is_safe_managed_install_path(
                    &service.runtime_root,
                    &installation.installation_id,
                    path,
                ) {
                    let _ = std::fs::remove_dir_all(path);
                }
            }
            market_error("activation_failed", error, true)
        })?;
    context.report_phase(LifecycleTaskPhase::ReloadingRegistry);
    if let Err(error) = service.runtime_manager.reload(tenant_id).await {
        let restore_result = match current {
            Some(previous) => service.repository.upsert_active(previous).await,
            None => {
                service
                    .repository
                    .delete(tenant_id, &installation.agent_id)
                    .await
            }
        };
        let _ = service.runtime_manager.reload(tenant_id).await;
        if let Some(path) = installation.install_dir.as_ref() {
            if is_safe_managed_install_path(
                &service.runtime_root,
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

    context.report_phase(LifecycleTaskPhase::CleaningUp);
    if let Some(previous) = current {
        if previous.ownership == Ownership::Managed
            && previous.install_dir != installation.install_dir
        {
            if let Some(path) = previous.install_dir.as_ref() {
                if is_safe_managed_install_path(
                    &service.runtime_root,
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
    let _ = std::fs::remove_dir_all(service.runtime_root.join(".staging").join(task_id));
    Ok(InstallOutcome {
        installation,
        warnings,
    })
}

fn download_artifact(
    url: &str,
    expected_size: Option<u64>,
    context: &InstallContext,
) -> Result<Vec<u8>, AgentMarketError> {
    #[cfg(test)]
    if let Some(bytes) = test_artifact(url) {
        return Ok(bytes);
    }
    if !crate::backend::agent_market::types::is_safe_artifact_url(url) {
        return Err(market_error(
            "artifact_invalid",
            "The binary artifact URL is not an allowed HTTPS endpoint.",
            false,
        ));
    }
    if expected_size.is_some_and(|size| size > MAX_BINARY_BYTES) {
        return Err(market_error(
            "artifact_size_invalid",
            "The Agent artifact exceeds the catalog size limit.",
            false,
        ));
    }
    let agent = ureq::AgentBuilder::new().timeout(context.timeout).build();
    let response = agent
        .get(url)
        .set("User-Agent", "AssetIWeave/agent-market")
        .call()
        .map_err(|_| market_error("download_failed", "Agent artifact download failed.", true))?;
    let mut bytes = Vec::new();
    let mut reader = response.into_reader();
    let mut buffer = [0_u8; 8192];
    loop {
        if crate::backend::agent_market::installers::is_cancelled(context) {
            return Err(market_error(
                "cancelled",
                "Agent installation was cancelled.",
                true,
            ));
        }
        let count = reader
            .read(&mut buffer)
            .map_err(|error| market_error("download_failed", error.to_string(), true))?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.len() as u64 > MAX_BINARY_BYTES {
            return Err(market_error(
                "artifact_size_invalid",
                "The Agent artifact exceeds the catalog size limit.",
                false,
            ));
        }
    }
    if bytes.len() as u64 > MAX_BINARY_BYTES
        || expected_size.is_some_and(|size| bytes.len() as u64 != size)
    {
        return Err(market_error(
            "artifact_size_invalid",
            "The Agent artifact exceeds or differs from the catalog size limit.",
            false,
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
pub(crate) fn register_test_artifact(url: &str, bytes: Vec<u8>) {
    let artifacts =
        TEST_ARTIFACTS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    artifacts
        .lock()
        .expect("test artifact registry")
        .insert(url.to_string(), bytes);
}

#[cfg(test)]
fn test_artifact(url: &str) -> Option<Vec<u8>> {
    TEST_ARTIFACTS
        .get()
        .and_then(|artifacts| artifacts.lock().ok()?.get(url).cloned())
}

#[cfg(test)]
static TEST_ARTIFACTS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
> = std::sync::OnceLock::new();

fn definition_for(
    item: &crate::backend::agent_market::types::CatalogItem,
    distribution: &Distribution,
    runtime: &crate::backend::agent_market::types::MaterializedRuntime,
) -> Result<AgentDefinition, AgentMarketError> {
    let id = AgentId::parse(item.id.clone())
        .map_err(|error| market_error("definition_invalid", error.to_string(), false))?;
    let protocol = match &item.protocol {
        AgentMarketProtocol::Acp => AgentProtocol::Acp,
        AgentMarketProtocol::Native => AgentProtocol::Native,
    };
    let definition = AgentDefinition {
        id,
        installation_id: Some(runtime.installation_id.clone()),
        display_name: item.display_name.clone(),
        protocol,
        command: runtime.resolved_program.to_string_lossy().to_string(),
        args: runtime.args.clone(),
        env: runtime
            .env
            .iter()
            .map(|(name, value)| crate::backend::agents::types::AgentEnvEntry::new(name, value))
            .collect(),
        declared_capabilities: DeclaredAgentCapabilities::acp_text(),
        availability_probe: Some(AgentCommandDefinition::with_command(
            runtime.resolved_program.to_string_lossy().to_string(),
            ["--version"],
        )),
        model_discovery: model_discovery_args(distribution).map(AgentCommandDefinition::new),
    };
    definition
        .validate()
        .map_err(|error| market_error("definition_invalid", error.to_string(), false))?;
    Ok(definition)
}

fn model_discovery_args(distribution: &Distribution) -> Option<Vec<String>> {
    match distribution {
        Distribution::System {
            model_discovery_args,
            ..
        }
        | Distribution::Binary {
            model_discovery_args,
            ..
        }
        | Distribution::Npx {
            model_discovery_args,
            ..
        }
        | Distribution::Uvx {
            model_discovery_args,
            ..
        } => model_discovery_args.clone(),
    }
}

fn conformance(
    definition: &AgentDefinition,
    workspace_root: &Path,
) -> (ProtocolStatus, Option<AgentMarketError>, Vec<String>) {
    let registry = match AgentRegistry::from_definitions([definition.clone()]) {
        Ok(registry) => Arc::new(registry),
        Err(error) => {
            return (
                ProtocolStatus::Failed,
                Some(market_error("definition_invalid", error.to_string(), false)),
                Vec::new(),
            )
        }
    };
    let executor = AgentExecutor::with_backends(
        registry,
        Arc::new(
            crate::backend::ai_execution::backends::acp::AcpExecutionBackend::new(
                workspace_root.join("conformance"),
            ),
        ),
        Arc::new(
            crate::backend::ai_execution::backends::native::NativeExecutionBackend::new(
                workspace_root.join("conformance"),
            ),
        ),
        1,
    );
    let id = definition.id.clone();
    let result = check_agent_connection_blocking(
        Arc::new(executor),
        id,
        crate::backend::agents::types::AgentConnectionCheckMode::Connection,
    );
    if result.connected {
        (ProtocolStatus::Ready, None, Vec::new())
    } else {
        let code = result
            .error_code
            .unwrap_or_else(|| "protocol_failed".to_string());
        let message = result
            .error
            .unwrap_or_else(|| "Agent protocol conformance failed.".to_string());
        let warning = format!("protocol conformance did not complete: {message}");
        (
            ProtocolStatus::Failed,
            Some(market_error(&code, message, true)),
            vec![warning],
        )
    }
}

fn install_error(
    error: crate::backend::agent_market::installers::InstallError,
) -> AgentMarketError {
    let (code, retryable) = match error {
        crate::backend::agent_market::installers::InstallError::RuntimeMissing(_) => {
            ("runtime_missing", true)
        }
        crate::backend::agent_market::installers::InstallError::IntegrityMismatch => {
            ("artifact_integrity_failed", false)
        }
        crate::backend::agent_market::installers::InstallError::ArchiveInvalid(_) => {
            ("archive_invalid", false)
        }
        crate::backend::agent_market::installers::InstallError::Cancelled => ("cancelled", true),
        crate::backend::agent_market::installers::InstallError::Timeout => ("timeout", true),
        _ => ("installation_failed", true),
    };
    market_error(code, error.to_string(), retryable)
}
