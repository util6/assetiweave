use agent_client_protocol::schema::v1::{SessionId, StopReason};

use crate::backend::{agents::protocol::acp::AcpRuntimeEvent, ai_execution::AiExecutionError};

#[derive(Debug)]
pub(crate) enum AggregatorAction {
    Continue,
    Complete { stop_reason: StopReason },
    CancelAndFail(AiExecutionError),
}

pub(crate) struct TranslationTextAggregator {
    session_id: SessionId,
    text: String,
    byte_limit: usize,
    chunks: usize,
    thinking_chunks: usize,
    ignored_session_events: usize,
}

impl TranslationTextAggregator {
    pub(crate) fn new(session_id: SessionId, byte_limit: usize) -> Self {
        Self {
            session_id,
            text: String::new(),
            byte_limit,
            chunks: 0,
            thinking_chunks: 0,
            ignored_session_events: 0,
        }
    }

    pub(crate) fn apply(&mut self, event: AcpRuntimeEvent) -> AggregatorAction {
        let event_session_id = match &event {
            AcpRuntimeEvent::AgentText { session_id, .. }
            | AcpRuntimeEvent::AgentThought { session_id, .. }
            | AcpRuntimeEvent::ToolCall { session_id, .. }
            | AcpRuntimeEvent::ToolCallUpdate { session_id, .. }
            | AcpRuntimeEvent::PermissionRequested { session_id }
            | AcpRuntimeEvent::Other { session_id }
            | AcpRuntimeEvent::TurnCompleted { session_id, .. } => session_id,
        };
        if event_session_id != &self.session_id {
            self.ignored_session_events += 1;
            return AggregatorAction::Continue;
        }

        match event {
            AcpRuntimeEvent::AgentText { text, .. } => {
                let Some(new_len) = self.text.len().checked_add(text.len()) else {
                    return AggregatorAction::CancelAndFail(AiExecutionError::OutputLimit {
                        limit: self.byte_limit,
                    });
                };
                if new_len > self.byte_limit {
                    return AggregatorAction::CancelAndFail(AiExecutionError::OutputLimit {
                        limit: self.byte_limit,
                    });
                }
                self.text.push_str(&text);
                self.chunks += 1;
                AggregatorAction::Continue
            }
            AcpRuntimeEvent::AgentThought { .. } => {
                self.thinking_chunks += 1;
                AggregatorAction::Continue
            }
            AcpRuntimeEvent::ToolCall { .. } | AcpRuntimeEvent::ToolCallUpdate { .. } => {
                AggregatorAction::CancelAndFail(AiExecutionError::ToolUseDenied)
            }
            AcpRuntimeEvent::PermissionRequested { .. } => {
                AggregatorAction::CancelAndFail(AiExecutionError::PermissionDenied)
            }
            AcpRuntimeEvent::Other { .. } => AggregatorAction::Continue,
            AcpRuntimeEvent::TurnCompleted { stop_reason, .. } => {
                AggregatorAction::Complete { stop_reason }
            }
        }
    }

    pub(crate) fn finish(self) -> Result<String, AiExecutionError> {
        let text = self.text.trim().to_owned();
        if text.is_empty() {
            return Err(AiExecutionError::EmptyOutput { program: None });
        }
        Ok(text)
    }

    pub(crate) fn diagnostics(&self) -> (usize, usize, usize) {
        (
            self.chunks,
            self.thinking_chunks,
            self.ignored_session_events,
        )
    }
}

/// Aggregates Recall output while allowing the agent to use the read-only
/// Recall MCP server. Other executions keep their fail-closed tool policy in
/// `TranslationTextAggregator`.
pub(crate) struct RecallStructuredAggregator {
    session_id: SessionId,
    text: String,
    byte_limit: usize,
    chunks: usize,
    thinking_chunks: usize,
    ignored_session_events: usize,
}

impl RecallStructuredAggregator {
    pub(crate) fn new(session_id: SessionId, byte_limit: usize) -> Self {
        Self {
            session_id,
            text: String::new(),
            byte_limit,
            chunks: 0,
            thinking_chunks: 0,
            ignored_session_events: 0,
        }
    }

