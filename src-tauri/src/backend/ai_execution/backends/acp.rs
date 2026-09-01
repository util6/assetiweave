use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use agent_client_protocol::schema::v1::{
    EnvVariable, McpServer, McpServerStdio, SessionId, StopReason,
};
use serde_json::Value;
use uuid::Uuid;

use crate::backend::{
    agents::{
        process::{ManagedAgentProcess, ManagedAgentProcessError},
        protocol::acp::{
            AcpConnectConfig, AcpError, AcpProtocol, AcpProtocolChannels, AcpRuntimeEvent,
            AcpToolStatus,
        },
        types::{AgentDefinition, AgentModelOption, AgentProtocol, SESSION_ID_PLACEHOLDER},
    },
    ai_execution::{
        AgentSessionMode, AiExecutionCancellation, AiExecutionCleanupReport, AiExecutionError,
        AiExecutionLimits, AiExecutionPhase, AiExecutionPurpose, AiExecutionRequest,
        AiExecutionResult, AiExecutionSessionDeleteMethod, SessionEvent, SessionEventDelivery,
        SessionEventIdentity, SessionEventKind, SessionProcessingState, SessionToolState,
    },
    host_process::{run_host_command, HostCommandSpec, HostInput},
    operation_log::{log_info, log_warn},
};

use super::acp_aggregator::{
    AggregatorAction, RecallStructuredAggregator, TranslationTextAggregator,
};

const PROTOCOL_EVENT_CAPACITY: usize = 128;
const PROVIDER_SESSION_DELETE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const ACP_TEXT_ITEM_ID: &str = "assistant-text";
const ACP_THINKING_ITEM_ID: &str = "assistant-thinking";
const ACP_PROCESSING_ITEM_ID: &str = "processing";
const ACP_TERMINAL_ITEM_ID: &str = "terminal";

#[derive(Clone, Debug)]
pub(crate) struct AcpExecutionBackend {
    workspace_root: PathBuf,
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
        let mut cleanup_fields = vec![
            ("execution_id", request.execution_id.clone()),
            ("agent_id", definition.id.to_string()),
            ("protocol", "acp".to_string()),
            ("phase", "cleaning_up".to_string()),
            ("process_reaped", cleanup.process_reaped.to_string()),
            ("workspace_removed", cleanup.workspace_removed.to_string()),
            ("stderr_bytes", cleanup.stderr_bytes.to_string()),
            ("stderr_truncated", cleanup.stderr_truncated.to_string()),
            ("failure_count", cleanup.failures.len().to_string()),
            (
                "session_closed",
                optional_bool_label(cleanup.session_closed).to_string(),
            ),
            (
                "session_deleted",
                optional_bool_label(cleanup.session_deleted).to_string(),
            ),
            (
                "session_delete_method",
                cleanup
                    .session_delete_method
                    .map(|method| match method {
                        AiExecutionSessionDeleteMethod::Acp => "acp",
                        AiExecutionSessionDeleteMethod::ProviderFallback => "provider_fallback",
                    })
                    .unwrap_or("none")
                    .to_string(),
            ),
        ];
        if let Some(process_id) = cleanup.process_id {
            cleanup_fields.push(("pid", process_id.to_string()));
        }
        if let Some(exit_code) = cleanup.exit_code {
            cleanup_fields.push(("exit_code", exit_code.to_string()));
        }
        if !request.replay {
            if cleanup.failures.is_empty() {
                log_info(
                    "ai_execution.cleanup",
                    "AI execution cleanup completed",
                    &cleanup_fields,
                );
            } else {
                log_warn(
                    "ai_execution.cleanup",
                    "AI execution cleanup reported failures",
                    &cleanup_fields,
                );
            }
        }

        if cleanup.failures.is_empty() {
            return outcome;
        }
        if outcome.is_ok() {
            return Err(AiExecutionError::CleanupFailed {
                failures: cleanup.failures,
            });
        }
        outcome
    }

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
        let cleanup = guard.cleanup(outcome.is_err(), &request, definition).await;
        if cleanup.failures.is_empty() {
            return outcome;
        }
        if outcome.is_ok() || !cleanup.process_reaped || !cleanup.workspace_removed {
            return Err(AiExecutionError::CleanupFailed {
                failures: cleanup.failures,
            });
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
        if cleanup.failures.is_empty() {
            return outcome;
        }
        if outcome.is_ok() || !cleanup.process_reaped || !cleanup.workspace_removed {
            return Err(AiExecutionError::CleanupFailed {
                failures: cleanup.failures,
            });
        }
        outcome
    }
}

async fn run_connection_probe(
    guard: &mut AcpExecutionGuard,
    definition: &AgentDefinition,
    request: &AiExecutionRequest,
) -> Result<(), AiExecutionError> {
    let session = run_session_probe(guard, definition, request).await?;
    parse_session_models(&session).map(|_| ())
}

async fn run_session_probe(
    guard: &mut AcpExecutionGuard,
    definition: &AgentDefinition,
    request: &AiExecutionRequest,
) -> Result<agent_client_protocol::schema::v1::NewSessionResponse, AiExecutionError> {
    request.report_phase(AiExecutionPhase::Spawning);
    let process = ManagedAgentProcess::spawn(
        definition,
        Some(&guard.workspace),
        request.limits.stderr_bytes,
    )
    .await
    .map_err(|error| map_process_error(definition, error))?;
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
    let mut config = AcpConnectConfig::new(request.limits.initialize_timeout);
    config.event_channel_capacity = PROTOCOL_EVENT_CAPACITY;
    request.report_phase(AiExecutionPhase::Initializing);
    let process = guard
        .process
        .as_ref()
        .expect("process stored before connect");
    let connect = AcpProtocol::connect(stdin, stdout, config);
    tokio::pin!(connect);
    let (protocol, _channels) = tokio::select! {
        biased;
        result = &mut connect => result.map_err(|error| map_acp_error("initialize", error))?,
        exit = process.wait_for_exit() => {
            return Err(AiExecutionError::AgentExited {
                code: exit.and_then(|exit| exit.code),
            });
        }
    };
    guard.protocol = Some(protocol);

    request.report_phase(AiExecutionPhase::CreatingSession);
    let session = tokio::time::timeout(
        request.limits.config_rpc_timeout,
        guard
            .protocol
            .as_ref()
            .expect("protocol stored before session")
            .new_session(guard.workspace.clone()),
    )
    .await
    .map_err(|_| AiExecutionError::Protocol {
        operation: "session_new_timeout",
    })?
    .map_err(|error| map_acp_error("session_new", error))?;
    guard.session_id = Some(session.session_id.clone());
    Ok(session)
}

fn connection_probe_request(definition: &AgentDefinition) -> AiExecutionRequest {
    AiExecutionRequest {
        execution_id: format!("agent-probe-{}", Uuid::new_v4()),
        agent_id: definition.id.clone(),
        purpose: AiExecutionPurpose::ConnectionTest,
        session_mode: AgentSessionMode::OneShot,
        prompt: "ACP connection probe".to_string(),
        model: None,
        limits: AiExecutionLimits {
            total_timeout: std::time::Duration::from_secs(35),
            initialize_timeout: std::time::Duration::from_secs(15),
            config_rpc_timeout: std::time::Duration::from_secs(15),
            cancel_grace: std::time::Duration::from_secs(2),
            close_timeout: std::time::Duration::from_secs(2),
            text_bytes: 1024,
            stderr_bytes: 64 * 1024,
        },
        cancellation: AiExecutionCancellation::default(),
        progress: None,
        tenant_id: None,
        execution_context_key: None,
        binding: None,
        replay: false,
        restore_only: false,
        team_tools: None,
        recall_tools: None,
    }
}

fn model_discovery_request(definition: &AgentDefinition) -> AiExecutionRequest {
    let mut request = connection_probe_request(definition);
    request.execution_id = format!("agent-models-{}", Uuid::new_v4());
    request.purpose = AiExecutionPurpose::ModelDiscovery;
    request.prompt = "ACP model discovery".to_string();
    request
}

