use std::{fmt, path::PathBuf, time::Duration};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::backend::agents::types::AgentId;

use super::AiExecutionPhase;

#[derive(Debug)]
pub(crate) enum AiExecutionError {
    AgentNotFound { agent_id: AgentId },
    RuntimeUnavailable { command_name: String },
    Spawn { program: PathBuf, message: String },
    Output { message: String },
    Timeout { program: PathBuf, timeout: Duration },
    Cancelled { program: PathBuf },
    OutputLimit { limit: usize },
    EmptyOutput { program: Option<PathBuf> },
    PermissionDenied,
    ToolUseDenied,
    Protocol { operation: &'static str },
    ModelSelectionFailed,
    AgentExited { code: Option<i32> },
    Workspace { operation: &'static str },
    CleanupFailed { failures: Vec<String> },
    InvalidPrompt(String),
    InvalidModel(String),
}

impl AiExecutionError {
    pub(crate) fn to_view(&self) -> AiExecutionErrorView {
        let (code, message, retryable) = match self {
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
            Self::ModelSelectionFailed => (
                "model_selection_failed",
                "The requested AI model could not be selected.",
                false,
            ),
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

impl fmt::Display for AiExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AgentNotFound { agent_id } => {
                write!(formatter, "AI agent '{agent_id}' is not registered")
            }
            Self::RuntimeUnavailable { command_name } => write!(
                formatter,
                "{command_name} was not found on this host. Install it and make `{command_name}` available on PATH or from a login shell."
            ),
            Self::Spawn { program, message } => {
                write!(formatter, "failed to start {}: {message}", program.display())
            }
            Self::Output { message } => formatter.write_str(message),
            Self::Timeout { program, timeout } => write!(
                formatter,
                "{} timed out after {} seconds",
                program.display(),
                timeout.as_secs()
            ),
            Self::Cancelled { program } => {
                write!(formatter, "{} was cancelled", program.display())
            }
            Self::OutputLimit { limit } => write!(
                formatter,
                "the AI agent exceeded the configured output limit of {limit} bytes"
            ),
            Self::EmptyOutput {
                program: Some(program),
            } => write!(formatter, "{} returned empty output", program.display()),
            Self::EmptyOutput { program: None } => {
                formatter.write_str("the AI agent returned empty output")
            }
            Self::PermissionDenied => {
                formatter.write_str("the AI agent requested a denied permission")
            }
            Self::ToolUseDenied => {
                formatter.write_str("the AI agent attempted denied tool use")
            }
            Self::Protocol { operation } => {
                write!(formatter, "the ACP {operation} operation failed")
            }
            Self::ModelSelectionFailed => {
                formatter.write_str("the requested AI model could not be selected")
            }
            Self::AgentExited { code: Some(code) } => {
                write!(formatter, "the AI agent exited with code {code}")
            }
            Self::AgentExited { code: None } => {
                formatter.write_str("the AI agent exited before execution completed")
            }
            Self::Workspace { operation } => {
                write!(formatter, "the isolated workspace {operation} operation failed")
            }
            Self::CleanupFailed { failures } => write!(
                formatter,
                "AI agent cleanup failed in {} step(s)",
                failures.len()
            ),
            Self::InvalidPrompt(message) | Self::InvalidModel(message) => {
                formatter.write_str(message)
            }
        }
    }
}

impl std::error::Error for AiExecutionError {}

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
}
