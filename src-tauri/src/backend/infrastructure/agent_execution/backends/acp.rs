use super::history_replay::HistoryReplayPort;
use agent_client_protocol::schema::v1::{EnvVariable, McpServer, McpServerStdio, SessionId};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Instant;

use crate::backend::{
    domain::agents::{AgentDefinition, AgentProtocol, PersistentExecutionBinding},
    infrastructure::agent_execution::{
        error::AiExecutionError,
        managed_process as agent_process,
        protocol::acp as acp_protocol,
        types::{
            AgentSessionMode, AiExecutionCleanupReport, AiExecutionPhase, AiExecutionRequest,
            AiExecutionResult, AiMemoryGenerationTools, AiRecallTools, SessionCleanupStatus,
        },
    },
};

pub(crate) use super::acp_event_bridge::*;
pub(crate) use super::acp_guard::*;
pub(crate) use super::acp_probe_report::*;
pub(crate) use super::acp_prompt::*;

pub(crate) const PROTOCOL_EVENT_CAPACITY: usize = 128;

#[derive(Clone, Debug)]
pub(crate) struct AcpExecutionBackend {
    pub(crate) workspace_root: PathBuf,
}

impl AcpExecutionBackend {
    pub(crate) fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }
    pub(crate) async fn execute(
        &self,
        definition: &AgentDefinition,
        request: AiExecutionRequest,
    ) -> Result<AiExecutionResult, AiExecutionError> {
        request.validate()?;
        if request.cancellation.is_cancelled() {
            request.report_phase(AiExecutionPhase::Cancelling);
            return Err(cancelled_error(definition));
        }
        let started = Instant::now();
        let (workspace, bound_session) = if let Some(binding) = request.binding.as_ref() {
            let workspace = PathBuf::from(&binding.workspace_path);
            if !workspace.is_absolute() || !workspace.is_dir() {
                return Err(AiExecutionError::ResumeUnavailable);
            }
            (workspace, Some(binding.provider_session_id.clone()))
        } else {
            (create_workspace(&self.workspace_root)?, None)
        };
        let mut guard = AcpExecutionGuard::new(workspace);
        guard.bound_session = bound_session;
        guard.preserve_session = matches!(request.session_mode, AgentSessionMode::Persistent);

        let outcome = {
            let execution = run_execution(&mut guard, definition, &request, started);
            tokio::pin!(execution);
            let cancellation = request.cancellation.cancelled();
            tokio::pin!(cancellation);
            let timeout = tokio::time::sleep(request.limits.total_timeout);
            tokio::pin!(timeout);
            tokio::select! {
                outcome = &mut execution => outcome,
                _ = &mut cancellation => Err(cancelled_error(definition)),
                _ = &mut timeout => {
                    request.cancellation.cancel();
                    Err(timeout_error(definition, request.limits.total_timeout))
                }
            }
        };
        if outcome.is_err() {
            request.report_phase(AiExecutionPhase::Cancelling);
        }
        // A bound Persistent session is the durable resume anchor. Keep it
        // intact even when the restore attempt itself fails so the next
        // recovery pass can retry the same provider session instead of
        // turning a transient provider error into permanent data loss.
        guard.preserve_session =
            guard.preserve_session && (outcome.is_ok() || request.binding.is_some());
        guard.preserve_workspace = guard.preserve_session;
        request.report_phase(AiExecutionPhase::Closing);
        let cleanup = guard.cleanup(outcome.is_err(), &request, definition).await;
        request.report_cleanup(AiExecutionCleanupReport {
            process_reaped: cleanup.process_reaped,
            workspace_removed: cleanup.workspace_removed,
            failure_count: cleanup.failures.len(),
            session_closed: cleanup.session_closed,
            session_deleted: cleanup.session_deleted,
            session_delete_method: cleanup.session_delete_method,
        });
        request.report_phase(AiExecutionPhase::CleaningUp);
        if !request.replay {
            if cleanup.failures.is_empty() {
                tracing::info!(
                    action = "ai_execution.cleanup",
                    execution_id = %request.execution_id,
                    agent_id = %definition.id,
                    protocol = "acp",
                    phase = "cleaning_up",
                    process_reaped = cleanup.process_reaped,
                    workspace_removed = cleanup.workspace_removed,
                    stderr_bytes = cleanup.stderr_bytes,
                    stderr_truncated = cleanup.stderr_truncated,
                    pid = ?cleanup.process_id,
                    exit_code = ?cleanup.exit_code,
                    "AI execution cleanup completed"
                );
            } else {
                tracing::warn!(
                    action = "ai_execution.cleanup",
                    execution_id = %request.execution_id,
                    agent_id = %definition.id,
                    protocol = "acp",
                    phase = "cleaning_up",
                    process_reaped = cleanup.process_reaped,
                    workspace_removed = cleanup.workspace_removed,
                    stderr_bytes = cleanup.stderr_bytes,
                    stderr_truncated = cleanup.stderr_truncated,
                    pid = ?cleanup.process_id,
                    exit_code = ?cleanup.exit_code,
                    failures = ?cleanup.failures,
                    "AI execution cleanup reported failures"
                );
            }
        }

        let session_cleanup = determine_session_cleanup_status(&cleanup);

        let critical_failures = cleanup
            .failures
            .iter()
            .filter(|failure| !is_session_delete_failure(failure))
            .cloned()
            .collect::<Vec<_>>();

        if !critical_failures.is_empty() && outcome.is_ok() {
            return Err(AiExecutionError::CleanupFailed {
                failures: critical_failures,
            });
        }

        outcome.map(|mut result| {
            result.session_cleanup = session_cleanup;
            result
        })
    }
}

