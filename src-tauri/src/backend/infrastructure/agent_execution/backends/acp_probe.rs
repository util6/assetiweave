use std::time::Instant;

use crate::backend::{
    domain::agents::AgentDefinition,
    infrastructure::agent_execution::{
        error::AiExecutionError,
        managed_process as agent_process,
        protocol::acp as acp_protocol,
        types::{AgentModelOption, AiExecutionCancellation, AiExecutionPhase, AiExecutionRequest},
    },
};

use super::acp::{AcpExecutionBackend, PROTOCOL_EVENT_CAPACITY};
use super::acp_guard::{AcpExecutionGuard, CleanupReport};
use super::acp_models::parse_session_models;
use super::acp_probe_report::*;
use super::acp_prompt::{
    cancelled_error, create_workspace, map_acp_error, map_process_error, timeout_error,
};

pub(crate) async fn run_connection_probe(
    guard: &mut AcpExecutionGuard,
    definition: &AgentDefinition,
    request: &AiExecutionRequest,
) -> Result<(), AiExecutionError> {
    let _session = run_session_probe(guard, definition, request).await?;
    Ok(())
}

pub(crate) async fn run_session_probe(
    guard: &mut AcpExecutionGuard,
    definition: &AgentDefinition,
    request: &AiExecutionRequest,
) -> Result<agent_client_protocol::schema::v1::NewSessionResponse, AiExecutionError> {
    let mut timings = AcpProbeTimings::default();
    let (res, _) = run_session_probe_with_timings(guard, definition, request, &mut timings).await;
    res
}

pub(crate) async fn run_session_probe_with_timings(
    guard: &mut AcpExecutionGuard,
    definition: &AgentDefinition,
    request: &AiExecutionRequest,
    timings: &mut AcpProbeTimings,
) -> (
    Result<agent_client_protocol::schema::v1::NewSessionResponse, AiExecutionError>,
    AcpConnectionStage,
) {
    request.report_phase(AiExecutionPhase::Spawning);
    let spawn_start = Instant::now();
    let process = tokio::select! {
        result = tokio::time::timeout(
            request.limits.spawn_timeout,
            agent_process::ManagedAgentProcess::spawn(
                definition,
                Some(&guard.workspace),
                request.limits.stderr_bytes,
            ),
        ) => match result {
            Ok(Ok(proc)) => proc,
            Ok(Err(error)) => {
                timings.spawn_duration_ms = spawn_start.elapsed().as_millis() as u64;
                return (Err(map_process_error(definition, error)), AcpConnectionStage::Spawn);
            }
            Err(_) => {
                timings.spawn_duration_ms = spawn_start.elapsed().as_millis() as u64;
                return (Err(AiExecutionError::Protocol { operation: "spawn_timeout" }), AcpConnectionStage::Spawn);
            }
        },
        _ = request.cancellation.cancelled() => {
            timings.spawn_duration_ms = spawn_start.elapsed().as_millis() as u64;
            return (Err(cancelled_error(definition)), AcpConnectionStage::Spawn);
        }
    };
    timings.spawn_duration_ms = spawn_start.elapsed().as_millis() as u64;
    guard.process = Some(process);

    let init_start = Instant::now();
    let (stdin, stdout) = match guard
        .process
        .as_ref()
        .expect("process stored before stdio")
        .take_stdio()
        .await
    {
        Ok(pair) => pair,
        Err(_) => {
            timings.initialize_duration_ms = init_start.elapsed().as_millis() as u64;
            return (
                Err(AiExecutionError::Protocol {
                    operation: "take_stdio",
                }),
                AcpConnectionStage::Transport,
            );
        }
    };

    let mut config = acp_protocol::AcpConnectConfig::new(request.limits.initialize_timeout);
    config.event_channel_capacity = PROTOCOL_EVENT_CAPACITY;
    request.report_phase(AiExecutionPhase::Initializing);
    let process = guard
        .process
        .as_ref()
        .expect("process stored before connect");
    let connect = acp_protocol::AcpProtocol::connect(stdin, stdout, config);
    tokio::pin!(connect);
    let (protocol, _channels) = tokio::select! {
        biased;
        result = &mut connect => match result {
            Ok(p) => p,
            Err(error) => {
                timings.initialize_duration_ms = init_start.elapsed().as_millis() as u64;
                return (Err(map_acp_error("initialize", error)), AcpConnectionStage::Initialize);
            }
        },
        _ = request.cancellation.cancelled() => {
            timings.initialize_duration_ms = init_start.elapsed().as_millis() as u64;
            return (Err(cancelled_error(definition)), AcpConnectionStage::Initialize);
        }
        exit = process.wait_for_exit() => {
            timings.initialize_duration_ms = init_start.elapsed().as_millis() as u64;
            return (Err(AiExecutionError::AgentExited {
                code: exit.and_then(|exit| exit.code),
            }), AcpConnectionStage::Initialize);
        }
    };
    timings.initialize_duration_ms = init_start.elapsed().as_millis() as u64;
    guard.protocol = Some(protocol);

    request.report_phase(AiExecutionPhase::CreatingSession);
    let session_start = Instant::now();
    let new_session = guard
        .protocol
        .as_ref()
        .expect("protocol stored before session")
        .new_session(guard.workspace.clone());
    tokio::pin!(new_session);
    let session = tokio::select! {
        result = tokio::time::timeout(request.limits.config_rpc_timeout, &mut new_session) => match result {
            Ok(Ok(sess)) => sess,
            Ok(Err(error)) => {
                timings.session_new_duration_ms = session_start.elapsed().as_millis() as u64;
                return (Err(map_acp_error("session_new", error)), AcpConnectionStage::SessionNew);
            }
            Err(_) => {
                timings.session_new_duration_ms = session_start.elapsed().as_millis() as u64;
                return (Err(AiExecutionError::Protocol {
                    operation: "session_new_timeout",
                }), AcpConnectionStage::SessionNew);
            }
        },
        _ = request.cancellation.cancelled() => {
            timings.session_new_duration_ms = session_start.elapsed().as_millis() as u64;
            return (Err(cancelled_error(definition)), AcpConnectionStage::SessionNew);
        }
    };
    timings.session_new_duration_ms = session_start.elapsed().as_millis() as u64;
    guard.session_id = Some(session.session_id.clone());
    (Ok(session), AcpConnectionStage::SessionNew)
}

