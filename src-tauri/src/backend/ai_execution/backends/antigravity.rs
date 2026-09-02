use std::{path::PathBuf, time::Instant};

use serde::Deserialize;
use tokio::io::AsyncReadExt;

use crate::backend::{
    agents::{
        process::ManagedAgentProcess,
        types::{AgentDefinition, AgentProtocol},
    },
    ai_execution::{
        AiExecutionError, AiExecutionPhase, AiExecutionRequest, AiExecutionResult, SessionEvent,
        SessionEventDelivery, SessionEventIdentity, SessionEventKind, SessionProcessingState,
        SessionToolState,
    },
};

use super::native::{cancelled_error, map_process_error, NativeExecutionGuard};
use super::{
    antigravity_history::AntigravityProviderHistoryReader,
    history_replay::{HistoryReplayEntry, HistoryReplayPort, HistoryReplayResult},
};

const PROVIDER_NAME: &str = "Antigravity";
const PRINT_TIMEOUT: &str = "10m";
const PROCESSING_ITEM_ID: &str = "agy:processing";
const TERMINAL_ITEM_ID: &str = "agy:terminal";
const CANCEL_ITEM_ID: &str = "agy:cancel";
const ERROR_ITEM_ID: &str = "agy:error";
const UNKNOWN_EVENT_CODE: &str = "provider_unknown_event";
const MALFORMED_EVENT_CODE: &str = "provider_malformed_event";