fn parse_session_models(
    session: &agent_client_protocol::schema::v1::NewSessionResponse,
) -> Result<(Vec<AgentModelOption>, Option<String>), AiExecutionError> {
    let value = serde_json::to_value(session).map_err(|_| AiExecutionError::Protocol {
        operation: "session_model_catalog_serialize",
    })?;
    let config_options = array_field(&value, &["config_options", "configOptions"]);
    let model_option = config_options.iter().find(|option| {
        let category = string_field(option, &["category"]).unwrap_or_default();
        let id = string_field(option, &["id"]).unwrap_or_default();
        let option_type = string_field(option, &["type", "option_type"]).unwrap_or_default();
        (category == "model" || id == "model")
            && (option_type.is_empty() || option_type == "select")
    });

    if let Some(model_option) = model_option {
        let models = array_field(model_option, &["options"])
            .into_iter()
            .filter_map(parse_model_option)
            .collect::<Vec<_>>();
        let current_model_id = string_field(
            model_option,
            &[
                "current_value",
                "currentValue",
                "selected_value",
                "selectedValue",
            ],
        )
        .or_else(|| string_field(&value, &["current_model_id", "currentModelId"]));
        if !models.is_empty() {
            return Ok((models, current_model_id));
        }
    }

    let models = array_field(&value, &["available_models", "availableModels"])
        .into_iter()
        .filter_map(parse_model_option)
        .collect::<Vec<_>>();
    let current_model_id = string_field(&value, &["current_model_id", "currentModelId"]);
    if models.is_empty() {
        return Err(AiExecutionError::Protocol {
            operation: "session_model_catalog_empty",
        });
    }
    Ok((models, current_model_id))
}