pub(crate) async fn cleanup_with_deadline(
    guard: &mut AcpExecutionGuard,
    cancel_before_close: bool,
    request: &AiExecutionRequest,
    definition: &AgentDefinition,
) -> CleanupReport {
    let deadline = Instant::now() + request.limits.cleanup_timeout;
    match tokio::time::timeout(
        request.limits.cleanup_timeout,
        guard.cleanup(cancel_before_close, request, definition),
    )
    .await
    {
        Ok(report) => report,
        Err(_) => {
            request.cancellation.cancel();
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                CleanupReport::timed_out()
            } else {
                match tokio::time::timeout(remaining, guard.cleanup(true, request, definition))
                    .await
                {
                    Ok(report) => report,
                    Err(_) => CleanupReport::timed_out(),
                }
            }
        }
    }
}

impl AcpExecutionBackend {
    pub(crate) async fn check_connection(
        &self,
        definition: &AgentDefinition,
    ) -> Result<(), AiExecutionError> {
        let request = connection_probe_request(definition);
        let workspace = create_workspace(&self.workspace_root)?;
        let mut guard = AcpExecutionGuard::new(workspace);
        let outcome = {
            let probe = run_connection_probe(&mut guard, definition, &request);
            tokio::pin!(probe);
            match tokio::time::timeout(request.limits.total_timeout, &mut probe).await {
                Ok(outcome) => outcome,
                Err(_) => {
                    request.cancellation.cancel();
                    Err(timeout_error(definition, request.limits.total_timeout))
                }
            }
        };
        let cleanup =
            cleanup_with_deadline(&mut guard, outcome.is_err(), &request, definition).await;
        if !cleanup.process_reaped || !cleanup.workspace_removed {
            return Err(AiExecutionError::CleanupFailed {
                failures: cleanup.failures,
            });
        }
        let critical_failures = cleanup
            .failures
            .iter()
            .filter(|failure| failure.as_str() != "delete_unsupported")
            .cloned()
            .collect::<Vec<_>>();
        if !critical_failures.is_empty() && outcome.is_ok() {
            return Err(AiExecutionError::CleanupFailed {
                failures: critical_failures,
            });
        }
        if !cleanup.failures.is_empty() {
            tracing::warn!(
                action = "ai_execution.check_connection.cleanup_warning",
                agent_id = %definition.id,
                failures = ?cleanup.failures,
                "ACP connection probe succeeded with non-critical cleanup warning"
            );
        }
        outcome
    }