    pub(crate) fn apply(&mut self, event: AcpRuntimeEvent) -> AggregatorAction {
        let event_session_id = match &event {
            AcpRuntimeEvent::AgentText { session_id, .. }
            | AcpRuntimeEvent::AgentThought { session_id, .. }
            | AcpRuntimeEvent::ToolCall { session_id, .. }
            | AcpRuntimeEvent::ToolCallUpdate { session_id, .. }
            | AcpRuntimeEvent::PermissionRequested { session_id }
            | AcpRuntimeEvent::Other { session_id }
            | AcpRuntimeEvent::TurnCompleted { session_id, .. } => session_id,
        };
        if event_session_id != &self.session_id {
            self.ignored_session_events += 1;
            return AggregatorAction::Continue;
        }

        match event {
            AcpRuntimeEvent::AgentText { text, .. } => {
                let Some(new_len) = self.text.len().checked_add(text.len()) else {
                    return AggregatorAction::CancelAndFail(AiExecutionError::OutputLimit {
                        limit: self.byte_limit,
                    });
                };
                if new_len > self.byte_limit {
                    return AggregatorAction::CancelAndFail(AiExecutionError::OutputLimit {
                        limit: self.byte_limit,
                    });
                }
                self.text.push_str(&text);
                self.chunks += 1;
                AggregatorAction::Continue
            }
            AcpRuntimeEvent::AgentThought { .. } => {
                self.thinking_chunks += 1;
                AggregatorAction::Continue
            }
            AcpRuntimeEvent::ToolCall { .. } | AcpRuntimeEvent::ToolCallUpdate { .. } => {
                AggregatorAction::Continue
            }
            AcpRuntimeEvent::PermissionRequested { .. } => {
                AggregatorAction::CancelAndFail(AiExecutionError::PermissionDenied)
            }
            AcpRuntimeEvent::Other { .. } => AggregatorAction::Continue,
            AcpRuntimeEvent::TurnCompleted { stop_reason, .. } => {
                AggregatorAction::Complete { stop_reason }
            }
        }
    }

    pub(crate) fn finish(self) -> Result<String, AiExecutionError> {
        let text = self.text.trim().to_owned();
        if text.is_empty() {
            return Err(AiExecutionError::EmptyOutput { program: None });
        }
        Ok(text)
    }