/// Direct-CLI Session Adapter for Antigravity.
///
/// The adapter deliberately owns the `agy` wire format and anchor rules. The
/// surrounding Native backend still owns the common process cleanup, while
/// callers continue to select this through the Agent Execution boundary.
pub(super) async fn run(
    guard: &mut NativeExecutionGuard,
    definition: &AgentDefinition,
    request: &AiExecutionRequest,
    started: Instant,
) -> Result<AiExecutionResult, AiExecutionError> {
    request.report_phase(AiExecutionPhase::Spawning);
    let existing_anchor = request
        .binding
        .as_ref()
        .map(|binding| binding.provider_session_id.trim().to_owned())
        .filter(|anchor| is_valid_resume_anchor(anchor));
    if request.binding.is_some() && existing_anchor.is_none() {
        return Err(AiExecutionError::ResumeUnavailable);
    }

    if request.replay {
        let history = if let Some(anchor) = existing_anchor.as_deref() {
            let mut reader = AntigravityProviderHistoryReader::from_request(
                definition,
                request.binding.as_ref(),
            );
            reader.replay(anchor, request.limits.text_bytes).await
        } else {
            HistoryReplayResult::unavailable()
        };
        let mut bridge = SessionEventBridge::new(request, existing_anchor.clone());
        bridge.emit_replay(&history);
        return Ok(replay_result(definition, request, started, history.text));
    }

    if request.restore_only {
        let anchor = existing_anchor
            .as_deref()
            .filter(|anchor| is_valid_resume_anchor(anchor))
            .ok_or(AiExecutionError::ResumeUnavailable)?;
        return Ok(result(
            definition,
            request,
            started,
            String::new(),
            Some(binding(request, definition, guard, anchor)),
        ));
    }

    let mut run_definition = definition.clone();
    run_definition.args = build_argv(
        definition,
        request,
        existing_anchor
            .as_deref()
            .filter(|anchor| is_valid_resume_anchor(anchor)),
        &guard.workspace,
    );
    let process = ManagedAgentProcess::spawn(
        &run_definition,
        Some(&guard.workspace),
        request.limits.stderr_bytes,
    )
    .await
    .map_err(|error| map_process_error(definition, error))?;
    guard.process = Some(process);

    let (stdin, mut stdout) = guard
        .process
        .as_ref()
        .expect("Antigravity process exists")
        .take_stdio()
        .await
        .map_err(|_| AiExecutionError::Protocol {
            operation: "take_stdio",
        })?;
    drop(stdin);

    request.report_phase(AiExecutionPhase::Prompting);
    let mut bridge = SessionEventBridge::new(request, existing_anchor.clone());
    bridge.emit_processing(SessionProcessingState::Started);
    let mut buffer = [0_u8; 16 * 1024];
    let mut pending_line = Vec::new();
    let mut output_bytes = 0_usize;
    let mut text = String::new();
    let mut result_response = String::new();
    let mut result_error = None;
    let mut interrupted = false;
    let mut terminal_seen = false;
    let mut anchor = existing_anchor;

    'read: loop {
        let read = tokio::select! {
            read = stdout.read(&mut buffer) => read.map_err(|error| AiExecutionError::Output {
                message: format!("failed to read {PROVIDER_NAME} output: {error}"),
            })?,
            _ = request.cancellation.cancelled() => {
                bridge.emit_cancel();
                return Err(cancelled_error(definition));
            }
        };
        if read == 0 {
            break;
        }
        output_bytes = output_bytes.saturating_add(read);
        if output_bytes > request.limits.text_bytes {
            bridge.emit_error("output_limit");
            return Err(AiExecutionError::OutputLimit {
                limit: request.limits.text_bytes,
            });
        }
        pending_line.extend_from_slice(&buffer[..read]);

        while let Some(newline) = pending_line.iter().position(|byte| *byte == b'\n') {
            let line = pending_line.drain(..=newline).collect::<Vec<_>>();
            let line = &line[..line.len().saturating_sub(1)];
            let event = match parse_line(line) {
                Ok(event) => event,
                Err(error) => {
                    bridge.emit_error(MALFORMED_EVENT_CODE);
                    return Err(AiExecutionError::Output { message: error });
                }
            };
            let Some(event) = event else { continue };
            if let Some(candidate) = event_conversation_id(&event).and_then(clean_anchor) {
                anchor = Some(candidate.clone());
                bridge.observe_anchor(&candidate);
            }
            if matches!(&event, AgyEvent::Unknown) {
                bridge.emit_unknown();
            }
            bridge.emit(&event, &mut text);
            match event {
                AgyEvent::Result(result) => {
                    terminal_seen = true;
                    result_response = result.response;
                    if result.status.eq_ignore_ascii_case("INTERRUPTED") {
                        interrupted = true;
                    } else if !result.status.eq_ignore_ascii_case("SUCCESS") {
                        result_error = Some(result.error.unwrap_or_else(|| {
                            "Antigravity returned an unsuccessful result".to_string()
                        }));
                    }
                    break 'read;
                }
                AgyEvent::Init(_) | AgyEvent::StepUpdate(_) | AgyEvent::Unknown => {}
            }
        }
    }

    if !pending_line.is_empty() {
        let event = match parse_line(&pending_line) {
            Ok(event) => event,
            Err(error) => {
                bridge.emit_error(MALFORMED_EVENT_CODE);
                return Err(AiExecutionError::Output { message: error });
            }
        };
        if let Some(event) = event {
            if let Some(candidate) = event_conversation_id(&event).and_then(clean_anchor) {
                anchor = Some(candidate.clone());
                bridge.observe_anchor(&candidate);
            }
            if matches!(&event, AgyEvent::Unknown) {
                bridge.emit_unknown();
            }
            bridge.emit(&event, &mut text);
            if let AgyEvent::Result(result) = event {
                terminal_seen = true;
                result_response = result.response;
                if result.status.eq_ignore_ascii_case("INTERRUPTED") {
                    interrupted = true;
                } else if !result.status.eq_ignore_ascii_case("SUCCESS") {
                    result_error = Some(result.error.unwrap_or_else(|| {
                        "Antigravity returned an unsuccessful result".to_string()
                    }));
                }
            }
        }
    }

    let exit = guard
        .process
        .as_ref()
        .expect("Antigravity process exists")
        .wait_for_exit()
        .await
        .ok_or(AiExecutionError::AgentExited { code: None })?;
    if interrupted {
        return Err(cancelled_error(definition));
    }
    if let Some(error) = result_error {
        return Err(AiExecutionError::Output {
            message: normalize_provider_error(&error),
        });
    }
    if !exit.success {
        bridge.emit_error("provider_process_failed");
        return Err(AiExecutionError::AgentExited { code: exit.code });
    }
    if !terminal_seen {
        bridge.emit_error("provider_missing_terminal");
        return Err(AiExecutionError::Output {
            message: "Antigravity did not emit a terminal result".to_string(),
        });
    }

    let text = if text.is_empty() {
        result_response
    } else {
        text
    };
    if text.trim().is_empty() {
        bridge.emit_error("provider_empty_output");
        return Err(AiExecutionError::EmptyOutput {
            program: Some(PathBuf::from(&definition.command)),
        });
    }
    let persistent_binding = if matches!(
        request.session_mode,
        crate::backend::ai_execution::AgentSessionMode::Persistent
    ) {
        anchor
            .as_deref()
            .filter(|anchor| is_valid_resume_anchor(anchor))
            .map(|anchor| binding(request, definition, guard, anchor))
    } else {
        None
    };
    Ok(result(
        definition,
        request,
        started,
        text,
        persistent_binding,
    ))
}