async fn run_execution(
    guard: &mut AcpExecutionGuard,
    definition: &AgentDefinition,
    request: &AiExecutionRequest,
    started: Instant,
) -> Result<AiExecutionResult, AiExecutionError> {
    request.report_phase(AiExecutionPhase::Spawning);
    let process = agent_process::ManagedAgentProcess::spawn(
        definition,
        Some(&guard.workspace),
        request.limits.stderr_bytes,
    )
    .await
    .map_err(|error| map_process_error(definition, error))?;
    if !request.replay {
        tracing::info!(
            action = "ai_execution.process",
            execution_id = %request.execution_id,
            agent_id = %definition.id,
            protocol = "acp",
            phase = "spawning",
            pid = %process.process_id(),
            arg_count = definition.args.len(),
            env_key_count = definition.env.len(),
            cwd_kind = if matches!(request.session_mode, AgentSessionMode::Persistent) {
                "stable"
            } else {
                "ephemeral"
            },
            "AI agent process started"
        );
    }
    guard.process = Some(process);

    let (stdin, stdout) = guard
        .process
        .as_ref()
        .expect("process stored before stdio")
        .take_stdio()
        .await
        .map_err(|_| AiExecutionError::Protocol {
            operation: "take_stdio",
        })?;
    let mut config = acp_protocol::AcpConnectConfig::new(request.limits.initialize_timeout);
    config.event_channel_capacity = PROTOCOL_EVENT_CAPACITY;
    request.report_phase(AiExecutionPhase::Initializing);
    let (protocol, mut channels) = acp_protocol::AcpProtocol::connect(stdin, stdout, config)
        .await
        .map_err(|error| map_acp_error("initialize", error))?;
    guard.protocol = Some(protocol);

    request.report_phase(AiExecutionPhase::CreatingSession);
    let protocol = guard
        .protocol
        .as_ref()
        .expect("protocol stored before session");
    let mut mcp_servers = Vec::new();
    if protocol.supports_stdio_mcp(&definition.id) {
        mcp_servers.extend(recall_mcp_servers(request.recall_tools.as_ref())?);
        mcp_servers.extend(memory_generation_mcp_servers(
            request.memory_generation_tools.as_ref(),
        )?);
    }
    let session = if let Some(bound_session) = guard.bound_session.clone() {
        let session_id = SessionId::new(bound_session);
        if request.replay {
            if !protocol.supports_load() {
                return Err(AiExecutionError::ResumeUnavailable);
            }
            protocol
                .load_session_with_mcp(
                    session_id.clone(),
                    guard.workspace.clone(),
                    mcp_servers.clone(),
                )
                .await
                .map_err(|_| AiExecutionError::ResumeUnavailable)?;
        } else {
            if !protocol.supports_resume() {
                return Err(AiExecutionError::ResumeUnavailable);
            }
            protocol
                .resume_session_with_mcp(
                    session_id.clone(),
                    guard.workspace.clone(),
                    mcp_servers.clone(),
                )
                .await
                .map_err(|_| AiExecutionError::ResumeUnavailable)?;
        }
        session_id
    } else if request.replay {
        return Err(AiExecutionError::ResumeUnavailable);
    } else {
        protocol
            .new_session_with_mcp(guard.workspace.clone(), mcp_servers)
            .await
            .map_err(|error| map_acp_error("session_new", error))?
            .session_id
    };
    guard.session_id = Some(session.clone());

    if request.restore_only {
        return Ok(AiExecutionResult {
            text: String::new(),
            agent_id: definition.id.clone(),
            protocol: AgentProtocol::Acp,
            requested_model: request.model.clone(),
            elapsed_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            persistent_binding: None,
            replay_text: None,
            session_cleanup: SessionCleanupStatus::Skipped,
        });
    }

    if request.replay {
        request.report_phase(AiExecutionPhase::Prompting);
        let mut replay_port = AcpHistoryReplayPort {
            events: &mut channels.events,
            session_id: &session,
            request,
        };
        let replay = replay_port
            .replay(&session.to_string(), request.limits.text_bytes)
            .await;
        return Ok(AiExecutionResult {
            text: replay.text.clone(),
            agent_id: definition.id.clone(),
            protocol: AgentProtocol::Acp,
            requested_model: request.model.clone(),
            elapsed_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            persistent_binding: None,
            replay_text: Some(replay.text),
            session_cleanup: SessionCleanupStatus::Skipped,
        });
    }

    if let Some(model) = request.model.as_deref() {
        request.report_phase(AiExecutionPhase::Configuring);
        guard
            .protocol
            .as_ref()
            .expect("protocol stored before model")
            .set_model(
                session.clone(),
                model.trim(),
                request.limits.config_rpc_timeout,
            )
            .await
            .map_err(|error| AiExecutionError::ModelSelectionFailed {
                detail: match error {
                    acp_protocol::AcpError::RequestFailed { message, .. } => Some(message),
                    _ => None,
                },
            })?;
    }

    request.report_phase(AiExecutionPhase::Prompting);
    let text = run_prompt_and_aggregate(
        guard
            .protocol
            .as_ref()
            .expect("protocol stored before prompt"),
        guard
            .process
            .as_ref()
            .expect("process stored before prompt"),
        channels,
        session.clone(),
        request,
        definition,
    )
    .await?;

    Ok(AiExecutionResult {
        text,
        agent_id: definition.id.clone(),
        protocol: AgentProtocol::Acp,
        requested_model: request.model.clone(),
        elapsed_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        persistent_binding: if matches!(request.session_mode, AgentSessionMode::Persistent)
            && request.binding.is_none()
        {
            Some(PersistentExecutionBinding {
                tenant_id: request.tenant_id.clone().unwrap_or_default(),
                execution_context_key: request.execution_context_key.clone().unwrap_or_default(),
                provider_session_id: session.to_string(),
                agent_id: definition.id.to_string(),
                installation_id: definition.installation_id.clone(),
                model: request.model.clone(),
                workspace_path: guard.workspace.to_string_lossy().into_owned(),
                binding_version: 1,
                provider_metadata_json: "{\"protocol\":\"acp\"}".to_string(),
            })
        } else {
            None
        },
        replay_text: None,
        session_cleanup: SessionCleanupStatus::Skipped,
    })
}