    pub(crate) fn diagnostics(&self) -> (usize, usize, usize) {
        (
            self.chunks,
            self.thinking_chunks,
            self.ignored_session_events,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    fn session() -> SessionId {
        SessionId::new("session")
    }

    fn text(session_id: &SessionId, value: &str) -> AcpRuntimeEvent {
        AcpRuntimeEvent::AgentText {
            session_id: session_id.clone(),
            text: value.to_owned(),
        }
    }

    fn complete(session_id: &SessionId) -> AcpRuntimeEvent {
        AcpRuntimeEvent::TurnCompleted {
            session_id: session_id.clone(),
            stop_reason: StopReason::EndTurn,
        }
    }

    #[test]
    fn evt_01_one_text_chunk_returns_exact_text() {
        let session = session();
        let mut aggregator = TranslationTextAggregator::new(session.clone(), 64);

        assert!(matches!(
            aggregator.apply(text(&session, "answer")),
            AggregatorAction::Continue
        ));

        assert_eq!(aggregator.finish().unwrap(), "answer");
    }

    #[test]
    fn evt_02_multiple_chunks_preserve_order() {
        let session = session();
        let mut aggregator = TranslationTextAggregator::new(session.clone(), 64);
        aggregator.apply(text(&session, "first "));
        aggregator.apply(text(&session, "second"));

        assert_eq!(aggregator.finish().unwrap(), "first second");
    }

    #[test]
    fn evt_03_unicode_chunks_remain_valid_and_ordered() {
        let session = session();
        let mut aggregator = TranslationTextAggregator::new(session.clone(), 64);
        aggregator.apply(text(&session, "你"));
        aggregator.apply(text(&session, "好🌍"));

        assert_eq!(aggregator.finish().unwrap(), "你好🌍");
    }

    #[test]
    fn evt_04_thinking_is_counted_but_not_returned() {
        let session = session();
        let mut aggregator = TranslationTextAggregator::new(session.clone(), 64);
        aggregator.apply(AcpRuntimeEvent::AgentThought {
            session_id: session.clone(),
            text: Some("provider thought".to_string()),
        });
        aggregator.apply(text(&session, "visible"));

        assert_eq!(aggregator.diagnostics(), (1, 1, 0));
        assert_eq!(aggregator.finish().unwrap(), "visible");
    }

    #[test]
    fn evt_05_wrong_session_is_ignored() {
        let session = session();
        let mut aggregator = TranslationTextAggregator::new(session.clone(), 64);
        aggregator.apply(text(&SessionId::new("other"), "wrong"));
        aggregator.apply(text(&session, "right"));

        assert_eq!(aggregator.diagnostics(), (1, 0, 1));
        assert_eq!(aggregator.finish().unwrap(), "right");
    }

    #[test]
    fn evt_06_permission_requires_cancel_and_fails_closed() {
        let session = session();
        let mut aggregator = TranslationTextAggregator::new(session.clone(), 64);

        assert!(matches!(
            aggregator.apply(AcpRuntimeEvent::PermissionRequested {
                session_id: session
            }),
            AggregatorAction::CancelAndFail(AiExecutionError::PermissionDenied)
        ));
    }

    #[test]
    fn evt_07_tool_activity_requires_cancel_and_fails_closed() {
        let session = session();
        let mut aggregator = TranslationTextAggregator::new(session.clone(), 64);

        assert!(matches!(
            aggregator.apply(AcpRuntimeEvent::ToolCall {
                session_id: session.clone(),
                tool_call_id: "tool".to_string(),
                title: "tool".to_string(),
                status: crate::backend::agents::protocol::acp::AcpToolStatus::Pending,
            }),
            AggregatorAction::CancelAndFail(AiExecutionError::ToolUseDenied)
        ));
    }

    #[test]
    fn evt_08_empty_output_fails() {
        let aggregator = TranslationTextAggregator::new(session(), 64);

        assert!(matches!(
            aggregator.finish(),
            Err(AiExecutionError::EmptyOutput { .. })
        ));
    }

    #[test]
    fn evt_09_whitespace_only_output_fails() {
        let session = session();
        let mut aggregator = TranslationTextAggregator::new(session.clone(), 64);
        aggregator.apply(text(&session, " \n\t "));

        assert!(matches!(
            aggregator.finish(),
            Err(AiExecutionError::EmptyOutput { .. })
        ));
    }

    #[test]
    fn evt_10_exact_byte_cap_succeeds() {
        let session = session();
        let value = "你好";
        let mut aggregator = TranslationTextAggregator::new(session.clone(), value.len());
        aggregator.apply(text(&session, value));

        assert_eq!(aggregator.finish().unwrap(), value);
    }

    #[test]
    fn evt_11_cap_plus_one_fails_without_appending_partial_chunk() {
        let session = session();
        let mut aggregator = TranslationTextAggregator::new(session.clone(), 5);
        aggregator.apply(text(&session, "12345"));

        assert!(matches!(
            aggregator.apply(text(&session, "6")),
            AggregatorAction::CancelAndFail(AiExecutionError::OutputLimit { limit: 5 })
        ));
        assert_eq!(aggregator.finish().unwrap(), "12345");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn evt_12_completion_marker_includes_the_late_final_chunk_without_sleep() {
        let session = session();
        let mut aggregator = TranslationTextAggregator::new(session.clone(), 64);
        let (tx, mut rx) = mpsc::channel(4);
        tx.send(text(&session, "before ")).await.unwrap();
        tx.send(text(&session, "late")).await.unwrap();
        tx.send(complete(&session)).await.unwrap();
        drop(tx);

        loop {
            let event = rx.recv().await.expect("completion marker");
            match aggregator.apply(event) {
                AggregatorAction::Continue => {}
                AggregatorAction::Complete { stop_reason } => {
                    assert_eq!(stop_reason, StopReason::EndTurn);
                    break;
                }
                AggregatorAction::CancelAndFail(error) => {
                    panic!("unexpected aggregation failure: {error}")
                }
            }
        }

        assert_eq!(aggregator.finish().unwrap(), "before late");
    }

    #[test]
    fn recall_evt_01_tool_activity_is_allowed_for_read_only_mcp() {
        let session = session();
        let mut aggregator = RecallStructuredAggregator::new(session.clone(), 64);

        assert!(matches!(
            aggregator.apply(AcpRuntimeEvent::ToolCall {
                session_id: session.clone(),
                tool_call_id: "tool".to_string(),
                title: "tool".to_string(),
                status: crate::backend::agents::protocol::acp::AcpToolStatus::Pending,
            }),
            AggregatorAction::Continue
        ));
        aggregator.apply(text(&session, "answer"));
        assert_eq!(aggregator.finish().unwrap(), "answer");
    }

    #[test]
    fn recall_evt_02_permission_is_still_denied() {
        let session = session();
        let mut aggregator = RecallStructuredAggregator::new(session.clone(), 64);

        assert!(matches!(
            aggregator.apply(AcpRuntimeEvent::PermissionRequested {
                session_id: session
            }),
            AggregatorAction::CancelAndFail(AiExecutionError::PermissionDenied)
        ));
    }
}
