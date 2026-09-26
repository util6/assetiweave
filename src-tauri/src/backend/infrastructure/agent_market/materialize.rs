use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use crate::backend::{
    domain::agents::{
        definition::{AgentCommandDefinition, AgentDefinition, AgentId, AgentProtocol},
        market::{validation::is_safe_artifact_url, CatalogItem},
        AgentConnectionCheckMode, AgentEnvEntry, AgentMarketProtocol, Distribution,
        MaterializedRuntime, Ownership, ProtocolStatus,
    },
    infrastructure::agent_execution::{
        backends::acp::{AcpExecutionBackend, AcpProtocolConnectionOutcome},
        check_agent_connection,
        executor::AgentExecutor,
        registry::AgentRegistry,
        AiExecutionCancellation,
    },
    infrastructure::agent_market::{
        error::{AgentMarketError, LifecycleTaskPhase},
        installers::{
            binary::BinaryInstaller, npx::NpxInstaller, system::SystemInstaller, uvx::UvxInstaller,
            InstallContext, InstallError, Installer, MAX_BINARY_BYTES,
        },
    },
    infrastructure::InfraError,
};

fn market_error(code: &str, message: impl std::fmt::Display, retryable: bool) -> AgentMarketError {
    let msg = format!("{message}");
    AgentMarketError::new(code, &msg, retryable)
}

#[derive(Clone, Debug)]
pub(crate) struct MaterializeOutcome {
    pub(crate) installation_id: String,
    pub(crate) active_program: PathBuf,
    pub(crate) active_dir: Option<PathBuf>,
    pub(crate) args: Vec<String>,
    pub(crate) version: String,
    pub(crate) ownership: Ownership,
    pub(crate) definition_json: serde_json::Value,
    pub(crate) integrity_json: Option<serde_json::Value>,
    pub(crate) protocol_status: ProtocolStatus,
    pub(crate) protocol_error_code: Option<String>,
    pub(crate) protocol_error_message: Option<String>,
    pub(crate) warnings: Vec<String>,
}

pub(crate) async fn materialize_and_activate(
    item: &CatalogItem,
    distribution: &Distribution,
    staging: &Path,
    runtime_root: &Path,
    cancellation: Option<Arc<AtomicBool>>,
    phase_sink: Option<Arc<dyn Fn(LifecycleTaskPhase) + Send + Sync>>,
) -> Result<MaterializeOutcome, AgentMarketError> {
    let mut context = InstallContext::new(staging.to_path_buf(), item.version.clone());
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
        Distribution::System { .. } => SystemInstaller::default()
            .materialize(distribution, &context)
            .await
            .map_err(install_error)?,
        Distribution::Binary { url, size, .. } => {
            let dist = distribution.clone();
            let ctx = context.clone();
            let url = url.clone();
            let size = *size;
            tokio::task::spawn_blocking(move || {
                download_and_materialize_binary(&dist, &ctx, &url, size)
            })
            .await
            .map_err(|error| {
                market_error(
                    "install_failed",
                    format!("binary install task panicked: {error}"),
                    true,
                )
            })??
        }
        Distribution::Npx { .. } => NpxInstaller::default()
            .materialize(distribution, &context)
            .await
            .map_err(install_error)?,
        Distribution::Uvx { .. } => UvxInstaller::default()
            .materialize(distribution, &context)
            .await
            .map_err(install_error)?,
    };
    context.report_phase(LifecycleTaskPhase::ValidatingLayout);

    let definition = definition_for(item, distribution, &materialized)?;
    context.report_phase(LifecycleTaskPhase::ProbingProtocol);
    let (protocol_status, mut protocol_error, warnings) =
        conformance(&definition, runtime_root).await;
    if !matches!(protocol_status, ProtocolStatus::Ready)
        && matches!(materialized.ownership, Ownership::Managed)
    {
        return Err(protocol_error.take().unwrap_or_else(|| {
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
        let target = runtime_root.join("active").join(&context.installation_id);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| market_error("activation_failed", error, true))?;
        }
        let staging_root = staging
            .canonicalize()
            .map_err(|error| market_error("installation_layout_invalid", error, false))?;
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
        std::fs::rename(staging, &target)
            .map_err(|error| market_error("activation_failed", error, true))?;
        active_program = target.join(relative);
        active_dir = Some(target);
    }

    let definition_json = serde_json::json!({
        "id": item.id,
        "display_name": item.display_name,
        "protocol": item.protocol.as_str(),
        "program": active_program.to_string_lossy(),
        "args": materialized.args,
        "env": [],
        "capabilities": item.capabilities,
    });

    let protocol_error_code = protocol_error
        .as_ref()
        .map(|error| error.code().to_string());
    let protocol_error_message = protocol_error
        .as_ref()
        .map(|error| error.message().to_string());

    Ok(MaterializeOutcome {
        installation_id: context.installation_id,
        active_program,
        active_dir,
        args: materialized.args,
        version: materialized.version,
        ownership: materialized.ownership,
        definition_json,
        integrity_json: materialized.integrity,
        protocol_status,
        protocol_error_code,
        protocol_error_message,
        warnings,
    })
}