fn recall_mcp_servers(
    recall_tools: Option<&AiRecallTools>,
) -> Result<Vec<McpServer>, AiExecutionError> {
    let Some(recall_tools) = recall_tools else {
        return Ok(Vec::new());
    };
    let executable = std::env::current_exe().map_err(|_| AiExecutionError::Protocol {
        operation: "recall_mcp_executable",
    })?;
    Ok(vec![McpServer::Stdio(
        McpServerStdio::new("assetiweave-memory-recall", executable)
            .args(vec!["--memory-recall-mcp-stdio".to_string()])
            .env(vec![
                EnvVariable::new("ASSETIWEAVE_DB_PATH", recall_tools.database_path.clone()),
                EnvVariable::new(
                    "ASSETIWEAVE_MEMORY_RECALL_TENANT_ID",
                    recall_tools.tenant_id.clone(),
                ),
                EnvVariable::new(
                    "ASSETIWEAVE_MEMORY_RECALL_SESSION_ID",
                    recall_tools.recall_session_id.clone(),
                ),
            ]),
    )])
}

fn memory_generation_mcp_servers(
    tools: Option<&AiMemoryGenerationTools>,
) -> Result<Vec<McpServer>, AiExecutionError> {
    let Some(tools) = tools else {
        return Ok(Vec::new());
    };
    let executable = std::env::current_exe().map_err(|_| AiExecutionError::Protocol {
        operation: "memory_generation_mcp_executable",
    })?;
    Ok(vec![McpServer::Stdio(
        McpServerStdio::new("assetiweave-memory-generation", executable)
            .args(vec!["--memory-generation-mcp-stdio".to_string()])
            .env(vec![
                EnvVariable::new("ASSETIWEAVE_DB_PATH", tools.database_path.clone()),
                EnvVariable::new(
                    "ASSETIWEAVE_MEMORY_GENERATION_TENANT_ID",
                    tools.tenant_id.clone(),
                ),
                EnvVariable::new("ASSETIWEAVE_MEMORY_GENERATION_JOB_ID", tools.job_id.clone()),
                EnvVariable::new(
                    "ASSETIWEAVE_MEMORY_GENERATION_OWNERSHIP_TOKEN",
                    tools.ownership_token.clone(),
                ),
            ]),
    )])
}

#[cfg(test)]
#[path = "acp_tests.rs"]
mod tests;
