use crate::backend::ai_execution::SessionItemSnapshot;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionRef {
    pub schema_version: u32,
    pub value: String,
}

impl AgentSessionRef {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            schema_version: 1,
            value: value.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfoView {
    pub id: String,
    pub display_name: Option<String>,
    pub model: Option<String>,
    pub protocol: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionContextView {
    pub team_id: Option<String>,
    pub member_id: Option<String>,
    pub memory_scope: Option<String>,
    pub memory_job_id: Option<String>,
    pub task_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionTerminalView {
    pub state: String,
    pub code: Option<String>,
    pub message: Option<String>,
    pub retryable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionCapabilitiesView {
    pub read: bool,
    pub send: bool,
    pub stop: bool,
    pub retry: bool,
    pub queue: bool,
    pub interrupt: bool,
    pub attach: bool,
    pub mention: bool,
    pub slash_command: bool,
    pub model_select: bool,
    pub permission_response: bool,
    pub copy: bool,
    pub open_artifact: bool,
}

impl Default for AgentSessionCapabilitiesView {
    fn default() -> Self {
        Self {
            read: true,
            send: false,
            stop: false,
            retry: false,
            queue: false,
            interrupt: false,
            attach: false,
            mention: false,
            slash_command: false,
            model_select: false,
            permission_response: false,
            copy: true,
            open_artifact: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionRetentionView {
    pub max_items: usize,
    pub max_events: usize,
    pub max_bytes: usize,
    pub truncated: bool,
    pub evicted_item_count: usize,
    pub rejected_event_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionUnavailableView {
    pub schema_version: u32,
    pub session_ref: AgentSessionRef,
    pub state: String,
    pub reason: String,
}

impl AgentSessionUnavailableView {
    pub fn new(session_ref: AgentSessionRef) -> Self {
        Self {
            schema_version: 1,
            session_ref,
            state: "unavailable".to_string(),
            reason: "notFoundOrExpired".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionView {
    pub schema_version: u32,
    pub session_ref: AgentSessionRef,
    pub execution_id: String,
    pub purpose: String,
    pub mode: String,
    pub tenant_id: Option<String>,
    pub agent: AgentInfoView,
    pub context: AgentSessionContextView,
    pub state: String,
    pub terminal: Option<AgentSessionTerminalView>,
    pub capabilities: AgentSessionCapabilitiesView,
    pub revision: u64,
    pub event_count: usize,
    pub items: Vec<SessionItemSnapshot>,
    pub retention: SessionRetentionView,
    pub started_at: Option<String>,
    pub updated_at: String,
    pub finished_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(untagged)]
pub enum AgentSessionGetResult {
    Unavailable(AgentSessionUnavailableView),
    Available(AgentSessionView),
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionGetParams {
    pub session_ref: AgentSessionRef,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionUpdatedEvent {
    pub session_ref: AgentSessionRef,
    pub revision: u64,
}
