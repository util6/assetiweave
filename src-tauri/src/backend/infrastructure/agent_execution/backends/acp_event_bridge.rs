use agent_client_protocol::schema::v1::SessionId;

use crate::backend::infrastructure::agent_execution::protocol::acp as acp_protocol;
use crate::backend::infrastructure::agent_execution::{
    session_events::{
        SessionEvent, SessionEventDelivery, SessionEventIdentity, SessionEventKind,
        SessionProcessingState, SessionToolState,
    },
    types::AiExecutionRequest,
};

use super::history_replay::{
    HistoryReplayFidelity, HistoryReplayFuture, HistoryReplayPort, HistoryReplayResult,
    HistoryReplayStatus,
};

pub(crate) const ACP_TEXT_ITEM_ID: &str = "assistant-text";
pub(crate) const ACP_THINKING_ITEM_ID: &str = "assistant-thinking";
pub(crate) const ACP_PROCESSING_ITEM_ID: &str = "processing";
pub(crate) const ACP_TERMINAL_ITEM_ID: &str = "terminal";

pub(crate) async fn collect_replay_text(
    events: &mut tokio::sync::mpsc::Receiver<acp_protocol::AcpRuntimeEvent>,
    session_id: &SessionId,
    request: &AiExecutionRequest,
    max_bytes: usize,
) -> HistoryReplayResult {
    let mut bridge = AcpSessionEventBridge::new(request, session_id);
    bridge.emit_processing(SessionProcessingState::Started);
    let deadline = tokio::time::sleep(std::time::Duration::from_millis(250));
    tokio::pin!(deadline);
    let mut text = String::new();
    let mut saw_history_event = false;
    let mut truncated = false;
    loop {
        tokio::select! {
            _ = &mut deadline => break,
            event = events.recv() => {
                let Some(event) = event else { break; };
                saw_history_event = saw_history_event || event_matches_session(&event, session_id);
                let is_completion = matches!(
                    &event,
                    acp_protocol::AcpRuntimeEvent::TurnCompleted {
                        session_id: event_session,
                        ..
                    } if event_session == session_id
                );
                if !is_completion {
                    if let acp_protocol::AcpRuntimeEvent::AgentText {
                        session_id: event_session,
                        text: chunk,
                    } = &event
                    {
                        if event_session == session_id
                            && text.len().saturating_add(chunk.len()) > max_bytes.max(1)
                        {
                            truncated = true;
                            break;
                        }
                    }
                    bridge.emit(&event);
                }
                match event {
                    acp_protocol::AcpRuntimeEvent::AgentText { session_id: event_session, text: chunk } if &event_session == session_id => {
                        text.push_str(&chunk);
                    }
                    acp_protocol::AcpRuntimeEvent::TurnCompleted { session_id: event_session, .. } if &event_session == session_id => break,
                    _ => {}
                }
            }
        }
    }
    let replay = if !saw_history_event {
        HistoryReplayResult::unavailable()
    } else if truncated {
        HistoryReplayResult::new(
            text,
            HistoryReplayFidelity::Partial,
            HistoryReplayStatus::Partial,
            Vec::new(),
        )
    } else {
        HistoryReplayResult::new(
            text,
            HistoryReplayFidelity::Full,
            HistoryReplayStatus::Ready,
            Vec::new(),
        )
    };
    bridge.emit_replay_status(&replay);
    bridge.emit_processing(SessionProcessingState::Completed);
    if replay.is_available() {
        bridge.emit_kind(
            ACP_TERMINAL_ITEM_ID,
            SessionEventKind::TerminalResult { text: None },
        );
    }
    replay
}

pub(crate) struct AcpHistoryReplayPort<'a> {
    pub(crate) events: &'a mut tokio::sync::mpsc::Receiver<acp_protocol::AcpRuntimeEvent>,
    pub(crate) session_id: &'a SessionId,
    pub(crate) request: &'a AiExecutionRequest,
}

impl HistoryReplayPort for AcpHistoryReplayPort<'_> {
    fn replay<'a>(
        &'a mut self,
        _provider_session_id: &'a str,
        max_bytes: usize,
    ) -> HistoryReplayFuture<'a> {
        Box::pin(collect_replay_text(
            self.events,
            self.session_id,
            self.request,
            max_bytes,
        ))
    }
}

fn event_matches_session(event: &acp_protocol::AcpRuntimeEvent, session_id: &SessionId) -> bool {
    let event_session_id = match event {
        acp_protocol::AcpRuntimeEvent::AgentText { session_id, .. }
        | acp_protocol::AcpRuntimeEvent::AgentThought { session_id, .. }
        | acp_protocol::AcpRuntimeEvent::ToolCall { session_id, .. }
        | acp_protocol::AcpRuntimeEvent::ToolCallUpdate { session_id, .. }
        | acp_protocol::AcpRuntimeEvent::PermissionRequested { session_id }
        | acp_protocol::AcpRuntimeEvent::Other { session_id }
        | acp_protocol::AcpRuntimeEvent::TurnCompleted { session_id, .. } => session_id,
    };
    event_session_id == session_id
}

pub(crate) struct AcpSessionEventBridge<'a> {
    request: &'a AiExecutionRequest,
    provider_session_id: SessionId,
    session_id: String,
    member_id: String,
    delivery: SessionEventDelivery,
    sequence: u64,
}

