use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::backend::infrastructure::agent_execution::session_events::SessionItemSnapshot;

#[derive(
    Clone, Debug, Deserialize, Eq, Hash, JsonSchema, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub struct SessionItemIdentityView {
    pub session_id: String,
    pub member_id: String,
    pub execution_id: String,
    pub turn_id: String,
    pub item_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEventDeliveryView {
    Live,
    Replay,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionItemKindView {
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
pub enum SessionItemStateView {
    Pending,
    Streaming,
    Completed,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTaskStatusView {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct SessionTruncationInfoView {
    pub original_bytes: usize,
    pub retained_bytes: usize,
    pub strategy: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AgentSessionItemView {
    pub identity: SessionItemIdentityView,
    pub kind: SessionItemKindView,
    pub sequence: u64,
    pub delivery: SessionEventDeliveryView,
    pub state: SessionItemStateView,
    pub text: Option<String>,
    pub status: Option<SessionTaskStatusView>,
    pub code: Option<String>,
    #[serde(default)]
    pub partial: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncation: Option<SessionTruncationInfoView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_output: Option<serde_json::Value>,
}

impl From<crate::backend::infrastructure::agent_execution::session_events::SessionItemIdentity>
    for SessionItemIdentityView
{
    fn from(
        id: crate::backend::infrastructure::agent_execution::session_events::SessionItemIdentity,
    ) -> Self {
        Self {
            session_id: id.session_id,
            member_id: id.member_id,
            execution_id: id.execution_id,
            turn_id: id.turn_id,
            item_id: id.item_id,
        }
    }
}

impl From<SessionItemIdentityView>
    for crate::backend::infrastructure::agent_execution::session_events::SessionItemIdentity
{
    fn from(view: SessionItemIdentityView) -> Self {
        Self {
            session_id: view.session_id,
            member_id: view.member_id,
            execution_id: view.execution_id,
            turn_id: view.turn_id,
            item_id: view.item_id,
        }
    }
}

impl From<crate::backend::infrastructure::agent_execution::session_events::SessionEventDelivery>
    for SessionEventDeliveryView
{
    fn from(
        delivery: crate::backend::infrastructure::agent_execution::session_events::SessionEventDelivery,
    ) -> Self {
        match delivery {
            crate::backend::infrastructure::agent_execution::session_events::SessionEventDelivery::Live => {
                Self::Live
            }
            crate::backend::infrastructure::agent_execution::session_events::SessionEventDelivery::Replay => {
                Self::Replay
            }
        }
    }
}

impl From<SessionEventDeliveryView>
    for crate::backend::infrastructure::agent_execution::session_events::SessionEventDelivery
{
    fn from(view: SessionEventDeliveryView) -> Self {
        match view {
            SessionEventDeliveryView::Live => Self::Live,
            SessionEventDeliveryView::Replay => Self::Replay,
        }
    }
}

impl From<crate::backend::infrastructure::agent_execution::session_events::SessionItemKind>
    for SessionItemKindView
{
    fn from(
        kind: crate::backend::infrastructure::agent_execution::session_events::SessionItemKind,
    ) -> Self {
        match kind {
            crate::backend::infrastructure::agent_execution::session_events::SessionItemKind::UserMessage => {
                Self::UserMessage
            }
            crate::backend::infrastructure::agent_execution::session_events::SessionItemKind::AssistantText => {
                Self::AssistantText
            }
            crate::backend::infrastructure::agent_execution::session_events::SessionItemKind::Processing => {
                Self::Processing
            }
            crate::backend::infrastructure::agent_execution::session_events::SessionItemKind::Thinking => {
                Self::Thinking
            }
            crate::backend::infrastructure::agent_execution::session_events::SessionItemKind::Tool => {
                Self::Tool
            }
            crate::backend::infrastructure::agent_execution::session_events::SessionItemKind::Task => {
                Self::Task
            }
            crate::backend::infrastructure::agent_execution::session_events::SessionItemKind::Notice => {
                Self::Notice
            }
            crate::backend::infrastructure::agent_execution::session_events::SessionItemKind::FinalResult => {
                Self::FinalResult
            }
            crate::backend::infrastructure::agent_execution::session_events::SessionItemKind::Cancelled => {
                Self::Cancelled
            }
            crate::backend::infrastructure::agent_execution::session_events::SessionItemKind::Error => {
                Self::Error
            }
        }
    }
}

impl From<SessionItemKindView>
    for crate::backend::infrastructure::agent_execution::session_events::SessionItemKind
{
    fn from(view: SessionItemKindView) -> Self {
        match view {
            SessionItemKindView::UserMessage => Self::UserMessage,
            SessionItemKindView::AssistantText => Self::AssistantText,
            SessionItemKindView::Processing => Self::Processing,
            SessionItemKindView::Thinking => Self::Thinking,
            SessionItemKindView::Tool => Self::Tool,
            SessionItemKindView::Task => Self::Task,
            SessionItemKindView::Notice => Self::Notice,
            SessionItemKindView::FinalResult => Self::FinalResult,
            SessionItemKindView::Cancelled => Self::Cancelled,
            SessionItemKindView::Error => Self::Error,
        }
    }
}

impl From<crate::backend::infrastructure::agent_execution::session_events::SessionItemState>
    for SessionItemStateView
{
    fn from(
        state: crate::backend::infrastructure::agent_execution::session_events::SessionItemState,
    ) -> Self {
        match state {
            crate::backend::infrastructure::agent_execution::session_events::SessionItemState::Pending => {
                Self::Pending
            }
            crate::backend::infrastructure::agent_execution::session_events::SessionItemState::Streaming => {
                Self::Streaming
            }
            crate::backend::infrastructure::agent_execution::session_events::SessionItemState::Completed => {
                Self::Completed
            }
            crate::backend::infrastructure::agent_execution::session_events::SessionItemState::Succeeded => {
                Self::Succeeded
            }
            crate::backend::infrastructure::agent_execution::session_events::SessionItemState::Failed => {
                Self::Failed
            }
            crate::backend::infrastructure::agent_execution::session_events::SessionItemState::Cancelled => {
                Self::Cancelled
            }
        }
    }
}

impl From<SessionItemStateView>
    for crate::backend::infrastructure::agent_execution::session_events::SessionItemState
{
    fn from(view: SessionItemStateView) -> Self {
        match view {
            SessionItemStateView::Pending => Self::Pending,
            SessionItemStateView::Streaming => Self::Streaming,
            SessionItemStateView::Completed => Self::Completed,
            SessionItemStateView::Succeeded => Self::Succeeded,
            SessionItemStateView::Failed => Self::Failed,
            SessionItemStateView::Cancelled => Self::Cancelled,
        }
    }
}

impl From<crate::backend::infrastructure::agent_execution::session_events::SessionTaskStatus>
    for SessionTaskStatusView
{
    fn from(
        status: crate::backend::infrastructure::agent_execution::session_events::SessionTaskStatus,
    ) -> Self {
        match status {
            crate::backend::infrastructure::agent_execution::session_events::SessionTaskStatus::Queued => {
                Self::Queued
            }
            crate::backend::infrastructure::agent_execution::session_events::SessionTaskStatus::Running => {
                Self::Running
            }
            crate::backend::infrastructure::agent_execution::session_events::SessionTaskStatus::Succeeded => {
                Self::Succeeded
            }
            crate::backend::infrastructure::agent_execution::session_events::SessionTaskStatus::Failed => {
                Self::Failed
            }
            crate::backend::infrastructure::agent_execution::session_events::SessionTaskStatus::Cancelled => {
                Self::Cancelled
            }
        }
    }
}

impl From<SessionTaskStatusView>
    for crate::backend::infrastructure::agent_execution::session_events::SessionTaskStatus
{
    fn from(view: SessionTaskStatusView) -> Self {
        match view {
            SessionTaskStatusView::Queued => Self::Queued,
            SessionTaskStatusView::Running => Self::Running,
            SessionTaskStatusView::Succeeded => Self::Succeeded,
            SessionTaskStatusView::Failed => Self::Failed,
            SessionTaskStatusView::Cancelled => Self::Cancelled,
        }
    }
}

impl From<crate::backend::infrastructure::agent_execution::session_events::TruncationInfo>
    for SessionTruncationInfoView
{
    fn from(
        info: crate::backend::infrastructure::agent_execution::session_events::TruncationInfo,
    ) -> Self {
        Self {
            original_bytes: info.original_bytes,
            retained_bytes: info.retained_bytes,
            strategy: info.strategy,
        }
    }
}

impl From<SessionTruncationInfoView>
    for crate::backend::infrastructure::agent_execution::session_events::TruncationInfo
{
    fn from(view: SessionTruncationInfoView) -> Self {
        Self {
            original_bytes: view.original_bytes,
            retained_bytes: view.retained_bytes,
            strategy: view.strategy,
        }
    }
}

impl From<SessionItemSnapshot> for AgentSessionItemView {
    fn from(item: SessionItemSnapshot) -> Self {
        Self {
            identity: item.identity.into(),
            kind: item.kind.into(),
            sequence: item.sequence,
            delivery: item.delivery.into(),
            state: item.state.into(),
            text: item.text,
            status: item.status.map(Into::into),
            code: item.code,
            partial: item.partial,
            truncation: item.truncation.map(Into::into),
            tool_call_id: item.tool_call_id,
            tool_name: item.tool_name,
            tool_input: item.tool_input,
            tool_output: item.tool_output,
        }
    }
}

impl From<AgentSessionItemView> for SessionItemSnapshot {
    fn from(view: AgentSessionItemView) -> Self {
        Self {
            identity: view.identity.into(),
            kind: view.kind.into(),
            sequence: view.sequence,
            delivery: view.delivery.into(),
            state: view.state.into(),
            text: view.text,
            status: view.status.map(Into::into),
            code: view.code,
            partial: view.partial,
            truncation: view.truncation.map(Into::into),
            tool_call_id: view.tool_call_id,
            tool_name: view.tool_name,
            tool_input: view.tool_input,
            tool_output: view.tool_output,
        }
    }
}
