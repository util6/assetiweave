use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(crate) const DEFAULT_SESSION_EVENT_ITEM_LIMIT: usize = 256;
pub(crate) const DEFAULT_SESSION_EVENT_LIMIT: usize = 2_048;
pub(crate) const DEFAULT_SESSION_EVENT_BYTES_LIMIT: usize = 4 * 1024 * 1024;
pub(crate) const SESSION_EVENT_SNAPSHOT_CHANNEL_CAPACITY: usize = 64;

#[derive(
    Clone, Debug, Deserialize, Eq, Hash, JsonSchema, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub(crate) struct SessionEventIdentity {
    pub(crate) session_id: String,
    pub(crate) member_id: String,
    pub(crate) execution_id: String,
    pub(crate) turn_id: String,
    pub(crate) item_id: String,
    pub(crate) event_id: String,
}

impl SessionEventIdentity {
    pub(crate) fn item_identity(&self) -> SessionItemIdentity {
        SessionItemIdentity {
            session_id: self.session_id.clone(),
            member_id: self.member_id.clone(),
            execution_id: self.execution_id.clone(),
            turn_id: self.turn_id.clone(),
            item_id: self.item_id.clone(),
        }
    }
}

#[derive(
    Clone, Debug, Deserialize, Eq, Hash, JsonSchema, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub(crate) struct SessionItemIdentity {
    pub(crate) session_id: String,
    pub(crate) member_id: String,
    pub(crate) execution_id: String,
    pub(crate) turn_id: String,
    pub(crate) item_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionEventDelivery {
    Live,
    Replay,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionProcessingState {
    Started,
    Active,
    Completed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionToolState {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionTaskStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum SessionEventKind {
    UserMessageAcknowledged {
        accepted: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    AssistantTextDelta {
        text: String,
    },
    AssistantTextSnapshot {
        text: String,
    },
    Processing {
        state: SessionProcessingState,
    },
    ThinkingDelta {
        text: String,
    },
    ThinkingSnapshot {
        text: String,
    },
    ToolStart {
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        raw_input: Option<serde_json::Value>,
    },
    ToolUpdate {
        state: SessionToolState,
        detail: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        raw_output: Option<serde_json::Value>,
    },
    ToolResult {
        success: bool,
        detail: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        raw_output: Option<serde_json::Value>,
    },
    TaskProjection {
        task_id: String,
    },
    TaskStatus {
        status: SessionTaskStatus,
    },
    TaskResult {
        success: bool,
        detail: Option<String>,
    },
    Notice {
        code: String,
        detail: Option<String>,
    },
    TerminalResult {
        text: Option<String>,
    },
    Cancel,
    Error {
        code: String,
        retryable: bool,
    },
}

impl fmt::Debug for SessionEventKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::UserMessageAcknowledged { .. } => "UserMessageAcknowledged",
            Self::AssistantTextDelta { .. } => "AssistantTextDelta",
            Self::AssistantTextSnapshot { .. } => "AssistantTextSnapshot",
            Self::Processing { .. } => "Processing",
            Self::ThinkingDelta { .. } => "ThinkingDelta",
            Self::ThinkingSnapshot { .. } => "ThinkingSnapshot",
            Self::ToolStart { .. } => "ToolStart",
            Self::ToolUpdate { .. } => "ToolUpdate",
            Self::ToolResult { .. } => "ToolResult",
            Self::TaskProjection { .. } => "TaskProjection",
            Self::TaskStatus { .. } => "TaskStatus",
            Self::TaskResult { .. } => "TaskResult",
            Self::Notice { .. } => "Notice",
            Self::TerminalResult { .. } => "TerminalResult",
            Self::Cancel => "Cancel",
            Self::Error { .. } => "Error",
        };
        formatter.debug_struct(name).finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) struct TruncationInfo {
    pub(crate) original_bytes: usize,
    pub(crate) retained_bytes: usize,
    pub(crate) strategy: String,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) struct SessionEvent {
    pub(crate) identity: SessionEventIdentity,
    pub(crate) sequence: u64,
    pub(crate) delivery: SessionEventDelivery,
    pub(crate) kind: SessionEventKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) truncation: Option<TruncationInfo>,
}

impl fmt::Debug for SessionEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SessionEvent")
            .field("identity", &self.identity)
            .field("sequence", &self.sequence)
            .field("delivery", &self.delivery)
            .field("kind", &self.kind)
            .field("truncation", &self.truncation)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionItemKind {
    UserMessage,
    AssistantText,
    Processing,
    Thinking,
    Tool,
    Task,
    Notice,
    FinalResult,
    Cancelled,
    Error,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionItemState {
    Pending,
    Streaming,
    Completed,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) struct SessionItemSnapshot {
    pub(crate) identity: SessionItemIdentity,
    pub(crate) kind: SessionItemKind,
    pub(crate) sequence: u64,
    pub(crate) delivery: SessionEventDelivery,
    pub(crate) state: SessionItemState,
    pub(crate) text: Option<String>,
    pub(crate) status: Option<SessionTaskStatus>,
    pub(crate) code: Option<String>,
    #[serde(default)]
    pub(crate) partial: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) truncation: Option<TruncationInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tool_input: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tool_output: Option<serde_json::Value>,
}

impl fmt::Debug for SessionItemSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SessionItemSnapshot")
            .field("identity", &self.identity)
            .field("kind", &self.kind)
            .field("sequence", &self.sequence)
            .field("delivery", &self.delivery)
            .field("state", &self.state)
            .field("partial", &self.partial)
            .field("truncation", &self.truncation)
            .field("text", &self.text.as_ref().map(|_| "<redacted>"))
            .field("status", &self.status)
            .field("code", &self.code)
            .field("tool_call_id", &self.tool_call_id)
            .field("tool_name", &self.tool_name)
            .field(
                "tool_input",
                &self.tool_input.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "tool_output",
                &self.tool_output.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) struct SessionSnapshot {
    pub(crate) revision: u64,
    pub(crate) event_count: usize,
    pub(crate) items: Vec<SessionItemSnapshot>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SessionEventProjectionLimits {
    pub(crate) max_items: usize,
    pub(crate) max_events: usize,
    pub(crate) max_bytes: usize,
}

impl Default for SessionEventProjectionLimits {
    fn default() -> Self {
        Self {
            max_items: DEFAULT_SESSION_EVENT_ITEM_LIMIT,
            max_events: DEFAULT_SESSION_EVENT_LIMIT,
            max_bytes: DEFAULT_SESSION_EVENT_BYTES_LIMIT,
        }
    }
}

impl SessionEventProjectionLimits {
    pub(crate) fn normalized(self) -> Self {
        Self {
            max_items: self.max_items.max(1),
            max_events: self.max_events.max(1),
            max_bytes: self.max_bytes.max(1),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SessionEventApplyResult {
    Applied,
    Duplicate,
    RejectedOversized,
}

#[allow(dead_code)]
pub(crate) trait SessionEventSink: Send + Sync {
    fn emit_session_event(&self, event: SessionEvent);
}