impl<'a> AcpSessionEventBridge<'a> {
    pub(crate) fn new(request: &'a AiExecutionRequest, provider_session_id: &SessionId) -> Self {
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

    pub(crate) fn emit(&mut self, event: &acp_protocol::AcpRuntimeEvent) {
        let event_session_id = match event {
            acp_protocol::AcpRuntimeEvent::AgentText { session_id, .. }
            | acp_protocol::AcpRuntimeEvent::AgentThought { session_id, .. }
            | acp_protocol::AcpRuntimeEvent::ToolCall { session_id, .. }
            | acp_protocol::AcpRuntimeEvent::ToolCallUpdate { session_id, .. }
            | acp_protocol::AcpRuntimeEvent::PermissionRequested { session_id }
            | acp_protocol::AcpRuntimeEvent::Other { session_id }
            | acp_protocol::AcpRuntimeEvent::TurnCompleted { session_id, .. } => session_id,
        };
        if event_session_id != &self.provider_session_id {
            return;
        }

        match event {
            acp_protocol::AcpRuntimeEvent::AgentText { text, .. } => {
                self.emit_kind(
                    ACP_TEXT_ITEM_ID,
                    SessionEventKind::AssistantTextDelta { text: text.clone() },
                );
            }
            acp_protocol::AcpRuntimeEvent::AgentThought { text, .. } => {
                if let Some(text) = text.as_ref().filter(|text| !text.is_empty()) {
                    self.emit_kind(
                        ACP_THINKING_ITEM_ID,
                        SessionEventKind::ThinkingDelta { text: text.clone() },
                    );
                } else {
                    self.emit_processing(SessionProcessingState::Active);
                }
            }
            acp_protocol::AcpRuntimeEvent::ToolCall {
                tool_call_id,
                title,
                status,
                raw_input,
                raw_output,
                ..
            } => {
                let item_id = tool_item_id(tool_call_id);
                self.emit_kind(
                    &item_id,
                    SessionEventKind::ToolStart {
                        name: (!title.trim().is_empty()).then(|| title.clone()),
                        raw_input: raw_input.clone(),
                    },
                );
                self.emit_tool_status(&item_id, *status, raw_output.clone());
            }
            acp_protocol::AcpRuntimeEvent::ToolCallUpdate {
                tool_call_id,
                title,
                status,
                raw_input,
                raw_output,
                ..
            } => {
                let item_id = tool_item_id(tool_call_id);
                if title.is_some() || raw_input.is_some() {
                    self.emit_kind(
                        &item_id,
                        SessionEventKind::ToolStart {
                            name: title.as_ref().filter(|t| !t.trim().is_empty()).cloned(),
                            raw_input: raw_input.clone(),
                        },
                    );
                }
                self.emit_tool_status(
                    &item_id,
                    status.unwrap_or(acp_protocol::AcpToolStatus::InProgress),
                    raw_output.clone(),
                );
            }
            acp_protocol::AcpRuntimeEvent::PermissionRequested { .. } => {
                self.emit_kind(
                    "permission",
                    SessionEventKind::Notice {
                        code: "permission_requested".to_string(),
                        detail: None,
                    },
                );
            }
            acp_protocol::AcpRuntimeEvent::TurnCompleted { stop_reason, .. } => {
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
            acp_protocol::AcpRuntimeEvent::Other { .. } => {}
        }
    }

    pub(crate) fn emit_processing(&mut self, state: SessionProcessingState) {
        self.emit_kind(
            ACP_PROCESSING_ITEM_ID,
            SessionEventKind::Processing { state },
        );
    }

    pub(crate) fn emit_replay_status(&mut self, replay: &HistoryReplayResult) {
        self.emit_kind(
            "history-status",
            SessionEventKind::Notice {
                code: "history_replay_status".to_string(),
                detail: Some(replay.status_detail()),
            },
        );
    }

    fn emit_tool_status(
        &mut self,
        item_id: &str,
        status: acp_protocol::AcpToolStatus,
        raw_output: Option<serde_json::Value>,
    ) {
        match status {
            acp_protocol::AcpToolStatus::Pending => {
                if raw_output.is_some() {
                    self.emit_kind(
                        item_id,
                        SessionEventKind::ToolUpdate {
                            state: SessionToolState::Running,
                            detail: None,
                            raw_output,
                        },
                    );
                }
            }
            acp_protocol::AcpToolStatus::InProgress => self.emit_kind(
                item_id,
                SessionEventKind::ToolUpdate {
                    state: SessionToolState::Running,
                    detail: None,
                    raw_output,
                },
            ),
            acp_protocol::AcpToolStatus::Completed => self.emit_kind(
                item_id,
                SessionEventKind::ToolResult {
                    success: true,
                    detail: None,
                    raw_output,
                },
            ),
            acp_protocol::AcpToolStatus::Failed => self.emit_kind(
                item_id,
                SessionEventKind::ToolResult {
                    success: false,
                    detail: None,
                    raw_output,
                },
            ),
        }
    }

    pub(crate) fn emit_kind(&mut self, item_id: &str, kind: SessionEventKind) {
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
            truncation: None,
        });
    }
}

pub(crate) fn tool_item_id(tool_call_id: &str) -> String {
    format!("tool:{tool_call_id}")
}
