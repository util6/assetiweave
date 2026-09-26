use crate::backend::domain::agents::session::{
    AgentInfoView, AgentSessionContextView, AgentSessionRef, AgentSessionTerminalView,
};
use crate::backend::infrastructure::agent_execution::session_events::SessionItemSnapshot;
use crate::backend::infrastructure::runtime::session_streams::AgentSessionEntrySnapshot;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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

pub use super::session_item_view::AgentSessionItemView;

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
    pub items: Vec<AgentSessionItemView>,
    pub retention: SessionRetentionView,
    pub started_at: Option<String>,
    pub updated_at: String,
    pub finished_at: Option<String>,
}

impl AgentSessionView {
    pub(crate) fn from_entry_snapshot(entry: &AgentSessionEntrySnapshot) -> Self {
        let snap = entry.projection.snapshot();
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            schema_version: 1,
            session_ref: entry.metadata.session_ref.clone(),
            execution_id: entry.metadata.execution_id.clone(),
            purpose: entry.metadata.purpose.clone(),
            mode: entry.metadata.mode.clone(),
            tenant_id: entry.metadata.tenant_id.clone(),
            agent: entry.metadata.agent.clone(),
            context: entry.metadata.context.clone(),
            state: if entry.active {
                "active".to_string()
            } else {
                "terminal".to_string()
            },
            terminal: entry.terminal_info.clone(),
            capabilities: AgentSessionCapabilitiesView {
                read: true,
                send: false,
                stop: entry.metadata.allow_stop && entry.active,
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
            },
            revision: snap.revision,
            event_count: snap.event_count,
            items: snap.items.into_iter().map(Into::into).collect(),
            retention: SessionRetentionView {
                max_items: 256,
                max_events: 1024,
                max_bytes: 1024 * 1024,
                truncated: false,
                evicted_item_count: 0,
                rejected_event_count: 0,
            },
            started_at: None,
            updated_at: now.clone(),
            finished_at: if entry.active { None } else { Some(now) },
        }
    }
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

#[cfg(test)]
#[path = "session_view_tests.rs"]
mod tests;
