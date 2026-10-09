use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AgentSessionMode {
    Ephemeral,
    Persistent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AgentExecutionPhase {
    Initializing,
    Running,
    Streaming,
    WaitingForApproval,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub call_id: String,
    pub tool_name: String,
    pub input_json: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub call_id: String,
    pub output_json: String,
    pub is_error: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AgentStreamEvent {
    ThoughtDelta(String),
    TextDelta(String),
    ToolCall(ToolCallRequest),
    ToolResult(ToolCallResult),
    PermissionRequest {
        request_id: String,
        description: String,
    },
    PhaseChanged(AgentExecutionPhase),
    Done {
        stop_reason: Option<String>,
    },
    Error(String),
}

#[derive(Clone, Debug, Default)]
pub struct AgentCancellation {
    cancelled: Arc<AtomicBool>,
}

impl AgentCancellation {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[derive(Clone, Debug)]
pub struct AgentExecutionLimits {
    pub timeout: Duration,
    pub max_response_bytes: usize,
    pub stderr_tail_cap: usize,
}

impl Default for AgentExecutionLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(120),
            max_response_bytes: 4 * 1024 * 1024,
            stderr_tail_cap: 32 * 1024,
        }
    }
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
