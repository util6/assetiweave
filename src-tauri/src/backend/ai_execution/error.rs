use std::{fmt, path::PathBuf, time::Duration};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::backend::agents::types::AgentId;

use super::{AgentSessionMode, AiExecutionPhase};

#[derive(Debug, thiserror::Error)]
pub(crate) enum AiExecutionError {
    #[error("Agent session mode {mode:?} is not supported yet")]
    UnsupportedSessionMode { mode: AgentSessionMode },
    #[error("persistent execution requires a context key")]
    InvalidContextKey,
    #[error("history replay requires a persistent session")]
    InvalidReplayMode,
    #[error("the saved AI execution session is unavailable")]
    ResumeUnavailable,
    #[error("the selected AI agent has no Team tool capability")]
    TeamToolsUnavailable,
    #[error("Recall execution requires an ACP agent with read-only Recall tools")]
    RecallToolsUnavailable,
    #[error("AI agent '{agent_id}' is not registered")]
    AgentNotFound { agent_id: AgentId },
    #[error("{command_name} was not found on this host. Install it and make `{command_name}` available on PATH or from a login shell.")]
    RuntimeUnavailable { command_name: String },
    #[error("failed to start {}: {message}", program.display())]
    Spawn { program: PathBuf, message: String },
    #[error("{message}")]
    Output { message: String },
    #[error("{} timed out after {} seconds", program.display(), timeout.as_secs())]
    Timeout { program: PathBuf, timeout: Duration },
    #[error("{} was cancelled", program.display())]
    Cancelled { program: PathBuf },
    #[error("the AI agent exceeded the configured output limit of {limit} bytes")]
    OutputLimit { limit: usize },
    #[error("{}", match program { Some(p) => format!("{} returned empty output", p.display()), None => "the AI agent returned empty output".to_string() })]
    EmptyOutput { program: Option<PathBuf> },
    #[error("the AI agent requested a denied permission")]
    PermissionDenied,
    #[error("the AI agent attempted denied tool use")]
    ToolUseDenied,
    #[error("the ACP {operation} operation failed")]
    Protocol { operation: &'static str },
    #[error("the ACP {operation} operation failed: {detail}")]
    ProtocolDetail {
        operation: &'static str,
        detail: String,
    },
    #[error("{}", match detail { Some(d) => format!("the requested AI model could not be selected: {d}"), None => "the requested AI model could not be selected".to_string() })]
    ModelSelectionFailed { detail: Option<String> },
    #[error("the selected AI model is unavailable: {detail}")]
    ModelUnavailable { detail: String },
    #[error("{}", match code { Some(c) => format!("the AI agent exited with code {c}"), None => "the AI agent exited before execution completed".to_string() })]
    AgentExited { code: Option<i32> },
    #[error("the isolated workspace {operation} operation failed")]
    Workspace { operation: &'static str },
    #[error("AI agent cleanup failed in {} step(s)", failures.len())]
    CleanupFailed { failures: Vec<String> },
    #[error("{0}")]
    InvalidPrompt(String),
    #[error("{0}")]
    InvalidModel(String),
}

impl AiExecutionError {
    pub(crate) fn to_view(&self) -> AiExecutionErrorView {
        let (code, message, retryable) = match self {
            Self::UnsupportedSessionMode { .. } => (
                "unsupported_session_mode",
                "The requested Agent session mode is not supported yet.",
                false,
            ),
            Self::InvalidContextKey | Self::InvalidReplayMode => (
                "invalid_request",
                "The persistent execution context is invalid.",
                false,
            ),
            Self::ResumeUnavailable => (
                "resume_unavailable",
                "The saved AI execution session is no longer available for resume.",
                false,
            ),
            Self::TeamToolsUnavailable => (
                "team_tools_unavailable",
                "The selected AI agent has not declared the Team tool capability.",
                false,
            ),
            Self::RecallToolsUnavailable => (
                "recall_tools_unavailable",
                "Recall execution requires an ACP agent with the read-only Recall tool capability.",
                false,
            ),
            Self::AgentNotFound { .. } => (
                "agent_not_found",
                "The selected AI agent is not registered.",
                false,
            ),
            Self::RuntimeUnavailable { .. } => (
                "agent_unavailable",
                "The selected AI agent is unavailable.",
                true,
            ),
            Self::Spawn { .. } => (
                "spawn_failed",
                "The AI agent process could not be started.",
                true,
            ),
            Self::Output { .. } => (
                "process_output_failed",
                "The AI agent process output could not be read.",
                true,
            ),
            Self::Timeout { .. } => ("timeout", "The AI agent execution timed out.", true),
            Self::Cancelled { .. } => ("cancelled", "The AI agent execution was cancelled.", false),
            Self::OutputLimit { .. } => (
                "output_limit",
                "The AI agent exceeded the configured output limit.",
                false,
            ),
            Self::EmptyOutput { .. } => ("empty_output", "The AI agent returned no text.", false),
            Self::PermissionDenied => (
                "permission_denied",
                "The AI agent requested a permission that this execution does not allow.",
                false,
            ),
            Self::ToolUseDenied => (
                "tool_use_denied",
                "The AI agent attempted tool use during a text-only execution.",
                false,
            ),
            Self::Protocol { .. } => (
                "protocol_failed",
                "The AI agent protocol operation failed.",
                true,
            ),
            Self::ProtocolDetail { detail, .. } => {
                let detail = crate::backend::runtime::sanitize_public_message(detail);
                return AiExecutionErrorView {
                    code: "protocol_failed".to_string(),
                    message: format!("The AI agent protocol operation failed: {detail}"),
                    retryable: true,
                    phase: None,
                };
            }
            Self::ModelSelectionFailed { detail } => {
                return AiExecutionErrorView {
                    code: "model_selection_failed".to_string(),
                    message: detail.as_deref().map_or_else(
                        || "The requested AI model could not be selected.".to_string(),
                        |detail| {
                            format!(
                                "The requested AI model could not be selected: {}",
                                crate::backend::runtime::sanitize_public_message(detail)
                            )
                        },
                    ),
                    retryable: false,
                    phase: None,
                };
            }
            Self::ModelUnavailable { detail } => {
                return AiExecutionErrorView {
                    code: "model_unavailable".to_string(),
                    message: format!(
                        "The selected AI model is currently unavailable. Choose another model in Agent settings. Provider response: {}",
                        crate::backend::runtime::sanitize_public_message(detail)
                    ),
                    retryable: false,
                    phase: None,
                };
            }
            Self::AgentExited { .. } => (
                "agent_exited",
                "The AI agent process exited before execution completed.",
                true,
            ),
            Self::Workspace { .. } => (
                "workspace_failed",
                "The isolated AI execution workspace could not be prepared.",
                true,
            ),
            Self::CleanupFailed { .. } => (
                "cleanup_failed",
                "The AI agent execution did not clean up completely.",
                true,
            ),
            Self::InvalidPrompt(_) | Self::InvalidModel(_) => (
                "invalid_request",
                "The AI execution request is invalid.",
                false,
            ),
        };

        AiExecutionErrorView {
            code: code.to_string(),
            message: message.to_string(),
            retryable,
            phase: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiExecutionErrorView {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) retryable: bool,
    pub(crate) phase: Option<AiExecutionPhase>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_error_view_has_a_stable_code() {
        let error = AiExecutionError::AgentExited { code: Some(1) };

        let view = error.to_view();
        let public_debug = format!("{view:?}");

        assert_eq!(view.code, "agent_exited");
        assert!(view.retryable);
        assert!(!public_debug.contains("/private/"));
    }

    #[test]
    fn public_protocol_error_preserves_the_sanitized_agent_message() {
        let view = AiExecutionError::ProtocolDetail {
            operation: "prompt",
            detail: "Free promotion has ended for the selected model.".to_string(),
        }
        .to_view();

        assert_eq!(view.code, "protocol_failed");
        assert!(view.message.contains("Free promotion has ended"));
    }

    #[test]
    fn public_model_unavailable_error_is_actionable() {
        let view = AiExecutionError::ModelUnavailable {
            detail: "No allowed providers are available for the selected model.".to_string(),
        }
        .to_view();

        assert_eq!(view.code, "model_unavailable");
        assert!(view
            .message
            .contains("Choose another model in Agent settings"));
        assert!(view.message.contains("No allowed providers"));
        assert!(!view.retryable);
    }
}