    pub(crate) async fn discover_models(
        &self,
        definition: &AgentDefinition,
    ) -> Result<(Vec<AgentModelOption>, Option<String>), AiExecutionError> {
        let request = model_discovery_request(definition);
        let workspace = create_workspace(&self.workspace_root)?;
        let mut guard = AcpExecutionGuard::new(workspace);
        let outcome = {
            let probe = run_session_probe(&mut guard, definition, &request);
            tokio::pin!(probe);
            match tokio::time::timeout(request.limits.total_timeout, &mut probe).await {
                Ok(Ok(session)) => parse_session_models(&session),
                Ok(Err(error)) => Err(error),
                Err(_) => {
                    request.cancellation.cancel();
                    Err(timeout_error(definition, request.limits.total_timeout))
                }
            }
        };
        let cleanup = guard.cleanup(outcome.is_err(), &request, definition).await;
        if !cleanup.process_reaped || !cleanup.workspace_removed {
            return Err(AiExecutionError::CleanupFailed {
                failures: cleanup.failures,
            });
        }
        let critical_failures = cleanup
            .failures
            .iter()
            .filter(|failure| failure.as_str() != "delete_unsupported")
            .cloned()
            .collect::<Vec<_>>();
        if !critical_failures.is_empty() && outcome.is_ok() {
            return Err(AiExecutionError::CleanupFailed {
                failures: critical_failures,
            });
        }
        if !cleanup.failures.is_empty() {
            tracing::warn!(
                action = "ai_execution.discover_models.cleanup_warning",
                agent_id = %definition.id,
                failures = ?cleanup.failures,
                "ACP model discovery succeeded with non-critical cleanup warning"
            );
        }
        outcome
    }