fn array_field<'a>(value: &'a Value, names: &[&str]) -> Vec<&'a Value> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_array))
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn string_field(value: &Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn parse_model_option(value: &Value) -> Option<AgentModelOption> {
    if let Some(id) = value.as_str().map(str::trim).filter(|id| !id.is_empty()) {
        return Some(AgentModelOption {
            id: id.to_owned(),
            label: id.to_owned(),
            description: None,
        });
    }
    let id = string_field(value, &["id", "value"])?;
    Some(AgentModelOption {
        label: string_field(value, &["label", "name"]).unwrap_or_else(|| id.clone()),
        description: string_field(value, &["description"]),
        id,
    })
}

async fn run_execution(
    guard: &mut AcpExecutionGuard,
    definition: &AgentDefinition,
    request: &AiExecutionRequest,
    started: Instant,
) -> Result<AiExecutionResult, AiExecutionError> {
    request.report_phase(AiExecutionPhase::Spawning);
    let process = ManagedAgentProcess::spawn(
        definition,
        Some(&guard.workspace),
        request.limits.stderr_bytes,
    )
    .await
    .map_err(|error| map_process_error(definition, error))?;
    if !request.replay {
        log_info(
            "ai_execution.process",
            "AI agent process started",
            &[
                ("execution_id", request.execution_id.clone()),
                ("agent_id", definition.id.to_string()),
                ("protocol", "acp".to_string()),
                ("phase", "spawning".to_string()),
                ("pid", process.process_id().to_string()),
                ("arg_count", definition.args.len().to_string()),
                ("env_key_count", definition.env.len().to_string()),
                (
                    "cwd_kind",
                    if matches!(request.session_mode, AgentSessionMode::Persistent) {
                        "stable"
                    } else {
                        "ephemeral"
                    }
                    .to_string(),
                ),
            ],
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
    let mut config = AcpConnectConfig::new(request.limits.initialize_timeout);
    config.event_channel_capacity = PROTOCOL_EVENT_CAPACITY;
    request.report_phase(AiExecutionPhase::Initializing);
    let (protocol, mut channels) = AcpProtocol::connect(stdin, stdout, config)
        .await
        .map_err(|error| map_acp_error("initialize", error))?;
    guard.protocol = Some(protocol);

    request.report_phase(AiExecutionPhase::CreatingSession);
    let protocol = guard
        .protocol
        .as_ref()
        .expect("protocol stored before session");
    let mut mcp_servers = team_mcp_servers(request.team_tools.as_ref())?;
    mcp_servers.extend(recall_mcp_servers(request.recall_tools.as_ref())?);
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
        });
    }

    if request.replay {
        request.report_phase(AiExecutionPhase::Prompting);
        let replay_text = collect_replay_text(
            &mut channels.events,
            &session,
            request,
            request.limits.text_bytes,
        )
        .await;
        return Ok(AiExecutionResult {
            text: replay_text.clone(),
            agent_id: definition.id.clone(),
            protocol: AgentProtocol::Acp,
            requested_model: request.model.clone(),
            elapsed_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            persistent_binding: None,
            replay_text: Some(replay_text),
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
                    AcpError::RequestFailed { message, .. } => Some(message),
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
            Some(crate::backend::ai_execution::PersistentExecutionBinding {
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
    })
}

fn team_mcp_servers(
    team_tools: Option<&crate::backend::ai_execution::AiTeamTools>,
) -> Result<Vec<McpServer>, AiExecutionError> {
    let Some(team_tools) = team_tools else {
        return Ok(Vec::new());
    };
    let executable = std::env::current_exe().map_err(|_| AiExecutionError::Protocol {
        operation: "team_mcp_executable",
    })?;
    Ok(vec![McpServer::Stdio(
        McpServerStdio::new("assetiweave-team", executable)
            .args(vec!["--team-mcp-stdio".to_string()])
            .env(vec![
                EnvVariable::new("ASSETIWEAVE_DB_PATH", team_tools.database_path.clone()),
                EnvVariable::new(
                    "ASSETIWEAVE_TEAM_TOOL_TENANT_ID",
                    team_tools.tenant_id.clone(),
                ),
                EnvVariable::new(
                    "ASSETIWEAVE_TEAM_TOOL_MEMBER_ID",
                    team_tools.member_id.clone(),
                ),
                EnvVariable::new(
                    "ASSETIWEAVE_TEAM_TOOL_CREDENTIAL",
                    team_tools.credential.clone(),
                ),
            ]),
    )])
}

fn recall_mcp_servers(
    recall_tools: Option<&crate::backend::ai_execution::AiRecallTools>,
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

async fn collect_replay_text(
    events: &mut tokio::sync::mpsc::Receiver<
        crate::backend::agents::protocol::acp::AcpRuntimeEvent,
    >,
    session_id: &SessionId,
    request: &AiExecutionRequest,
    max_bytes: usize,
) -> String {
    let mut bridge = AcpSessionEventBridge::new(request, session_id);
    bridge.emit_processing(SessionProcessingState::Started);
    let deadline = tokio::time::sleep(std::time::Duration::from_millis(250));
    tokio::pin!(deadline);
    let mut text = String::new();
    loop {
        tokio::select! {
            _ = &mut deadline => break,
            event = events.recv() => {
                let Some(event) = event else { break; };
                bridge.emit(&event);
                match event {
                    crate::backend::agents::protocol::acp::AcpRuntimeEvent::AgentText { session_id: event_session, text: chunk } if &event_session == session_id => {
                        if text.len().saturating_add(chunk.len()) > max_bytes { break; }
                        text.push_str(&chunk);
                    }
                    crate::backend::agents::protocol::acp::AcpRuntimeEvent::TurnCompleted { session_id: event_session, .. } if &event_session == session_id => break,
                    _ => {}
                }
            }
        }
    }
    text
}

struct AcpSessionEventBridge<'a> {
    request: &'a AiExecutionRequest,
    provider_session_id: SessionId,
    session_id: String,
    member_id: String,
    delivery: SessionEventDelivery,
    sequence: u64,
}

impl<'a> AcpSessionEventBridge<'a> {
    fn new(request: &'a AiExecutionRequest, provider_session_id: &SessionId) -> Self {
        let stable_context = request
            .execution_context_key
            .clone()
            .unwrap_or_else(|| format!("execution:{}", request.execution_id));
        Self {
            request,
            provider_session_id: provider_session_id.clone(),
            session_id: stable_context.clone(),
            member_id: stable_context,
            delivery: if request.replay {
                SessionEventDelivery::Replay
            } else {
                SessionEventDelivery::Live
            },
            sequence: 0,
        }
    }

    fn emit(&mut self, event: &AcpRuntimeEvent) {
        let event_session_id = match event {
            AcpRuntimeEvent::AgentText { session_id, .. }
            | AcpRuntimeEvent::AgentThought { session_id, .. }
            | AcpRuntimeEvent::ToolCall { session_id, .. }
            | AcpRuntimeEvent::ToolCallUpdate { session_id, .. }
            | AcpRuntimeEvent::PermissionRequested { session_id }
            | AcpRuntimeEvent::Other { session_id }
            | AcpRuntimeEvent::TurnCompleted { session_id, .. } => session_id,
        };
        if event_session_id != &self.provider_session_id {
            return;
        }

        match event {
            AcpRuntimeEvent::AgentText { text, .. } => {
                self.emit_kind(
                    ACP_TEXT_ITEM_ID,
                    SessionEventKind::AssistantTextDelta { text: text.clone() },
                );
            }
            AcpRuntimeEvent::AgentThought { text, .. } => {
                if let Some(text) = text.as_ref().filter(|text| !text.is_empty()) {
                    self.emit_kind(
                        ACP_THINKING_ITEM_ID,
                        SessionEventKind::ThinkingDelta { text: text.clone() },
                    );
                } else {
                    self.emit_processing(SessionProcessingState::Active);
                }
            }
            AcpRuntimeEvent::ToolCall {
                tool_call_id,
                title,
                status,
                ..
            } => {
                let item_id = tool_item_id(tool_call_id);
                self.emit_kind(
                    &item_id,
                    SessionEventKind::ToolStart {
                        name: (!title.trim().is_empty()).then(|| title.clone()),
                    },
                );
                self.emit_tool_status(&item_id, *status);
            }
            AcpRuntimeEvent::ToolCallUpdate {
                tool_call_id,
                title,
                status,
                ..
            } => {
                let item_id = tool_item_id(tool_call_id);
                if let Some(title) = title.as_ref() {
                    self.emit_kind(
                        &item_id,
                        SessionEventKind::ToolStart {
                            name: (!title.trim().is_empty()).then(|| title.clone()),
                        },
                    );
                }
                self.emit_tool_status(&item_id, status.unwrap_or(AcpToolStatus::InProgress));
            }
            AcpRuntimeEvent::PermissionRequested { .. } => {
                self.emit_kind(
                    "permission",
                    SessionEventKind::Notice {
                        code: "permission_requested".to_string(),
                        detail: None,
                    },
                );
            }
            AcpRuntimeEvent::TurnCompleted { stop_reason, .. } => {
                self.emit_processing(SessionProcessingState::Completed);
                if matches!(
                    stop_reason,
                    agent_client_protocol::schema::v1::StopReason::Cancelled
                ) {
                    self.emit_kind("cancel", SessionEventKind::Cancel);
                } else if matches!(
                    stop_reason,
                    agent_client_protocol::schema::v1::StopReason::EndTurn
                        | agent_client_protocol::schema::v1::StopReason::MaxTokens
                        | agent_client_protocol::schema::v1::StopReason::MaxTurnRequests
                ) {
                    self.emit_kind(
                        ACP_TERMINAL_ITEM_ID,
                        SessionEventKind::TerminalResult { text: None },
                    );
                } else {
                    self.emit_kind(
                        "error",
                        SessionEventKind::Error {
                            code: "provider_turn_stopped".to_string(),
                            retryable: false,
                        },
                    );
                }
            }
            AcpRuntimeEvent::Other { .. } => {}
        }
    }

    fn emit_processing(&mut self, state: SessionProcessingState) {
        self.emit_kind(
            ACP_PROCESSING_ITEM_ID,
            SessionEventKind::Processing { state },
        );
    }

    fn emit_tool_status(&mut self, item_id: &str, status: AcpToolStatus) {
        match status {
            AcpToolStatus::Pending => {}
            AcpToolStatus::InProgress => self.emit_kind(
                item_id,
                SessionEventKind::ToolUpdate {
                    state: SessionToolState::Running,
                    detail: None,
                },
            ),
            AcpToolStatus::Completed => self.emit_kind(
                item_id,
                SessionEventKind::ToolResult {
                    success: true,
                    detail: None,
                },
            ),
            AcpToolStatus::Failed => self.emit_kind(
                item_id,
                SessionEventKind::ToolResult {
                    success: false,
                    detail: None,
                },
            ),
        }
    }

    fn emit_kind(&mut self, item_id: &str, kind: SessionEventKind) {
        self.sequence = self.sequence.saturating_add(1);
        let event_id = format!("acp:{}:{}", self.request.execution_id, self.sequence);
        self.request.report_session_event(SessionEvent {
            identity: SessionEventIdentity {
                session_id: self.session_id.clone(),
                member_id: self.member_id.clone(),
                execution_id: self.request.execution_id.clone(),
                turn_id: self.request.execution_id.clone(),
                item_id: item_id.to_string(),
                event_id,
            },
            sequence: self.sequence,
            delivery: self.delivery,
            kind,
        });
    }
}

fn tool_item_id(tool_call_id: &str) -> String {
    format!("tool:{tool_call_id}")
}

async fn run_prompt_and_aggregate(
    protocol: &AcpProtocol,
    process: &ManagedAgentProcess,
    channels: AcpProtocolChannels,
    session_id: SessionId,
    request: &AiExecutionRequest,
    definition: &AgentDefinition,
) -> Result<String, AiExecutionError> {
    let AcpProtocolChannels {
        mut events,
        mut disconnects,
    } = channels;
    let mut aggregator = if matches!(request.purpose, AiExecutionPurpose::Recall) {
        PromptAggregator::Recall(RecallStructuredAggregator::new(
            session_id.clone(),
            request.limits.text_bytes,
        ))
    } else {
        PromptAggregator::Translation(TranslationTextAggregator::new(
            session_id.clone(),
            request.limits.text_bytes,
        ))
    };
    let mut bridge = AcpSessionEventBridge::new(request, &session_id);
    bridge.emit_processing(SessionProcessingState::Started);
    let mut prompt =
        Box::pin(protocol.prompt(session_id.clone(), request.prompt.trim().to_owned()));
    let mut process_exit = Box::pin(process.wait_for_exit());
    let cancellation = request.cancellation.cancelled();
    tokio::pin!(cancellation);
    let mut prompt_response = None;

    loop {
        tokio::select! {
            response = &mut prompt, if prompt_response.is_none() => {
                prompt_response = Some(response.map_err(|error| map_acp_error("prompt", error))?);
            }
            event = events.recv() => {
                let Some(event) = event else {
                    return Err(AiExecutionError::Protocol { operation: "event_stream" });
                };
                bridge.emit(&event);
                match aggregator.apply(event) {
                    AggregatorAction::Continue => {}
                    AggregatorAction::CancelAndFail(error) => {
                        bridge.emit_kind("cancel", SessionEventKind::Cancel);
                        let _ = protocol
                            .cancel_and_wait(session_id.clone(), request.limits.cancel_grace)
                            .await;
                        return Err(error);
                    }
                    AggregatorAction::Complete { stop_reason } => {
                        let response = match prompt_response.take() {
                            Some(response) => response,
                            None => prompt.await.map_err(|error| map_acp_error("prompt", error))?,
                        };
                        if response.stop_reason != stop_reason {
                            return Err(AiExecutionError::Protocol { operation: "prompt_completion" });
                        }
                        let diagnostics = aggregator.diagnostics();
                        let outcome = match stop_reason {
                            StopReason::EndTurn | StopReason::MaxTokens | StopReason::MaxTurnRequests => {
                                aggregator.finish()
                            }
                            StopReason::Cancelled => Err(cancelled_error(definition)),
                            StopReason::Refusal => Err(AiExecutionError::Protocol { operation: "prompt_refused" }),
                            _ => Err(AiExecutionError::Protocol { operation: "prompt_stopped" }),
                        };
                        let text_bytes = outcome.as_ref().map(|text| text.len()).unwrap_or_default();
                        if !request.replay {
                            log_info(
                                "ai_execution.output",
                                "AI execution output aggregated",
                                &[
                                    ("execution_id", request.execution_id.clone()),
                                    ("agent_id", definition.id.to_string()),
                                    ("protocol", "acp".to_string()),
                                    ("phase", "prompting".to_string()),
                                    ("text_bytes", text_bytes.to_string()),
                                    ("chunk_count", diagnostics.0.to_string()),
                                    ("thinking_chunk_count", diagnostics.1.to_string()),
                                    ("ignored_session_event_count", diagnostics.2.to_string()),
                                    ("stop_reason", format!("{stop_reason:?}").to_ascii_lowercase()),
                                ],
                            );
                        }
                        return outcome;
                    }
                }
            }
            _ = &mut cancellation => {
                bridge.emit_kind("cancel", SessionEventKind::Cancel);
                let _ = protocol
                    .cancel_and_wait(session_id.clone(), request.limits.cancel_grace)
                    .await;
                return Err(cancelled_error(definition));
            }
            exit = &mut process_exit => {
                return Err(AiExecutionError::AgentExited {
                    code: exit.and_then(|exit| exit.code),
                });
            }
            changed = disconnects.changed() => {
                if changed.is_err() || disconnects.borrow().is_some() {
                    return Err(AiExecutionError::Protocol { operation: "disconnect" });
                }
            }
        }
    }
}

enum PromptAggregator {
    Translation(TranslationTextAggregator),
    Recall(RecallStructuredAggregator),
}

impl PromptAggregator {
    fn apply(
        &mut self,
        event: crate::backend::agents::protocol::acp::AcpRuntimeEvent,
    ) -> AggregatorAction {
        match self {
            Self::Translation(aggregator) => aggregator.apply(event),
            Self::Recall(aggregator) => aggregator.apply(event),
        }
    }

    fn finish(self) -> Result<String, AiExecutionError> {
        match self {
            Self::Translation(aggregator) => aggregator.finish(),
            Self::Recall(aggregator) => aggregator.finish(),
        }
    }

    fn diagnostics(&self) -> (usize, usize, usize) {
        match self {
            Self::Translation(aggregator) => aggregator.diagnostics(),
            Self::Recall(aggregator) => aggregator.diagnostics(),
        }
    }
}

fn create_workspace(root: &Path) -> Result<PathBuf, AiExecutionError> {
    fs::create_dir_all(root).map_err(|_| AiExecutionError::Workspace {
        operation: "create_root",
    })?;
    let workspace = root.join(format!("execution-{}", Uuid::new_v4()));
    fs::create_dir(&workspace).map_err(|_| AiExecutionError::Workspace {
        operation: "create",
    })?;
    if !workspace.is_absolute() {
        let _ = fs::remove_dir_all(&workspace);
        return Err(AiExecutionError::Workspace {
            operation: "require_absolute_path",
        });
    }
    Ok(workspace)
}

fn map_process_error(
    definition: &AgentDefinition,
    error: ManagedAgentProcessError,
) -> AiExecutionError {
    match error {
        ManagedAgentProcessError::ExecutableNotFound { command_name } => {
            AiExecutionError::RuntimeUnavailable { command_name }
        }
        other => AiExecutionError::Spawn {
            program: PathBuf::from(&definition.command),
            message: other.to_string(),
        },
    }
}

fn map_acp_error(operation: &'static str, error: AcpError) -> AiExecutionError {
    match error {
        AcpError::RequestFailed { message, .. } if is_model_unavailable_message(&message) => {
            AiExecutionError::ModelUnavailable {
                detail: normalize_provider_error_message(&message),
            }
        }
        AcpError::RequestFailed { message, .. } => AiExecutionError::ProtocolDetail {
            operation,
            detail: normalize_provider_error_message(&message),
        },
        _ => AiExecutionError::Protocol { operation },
    }
}

fn is_model_unavailable_message(message: &str) -> bool {
    message
        .to_ascii_lowercase()
        .contains("no allowed providers are available for the selected model")
}

fn normalize_provider_error_message(message: &str) -> String {
    let message = message.trim();
    let message = message
        .strip_prefix("Internal error:")
        .map(str::trim)
        .unwrap_or(message);
    message
        .strip_prefix("Error from provider (Console):")
        .map(str::trim)
        .unwrap_or(message)
        .to_owned()
}

fn cancelled_error(definition: &AgentDefinition) -> AiExecutionError {
    AiExecutionError::Cancelled {
        program: PathBuf::from(&definition.command),
    }
}

fn timeout_error(definition: &AgentDefinition, timeout: std::time::Duration) -> AiExecutionError {
    AiExecutionError::Timeout {
        program: PathBuf::from(&definition.command),
        timeout,
    }
}

struct AcpExecutionGuard {
    workspace: PathBuf,
    process: Option<ManagedAgentProcess>,
    protocol: Option<AcpProtocol>,
    session_id: Option<SessionId>,
    cleaned: bool,
    bound_session: Option<String>,
    preserve_session: bool,
    preserve_workspace: bool,
}

impl AcpExecutionGuard {
    fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            process: None,
            protocol: None,
            session_id: None,
            cleaned: false,
            bound_session: None,
            preserve_session: false,
            preserve_workspace: false,
        }
    }

    async fn cleanup(
        &mut self,
        cancel_before_close: bool,
        request: &AiExecutionRequest,
        definition: &AgentDefinition,
    ) -> CleanupReport {
        if self.cleaned {
            return CleanupReport::already_cleaned();
        }
        self.cleaned = true;
        let mut report = CleanupReport::default();
        let mut standard_delete_failure = None;
        if self.session_id.is_some() {
            report.session_closed = Some(false);
            report.session_deleted = Some(false);
        }

        if let Some(protocol) = self.protocol.as_ref() {
            if cancel_before_close {
                if let Some(session_id) = self.session_id.clone() {
                    let cancel_timeout = request
                        .limits
                        .cancel_grace
                        .min(request.limits.close_timeout);
                    match protocol.cancel_and_wait(session_id, cancel_timeout).await {
                        Ok(()) => {}
                        Err(_) => report.failures.push("cancel".to_owned()),
                    }
                }
            }
            if let Some(session_id) = self.session_id.clone() {
                if self.preserve_session {
                    // Persistent keeps the provider session resumable; only
                    // the process is reaped below.
                } else {
                    match tokio::time::timeout(
                        request.limits.close_timeout,
                        protocol.close_session(session_id),
                    )
                    .await
                    {
                        Ok(Ok(Some(_))) => report.session_closed = Some(true),
                        Ok(Ok(None)) => report.session_closed = None,
                        Ok(Err(_)) => report.failures.push("close".to_owned()),
                        Err(_) => report.failures.push("close_timeout".to_owned()),
                    }
                }
            }
            if let Some(session_id) = self.session_id.clone() {
                if self.preserve_session {
                    // Persistent sessions must not be deleted.
                } else {
                    match tokio::time::timeout(
                        request.limits.close_timeout,
                        protocol.delete_session(session_id),
                    )
                    .await
                    {
                        Ok(Ok(Some(_))) => {
                            report.session_deleted = Some(true);
                            report.session_delete_method =
                                Some(AiExecutionSessionDeleteMethod::Acp);
                        }
                        Ok(Ok(None)) => standard_delete_failure = Some("delete_unsupported"),
                        Ok(Err(AcpError::RequestFailed { message, .. }))
                            if matches_declared_not_found(
                                &message,
                                &definition.session_cleanup_not_found_markers,
                            ) =>
                        {
                            report.session_deleted = Some(true);
                            report.session_delete_method =
                                Some(AiExecutionSessionDeleteMethod::Acp);
                        }
                        Ok(Err(_)) => standard_delete_failure = Some("delete"),
                        Err(_) => standard_delete_failure = Some("delete_timeout"),
                    }
                }
            }
            if protocol
                .shutdown(request.limits.close_timeout)
                .await
                .is_err()
            {
                report.failures.push("protocol_shutdown".to_owned());
            }
        }
        self.protocol.take();

        if let Some(process) = self.process.as_ref() {
            report.process_id = Some(process.process_id());
            let termination = process.terminate(request.limits.cancel_grace).await;
            report.process_reaped = termination.exit.is_some();
            report.exit_code = termination.exit.as_ref().and_then(|exit| exit.code);
            if !termination.signal_errors.is_empty() {
                report.failures.push("process_signal".to_owned());
            }
            if !report.process_reaped {
                report.failures.push("process_reap".to_owned());
            }
            if !process
                .wait_for_stderr_eof(request.limits.close_timeout)
                .await
            {
                report.failures.push("stderr_join".to_owned());
            }
            match process.stderr_tail() {
                Ok(stderr) => {
                    report.stderr_bytes = stderr.bytes.len();
                    report.stderr_truncated = stderr.truncated;
                    if stderr.read_error {
                        report.failures.push("stderr_read".to_owned());
                    }
                }
                Err(_) => report.failures.push("stderr_state".to_owned()),
            }
        } else {
            report.process_reaped = true;
        }
        self.process.take();

        if !self.preserve_session
            && self.session_id.is_some()
            && report.session_deleted != Some(true)
        {
            if let Some(cleanup) = definition.session_cleanup.as_ref() {
                let session_id = self
                    .session_id
                    .as_ref()
                    .expect("session checked before fallback")
                    .to_string();
                let args = cleanup
                    .args
                    .iter()
                    .map(|arg| {
                        if arg == SESSION_ID_PLACEHOLDER {
                            session_id.clone()
                        } else {
                            arg.clone()
                        }
                    })
                    .collect();
                let output = run_host_command(
                    HostCommandSpec {
                        program: PathBuf::from(&definition.command),
                        args,
                        env: definition
                            .env
                            .iter()
                            .map(|entry| (entry.name.clone(), entry.value.clone()))
                            .collect(),
                        working_dir: Some(self.workspace.clone()),
                        stdin: HostInput::Null,
                        timeout: PROVIDER_SESSION_DELETE_TIMEOUT,
                        stdout_limit: request.limits.stderr_bytes,
                        stderr_limit: request.limits.stderr_bytes,
                    },
                    tokio_util::sync::CancellationToken::new(),
                )
                .await;
                match output {
                    Ok(output) if !output.stdout_truncated && !output.stderr_truncated => {
                        let missing_is_success = !output.status.success()
                            && std::str::from_utf8(&output.stderr)
                                .ok()
                                .is_some_and(|stderr| {
                                    matches_declared_not_found(
                                        stderr,
                                        &definition.session_cleanup_not_found_markers,
                                    )
                                });
                        if output.status.success() || missing_is_success {
                            report.session_deleted = Some(true);
                            report.session_delete_method =
                                Some(AiExecutionSessionDeleteMethod::ProviderFallback);
                        } else {
                            if let Some(failure) = standard_delete_failure {
                                report.failures.push(failure.to_owned());
                            }
                            report.failures.push("delete_fallback".to_owned());
                        }
                    }
                    _ => {
                        if let Some(failure) = standard_delete_failure {
                            report.failures.push(failure.to_owned());
                        }
                        report.failures.push("delete_fallback".to_owned());
                    }
                }
            } else if let Some(failure) = standard_delete_failure {
                report.failures.push(failure.to_owned());
            }
        }

        report.workspace_removed = if self.preserve_workspace {
            false
        } else {
            match fs::remove_dir_all(&self.workspace) {
                Ok(()) => true,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
                Err(_) => {
                    report.failures.push("workspace_remove".to_owned());
                    false
                }
            }
        };
        report
    }
}

fn matches_declared_not_found(message: &str, markers: &[String]) -> bool {
    message.lines().any(|line| {
        let line = line.trim_start();
        markers.iter().any(|marker| line.starts_with(marker))
    })
}

fn optional_bool_label(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "not_applicable",
    }
}