pub(crate) fn download_and_materialize_binary(
    distribution: &Distribution,
    context: &InstallContext,
    url: &str,
    expected_size: Option<u64>,
) -> Result<MaterializedRuntime, AgentMarketError> {
    if !is_safe_artifact_url(url) {
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
    let part_path = context.staging_dir.join("artifact.part");

    #[cfg(test)]
    let has_test_artifact = if let Some(bytes) = test_artifact(url) {
        if bytes.len() as u64 > MAX_BINARY_BYTES
            || expected_size.is_some_and(|size| bytes.len() as u64 != size)
        {
            return Err(market_error(
                "artifact_size_invalid",
                "The Agent artifact exceeds or differs from the catalog size limit.",
                false,
            ));
        }
        std::fs::write(&part_path, &bytes)
            .map_err(|error| market_error("download_failed", error, true))?;
        true
    } else {
        false
    };
    #[cfg(not(test))]
    let has_test_artifact = false;

    if !has_test_artifact {
        let client = crate::backend::infrastructure::http_client::shared_http_client()
            .map_err(|error| market_error("download_failed", error, true))?;
        let cancelled =
            || crate::backend::infrastructure::agent_market::installers::is_cancelled(context);
        let spec = crate::backend::infrastructure::http_client::DownloadSpec {
            url,
            path: &part_path,
            max_bytes: MAX_BINARY_BYTES,
            expected_size,
            timeout: context.timeout,
        };
        crate::backend::infrastructure::http_client::download_to_file(&client, spec, &cancelled)
            .map_err(|error| match error {
                InfraError::Cancelled(_) => {
                    market_error("cancelled", "Agent installation was cancelled.", true)
                }
                InfraError::Validation(message) if message == "artifact_size_invalid" => {
                    market_error(
                        "artifact_size_invalid",
                        "The Agent artifact exceeds or differs from the catalog size limit.",
                        false,
                    )
                }
                other => market_error("download_failed", other, true),
            })?;
    }

    context.report_phase(LifecycleTaskPhase::ValidatingIntegrity);
    let materialized = BinaryInstaller
        .materialize_file(distribution, context, &part_path)
        .map_err(install_error)?;
    let _ = std::fs::remove_file(&part_path);
    Ok(materialized)
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

pub(crate) fn definition_for(
    item: &CatalogItem,
    distribution: &Distribution,
    runtime: &MaterializedRuntime,
) -> Result<AgentDefinition, AgentMarketError> {
    let id = AgentId::parse(item.id.clone())
        .map_err(|error| market_error("definition_invalid", error, false))?;
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
            .map(|(name, value)| AgentEnvEntry::new(name, value))
            .collect(),
        declared_capabilities: item
            .capabilities
            .to_declared_agent_capabilities(&item.protocol),
        availability_probe: Some(AgentCommandDefinition::with_command(
            runtime.resolved_program.to_string_lossy().to_string(),
            ["--version"],
        )),
        model_discovery: model_discovery_args(distribution).map(AgentCommandDefinition::new),
        session_cleanup: session_cleanup_args(distribution).map(AgentCommandDefinition::new),
        session_cleanup_not_found_markers: distribution
            .session_cleanup_not_found_markers()
            .to_vec(),
    };
    definition
        .validate()
        .map_err(|error| market_error("definition_invalid", error, false))?;
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

fn session_cleanup_args(distribution: &Distribution) -> Option<Vec<String>> {
    match distribution {
        Distribution::System {
            session_cleanup_args,
            ..
        }
        | Distribution::Binary {
            session_cleanup_args,
            ..
        }
        | Distribution::Npx {
            session_cleanup_args,
            ..
        }
        | Distribution::Uvx {
            session_cleanup_args,
            ..
        } => session_cleanup_args.clone(),
    }
}

pub(crate) async fn conformance(
    definition: &AgentDefinition,
    workspace_root: &Path,
) -> (ProtocolStatus, Option<AgentMarketError>, Vec<String>) {
    if definition.protocol == AgentProtocol::Acp {
        let report = AcpExecutionBackend::new(workspace_root.join("conformance"))
            .probe_connection_and_models(definition, AiExecutionCancellation::default())
            .await;
        return match report.protocol_connection {
            AcpProtocolConnectionOutcome::Connected => (ProtocolStatus::Ready, None, Vec::new()),
            AcpProtocolConnectionOutcome::Failed { error_message, .. } => {
                let warning = format!("protocol conformance did not complete: {error_message}");
                (
                    ProtocolStatus::Failed,
                    Some(market_error("acp_connection_failed", error_message, true)),
                    vec![warning],
                )
            }
            AcpProtocolConnectionOutcome::Cancelled => (
                ProtocolStatus::Failed,
                Some(market_error(
                    "cancelled",
                    "Agent protocol conformance was cancelled.",
                    true,
                )),
                Vec::new(),
            ),
        };
    }

    let registry = match AgentRegistry::from_definitions([definition.clone()]) {
        Ok(registry) => Arc::new(registry),
        Err(error) => {
            return (
                ProtocolStatus::Failed,
                Some(market_error("definition_invalid", error, false)),
                Vec::new(),
            )
        }
    };
    let executor = AgentExecutor::with_backends(
        registry,
        Arc::new(AcpExecutionBackend::new(workspace_root.join("conformance"))),
        Arc::new(
            crate::backend::infrastructure::agent_execution::backends::native::NativeExecutionBackend::new(
                workspace_root.join("conformance"),
            ),
        ),
        1,
    );
    let id = definition.id.clone();
    let result =
        check_agent_connection(Arc::new(executor), id, AgentConnectionCheckMode::Connection).await;
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

pub(crate) fn install_error(error: InstallError) -> AgentMarketError {
    let (code, retryable) = match error {
        InstallError::RuntimeMissing(_) => ("runtime_missing", true),
        InstallError::IntegrityMismatch => ("artifact_integrity_failed", false),
        InstallError::ArchiveInvalid(_) => ("archive_invalid", false),
        InstallError::Cancelled => ("cancelled", true),
        InstallError::Timeout => ("timeout", true),
        _ => ("installation_failed", true),
    };
    market_error(code, error, retryable)
}