fn build_argv(
    definition: &AgentDefinition,
    request: &AiExecutionRequest,
    resume_anchor: Option<&str>,
    workspace: &std::path::Path,
) -> Vec<String> {
    let mut args = definition.args.clone();
    args.extend([
        "-p".to_string(),
        request.prompt.trim().to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--print-timeout".to_string(),
        PRINT_TIMEOUT.to_string(),
    ]);
    match resume_anchor {
        Some(anchor) => args.extend(["--conversation".to_string(), anchor.to_string()]),
        None => args.push("--new-project".to_string()),
    }
    args.extend([
        "--add-dir".to_string(),
        workspace.to_string_lossy().into_owned(),
    ]);
    if let Some(model) = request
        .model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
    {
        args.extend(["--model".to_string(), model.to_string()]);
    }
    args
}

fn is_valid_resume_anchor(anchor: &str) -> bool {
    let anchor = anchor.trim();
    !anchor.is_empty()
        && anchor.len() <= 256
        && !anchor.chars().any(char::is_control)
        && !anchor.starts_with("native-session-")
}

fn clean_anchor(anchor: &str) -> Option<String> {
    let anchor = anchor.trim();
    is_valid_resume_anchor(anchor).then(|| anchor.to_string())
}

fn binding(
    request: &AiExecutionRequest,
    definition: &AgentDefinition,
    guard: &NativeExecutionGuard,
    provider_session_id: &str,
) -> crate::backend::ai_execution::PersistentExecutionBinding {
    crate::backend::ai_execution::PersistentExecutionBinding {
        tenant_id: request.tenant_id.clone().unwrap_or_default(),
        execution_context_key: request.execution_context_key.clone().unwrap_or_default(),
        provider_session_id: provider_session_id.to_string(),
        agent_id: definition.id.to_string(),
        installation_id: definition.installation_id.clone(),
        model: request.model.clone(),
        workspace_path: guard.workspace.to_string_lossy().into_owned(),
        binding_version: 1,
        provider_metadata_json: "{\"protocol\":\"native\",\"adapter\":\"antigravity-direct-cli\"}"
            .to_string(),
    }
}

fn result(
    definition: &AgentDefinition,
    request: &AiExecutionRequest,
    started: Instant,
    text: String,
    persistent_binding: Option<crate::backend::ai_execution::PersistentExecutionBinding>,
) -> AiExecutionResult {
    AiExecutionResult {
        text,
        agent_id: definition.id.clone(),
        protocol: AgentProtocol::Native,
        requested_model: request.model.clone(),
        elapsed_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        persistent_binding,
        replay_text: None,
    }
}

fn replay_result(
    definition: &AgentDefinition,
    request: &AiExecutionRequest,
    started: Instant,
    text: String,
) -> AiExecutionResult {
    AiExecutionResult {
        text: text.clone(),
        agent_id: definition.id.clone(),
        protocol: AgentProtocol::Native,
        requested_model: request.model.clone(),
        elapsed_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        persistent_binding: None,
        replay_text: Some(text),
    }
}

