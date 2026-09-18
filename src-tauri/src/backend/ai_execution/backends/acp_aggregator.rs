use std::collections::HashSet;

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

/// Aggregates text output while allowing calls to an execution-scoped,
/// read-only MCP server. Requests without such a server keep the fail-closed
/// tool policy in `TranslationTextAggregator`.
pub(crate) struct ReadOnlyToolTextAggregator {
    session_id: SessionId,
    text: String,
    byte_limit: usize,
    chunks: usize,
    thinking_chunks: usize,
    ignored_session_events: usize,
    allowed_tool_names: HashSet<String>,
    allowed_tool_call_ids: HashSet<String>,
}

impl ReadOnlyToolTextAggregator {
    pub(crate) fn new(
        session_id: SessionId,
        byte_limit: usize,
        allowed_tool_names: impl IntoIterator<Item = &'static str>,
    ) -> Self {
        Self {
            session_id,
            text: String::new(),
            byte_limit,
            chunks: 0,
            thinking_chunks: 0,
            ignored_session_events: 0,
            allowed_tool_names: allowed_tool_names.into_iter().map(str::to_owned).collect(),
            allowed_tool_call_ids: HashSet::new(),
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
            AcpRuntimeEvent::ToolCall {
                tool_call_id,
                title,
                ..
            } => self.authorize_tool_call(tool_call_id, Some(&title)),
            AcpRuntimeEvent::ToolCallUpdate {
                tool_call_id,
                title,
                ..
            } => self.authorize_tool_call(tool_call_id, title.as_deref()),
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

    fn authorize_tool_call(
        &mut self,
        tool_call_id: String,
        title: Option<&str>,
    ) -> AggregatorAction {
        if self.allowed_tool_call_ids.contains(&tool_call_id) {
            return AggregatorAction::Continue;
        }
        let allowed = title
            .map(|title| {
                let trimmed = title.trim();
                !trimmed.is_empty()
                    && self
                        .allowed_tool_names
                        .iter()
                        .any(|tool_name| title_mentions_tool(trimmed, tool_name))
            })
            .unwrap_or(false);

        if allowed {
            self.allowed_tool_call_ids.insert(tool_call_id);
            AggregatorAction::Continue
        } else {
            tracing::warn!(
                "tool_use_denied: tool_call_id={}, title={:?}, allowed={:?}",
                tool_call_id,
                title,
                self.allowed_tool_names
            );
            AggregatorAction::CancelAndFail(AiExecutionError::ToolUseDenied)
        }
    }
}

fn title_mentions_tool(title: &str, tool_name: &str) -> bool {
    let title = title.trim();
    if title.eq_ignore_ascii_case(tool_name) {
        return true;
    }

    let title = title.to_ascii_lowercase();
    let tool_name = tool_name.to_ascii_lowercase();
    if title.ends_with(&tool_name) || title.starts_with(&tool_name) {
        return true;
    }
    title.match_indices(&tool_name).any(|(start, matched)| {
        let end = start + matched.len();
        let before_is_boundary = if start == 0 {
            true
        } else {
            let prev_char = title[..start].chars().next_back().unwrap();
            !prev_char.is_ascii_alphanumeric()
                || title[..start].ends_with("assetiweave_memory_generation")
                || title[..start].ends_with("assetiweave-memory-generation")
        };
        let after_is_boundary = if end == title.len() {
            true
        } else {
            let next_char = title[end..].chars().next().unwrap();
            !next_char.is_ascii_alphanumeric()
        };
        before_is_boundary && after_is_boundary
    })
}

#[cfg(test)]
#[path = "acp_aggregator_tests.rs"]
mod tests;
