use super::types::{
    SessionEvent, SessionEventDelivery, SessionEventKind, SessionItemIdentity, SessionItemKind,
    SessionItemSnapshot, SessionItemState, SessionProcessingState, SessionTaskStatus,
    SessionToolState,
};

pub(super) fn create_initial_item_snapshot(identity: SessionItemIdentity) -> SessionItemSnapshot {
    SessionItemSnapshot {
        identity,
        kind: SessionItemKind::Notice,
        sequence: 0,
        delivery: SessionEventDelivery::Replay,
        state: SessionItemState::Pending,
        partial: false,
        truncation: None,
        text: None,
        status: None,
        code: None,
        tool_call_id: None,
        tool_name: None,
        tool_input: None,
        tool_output: None,
    }
}

pub(super) fn apply_event_to_snapshot(snapshot: &mut SessionItemSnapshot, event: &SessionEvent) {
    snapshot.sequence = snapshot.sequence.max(event.sequence);
    if matches!(event.delivery, SessionEventDelivery::Live) {
        snapshot.delivery = SessionEventDelivery::Live;
    }
    if let Some(t) = &event.truncation {
        snapshot.truncation = Some(t.clone());
    }

    match &event.kind {
        SessionEventKind::UserMessageAcknowledged { accepted, text } => {
            snapshot.kind = SessionItemKind::UserMessage;
            if text.is_some() {
                snapshot.text = text.clone();
            }
            snapshot.state = if *accepted {
                SessionItemState::Completed
            } else {
                SessionItemState::Failed
            };
        }
        SessionEventKind::AssistantTextDelta { text }
        | SessionEventKind::ThinkingDelta { text } => {
            let is_thinking = matches!(&event.kind, SessionEventKind::ThinkingDelta { .. });
            snapshot.kind = if is_thinking {
                SessionItemKind::Thinking
            } else {
                SessionItemKind::AssistantText
            };
            snapshot.text.get_or_insert_with(String::new).push_str(text);
            snapshot.state = SessionItemState::Streaming;
        }
        SessionEventKind::AssistantTextSnapshot { text }
        | SessionEventKind::ThinkingSnapshot { text } => {
            let is_thinking = matches!(&event.kind, SessionEventKind::ThinkingSnapshot { .. });
            snapshot.kind = if is_thinking {
                SessionItemKind::Thinking
            } else {
                SessionItemKind::AssistantText
            };
            snapshot.text = Some(text.clone());
            snapshot.state = SessionItemState::Streaming;
        }
        SessionEventKind::Processing { state } => {
            snapshot.kind = SessionItemKind::Processing;
            snapshot.state = match state {
                SessionProcessingState::Started => SessionItemState::Pending,
                SessionProcessingState::Active => SessionItemState::Streaming,
                SessionProcessingState::Completed => SessionItemState::Completed,
            };
        }
        SessionEventKind::ToolStart { name, raw_input } => {
            snapshot.kind = SessionItemKind::Tool;
            if snapshot.tool_call_id.is_none() {
                snapshot.tool_call_id = Some(
                    snapshot
                        .identity
                        .item_id
                        .strip_prefix("tool:")
                        .unwrap_or(&snapshot.identity.item_id)
                        .to_string(),
                );
            }
            if name.is_some() {
                snapshot.tool_name = name.clone();
                snapshot.text = name.clone();
            }
            if raw_input.is_some() {
                snapshot.tool_input = raw_input.clone();
            }
            if !matches!(
                snapshot.state,
                SessionItemState::Succeeded
                    | SessionItemState::Failed
                    | SessionItemState::Cancelled
            ) {
                snapshot.state = SessionItemState::Pending;
            }
        }
        SessionEventKind::ToolUpdate {
            state,
            detail,
            raw_output,
        } => {
            snapshot.kind = SessionItemKind::Tool;
            if snapshot.tool_call_id.is_none() {
                snapshot.tool_call_id = Some(
                    snapshot
                        .identity
                        .item_id
                        .strip_prefix("tool:")
                        .unwrap_or(&snapshot.identity.item_id)
                        .to_string(),
                );
            }
            if detail.is_some() {
                snapshot.text = detail.clone();
            }
            if raw_output.is_some() {
                snapshot.tool_output = raw_output.clone();
            }
            if !matches!(
                snapshot.state,
                SessionItemState::Succeeded
                    | SessionItemState::Failed
                    | SessionItemState::Cancelled
            ) {
                snapshot.state = tool_state(*state);
            }
        }
        SessionEventKind::ToolResult {
            success,
            detail,
            raw_output,
        } => {
            snapshot.kind = SessionItemKind::Tool;
            if snapshot.tool_call_id.is_none() {
                snapshot.tool_call_id = Some(
                    snapshot
                        .identity
                        .item_id
                        .strip_prefix("tool:")
                        .unwrap_or(&snapshot.identity.item_id)
                        .to_string(),
                );
            }
            if detail.is_some() {
                snapshot.text = detail.clone();
            }
            if raw_output.is_some() {
                snapshot.tool_output = raw_output.clone();
            }
            let target_state = if *success {
                SessionItemState::Succeeded
            } else {
                SessionItemState::Failed
            };
            if matches!(
                snapshot.state,
                SessionItemState::Succeeded
                    | SessionItemState::Failed
                    | SessionItemState::Cancelled
            ) {
                if snapshot.state != target_state {
                    snapshot.code = Some("conflicting_terminal_event".to_string());
                }
            } else {
                snapshot.state = target_state;
            }
        }
        SessionEventKind::TaskProjection { task_id } => {
            snapshot.kind = SessionItemKind::Task;
            snapshot.code = Some(task_id.clone());
            snapshot.state = SessionItemState::Pending;
        }
        SessionEventKind::TaskStatus { status } => {
            snapshot.kind = SessionItemKind::Task;
            snapshot.status = Some(*status);
            snapshot.state = task_status_state(*status);
        }
        SessionEventKind::TaskResult { success, detail } => {
            snapshot.kind = SessionItemKind::Task;
            if detail.is_some() {
                snapshot.text = detail.clone();
            }
            snapshot.state = if *success {
                SessionItemState::Succeeded
            } else {
                SessionItemState::Failed
            };
        }
        SessionEventKind::Notice { code, detail } => {
            snapshot.kind = SessionItemKind::Notice;
            snapshot.code = Some(code.clone());
            snapshot.text = detail.clone();
            snapshot.state = SessionItemState::Completed;
        }
        SessionEventKind::TerminalResult { text } => {
            if snapshot.kind != SessionItemKind::AssistantText {
                snapshot.kind = SessionItemKind::FinalResult;
            }
            if text.is_some() {
                snapshot.text = text.clone();
            }
            snapshot.state = SessionItemState::Completed;
        }
        SessionEventKind::Cancel => {
            if matches!(
                snapshot.state,
                SessionItemState::Succeeded
                    | SessionItemState::Failed
                    | SessionItemState::Cancelled
            ) {
                if snapshot.state != SessionItemState::Cancelled {
                    snapshot.code = Some("conflicting_terminal_event".to_string());
                }
            } else {
                snapshot.kind = SessionItemKind::Cancelled;
                snapshot.state = SessionItemState::Cancelled;
            }
        }
        SessionEventKind::Error { code, .. } => {
            snapshot.kind = SessionItemKind::Error;
            snapshot.code = Some(code.clone());
            snapshot.state = SessionItemState::Failed;
        }
    }
}

fn tool_state(state: SessionToolState) -> SessionItemState {
    match state {
        SessionToolState::Running => SessionItemState::Streaming,
        SessionToolState::Succeeded => SessionItemState::Succeeded,
        SessionToolState::Failed => SessionItemState::Failed,
        SessionToolState::Cancelled => SessionItemState::Cancelled,
    }
}

fn task_status_state(status: SessionTaskStatus) -> SessionItemState {
    match status {
        SessionTaskStatus::Queued => SessionItemState::Pending,
        SessionTaskStatus::Running => SessionItemState::Streaming,
        SessionTaskStatus::Succeeded => SessionItemState::Succeeded,
        SessionTaskStatus::Failed => SessionItemState::Failed,
        SessionTaskStatus::Cancelled => SessionItemState::Cancelled,
    }
}