fn normalize_provider_error(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("authentication")
        || lower.contains("not logged in")
        || lower.contains("sign in")
    {
        "Antigravity authentication failed; run agy to sign in before retrying".to_string()
    } else {
        "Antigravity returned an unsuccessful result".to_string()
    }
}

#[derive(Debug, Clone)]
enum AgyEvent {
    Init(AgyInit),
    StepUpdate(AgyStepUpdate),
    Result(AgyResult),
    Unknown,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct AgyInit {
    #[serde(default)]
    conversation_id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct AgyStepUpdate {
    #[serde(default)]
    conversation_id: String,
    #[serde(default)]
    step_index: u64,
    #[serde(default)]
    state: String,
    #[serde(default)]
    step_type: String,
    #[serde(default)]
    text_delta: Option<String>,
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    tool_info: Option<AgyToolInfo>,
    #[serde(default)]
    usage: Option<AgyUsage>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct AgyToolInfo {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct AgyUsage {}

#[derive(Debug, Clone, Default, Deserialize)]
struct AgyResult {
    #[serde(default)]
    conversation_id: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    response: String,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    usage: Option<AgyUsage>,
}

#[derive(Deserialize)]
struct AgyEnvelope {
    #[serde(default)]
    event: String,
    #[serde(default)]
    conversation_id: Option<String>,
    #[serde(default)]
    init: Option<serde_json::Value>,
    #[serde(default)]
    step_update: Option<serde_json::Value>,
    #[serde(default)]
    result: Option<serde_json::Value>,
}

fn parse_line(line: &[u8]) -> Result<Option<AgyEvent>, String> {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let trimmed = line
        .iter()
        .copied()
        .skip_while(u8::is_ascii_whitespace)
        .collect::<Vec<_>>();
    if trimmed.is_empty() || !trimmed.starts_with(b"{") {
        return Ok(None);
    }
    let envelope = serde_json::from_slice::<AgyEnvelope>(&trimmed)
        .map_err(|_| "Antigravity emitted malformed JSON".to_string())?;
    match envelope.event.as_str() {
        "init" => {
            let mut init = serde_json::from_value::<AgyInit>(
                envelope
                    .init
                    .ok_or_else(|| "Antigravity init event has no payload".to_string())?,
            )
            .map_err(|_| "Antigravity init event has malformed payload".to_string())?;
            if init.conversation_id.is_empty() {
                init.conversation_id = envelope.conversation_id.unwrap_or_default();
            }
            Ok(Some(AgyEvent::Init(init)))
        }
        "step_update" => Ok(Some(AgyEvent::StepUpdate(
            serde_json::from_value(
                envelope
                    .step_update
                    .ok_or_else(|| "Antigravity step event has no payload".to_string())?,
            )
            .map_err(|_| "Antigravity step event has malformed payload".to_string())?,
        ))),
        "result" => Ok(Some(AgyEvent::Result(
            serde_json::from_value(
                envelope
                    .result
                    .ok_or_else(|| "Antigravity result event has no payload".to_string())?,
            )
            .map_err(|_| "Antigravity result event has malformed payload".to_string())?,
        ))),
        _ => Ok(Some(AgyEvent::Unknown)),
    }
}

fn event_conversation_id(event: &AgyEvent) -> Option<&str> {
    match event {
        AgyEvent::Init(init) => Some(&init.conversation_id),
        AgyEvent::StepUpdate(step) => Some(&step.conversation_id),
        AgyEvent::Result(result) => Some(&result.conversation_id),
        AgyEvent::Unknown => None,
    }
}

struct SessionEventBridge<'a> {
    request: &'a AiExecutionRequest,
    provider_session_id: Option<String>,
    session_id: String,
    member_id: String,
    delivery: SessionEventDelivery,
    sequence: u64,
}

impl<'a> SessionEventBridge<'a> {
    fn new(request: &'a AiExecutionRequest, provider_session_id: Option<String>) -> Self {
        let stable_context = request
            .execution_context_key
            .clone()
            .unwrap_or_else(|| format!("execution:{}", request.execution_id));
        Self {
            request,
            provider_session_id,
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

    fn observe_anchor(&mut self, provider_session_id: &str) {
        if let Some(provider_session_id) = clean_anchor(provider_session_id) {
            self.provider_session_id = Some(provider_session_id);
        }
    }

    fn emit(&mut self, event: &AgyEvent, accumulated_text: &mut String) {
        match event {
            AgyEvent::Init(_) => {}
            AgyEvent::StepUpdate(step) => self.emit_step(step, accumulated_text),
            AgyEvent::Result(result) => self.emit_result(result, accumulated_text),
            AgyEvent::Unknown => {}
        }
    }

    fn emit_replay(&mut self, history: &HistoryReplayResult) {
        self.emit_processing(SessionProcessingState::Started);
        for (entry_index, entry) in history.entries.iter().enumerate() {
            match entry {
                HistoryReplayEntry::UserMessage => {
                    self.emit_kind(
                        &format!("agy:history:{entry_index}:user"),
                        SessionEventKind::UserMessageAcknowledged { accepted: true },
                    );
                }
                HistoryReplayEntry::AssistantText { text } => {
                    self.emit_kind(
                        &format!("agy:history:{entry_index}:assistant"),
                        SessionEventKind::AssistantTextDelta { text: text.clone() },
                    );
                }
                HistoryReplayEntry::ToolStart { item_id, name } => {
                    self.emit_kind(
                        &format!("agy:history:tool:{item_id}"),
                        SessionEventKind::ToolStart {
                            name: Some(name.clone()),
                        },
                    );
                }
                HistoryReplayEntry::ToolResult { item_id, success } => {
                    self.emit_kind(
                        &format!("agy:history:tool:{item_id}"),
                        SessionEventKind::ToolResult {
                            success: *success,
                            detail: None,
                        },
                    );
                }
                HistoryReplayEntry::Notice { code } => {
                    self.emit_kind(
                        &format!("agy:history:{entry_index}:notice"),
                        SessionEventKind::Notice {
                            code: code.clone(),
                            detail: None,
                        },
                    );
                }
            }
        }
        self.emit_kind(
            "agy:history:status",
            SessionEventKind::Notice {
                code: "history_replay_status".to_string(),
                detail: Some(history.status_detail()),
            },
        );
        self.emit_processing(SessionProcessingState::Completed);
        if history.is_available() {
            self.emit_kind(
                "agy:history:terminal",
                SessionEventKind::TerminalResult { text: None },
            );
        }
    }

    fn emit_step(&mut self, step: &AgyStepUpdate, accumulated_text: &mut String) {
        let step_type = step.step_type.to_ascii_lowercase();
        if step_type == "agent_response" {
            if let Some(text) = step.text_delta.as_ref().filter(|text| !text.is_empty()) {
                let item_id = self.text_item_id();
                self.emit_kind(
                    &item_id,
                    SessionEventKind::AssistantTextDelta { text: text.clone() },
                );
                accumulated_text.push_str(text);
            }
        } else if step_type.contains("tool") || step_type == "subagent" {
            let item_id = self.step_item_id(step.step_index);
            let name = step
                .tool_info
                .as_ref()
                .map(|info| info.name.trim())
                .filter(|name| !name.is_empty())
                .or_else(|| {
                    step.tool_name
                        .as_deref()
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                })
                .unwrap_or("tool")
                .to_string();
            match step.state.to_ascii_uppercase().as_str() {
                "ACTIVE" => {
                    self.emit_kind(&item_id, SessionEventKind::ToolStart { name: Some(name) });
                    self.emit_kind(
                        &item_id,
                        SessionEventKind::ToolUpdate {
                            state: SessionToolState::Running,
                            detail: None,
                        },
                    );
                }
                "DONE" => self.emit_kind(
                    &item_id,
                    SessionEventKind::ToolResult {
                        success: true,
                        detail: None,
                    },
                ),
                "ERROR" => self.emit_kind(
                    &item_id,
                    SessionEventKind::ToolResult {
                        success: false,
                        detail: None,
                    },
                ),
                _ => self.emit_kind(
                    "agy:unknown-step",
                    SessionEventKind::Notice {
                        code: "provider_unknown_step_state".to_string(),
                        detail: None,
                    },
                ),
            }
        } else if step_type == "error_message" && step.state.eq_ignore_ascii_case("DONE") {
            let item_id = self.step_item_id(step.step_index);
            self.emit_kind(
                &item_id,
                SessionEventKind::Notice {
                    code: "provider_step_failed".to_string(),
                    detail: None,
                },
            );
        } else if !step_type.is_empty()
            && !matches!(
                step_type.as_str(),
                "user_input" | "checkpoint" | "system_message"
            )
        {
            self.emit_kind(
                "agy:unknown-step",
                SessionEventKind::Notice {
                    code: "provider_unknown_step".to_string(),
                    detail: None,
                },
            );
        }
        if step.usage.is_some() {
            self.emit_processing(SessionProcessingState::Active);
        }
    }

    fn emit_result(&mut self, result: &AgyResult, accumulated_text: &mut String) {
        if result.usage.is_some() {
            self.emit_processing(SessionProcessingState::Active);
        }
        if result.status.eq_ignore_ascii_case("SUCCESS")
            && accumulated_text.is_empty()
            && !result.response.is_empty()
        {
            let item_id = self.text_item_id();
            self.emit_kind(
                &item_id,
                SessionEventKind::AssistantTextDelta {
                    text: result.response.clone(),
                },
            );
            accumulated_text.push_str(&result.response);
        }
        self.emit_processing(SessionProcessingState::Completed);
        if result.status.eq_ignore_ascii_case("INTERRUPTED") {
            self.emit_kind(CANCEL_ITEM_ID, SessionEventKind::Cancel);
        } else if result.status.eq_ignore_ascii_case("SUCCESS") {
            self.emit_kind(
                TERMINAL_ITEM_ID,
                SessionEventKind::TerminalResult { text: None },
            );
        } else {
            self.emit_error("provider_result_failed");
        }
    }

    fn emit_processing(&mut self, state: SessionProcessingState) {
        self.emit_kind(PROCESSING_ITEM_ID, SessionEventKind::Processing { state });
    }

    fn emit_cancel(&mut self) {
        self.emit_processing(SessionProcessingState::Completed);
        self.emit_kind(CANCEL_ITEM_ID, SessionEventKind::Cancel);
    }

    fn emit_unknown(&mut self) {
        self.emit_kind(
            "agy:unknown-event",
            SessionEventKind::Notice {
                code: UNKNOWN_EVENT_CODE.to_string(),
                detail: None,
            },
        );
    }

    fn emit_error(&mut self, code: &str) {
        self.emit_kind(
            ERROR_ITEM_ID,
            SessionEventKind::Error {
                code: code.to_string(),
                retryable: false,
            },
        );
    }

    fn text_item_id(&self) -> String {
        format!("agy:{}:assistant", self.namespace())
    }

    fn step_item_id(&self, step_index: u64) -> String {
        format!("agy:{}:step:{step_index}", self.namespace())
    }

    fn namespace(&self) -> &str {
        self.provider_session_id
            .as_deref()
            .unwrap_or(&self.request.execution_id)
    }

    fn emit_kind(&mut self, item_id: &str, kind: SessionEventKind) {
        self.sequence = self.sequence.saturating_add(1);
        self.request.report_session_event(SessionEvent {
            identity: SessionEventIdentity {
                session_id: self.session_id.clone(),
                member_id: self.member_id.clone(),
                execution_id: self.request.execution_id.clone(),
                turn_id: self.request.execution_id.clone(),
                item_id: item_id.to_string(),
                event_id: format!(
                    "antigravity:{}:{}",
                    self.request.execution_id, self.sequence
                ),
            },
            sequence: self.sequence,
            delivery: self.delivery,
            kind,
        });
    }
}

#[cfg(test)]
#[path = "antigravity_tests.rs"]
mod tests;