    pub(crate) async fn probe_connection_and_models(
        &self,
        definition: &AgentDefinition,
        cancellation: AiExecutionCancellation,
    ) -> AcpProbeReport {
        let probe_start = Instant::now();
        let mut request = model_discovery_request(definition);
        request.cancellation = cancellation;
        let workspace = match create_workspace(&self.workspace_root) {
            Ok(workspace) => workspace,
            Err(error) => {
                return AcpProbeReport {
                    protocol_connection: AcpProtocolConnectionOutcome::Failed {
                        stage: AcpConnectionStage::Spawn,
                        error_code: "workspace_create_failed".to_string(),
                        error_message: error.to_string(),
                    },
                    model_discovery: AcpModelDiscoveryOutcome::Skipped,
                    cleanup: AcpCleanupOutcome {
                        process_reaped: true,
                        workspace_removed: true,
                        timed_out: false,
                        failures: Vec::new(),
                    },
                    timings: AcpProbeTimings {
                        spawn_duration_ms: 0,
                        initialize_duration_ms: 0,
                        session_new_duration_ms: 0,
                        model_discovery_duration_ms: 0,
                        cleanup_duration_ms: 0,
                        total_duration_ms: probe_start.elapsed().as_millis() as u64,
                    },
                };
            }
        };
        let mut guard = AcpExecutionGuard::new(workspace);
        let mut timings = AcpProbeTimings::default();

        let (session_outcome, session_stage) =
            run_session_probe_with_timings(&mut guard, definition, &request, &mut timings).await;

        let (protocol_connection, model_discovery, cleanup_failed_flag) = match session_outcome {
            Ok(session) => {
                let model_start = Instant::now();
                let discovery = match parse_session_models(&session) {
                    Ok((models, current_model_id)) => {
                        if models.is_empty() {
                            AcpModelDiscoveryOutcome::Empty
                        } else {
                            AcpModelDiscoveryOutcome::Success {
                                models,
                                current_model_id,
                            }
                        }
                    }
                    Err(err) => match err {
                        AiExecutionError::Protocol {
                            operation: "session_model_catalog_empty",
                        } => AcpModelDiscoveryOutcome::Empty,
                        AiExecutionError::Protocol {
                            operation: "session_model_catalog_invalid",
                        } => AcpModelDiscoveryOutcome::Invalid {
                            error_code: "model_catalog_invalid".to_string(),
                            error_message: "The ACP session model catalog is malformed or invalid."
                                .to_string(),
                        },
                        AiExecutionError::Protocol {
                            operation: "session_model_catalog_unsupported",
                        } => AcpModelDiscoveryOutcome::Unsupported,
                        AiExecutionError::Timeout { .. } => AcpModelDiscoveryOutcome::Timeout,
                        other => AcpModelDiscoveryOutcome::Failed {
                            error_code: "model_discovery_failed".to_string(),
                            error_message: other.to_string(),
                        },
                    },
                };
                timings.model_discovery_duration_ms = model_start.elapsed().as_millis() as u64;
                (AcpProtocolConnectionOutcome::Connected, discovery, false)
            }
            Err(error) => {
                let outcome = if matches!(error, AiExecutionError::Cancelled { .. }) {
                    AcpProtocolConnectionOutcome::Cancelled
                } else {
                    let code = connection_error_code(&error);
                    let message = error.to_string();
                    AcpProtocolConnectionOutcome::Failed {
                        stage: session_stage,
                        error_code: code.to_string(),
                        error_message: message,
                    }
                };
                (outcome, AcpModelDiscoveryOutcome::Skipped, true)
            }
        };

        let cleanup_start = Instant::now();
        let cleanup =
            cleanup_with_deadline(&mut guard, cleanup_failed_flag, &request, definition).await;
        timings.cleanup_duration_ms = cleanup_start.elapsed().as_millis() as u64;
        timings.total_duration_ms = probe_start.elapsed().as_millis() as u64;

        let timed_out = cleanup.failures.iter().any(|f| f == "cleanup_timeout");
        let cleanup_outcome = AcpCleanupOutcome {
            process_reaped: cleanup.process_reaped,
            workspace_removed: cleanup.workspace_removed,
            timed_out,
            failures: cleanup.failures.clone(),
        };

        if (!cleanup.process_reaped || !cleanup.workspace_removed)
            && !matches!(protocol_connection, AcpProtocolConnectionOutcome::Cancelled)
        {
            return AcpProbeReport {
                protocol_connection: AcpProtocolConnectionOutcome::Failed {
                    stage: AcpConnectionStage::Cleanup,
                    error_code: "cleanup_failed".to_string(),
                    error_message: "Failed to reap child process or remove workspace".to_string(),
                },
                model_discovery: AcpModelDiscoveryOutcome::Skipped,
                cleanup: cleanup_outcome,
                timings,
            };
        }

        let critical_failures = cleanup
            .failures
            .iter()
            .filter(|failure| failure.as_str() != "delete_unsupported")
            .cloned()
            .collect::<Vec<_>>();
        if !critical_failures.is_empty()
            && !cleanup_failed_flag
            && !matches!(protocol_connection, AcpProtocolConnectionOutcome::Cancelled)
        {
            return AcpProbeReport {
                protocol_connection: AcpProtocolConnectionOutcome::Failed {
                    stage: AcpConnectionStage::Cleanup,
                    error_code: "cleanup_failed".to_string(),
                    error_message: format!(
                        "Cleanup reported critical failures: {:?}",
                        critical_failures
                    ),
                },
                model_discovery: AcpModelDiscoveryOutcome::Skipped,
                cleanup: cleanup_outcome,
                timings,
            };
        }

        if !cleanup.failures.is_empty() {
            tracing::warn!(
                action = "ai_execution.probe_connection_and_models.cleanup_warning",
                agent_id = %definition.id,
                failures = ?cleanup.failures,
                "ACP probe finished with non-critical cleanup warning"
            );
        }

        AcpProbeReport {
            protocol_connection,
            model_discovery,
            cleanup: cleanup_outcome,
            timings,
        }
    }
}
