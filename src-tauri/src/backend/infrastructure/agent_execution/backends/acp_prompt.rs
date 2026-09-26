use agent_client_protocol::schema::v1::{SessionId, StopReason};
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

use crate::backend::infrastructure::agent_execution::managed_process as agent_process;
use crate::backend::infrastructure::agent_execution::protocol::acp as acp_protocol;
use crate::backend::{
    domain::agents::AgentDefinition,
    infrastructure::agent_execution::{
        error::AiExecutionError,
        session_events::{SessionEventKind, SessionProcessingState},
        types::{AiExecutionPurpose, AiExecutionRequest},
    },
};

use super::acp_aggregator::{
    AggregatorAction, ReadOnlyToolTextAggregator, TranslationTextAggregator,
};
use super::acp_event_bridge::AcpSessionEventBridge;

pub(crate) async fn run_prompt_and_aggregate(
    protocol: &acp_protocol::AcpProtocol,
    process: &agent_process::ManagedAgentProcess,
    channels: acp_protocol::AcpProtocolChannels,
    session_id: SessionId,
    request: &AiExecutionRequest,
    definition: &AgentDefinition,
) -> Result<String, AiExecutionError> {
    let acp_protocol::AcpProtocolChannels {
        mut events,
        mut disconnects,
    } = channels;
    let mut aggregator = if matches!(request.purpose, AiExecutionPurpose::Recall) {
        PromptAggregator::ReadOnlyTools(ReadOnlyToolTextAggregator::new(
            session_id.clone(),
            request.limits.text_bytes,
            ["memory_recall_search", "memory_recall_block"],
        ))
    } else if request.memory_generation_tools.is_some() {
        PromptAggregator::ReadOnlyTools(ReadOnlyToolTextAggregator::new(
            session_id.clone(),
            request.limits.text_bytes,
            [
                "get_session_outline",
                "search_session_content",
                "read_question_content",
                "read_content_node",
            ],
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
                            tracing::info!(
                                action = "ai_execution.output",
                                execution_id = %request.execution_id,
                                agent_id = %definition.id,
                                protocol = "acp",
                                phase = "prompting",
                                text_bytes,
                                chunk_count = diagnostics.0,
                                thinking_chunk_count = diagnostics.1,
                                ignored_session_event_count = diagnostics.2,
                                stop_reason = ?stop_reason,
                                "AI execution output aggregated"
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

pub(crate) enum PromptAggregator {
    Translation(TranslationTextAggregator),
    ReadOnlyTools(ReadOnlyToolTextAggregator),
}

impl PromptAggregator {
    fn apply(&mut self, event: acp_protocol::AcpRuntimeEvent) -> AggregatorAction {
        match self {
            Self::Translation(aggregator) => aggregator.apply(event),
            Self::ReadOnlyTools(aggregator) => aggregator.apply(event),
        }
    }

    fn finish(self) -> Result<String, AiExecutionError> {
        match self {
            Self::Translation(aggregator) => aggregator.finish(),
            Self::ReadOnlyTools(aggregator) => aggregator.finish(),
        }
    }

    fn diagnostics(&self) -> (usize, usize, usize) {
        match self {
            Self::Translation(aggregator) => aggregator.diagnostics(),
            Self::ReadOnlyTools(aggregator) => aggregator.diagnostics(),
        }
    }
}

pub(crate) fn create_workspace(root: &Path) -> Result<PathBuf, AiExecutionError> {
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

pub(crate) fn map_process_error(
    definition: &AgentDefinition,
    error: agent_process::ManagedAgentProcessError,
) -> AiExecutionError {
    match error {
        agent_process::ManagedAgentProcessError::ExecutableNotFound { command_name } => {
            AiExecutionError::RuntimeUnavailable { command_name }
        }
        other => AiExecutionError::Spawn {
            program: PathBuf::from(&definition.command),
            message: other.to_string(),
        },
    }
}

pub(crate) fn map_acp_error(
    operation: &'static str,
    error: acp_protocol::AcpError,
) -> AiExecutionError {
    match error {
        acp_protocol::AcpError::RequestFailed { message, .. }
            if is_model_unavailable_message(&message) =>
        {
            AiExecutionError::ModelUnavailable {
                detail: normalize_provider_error_message(&message),
            }
        }
        acp_protocol::AcpError::RequestFailed { message, .. } => AiExecutionError::ProtocolDetail {
            operation,
            detail: normalize_provider_error_message(&message),
        },
        _ => AiExecutionError::Protocol { operation },
    }
}

pub(crate) fn is_model_unavailable_message(message: &str) -> bool {
    message
        .to_ascii_lowercase()
        .contains("no allowed providers are available for the selected model")
}

pub(crate) fn normalize_provider_error_message(message: &str) -> String {
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

pub(crate) fn cancelled_error(definition: &AgentDefinition) -> AiExecutionError {
    AiExecutionError::Cancelled {
        program: PathBuf::from(&definition.command),
    }
}

pub(crate) fn timeout_error(
    definition: &AgentDefinition,
    timeout: std::time::Duration,
) -> AiExecutionError {
    AiExecutionError::Timeout {
        program: PathBuf::from(&definition.command),
        timeout,
    }
}