#[derive(Default, Debug)]
struct CleanupReport {
    failures: Vec<String>,
    process_id: Option<u32>,
    process_reaped: bool,
    workspace_removed: bool,
    stderr_bytes: usize,
    stderr_truncated: bool,
    exit_code: Option<i32>,
    session_deleted: Option<bool>,
    session_closed: Option<bool>,
    session_delete_method: Option<AiExecutionSessionDeleteMethod>,
}

impl CleanupReport {
    fn already_cleaned() -> Self {
        Self {
            process_reaped: true,
            workspace_removed: true,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{
        agents::types::{AgentEnvEntry, AgentId, DeclaredAgentCapabilities},
        ai_execution::{
            AgentSessionMode, AiExecutionCancellation, AiExecutionLimits, AiExecutionProgressSink,
            AiExecutionPurpose, SessionEvent,
        },
    };
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    #[test]
    fn provider_routing_failure_is_reported_as_an_actionable_model_error() {
        let error = map_acp_error(
            "prompt",
            AcpError::RequestFailed {
                operation: crate::backend::agents::protocol::acp::AcpOperation::Prompt,
                message: "Internal error: Error from provider (Console): Upstream request failed: [404] No allowed providers are available for the selected model.".to_string(),
            },
        );

        let view = error.to_view();
        assert_eq!(view.code, "model_unavailable");
        assert!(view
            .message
            .contains("Choose another model in Agent settings"));
        assert!(view.message.contains("Upstream request failed: [404]"));
        assert!(!view.message.contains("Internal error"));
        assert!(!view.message.contains("Error from provider (Console)"));
    }

    fn definition(mode: &str, record_path: &Path) -> AgentDefinition {
        AgentDefinition {
            id: AgentId::parse("fake-acp").unwrap(),
            installation_id: None,
            display_name: "Fake ACP".to_owned(),
            protocol: AgentProtocol::Acp,
            command: "node".to_owned(),
            args: vec![Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("test-fixtures/fake-acp-agent.mjs")
                .to_string_lossy()
                .into_owned()],
            env: vec![
                AgentEnvEntry::new("ASSETIWEAVE_FAKE_ACP_MODE", mode),
                AgentEnvEntry::new(
                    "ASSETIWEAVE_FAKE_ACP_RECORD_PATH",
                    record_path.to_string_lossy(),
                ),
            ],
            declared_capabilities: DeclaredAgentCapabilities::acp_text(),
            availability_probe: None,
            model_discovery: None,
            session_cleanup: None,
            session_cleanup_not_found_markers: Vec::new(),
        }
    }

    fn definition_with_fallback(
        mode: &str,
        record_path: &Path,
        cleanup_mode: &str,
    ) -> AgentDefinition {
        let mut definition = definition(mode, record_path);
        definition.env.push(AgentEnvEntry::new(
            "ASSETIWEAVE_FAKE_SESSION_CLEANUP_MODE",
            cleanup_mode,
        ));
        definition.session_cleanup = Some(
            crate::backend::agents::types::AgentCommandDefinition::new([
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("test-fixtures/fake-session-cleanup.mjs")
                    .to_string_lossy()
                    .into_owned(),
                "{session_id}".to_string(),
            ]),
        );
        definition
    }

    fn request(model: Option<&str>) -> AiExecutionRequest {
        AiExecutionRequest {
            execution_id: Uuid::new_v4().to_string(),
            agent_id: AgentId::parse("fake-acp").unwrap(),
            purpose: AiExecutionPurpose::Translation,
            session_mode: AgentSessionMode::OneShot,
            prompt: "translate fixture".to_owned(),
            model: model.map(str::to_owned),
            limits: AiExecutionLimits {
                initialize_timeout: Duration::from_secs(5),
                config_rpc_timeout: Duration::from_secs(2),
                cancel_grace: Duration::from_millis(500),
                close_timeout: Duration::from_millis(500),
                text_bytes: 1024,
                stderr_bytes: 1024,
                ..AiExecutionLimits::default()
            },
            cancellation: AiExecutionCancellation::default(),
            progress: None,
            tenant_id: None,
            execution_context_key: None,
            binding: None,
            replay: false,
            restore_only: false,
            team_tools: None,
            recall_tools: None,
        }
    }

    fn test_paths(name: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("assetiweave-acp-{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let record = root.join("record.ndjson");
        (root, record)
    }

    fn records(path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_default()
    }

    #[derive(Default)]
    struct CaptureSessionEvents {
        events: Mutex<Vec<SessionEvent>>,
    }

    impl AiExecutionProgressSink for CaptureSessionEvents {
        fn set_phase(&self, _phase: AiExecutionPhase) {}

        fn emit_session_event(&self, event: SessionEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn t03_fake_acp_events_reach_the_typed_session_sink() {
        let (root, record) = test_paths("session-events");
        let backend = AcpExecutionBackend::new(root.join("workspaces"));
        let capture = Arc::new(CaptureSessionEvents::default());
        let mut execution_request = request(None);
        execution_request.purpose = AiExecutionPurpose::Recall;
        execution_request.recall_tools = Some(crate::backend::ai_execution::AiRecallTools {
            tenant_id: "tenant-fixture".to_string(),
            recall_session_id: "recall-fixture".to_string(),
            database_path: root.join("fixture.db").to_string_lossy().into_owned(),
        });
        execution_request.execution_context_key = Some("member-fixture".to_string());
        execution_request.progress = Some(capture.clone());

        backend
            .execute(&definition("session_events", &record), execution_request)
            .await
            .expect("ACP event fixture execution");

        let events = capture.events.lock().unwrap();
        assert!(events.iter().any(|event| matches!(
            event.kind,
            crate::backend::ai_execution::SessionEventKind::AssistantTextDelta { ref text }
                if text == "visible "
        )));
        assert!(events.iter().any(|event| matches!(
            event.kind,
            crate::backend::ai_execution::SessionEventKind::AssistantTextDelta { ref text }
                if text == "answer"
        )));
        assert!(events.iter().any(|event| matches!(
            event.kind,
            crate::backend::ai_execution::SessionEventKind::ThinkingDelta { ref text }
                if text == "provider thought"
        )));
        assert!(events.iter().any(|event| matches!(
            event.kind,
            crate::backend::ai_execution::SessionEventKind::Processing {
                state: SessionProcessingState::Active
            }
        )));
        assert!(!events.iter().any(|event| matches!(
            event.kind,
            crate::backend::ai_execution::SessionEventKind::ThinkingDelta { ref text }
                if text.contains("thought-metadata")
        )));
        assert!(events.iter().any(|event| matches!(
            event.kind,
            crate::backend::ai_execution::SessionEventKind::ToolStart { ref name }
                if name.as_deref() == Some("read fixture")
        )));
        assert!(events.iter().any(|event| matches!(
            event.kind,
            crate::backend::ai_execution::SessionEventKind::ToolResult { success: true, .. }
        )));
        assert!(events.iter().any(|event| matches!(
            event.kind,
            crate::backend::ai_execution::SessionEventKind::ToolUpdate {
                state: crate::backend::ai_execution::SessionToolState::Running,
                ..
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event.kind,
            crate::backend::ai_execution::SessionEventKind::TerminalResult { .. }
        )));
        let event_shapes = events
            .iter()
            .map(|event| match &event.kind {
                crate::backend::ai_execution::SessionEventKind::Processing { state } => match state
                {
                    SessionProcessingState::Started => "processing_started",
                    SessionProcessingState::Active => "processing_active",
                    SessionProcessingState::Completed => "processing_completed",
                },
                crate::backend::ai_execution::SessionEventKind::AssistantTextDelta { .. } => "text",
                crate::backend::ai_execution::SessionEventKind::ThinkingDelta { .. } => "thinking",
                crate::backend::ai_execution::SessionEventKind::ToolStart { .. } => "tool_start",
                crate::backend::ai_execution::SessionEventKind::ToolUpdate { .. } => "tool_update",
                crate::backend::ai_execution::SessionEventKind::ToolResult { .. } => "tool_result",
                crate::backend::ai_execution::SessionEventKind::TerminalResult { .. } => "terminal",
                _ => "other",
            })
            .collect::<Vec<_>>();
        assert_eq!(
            event_shapes,
            vec![
                "processing_started",
                "text",
                "text",
                "thinking",
                "processing_active",
                "tool_start",
                "tool_update",
                "tool_result",
                "processing_completed",
                "terminal",
            ]
        );
        let execution_id = events[0].identity.execution_id.clone();
        assert!(events
            .iter()
            .all(|event| event.identity.execution_id == execution_id));
        assert!(events
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence));
        assert!(events.iter().all(|event| {
            event.identity.session_id == "member-fixture"
                && event.identity.member_id == "member-fixture"
                && event.identity.execution_id != ""
                && event.identity.turn_id != ""
                && event.identity.item_id != ""
                && event.identity.event_id != ""
                && matches!(
                    event.delivery,
                    crate::backend::ai_execution::SessionEventDelivery::Live
                )
        }));
        assert!(events
            .iter()
            .all(|event| event.identity.session_id != "fixture-session"));
        let projection = crate::backend::ai_execution::SessionEventProjection::default();
        for event in events.iter().cloned() {
            assert_eq!(
                projection.apply(event),
                crate::backend::ai_execution::SessionEventApplyResult::Applied
            );
        }
        let snapshot = projection.snapshot();
        assert_eq!(
            snapshot
                .items
                .iter()
                .find(|item| item.kind
                    == crate::backend::ai_execution::SessionItemKind::AssistantText)
                .and_then(|item| item.text.as_deref()),
            Some("visible answer")
        );
        assert!(snapshot.items.iter().any(|item| {
            item.kind == crate::backend::ai_execution::SessionItemKind::Tool
                && item.state == crate::backend::ai_execution::SessionItemState::Succeeded
        }));
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn t03_replay_events_are_marked_replay_and_do_not_prompt_again() {
        let (root, record) = test_paths("session-events-replay");
        let backend = AcpExecutionBackend::new(root.join("workspaces"));
        let context_key = "member-replay-fixture";
        let recall_tools = || crate::backend::ai_execution::AiRecallTools {
            tenant_id: "tenant-fixture".to_string(),
            recall_session_id: "recall-fixture".to_string(),
            database_path: root.join("fixture.db").to_string_lossy().into_owned(),
        };

        let live_capture = Arc::new(CaptureSessionEvents::default());
        let mut live_request = request(None);
        live_request.purpose = AiExecutionPurpose::Recall;
        live_request.session_mode = AgentSessionMode::Persistent;
        live_request.execution_context_key = Some(context_key.to_string());
        live_request.recall_tools = Some(recall_tools());
        live_request.progress = Some(live_capture);
        let binding = backend
            .execute(&definition("session_events", &record), live_request)
            .await
            .expect("live persistent ACP execution")
            .persistent_binding
            .expect("persistent binding");

        let replay_capture = Arc::new(CaptureSessionEvents::default());
        let mut replay_request = request(None);
        replay_request.purpose = AiExecutionPurpose::Recall;
        replay_request.session_mode = AgentSessionMode::Persistent;
        replay_request.execution_context_key = Some(context_key.to_string());
        replay_request.recall_tools = Some(recall_tools());
        replay_request.binding = Some(binding);
        replay_request.replay = true;
        replay_request.progress = Some(replay_capture.clone());
        let replay = backend
            .execute(&definition("session_events", &record), replay_request)
            .await
            .expect("provider history replay");

        assert_eq!(
            replay.replay_text.as_deref(),
            Some("replayed:fixture-history")
        );
        let events = replay_capture.events.lock().unwrap();
        assert!(events.iter().any(|event| matches!(
            event.kind,
            crate::backend::ai_execution::SessionEventKind::ThinkingDelta { ref text }
                if text == "replayed provider thought"
        )));
        assert!(events.iter().any(|event| matches!(
            event.kind,
            crate::backend::ai_execution::SessionEventKind::ToolResult { success: true, .. }
        )));
        assert!(events.iter().all(|event| {
            matches!(
                event.delivery,
                crate::backend::ai_execution::SessionEventDelivery::Replay
            ) && event.identity.session_id == context_key
                && event.identity.member_id == context_key
        }));
        let record_contents = records(&record);
        assert_eq!(record_contents.matches("\"event\":\"prompt\"").count(), 1);
        assert_eq!(record_contents.matches("\"event\":\"load\"").count(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn life_01_happy_stdio_flow_closes_reaps_and_removes_workspace() {
        let (root, record) = test_paths("happy");
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());

        let result = backend
            .execute(&definition("happy", &record), request(Some("vendor/model")))
            .await
            .expect("happy execution");

        assert_eq!(result.text, "translated");
        assert_eq!(result.requested_model.as_deref(), Some("vendor/model"));
        assert_eq!(result.protocol, AgentProtocol::Acp);
        let record = records(&record);
        assert!(record.contains("\"event\":\"initialize\""));
        assert!(record.contains("\"event\":\"new\""));
        assert!(record.contains("\"mcpCount\":0"));
        assert!(record.contains("\"event\":\"model\""));
        assert!(record.contains("\"event\":\"prompt\""));
        assert!(record.contains("\"event\":\"close\""));
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn one_shot_cleanup_deletes_the_session_after_close() {
        let (root, record) = test_paths("one-shot-delete");
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());

        backend
            .execute(&definition("happy", &record), request(None))
            .await
            .expect("one-shot execution");

        let records = records(&record);
        let close = records.find("\"event\":\"close\"").expect("close record");
        let delete = records.find("\"event\":\"delete\"").expect("delete record");
        assert!(
            close < delete,
            "session/delete must run after session/close"
        );
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn one_shot_delete_succeeds_when_close_is_not_advertised() {
        let (root, record) = test_paths("one-shot-no-close");
        let backend = AcpExecutionBackend::new(root.join("workspaces"));

        backend
            .execute(&definition("no_close", &record), request(None))
            .await
            .expect("delete does not require close capability");

        let records = records(&record);
        assert!(!records.contains("\"event\":\"close\""));
        assert!(records.contains("\"event\":\"delete\""));
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn one_shot_standard_delete_success_skips_the_declared_fallback() {
        let (root, record) = test_paths("one-shot-standard-delete");
        let backend = AcpExecutionBackend::new(root.join("workspaces"));

        backend
            .execute(
                &definition_with_fallback("happy", &record, "failure"),
                request(None),
            )
            .await
            .expect("standard delete completes cleanup");

        let records = records(&record);
        assert!(records.contains("\"event\":\"delete\""));
        assert!(!records.contains("\"event\":\"fallback_delete\""));
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn one_shot_standard_delete_treats_a_declared_missing_session_as_deleted() {
        let (root, record) = test_paths("one-shot-standard-not-found");
        let backend = AcpExecutionBackend::new(root.join("workspaces"));
        let mut definition = definition_with_fallback("delete_not_found", &record, "failure");
        definition.session_cleanup_not_found_markers = vec!["Session not found:".to_string()];

        backend
            .execute(&definition, request(None))
            .await
            .expect("already missing is idempotent success");

        let records = records(&record);
        assert!(records.contains("\"event\":\"delete\""));
        assert!(!records.contains("\"event\":\"fallback_delete\""));
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn one_shot_standard_delete_failure_or_timeout_uses_the_declared_fallback() {
        for mode in ["delete_error", "delete_hang"] {
            let (root, record) = test_paths(mode);
            let backend = AcpExecutionBackend::new(root.join("workspaces"));

            backend
                .execute(
                    &definition_with_fallback(mode, &record, "success"),
                    request(None),
                )
                .await
                .expect("fallback completes cleanup");

            let records = records(&record);
            assert!(records.contains("\"event\":\"delete\""));
            assert!(records.contains("\"event\":\"fallback_delete\""));
            let _ = fs::remove_dir_all(root);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn one_shot_uses_declared_fallback_after_reaping_an_agent_without_standard_delete() {
        let (root, record) = test_paths("one-shot-fallback");
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());

        let result = backend
            .execute(
                &definition_with_fallback("no_delete", &record, "success"),
                request(None),
            )
            .await
            .expect("successful fallback completes the OneShot task");

        assert_eq!(result.text, "translated");
        let records = records(&record);
        assert!(!records.contains("\"event\":\"delete\""));
        let reaped = records.find("\"event\":\"sigterm\"").expect("reap record");
        let fallback = records
            .find("\"event\":\"fallback_delete\"")
            .expect("fallback record");
        assert!(
            reaped < fallback,
            "fallback must run after ACP process reap"
        );
        assert!(records.contains("\"sessionId\":\"fixture-session\""));
        assert!(records.contains("\"originalProcessReaped\":true"));
        assert!(records.contains("\"workspaceExists\":true"));
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn one_shot_allows_a_slow_provider_fallback_to_finish_after_the_acp_close_timeout() {
        let (root, record) = test_paths("one-shot-slow-fallback");
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());

        let result = backend
            .execute(
                &definition_with_fallback("no_delete", &record, "slow_success"),
                request(None),
            )
            .await;

        assert!(
            result.is_ok(),
            "a completed prompt must survive a slow session deletion: {result:?}"
        );
        assert!(records(&record).contains("\"event\":\"fallback_delete\""));
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn one_shot_fallback_failure_turns_a_successful_execution_into_cleanup_failed() {
        for cleanup_mode in ["failure", "timeout"] {
            let (root, record) = test_paths(cleanup_mode);
            let workspace_root = root.join("workspaces");
            let backend = AcpExecutionBackend::new(workspace_root.clone());

            let result = backend
                .execute(
                    &definition_with_fallback("no_delete", &record, cleanup_mode),
                    request(None),
                )
                .await;

            assert!(matches!(
                result,
                Err(AiExecutionError::CleanupFailed { .. })
            ));
            assert!(records(&record).contains("\"event\":\"fallback_delete\""));
            assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
            let _ = fs::remove_dir_all(root);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn one_shot_execution_failure_remains_primary_when_fallback_also_fails() {
        let (root, record) = test_paths("one-shot-primary-failure");
        let backend = AcpExecutionBackend::new(root.join("workspaces"));

        let result = backend
            .execute(
                &definition_with_fallback("no_delete_empty", &record, "failure"),
                request(None),
            )
            .await;

        assert!(matches!(result, Err(AiExecutionError::EmptyOutput { .. })));
        assert!(records(&record).contains("\"event\":\"fallback_delete\""));
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn one_shot_fallback_treats_a_declared_missing_session_marker_as_deleted() {
        let (root, record) = test_paths("one-shot-fallback-not-found");
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());
        let mut definition = definition_with_fallback("no_delete", &record, "not_found");
        definition.session_cleanup_not_found_markers = vec!["Session not found:".to_string()];

        let result = backend.execute(&definition, request(None)).await;

        assert!(result.is_ok());
        assert!(records(&record).contains("\"event\":\"fallback_delete\""));
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn persistent_mode_creates_a_stable_binding_and_reuses_resume() {
        let (root, record) = test_paths("persistent-resume");
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());
        let definition = definition("happy", &record);
        let mut first_request = request(None);
        first_request.session_mode = AgentSessionMode::Persistent;
        first_request.tenant_id = Some("tenant-fixture".to_string());
        first_request.execution_context_key = Some("team-member-fixture".to_string());
        let first = backend
            .execute(&definition, first_request)
            .await
            .expect("first persistent execution");
        let binding = first
            .persistent_binding
            .clone()
            .expect("first execution returns a binding");
        assert!(Path::new(&binding.workspace_path).is_dir());

        let mut second_request = request(None);
        second_request.session_mode = AgentSessionMode::Persistent;
        second_request.tenant_id = Some("tenant-fixture".to_string());
        second_request.execution_context_key = Some("team-member-fixture".to_string());
        second_request.binding = Some(binding.clone());
        backend
            .execute(&definition, second_request)
            .await
            .expect("resumed persistent execution");

        let mut replay_request = request(None);
        replay_request.session_mode = AgentSessionMode::Persistent;
        replay_request.tenant_id = Some("tenant-fixture".to_string());
        replay_request.execution_context_key = Some("team-member-fixture".to_string());
        replay_request.binding = Some(binding);
        replay_request.replay = true;
        let replay = backend
            .execute(&definition, replay_request)
            .await
            .expect("persistent history replay");
        assert_eq!(
            replay.replay_text.as_deref(),
            Some("replayed:fixture-history")
        );

        let records = records(&record);
        assert_eq!(records.matches("\"event\":\"new\"").count(), 1);
        assert_eq!(records.matches("\"event\":\"resume\"").count(), 1);
        assert_eq!(records.matches("\"event\":\"load\"").count(), 1);
        assert_eq!(records.matches("\"event\":\"prompt\"").count(), 2);
        assert!(!records.contains("\"event\":\"close\""));
        assert!(!records.contains("\"event\":\"delete\""));
        assert!(Path::new(&first.persistent_binding.unwrap().workspace_path).is_dir());
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn life_02_connection_probe_performs_initialize_and_session_new_without_prompt() {
        let (root, record) = test_paths("connection-probe");
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());

        backend
            .check_connection(&definition("happy", &record))
            .await
            .expect("ACP connection probe");

        let records = records(&record);
        assert!(records.contains("\"event\":\"initialize\""));
        assert!(records.contains("\"event\":\"new\""));
        assert!(records.contains("\"event\":\"close\""));
        assert!(!records.contains("\"event\":\"prompt\""));
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn connection_probe_accepts_declared_delete_not_found_from_error_data() {
        let (root, record) = test_paths("connection-probe-delete-not-found-data");
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());
        let mut agent = definition("delete_not_found_in_data", &record);
        agent.session_cleanup_not_found_markers =
            vec!["Internal error: no rollout found for thread id".to_string()];

        backend
            .check_connection(&agent)
            .await
            .expect("an absent empty probe session is already deleted");

        let records = records(&record);
        assert!(records.contains("\"event\":\"delete\""));
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn life_03_connection_probe_requires_a_non_empty_model_list() {
        let (root, record) = test_paths("connection-probe-no-models");
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());

        let error = backend
            .check_connection(&definition("no_models", &record))
            .await
            .expect_err("an ACP session without models is not usable");

        assert!(matches!(
            error,
            AiExecutionError::Protocol {
                operation: "session_model_catalog_empty"
            }
        ));
        let records = records(&record);
        assert!(records.contains("\"event\":\"initialize\""));
        assert!(records.contains("\"event\":\"new\""));
        assert!(records.contains("\"event\":\"close\""));
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn life_04_model_discovery_reads_session_config_options_without_prompt() {
        let (root, record) = test_paths("model-discovery");
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());

        let (models, current_model_id) = backend
            .discover_models(&definition("happy", &record))
            .await
            .expect("ACP model discovery");

        assert_eq!(current_model_id.as_deref(), Some("fixture/model-fast"));
        assert_eq!(models[0].id, "fixture/model-fast");
        assert_eq!(models[0].label, "Fixture Fast");
        assert_eq!(models[0].description.as_deref(), Some("Fast fixture model"));
        assert!(records(&record).contains("\"event\":\"new\""));
        assert!(!records(&record).contains("\"event\":\"prompt\""));
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn life_05_model_discovery_rejects_an_empty_model_list() {
        let (root, record) = test_paths("model-discovery-no-models");
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());

        let error = backend
            .discover_models(&definition("no_models", &record))
            .await
            .expect_err("empty model discovery must fail health checks");

        assert!(matches!(
            error,
            AiExecutionError::Protocol {
                operation: "session_model_catalog_empty"
            }
        ));
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn chunked_wrong_session_thinking_and_late_chunk_modes_are_aggregated() {
        for (mode, expected) in [
            ("chunked", "你好🌍"),
            ("wrong_session", "right"),
            ("thinking", "visible"),
            ("late_chunk", "before late"),
        ] {
            let (root, record) = test_paths(mode);
            let backend = AcpExecutionBackend::new(root.join("workspaces"));
            let result = backend
                .execute(&definition(mode, &record), request(None))
                .await
                .unwrap();
            assert_eq!(result.text, expected, "mode {mode}");
            let _ = fs::remove_dir_all(root);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn init_new_and_model_failures_still_cleanup_and_model_never_prompts() {
        for mode in [
            "initialize_error",
            "initialize_timeout",
            "new_error",
            "model_reject",
            "model_timeout",
        ] {
            let (root, record) = test_paths(mode);
            let workspace_root = root.join("workspaces");
            let backend = AcpExecutionBackend::new(workspace_root.clone());
            let result = backend
                .execute(&definition(mode, &record), request(Some("vendor/model")))
                .await;
            assert!(result.is_err(), "mode {mode}");
            if mode.starts_with("model_") {
                assert!(!records(&record).contains("\"event\":\"prompt\""));
            }
            assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
            let _ = fs::remove_dir_all(root);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn permission_tool_empty_and_output_limit_fail_closed_and_cleanup() {
        for mode in ["permission", "tool_call", "empty", "oversized"] {
            let (root, record) = test_paths(mode);
            let workspace_root = root.join("workspaces");
            let backend = AcpExecutionBackend::new(workspace_root.clone());
            let mut execution_request = request(None);
            execution_request.limits.text_bytes = 64;
            let result = backend
                .execute(&definition(mode, &record), execution_request)
                .await;
            match mode {
                "permission" => assert!(matches!(result, Err(AiExecutionError::PermissionDenied))),
                "tool_call" => assert!(matches!(result, Err(AiExecutionError::ToolUseDenied))),
                "empty" => assert!(matches!(result, Err(AiExecutionError::EmptyOutput { .. }))),
                "oversized" => assert!(matches!(result, Err(AiExecutionError::OutputLimit { .. }))),
                _ => unreachable!(),
            }
            if matches!(mode, "permission" | "tool_call" | "oversized") {
                assert!(records(&record).contains("\"event\":\"cancel\""));
            }
            assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
            let _ = fs::remove_dir_all(root);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn user_cancel_reaches_in_flight_agent_before_terminal_cleanup() {
        let (root, record) = test_paths("cancel");
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());
        let execution_request = request(None);
        let cancellation = execution_request.cancellation.clone();
        let definition = definition("cancel_wait", &record);

        let execution = backend.execute(&definition, execution_request);
        tokio::pin!(execution);
        loop {
            tokio::select! {
                result = &mut execution => panic!("execution completed before cancellation: {result:?}"),
                _ = async {
                    if records(&record).contains("\"event\":\"prompt\"") {
                        return;
                    }
                    tokio::task::yield_now().await;
                } => {
                    if records(&record).contains("\"event\":\"prompt\"") {
                        break;
                    }
                }
            }
        }
        cancellation.cancel();
        let result = tokio::time::timeout(Duration::from_secs(2), &mut execution)
            .await
            .expect("cancel cleanup timeout");

        assert!(matches!(result, Err(AiExecutionError::Cancelled { .. })));
        assert!(records(&record).contains("\"event\":\"cancel\""));
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn total_timeout_cancels_in_flight_prompt_and_waits_for_cleanup() {
        let (root, record) = test_paths("total-timeout");
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());
        let mut execution_request = request(None);
        execution_request.limits.total_timeout = Duration::from_millis(500);

        let result = backend
            .execute(&definition("cancel_wait", &record), execution_request)
            .await;

        assert!(matches!(result, Err(AiExecutionError::Timeout { .. })));
        assert!(records(&record).contains("\"event\":\"cancel\""));
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn close_failure_never_returns_success_and_process_is_reaped() {
        for mode in ["close_error", "close_hang"] {
            let (root, record) = test_paths(mode);
            let workspace_root = root.join("workspaces");
            let backend = AcpExecutionBackend::new(workspace_root.clone());
            let result = backend
                .execute(&definition(mode, &record), request(None))
                .await;

            assert!(matches!(
                result,
                Err(AiExecutionError::CleanupFailed { .. })
            ));
            assert!(records(&record).contains("\"event\":\"delete\""));
            assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
            let _ = fs::remove_dir_all(root);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn disconnect_and_process_exit_are_classified_and_cleaned() {
        for mode in ["disconnect", "exit_during_prompt"] {
            let (root, record) = test_paths(mode);
            let workspace_root = root.join("workspaces");
            let backend = AcpExecutionBackend::new(workspace_root.clone());
            let result = backend
                .execute(&definition(mode, &record), request(None))
                .await;

            assert!(matches!(
                result,
                Err(AiExecutionError::Protocol { .. }
                    | AiExecutionError::ProtocolDetail { .. }
                    | AiExecutionError::AgentExited { .. })
            ));
            assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
            let _ = fs::remove_dir_all(root);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cleanup_is_idempotent() {
        let (root, _record) = test_paths("cleanup-repeat");
        let workspace = create_workspace(&root.join("workspaces")).unwrap();
        let mut guard = AcpExecutionGuard::new(workspace);
        let execution_request = request(None);

        let definition = definition("happy", &_record);
        let first = guard.cleanup(false, &execution_request, &definition).await;
        let second = guard.cleanup(false, &execution_request, &definition).await;

        assert!(first.workspace_removed);
        assert!(second.workspace_removed);
        assert!(second.process_reaped);
        assert!(second.failures.is_empty());
        let _ = fs::remove_dir_all(root);
    }
}
