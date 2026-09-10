use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

pub(crate) const DEFAULT_SESSION_EVENT_ITEM_LIMIT: usize = 256;
pub(crate) const DEFAULT_SESSION_EVENT_LIMIT: usize = 2_048;
pub(crate) const DEFAULT_SESSION_EVENT_BYTES_LIMIT: usize = 4 * 1024 * 1024;
const SESSION_EVENT_SNAPSHOT_CHANNEL_CAPACITY: usize = 64;

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

/// Protocol-neutral facts emitted by a Provider Session.
///
/// Textual fields are intentionally retained only by the transient projection
/// or the Provider-owned history path. The custom `Debug` implementation below
/// keeps them out of diagnostics when an event crosses an execution boundary.
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

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
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

/// A read model for the bounded, process-local event projection.
///
/// This type is serializable for snapshot transport, but the owning
/// `SessionEventProjection` has no persistence implementation and is cleared
/// explicitly during application shutdown.
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
    fn normalized(self) -> Self {
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

#[derive(Clone)]
pub(crate) struct SessionEventProjection {
    state: Arc<Mutex<ProjectionState>>,
    events: Arc<broadcast::Sender<SessionSnapshot>>,
}

impl Default for SessionEventProjection {
    fn default() -> Self {
        Self::new(SessionEventProjectionLimits::default())
    }
}

impl SessionEventProjection {
    pub(crate) fn new(limits: SessionEventProjectionLimits) -> Self {
        let (events, _) = broadcast::channel(SESSION_EVENT_SNAPSHOT_CHANNEL_CAPACITY);
        Self {
            state: Arc::new(Mutex::new(ProjectionState::new(limits.normalized()))),
            events: Arc::new(events),
        }
    }

    pub(crate) fn apply(&self, event: SessionEvent) -> SessionEventApplyResult {
        let mut event = event;
        let mut memory_bytes = event.memory_bytes();
        let mut state = self.lock_state();
        if state
            .seen_events
            .contains(&SessionEventDedupKey::from_event(&event))
        {
            return SessionEventApplyResult::Duplicate;
        }
        if memory_bytes > state.limits.max_bytes {
            if let Some(truncated_event) = event.truncate_to_fit(state.limits.max_bytes) {
                event = truncated_event;
                memory_bytes = event.memory_bytes();
            } else {
                return SessionEventApplyResult::RejectedOversized;
            }
        }

        let item_identity = event.identity.item_identity();
        if !state.items.contains_key(&item_identity) {
            while state.items.len() >= state.limits.max_items {
                state.evict_oldest_item();
            }
            state.item_order.push_back(item_identity.clone());
        }
        // Eviction is FIFO by accepted event. Sequence ordering is applied
        // when materializing each logical item, so reconnects may safely
        // deliver an older sequence after a newer one.
        while state.event_count >= state.limits.max_events
            || state.memory_bytes.saturating_add(memory_bytes) > state.limits.max_bytes
        {
            if !state.evict_oldest_event() {
                return SessionEventApplyResult::RejectedOversized;
            }
        }

        let sort_key = SessionEventOrderKey::from_event(&event);
        let dedup_key = SessionEventDedupKey::from_event(&event);
        state
            .items
            .entry(item_identity.clone())
            .or_insert_with(ItemState::new)
            .events
            .insert(sort_key.clone(), event);
        state.seen_events.insert(dedup_key);
        state.event_order.push_back(StoredEventKey {
            item_identity,
            sort_key,
        });
        state.event_count += 1;
        state.memory_bytes += memory_bytes;
        state.revision += 1;
        let snapshot = state.snapshot();
        drop(state);
        let _ = self.events.send(snapshot);
        SessionEventApplyResult::Applied
    }

    pub(crate) fn snapshot(&self) -> SessionSnapshot {
        self.lock_state().snapshot()
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<SessionSnapshot> {
        self.events.subscribe()
    }

    pub(crate) fn clear(&self) {
        let snapshot = {
            let mut state = self.lock_state();
            state.clear();
            state.snapshot()
        };
        let _ = self.events.send(snapshot);
    }

    fn lock_state(&self) -> MutexGuard<'_, ProjectionState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl SessionEventSink for SessionEventProjection {
    fn emit_session_event(&self, event: SessionEvent) {
        let _ = self.apply(event);
    }
}

struct ProjectionState {
    limits: SessionEventProjectionLimits,
    revision: u64,
    event_count: usize,
    memory_bytes: usize,
    items: HashMap<SessionItemIdentity, ItemState>,
    item_order: VecDeque<SessionItemIdentity>,
    event_order: VecDeque<StoredEventKey>,
    seen_events: HashSet<SessionEventDedupKey>,
}

impl ProjectionState {
    fn new(limits: SessionEventProjectionLimits) -> Self {
        Self {
            limits,
            revision: 0,
            event_count: 0,
            memory_bytes: 0,
            items: HashMap::new(),
            item_order: VecDeque::new(),
            event_order: VecDeque::new(),
            seen_events: HashSet::new(),
        }
    }

    fn snapshot(&self) -> SessionSnapshot {
        let mut items = self
            .items
            .iter()
            .map(|(identity, state)| state.snapshot(identity.clone()))
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            let left_key = self
                .items
                .get(&left.identity)
                .and_then(ItemState::first_sort_key);
            let right_key = self
                .items
                .get(&right.identity)
                .and_then(ItemState::first_sort_key);
            left_key
                .cmp(&right_key)
                .then_with(|| left.identity.cmp(&right.identity))
        });
        let mut last_assistant_text: Option<String> = None;
        for item in &mut items {
            if item.kind == SessionItemKind::AssistantText {
                if let Some(t) = &item.text {
                    last_assistant_text = Some(t.clone());
                }
            } else if item.kind == SessionItemKind::FinalResult {
                if let (Some(terminal_text), Some(assistant_text)) =
                    (&item.text, &last_assistant_text)
                {
                    if terminal_text == assistant_text {
                        item.text = None;
                    }
                }
            }
        }
        SessionSnapshot {
            revision: self.revision,
            event_count: self.event_count,
            items,
        }
    }

    fn clear(&mut self) {
        self.revision += 1;
        self.event_count = 0;
        self.memory_bytes = 0;
        self.items.clear();
        self.item_order.clear();
        self.event_order.clear();
        self.seen_events.clear();
    }

    fn evict_oldest_item(&mut self) {
        while let Some(identity) = self.item_order.pop_front() {
            if self.items.contains_key(&identity) {
                self.remove_item(&identity);
                return;
            }
        }
        if let Some(identity) = self.items.keys().next().cloned() {
            self.remove_item(&identity);
        }
    }

    fn evict_oldest_event(&mut self) -> bool {
        while let Some(key) = self.event_order.pop_front() {
            let Some(item) = self.items.get_mut(&key.item_identity) else {
                continue;
            };
            let Some(event) = item.events.remove(&key.sort_key) else {
                continue;
            };
            self.seen_events
                .remove(&SessionEventDedupKey::from_event(&event));
            self.event_count = self.event_count.saturating_sub(1);
            self.memory_bytes = self.memory_bytes.saturating_sub(event.memory_bytes());
            if item.events.is_empty() {
                self.items.remove(&key.item_identity);
            }
            return true;
        }
        false
    }

    fn remove_item(&mut self, identity: &SessionItemIdentity) {
        let Some(item) = self.items.remove(identity) else {
            return;
        };
        for event in item.events.values() {
            self.seen_events
                .remove(&SessionEventDedupKey::from_event(event));
            self.event_count = self.event_count.saturating_sub(1);
            self.memory_bytes = self.memory_bytes.saturating_sub(event.memory_bytes());
        }
    }
}

struct ItemState {
    events: BTreeMap<SessionEventOrderKey, SessionEvent>,
}

impl ItemState {
    fn new() -> Self {
        Self {
            events: BTreeMap::new(),
        }
    }

    fn first_sort_key(&self) -> Option<&SessionEventOrderKey> {
        self.events.keys().next()
    }

    fn snapshot(&self, identity: SessionItemIdentity) -> SessionItemSnapshot {
        let mut snapshot = SessionItemSnapshot::new(identity);
        for event in self.events.values() {
            snapshot.apply(event);
        }
        snapshot
    }
}

impl SessionItemSnapshot {
    fn new(identity: SessionItemIdentity) -> Self {
        Self {
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

    fn apply(&mut self, event: &SessionEvent) {
        self.sequence = self.sequence.max(event.sequence);
        if matches!(event.delivery, SessionEventDelivery::Live) {
            self.delivery = SessionEventDelivery::Live;
        }
        if let Some(t) = &event.truncation {
            self.truncation = Some(t.clone());
        }

        match &event.kind {
            SessionEventKind::UserMessageAcknowledged { accepted, text } => {
                self.kind = SessionItemKind::UserMessage;
                if text.is_some() {
                    self.text = text.clone();
                }
                self.state = if *accepted {
                    SessionItemState::Completed
                } else {
                    SessionItemState::Failed
                };
            }
            SessionEventKind::AssistantTextDelta { text }
            | SessionEventKind::ThinkingDelta { text } => {
                let is_thinking = matches!(&event.kind, SessionEventKind::ThinkingDelta { .. });
                self.kind = if is_thinking {
                    SessionItemKind::Thinking
                } else {
                    SessionItemKind::AssistantText
                };
                self.text.get_or_insert_with(String::new).push_str(text);
                self.state = SessionItemState::Streaming;
            }
            SessionEventKind::AssistantTextSnapshot { text }
            | SessionEventKind::ThinkingSnapshot { text } => {
                let is_thinking = matches!(&event.kind, SessionEventKind::ThinkingSnapshot { .. });
                self.kind = if is_thinking {
                    SessionItemKind::Thinking
                } else {
                    SessionItemKind::AssistantText
                };
                self.text = Some(text.clone());
                self.state = SessionItemState::Streaming;
            }
            SessionEventKind::Processing { state } => {
                self.kind = SessionItemKind::Processing;
                self.state = match state {
                    SessionProcessingState::Started => SessionItemState::Pending,
                    SessionProcessingState::Active => SessionItemState::Streaming,
                    SessionProcessingState::Completed => SessionItemState::Completed,
                };
            }
            SessionEventKind::ToolStart { name, raw_input } => {
                self.kind = SessionItemKind::Tool;
                if self.tool_call_id.is_none() {
                    self.tool_call_id = Some(
                        self.identity
                            .item_id
                            .strip_prefix("tool:")
                            .unwrap_or(&self.identity.item_id)
                            .to_string(),
                    );
                }
                if name.is_some() {
                    self.tool_name = name.clone();
                    self.text = name.clone();
                }
                if raw_input.is_some() {
                    self.tool_input = raw_input.clone();
                }
                if !matches!(
                    self.state,
                    SessionItemState::Succeeded
                        | SessionItemState::Failed
                        | SessionItemState::Cancelled
                ) {
                    self.state = SessionItemState::Pending;
                }
            }
            SessionEventKind::ToolUpdate {
                state,
                detail,
                raw_output,
            } => {
                self.kind = SessionItemKind::Tool;
                if self.tool_call_id.is_none() {
                    self.tool_call_id = Some(
                        self.identity
                            .item_id
                            .strip_prefix("tool:")
                            .unwrap_or(&self.identity.item_id)
                            .to_string(),
                    );
                }
                if detail.is_some() {
                    self.text = detail.clone();
                }
                if raw_output.is_some() {
                    self.tool_output = raw_output.clone();
                }
                if !matches!(
                    self.state,
                    SessionItemState::Succeeded
                        | SessionItemState::Failed
                        | SessionItemState::Cancelled
                ) {
                    self.state = tool_state(*state);
                }
            }
            SessionEventKind::ToolResult {
                success,
                detail,
                raw_output,
            } => {
                self.kind = SessionItemKind::Tool;
                if self.tool_call_id.is_none() {
                    self.tool_call_id = Some(
                        self.identity
                            .item_id
                            .strip_prefix("tool:")
                            .unwrap_or(&self.identity.item_id)
                            .to_string(),
                    );
                }
                if detail.is_some() {
                    self.text = detail.clone();
                }
                if raw_output.is_some() {
                    self.tool_output = raw_output.clone();
                }
                let target_state = if *success {
                    SessionItemState::Succeeded
                } else {
                    SessionItemState::Failed
                };
                if matches!(
                    self.state,
                    SessionItemState::Succeeded
                        | SessionItemState::Failed
                        | SessionItemState::Cancelled
                ) {
                    if self.state != target_state {
                        self.code = Some("conflicting_terminal_event".to_string());
                    }
                } else {
                    self.state = target_state;
                }
            }
            SessionEventKind::TaskProjection { task_id } => {
                self.kind = SessionItemKind::Task;
                self.code = Some(task_id.clone());
                self.state = SessionItemState::Pending;
            }
            SessionEventKind::TaskStatus { status } => {
                self.kind = SessionItemKind::Task;
                self.status = Some(*status);
                self.state = task_status_state(*status);
            }
            SessionEventKind::TaskResult { success, detail } => {
                self.kind = SessionItemKind::Task;
                if detail.is_some() {
                    self.text = detail.clone();
                }
                self.state = if *success {
                    SessionItemState::Succeeded
                } else {
                    SessionItemState::Failed
                };
            }
            SessionEventKind::Notice { code, detail } => {
                self.kind = SessionItemKind::Notice;
                self.code = Some(code.clone());
                self.text = detail.clone();
                self.state = SessionItemState::Completed;
            }
            SessionEventKind::TerminalResult { text } => {
                if self.kind != SessionItemKind::AssistantText {
                    self.kind = SessionItemKind::FinalResult;
                }
                if text.is_some() {
                    self.text = text.clone();
                }
                self.state = SessionItemState::Completed;
            }
            SessionEventKind::Cancel => {
                if matches!(
                    self.state,
                    SessionItemState::Succeeded
                        | SessionItemState::Failed
                        | SessionItemState::Cancelled
                ) {
                    if self.state != SessionItemState::Cancelled {
                        self.code = Some("conflicting_terminal_event".to_string());
                    }
                } else {
                    self.kind = SessionItemKind::Cancelled;
                    self.state = SessionItemState::Cancelled;
                }
            }
            SessionEventKind::Error { code, .. } => {
                self.kind = SessionItemKind::Error;
                self.code = Some(code.clone());
                self.state = SessionItemState::Failed;
            }
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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct SessionEventDedupKey {
    item_identity: SessionItemIdentity,
    event_id: String,
}

impl SessionEventDedupKey {
    fn from_event(event: &SessionEvent) -> Self {
        Self {
            item_identity: event.identity.item_identity(),
            event_id: event.identity.event_id.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct SessionEventOrderKey {
    sequence: u64,
    delivery: u8,
    event_id: String,
}

impl SessionEventOrderKey {
    fn from_event(event: &SessionEvent) -> Self {
        Self {
            sequence: event.sequence,
            delivery: match event.delivery {
                SessionEventDelivery::Replay => 0,
                SessionEventDelivery::Live => 1,
            },
            event_id: event.identity.event_id.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct StoredEventKey {
    item_identity: SessionItemIdentity,
    sort_key: SessionEventOrderKey,
}

impl SessionEvent {
    fn memory_bytes(&self) -> usize {
        let identity_bytes = [
            self.identity.session_id.len(),
            self.identity.member_id.len(),
            self.identity.execution_id.len(),
            self.identity.turn_id.len(),
            self.identity.item_id.len(),
            self.identity.event_id.len(),
        ]
        .into_iter()
        .sum::<usize>();
        match &self.kind {
            SessionEventKind::UserMessageAcknowledged { .. }
            | SessionEventKind::Processing { .. }
            | SessionEventKind::Cancel => identity_bytes,
            SessionEventKind::AssistantTextDelta { text }
            | SessionEventKind::AssistantTextSnapshot { text }
            | SessionEventKind::ThinkingDelta { text }
            | SessionEventKind::ThinkingSnapshot { text } => identity_bytes + text.len(),
            SessionEventKind::ToolStart { name, raw_input } => {
                identity_bytes
                    + name.as_deref().map_or(0, str::len)
                    + raw_input
                        .as_ref()
                        .map_or(0, |v| serde_json::to_string(v).map_or(32, |s| s.len()))
            }
            SessionEventKind::ToolUpdate {
                detail, raw_output, ..
            }
            | SessionEventKind::ToolResult {
                detail, raw_output, ..
            } => {
                identity_bytes
                    + detail.as_deref().map_or(0, str::len)
                    + raw_output
                        .as_ref()
                        .map_or(0, |v| serde_json::to_string(v).map_or(32, |s| s.len()))
            }
            SessionEventKind::TaskResult { detail, .. }
            | SessionEventKind::Notice { detail, .. } => {
                identity_bytes + detail.as_deref().map_or(0, str::len)
            }
            SessionEventKind::TaskProjection { task_id } => identity_bytes + task_id.len(),
            SessionEventKind::TaskStatus { .. } => identity_bytes,
            SessionEventKind::TerminalResult { text } => {
                identity_bytes + text.as_deref().map_or(0, str::len)
            }
            SessionEventKind::Error { code, .. } => identity_bytes + code.len(),
        }
    }

    fn truncate_to_fit(mut self, max_bytes: usize) -> Option<Self> {
        let identity_bytes = [
            self.identity.session_id.len(),
            self.identity.member_id.len(),
            self.identity.execution_id.len(),
            self.identity.turn_id.len(),
            self.identity.item_id.len(),
            self.identity.event_id.len(),
        ]
        .into_iter()
        .sum::<usize>();

        if identity_bytes >= max_bytes {
            return None;
        }
        let budget = max_bytes - identity_bytes;

        match &mut self.kind {
            SessionEventKind::AssistantTextDelta { text }
            | SessionEventKind::AssistantTextSnapshot { text }
            | SessionEventKind::ThinkingDelta { text }
            | SessionEventKind::ThinkingSnapshot { text } => {
                let (new_text, info) = truncate_head_tail(text, budget);
                *text = new_text;
                self.truncation = Some(info);
                Some(self)
            }
            SessionEventKind::ToolStart { name, raw_input } => {
                let name_bytes = name.as_deref().map_or(0, str::len);
                let available = budget.saturating_sub(name_bytes);
                if available == 0 {
                    return None;
                }
                let raw_str = raw_input
                    .as_ref()
                    .map(|v| serde_json::to_string(v).unwrap_or_default())
                    .unwrap_or_default();
                let (new_str, info) = truncate_head_tail(&raw_str, available);
                *raw_input = Some(serde_json::Value::String(new_str));
                self.truncation = Some(info);
                Some(self)
            }
            SessionEventKind::ToolUpdate {
                detail, raw_output, ..
            }
            | SessionEventKind::ToolResult {
                detail, raw_output, ..
            } => {
                let detail_bytes = detail.as_deref().map_or(0, str::len);
                let available = budget.saturating_sub(detail_bytes);
                if available == 0 {
                    return None;
                }
                let raw_str = raw_output
                    .as_ref()
                    .map(|v| serde_json::to_string(v).unwrap_or_default())
                    .unwrap_or_default();
                let (new_str, info) = truncate_head_tail(&raw_str, available);
                *raw_output = Some(serde_json::Value::String(new_str));
                self.truncation = Some(info);
                Some(self)
            }
            _ => None,
        }
    }
}

pub(crate) fn truncate_head_tail(input: &str, max_bytes: usize) -> (String, TruncationInfo) {
    let original_bytes = input.len();
    if original_bytes <= max_bytes {
        return (
            input.to_string(),
            TruncationInfo {
                original_bytes,
                retained_bytes: original_bytes,
                strategy: "headTail".to_string(),
            },
        );
    }

    const MARKER: &str = "\n... [truncated] ...\n";
    if max_bytes <= MARKER.len() {
        let mut cutoff = max_bytes;
        while !input.is_char_boundary(cutoff) && cutoff > 0 {
            cutoff -= 1;
        }
        let truncated = input[..cutoff].to_string();
        let retained_bytes = truncated.len();
        return (
            truncated,
            TruncationInfo {
                original_bytes,
                retained_bytes,
                strategy: "headTail".to_string(),
            },
        );
    }

    let available = max_bytes - MARKER.len();
    let head_budget = (available * 3) / 4;
    let tail_budget = available - head_budget;

    let mut head_idx = head_budget.min(input.len());
    while !input.is_char_boundary(head_idx) && head_idx > 0 {
        head_idx -= 1;
    }
    let head = &input[..head_idx];

    let mut tail_start = input.len().saturating_sub(tail_budget);
    while !input.is_char_boundary(tail_start) && tail_start < input.len() {
        tail_start += 1;
    }
    let tail = &input[tail_start..];

    let combined = format!("{head}{MARKER}{tail}");
    let retained_bytes = combined.len();
    (
        combined,
        TruncationInfo {
            original_bytes,
            retained_bytes,
            strategy: "headTail".to_string(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ses_01_projection_merges_duplicate_and_out_of_order_events() {
        let projection = SessionEventProjection::new(SessionEventProjectionLimits {
            max_items: 8,
            max_events: 32,
            max_bytes: 4096,
        });
        let first = event(
            2,
            "text",
            SessionEventKind::AssistantTextDelta {
                text: "world".to_string(),
            },
            SessionEventDelivery::Live,
        );
        let second = event(
            1,
            "text",
            SessionEventKind::AssistantTextDelta {
                text: "hello ".to_string(),
            },
            SessionEventDelivery::Live,
        );

        assert_eq!(
            projection.apply(first.clone()),
            SessionEventApplyResult::Applied
        );
        assert_eq!(projection.apply(first), SessionEventApplyResult::Duplicate);
        assert_eq!(projection.apply(second), SessionEventApplyResult::Applied);

        let snapshot = projection.snapshot();
        assert_eq!(snapshot.items.len(), 1);
        assert_eq!(snapshot.items[0].text.as_deref(), Some("hello world"));
        assert_eq!(snapshot.items[0].sequence, 2);
        assert_eq!(snapshot.event_count, 2);
    }

    #[test]
    fn ses_02_projection_attaches_tool_result_and_separates_concurrent_executions() {
        let projection = SessionEventProjection::new(SessionEventProjectionLimits {
            max_items: 8,
            max_events: 32,
            max_bytes: 4096,
        });
        projection.apply(event(
            1,
            "tool",
            SessionEventKind::ToolStart {
                name: Some("search".to_string()),
                raw_input: None,
            },
            SessionEventDelivery::Live,
        ));
        projection.apply(event(
            2,
            "tool",
            SessionEventKind::ToolResult {
                success: true,
                detail: Some("done".to_string()),
                raw_output: None,
            },
            SessionEventDelivery::Live,
        ));
        projection.apply(event_for_execution(
            "execution-b",
            1,
            "other-text",
            SessionEventKind::AssistantTextSnapshot {
                text: "other".to_string(),
            },
            SessionEventDelivery::Live,
        ));

        let snapshot = projection.snapshot();
        assert_eq!(snapshot.items.len(), 2);
        let tool = snapshot
            .items
            .iter()
            .find(|item| item.kind == SessionItemKind::Tool)
            .expect("tool item");
        assert_eq!(tool.state, SessionItemState::Succeeded);
        assert_eq!(tool.text.as_deref(), Some("done"));
        assert_eq!(
            snapshot
                .items
                .iter()
                .find(|item| item.identity.execution_id == "execution-b")
                .and_then(|item| item.text.as_deref()),
            Some("other")
        );
    }

    #[test]
    fn ses_03_replay_and_live_events_merge_without_duplicate_items() {
        let projection = SessionEventProjection::new(SessionEventProjectionLimits::default());
        projection.apply(event(
            4,
            "text",
            SessionEventKind::AssistantTextSnapshot {
                text: "replayed".to_string(),
            },
            SessionEventDelivery::Replay,
        ));
        projection.apply(event(
            5,
            "text",
            SessionEventKind::AssistantTextDelta {
                text: " live".to_string(),
            },
            SessionEventDelivery::Live,
        ));
        projection.apply(event(
            4,
            "text",
            SessionEventKind::AssistantTextSnapshot {
                text: "replayed".to_string(),
            },
            SessionEventDelivery::Replay,
        ));

        let snapshot = projection.snapshot();
        assert_eq!(snapshot.items.len(), 1);
        assert_eq!(snapshot.items[0].text.as_deref(), Some("replayed live"));
        assert_eq!(snapshot.items[0].delivery, SessionEventDelivery::Live);
    }

    #[test]
    fn ses_04_projection_evicts_oldest_events_and_items_with_explicit_limits() {
        let projection = SessionEventProjection::new(SessionEventProjectionLimits {
            max_items: 2,
            max_events: 2,
            max_bytes: 4096,
        });
        projection.apply(event(
            1,
            "item-a",
            SessionEventKind::Notice {
                code: "a".to_string(),
                detail: None,
            },
            SessionEventDelivery::Live,
        ));
        projection.apply(event(
            2,
            "item-b",
            SessionEventKind::Notice {
                code: "b".to_string(),
                detail: None,
            },
            SessionEventDelivery::Live,
        ));
        projection.apply(event(
            3,
            "item-c",
            SessionEventKind::Notice {
                code: "c".to_string(),
                detail: None,
            },
            SessionEventDelivery::Live,
        ));

        let snapshot = projection.snapshot();
        assert_eq!(snapshot.items.len(), 2);
        assert_eq!(snapshot.event_count, 2);
        assert_eq!(snapshot.items[0].identity.item_id, "item-b");
        assert_eq!(snapshot.items[1].identity.item_id, "item-c");
    }

    #[test]
    fn ses_05_debug_output_redacts_event_and_snapshot_content() {
        let projection = SessionEventProjection::new(SessionEventProjectionLimits::default());
        projection.apply(event(
            1,
            "text",
            SessionEventKind::AssistantTextDelta {
                text: "SESSION_EVENT_SECRET".to_string(),
            },
            SessionEventDelivery::Live,
        ));

        let event_debug = format!(
            "{:?}",
            event(
                1,
                "text",
                SessionEventKind::AssistantTextDelta {
                    text: "SESSION_EVENT_SECRET".to_string(),
                },
                SessionEventDelivery::Live,
            )
        );
        let snapshot_debug = format!("{:?}", projection.snapshot());

        assert!(!event_debug.contains("SESSION_EVENT_SECRET"));
        assert!(!snapshot_debug.contains("SESSION_EVENT_SECRET"));
    }

    #[test]
    fn ses_06_projection_broadcasts_snapshot_and_clear_is_in_memory_only() {
        let projection = SessionEventProjection::default();
        let mut subscriber = projection.subscribe();
        projection.apply(event(
            1,
            "text",
            SessionEventKind::AssistantTextSnapshot {
                text: "temporary".to_string(),
            },
            SessionEventDelivery::Live,
        ));

        let published = subscriber.try_recv().expect("published snapshot");
        assert_eq!(published.items.len(), 1);
        projection.clear();
        assert!(projection.snapshot().items.is_empty());
        assert_eq!(projection.snapshot().event_count, 0);
    }

    #[test]
    fn ses_07_projection_accepts_concurrent_events_without_losing_ordered_deltas() {
        let projection =
            std::sync::Arc::new(SessionEventProjection::new(SessionEventProjectionLimits {
                max_items: 2,
                max_events: 16,
                max_bytes: 4096,
            }));
        let handles = (0..8)
            .map(|sequence| {
                let projection = projection.clone();
                std::thread::spawn(move || {
                    projection.apply(event(
                        sequence,
                        "text",
                        SessionEventKind::AssistantTextDelta {
                            text: sequence.to_string(),
                        },
                        SessionEventDelivery::Live,
                    ))
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            assert_eq!(
                handle.join().expect("projection worker"),
                SessionEventApplyResult::Applied
            );
        }

        let snapshot = projection.snapshot();
        assert_eq!(snapshot.event_count, 8);
        assert_eq!(snapshot.items[0].text.as_deref(), Some("01234567"));
    }

    #[test]
    fn t02_one_complete_tool_step_aggregates_start_update_result_with_typed_payload_and_redaction()
    {
        let projection = SessionEventProjection::default();
        let secret_input = "SECRET_INPUT_KEYWORD";
        let secret_output = "SECRET_OUTPUT_PAYLOAD";

        // 1. ToolStart with name and raw input
        projection.apply(event(
            1,
            "tool:read_1",
            SessionEventKind::ToolStart {
                name: Some("read_file".to_string()),
                raw_input: Some(serde_json::json!({"path": "src/main.rs", "secret": secret_input})),
            },
            SessionEventDelivery::Live,
        ));

        // 2. ToolUpdate with Running and detail
        projection.apply(event(
            2,
            "tool:read_1",
            SessionEventKind::ToolUpdate {
                state: SessionToolState::Running,
                detail: Some("reading 1024 bytes".to_string()),
                raw_output: None,
            },
            SessionEventDelivery::Live,
        ));

        // 3. ToolResult with Succeeded and raw output
        let result_event = event(
            3,
            "tool:read_1",
            SessionEventKind::ToolResult {
                success: true,
                detail: Some("completed reading".to_string()),
                raw_output: Some(serde_json::json!({"bytes": 1024, "token": secret_output})),
            },
            SessionEventDelivery::Live,
        );
        projection.apply(result_event.clone());

        let snapshot = projection.snapshot();
        // 1 logical tool item
        assert_eq!(snapshot.items.len(), 1);
        let item = &snapshot.items[0];
        assert_eq!(item.kind, SessionItemKind::Tool);
        assert_eq!(item.state, SessionItemState::Succeeded);
        assert_eq!(item.tool_name.as_deref(), Some("read_file"));
        assert_eq!(item.tool_call_id.as_deref(), Some("read_1"));
        assert_eq!(
            item.tool_input,
            Some(serde_json::json!({"path": "src/main.rs", "secret": secret_input}))
        );
        assert_eq!(
            item.tool_output,
            Some(serde_json::json!({"bytes": 1024, "token": secret_output}))
        );

        // State monotonicity: subsequent InProgress update does not retreat state from Succeeded
        projection.apply(event(
            4,
            "tool:read_1",
            SessionEventKind::ToolUpdate {
                state: SessionToolState::Running,
                detail: Some("late running event".to_string()),
                raw_output: None,
            },
            SessionEventDelivery::Live,
        ));
        let snapshot_after = projection.snapshot();
        assert_eq!(snapshot_after.items[0].state, SessionItemState::Succeeded);

        // Redaction assertions: Debug format must NEVER contain secret payload
        let event_debug = format!("{result_event:?}");
        let snapshot_debug = format!("{snapshot:?}");
        let item_debug = format!("{item:?}");

        assert!(!event_debug.contains(secret_input));
        assert!(!event_debug.contains(secret_output));
        assert!(!snapshot_debug.contains(secret_input));
        assert!(!snapshot_debug.contains(secret_output));
        assert!(!item_debug.contains(secret_input));
        assert!(!item_debug.contains(secret_output));
    }

    #[test]
    fn t03_complete_agent_session_canonical_fixture_and_edge_cases() {
        let projection = SessionEventProjection::new(SessionEventProjectionLimits {
            max_items: 32,
            max_events: 128,
            max_bytes: 64 * 1024,
        });

        // 1. User request
        projection.apply(event(
            1,
            "user",
            SessionEventKind::UserMessageAcknowledged {
                accepted: true,
                text: Some("Inspect the workspace and update the target.".to_string()),
            },
            SessionEventDelivery::Live,
        ));

        // 2. Assistant text 1
        projection.apply(event(
            2,
            "assistant_1",
            SessionEventKind::AssistantTextDelta {
                text: "I will inspect".to_string(),
            },
            SessionEventDelivery::Live,
        ));

        // 3. Thinking
        projection.apply(event(
            3,
            "thinking",
            SessionEventKind::ThinkingDelta {
                text: "Analyzing the codebase and tools...".to_string(),
            },
            SessionEventDelivery::Live,
        ));

        // 4. Tool A start
        projection.apply(event(
            4,
            "tool:run_cmd",
            SessionEventKind::ToolStart {
                name: Some("run_command".to_string()),
                raw_input: Some(serde_json::json!({"cmd": "ls"})),
            },
            SessionEventDelivery::Live,
        ));

        // 5. Tool A update
        projection.apply(event(
            5,
            "tool:run_cmd",
            SessionEventKind::ToolUpdate {
                state: SessionToolState::Running,
                detail: Some("running command".to_string()),
                raw_output: Some(serde_json::json!({"stdout": "Cargo.toml\n"})),
            },
            SessionEventDelivery::Live,
        ));

        // 6. Tool A result (success)
        projection.apply(event(
            6,
            "tool:run_cmd",
            SessionEventKind::ToolResult {
                success: true,
                detail: Some("done".to_string()),
                raw_output: Some(serde_json::json!({"exit": 0})),
            },
            SessionEventDelivery::Live,
        ));

        // 7. Assistant text 2
        projection.apply(event(
            7,
            "assistant_2",
            SessionEventKind::AssistantTextDelta {
                text: "I found the file.".to_string(),
            },
            SessionEventDelivery::Live,
        ));

        // 8. Tool B start
        projection.apply(event(
            8,
            "tool:edit",
            SessionEventKind::ToolStart {
                name: Some("file_edit".to_string()),
                raw_input: Some(serde_json::json!({"file": "Cargo.toml"})),
            },
            SessionEventDelivery::Live,
        ));

        // 9. Tool B result
        projection.apply(event(
            9,
            "tool:edit",
            SessionEventKind::ToolResult {
                success: true,
                detail: Some("edited".to_string()),
                raw_output: Some(serde_json::json!({"diff": "+[dependencies]"})),
            },
            SessionEventDelivery::Live,
        ));

        // 10. Tool C result (failure)
        projection.apply(event(
            10,
            "tool:test",
            SessionEventKind::ToolResult {
                success: false,
                detail: Some("test failed".to_string()),
                raw_output: Some(serde_json::json!({"stderr": "failed"})),
            },
            SessionEventDelivery::Live,
        ));

        // 11. Assistant final text
        projection.apply(event(
            11,
            "assistant_3",
            SessionEventKind::AssistantTextDelta {
                text: "All done.".to_string(),
            },
            SessionEventDelivery::Live,
        ));

        // 12. Terminal result with same text as assistant_3
        projection.apply(event(
            12,
            "terminal",
            SessionEventKind::TerminalResult {
                text: Some("All done.".to_string()),
            },
            SessionEventDelivery::Live,
        ));

        let snapshot = projection.snapshot();
        assert_eq!(snapshot.items.len(), 9);

        // Sequence verification
        assert_eq!(snapshot.items[0].kind, SessionItemKind::UserMessage);
        assert_eq!(
            snapshot.items[0].text.as_deref(),
            Some("Inspect the workspace and update the target.")
        );

        assert_eq!(snapshot.items[1].kind, SessionItemKind::AssistantText);
        assert_eq!(snapshot.items[1].text.as_deref(), Some("I will inspect"));

        assert_eq!(snapshot.items[2].kind, SessionItemKind::Thinking);

        assert_eq!(snapshot.items[3].kind, SessionItemKind::Tool);
        assert_eq!(snapshot.items[3].tool_name.as_deref(), Some("run_command"));
        assert_eq!(snapshot.items[3].state, SessionItemState::Succeeded);

        assert_eq!(snapshot.items[4].kind, SessionItemKind::AssistantText);
        assert_eq!(snapshot.items[4].text.as_deref(), Some("I found the file."));

        assert_eq!(snapshot.items[5].kind, SessionItemKind::Tool);
        assert_eq!(snapshot.items[5].tool_name.as_deref(), Some("file_edit"));
        assert_eq!(snapshot.items[5].state, SessionItemState::Succeeded);

        assert_eq!(snapshot.items[6].kind, SessionItemKind::Tool);
        assert_eq!(snapshot.items[6].state, SessionItemState::Failed);

        assert_eq!(snapshot.items[7].kind, SessionItemKind::AssistantText);
        assert_eq!(snapshot.items[7].text.as_deref(), Some("All done."));

        // C-072: terminal text identical to last assistant text is suppressed from duplicating text
        assert_eq!(snapshot.items[8].kind, SessionItemKind::FinalResult);
        assert_eq!(snapshot.items[8].text, None);
        assert_eq!(snapshot.items[8].state, SessionItemState::Completed);

        // Edge case: Conflicting terminal events keep monotonic terminal and record notice code
        projection.apply(event(
            13,
            "tool:run_cmd",
            SessionEventKind::ToolResult {
                success: false,
                detail: Some("conflicting result".to_string()),
                raw_output: None,
            },
            SessionEventDelivery::Live,
        ));
        let snapshot_conflict = projection.snapshot();
        let tool_run_cmd = snapshot_conflict
            .items
            .iter()
            .find(|i| i.tool_name.as_deref() == Some("run_command"))
            .unwrap();
        assert_eq!(tool_run_cmd.state, SessionItemState::Succeeded);
        assert_eq!(
            tool_run_cmd.code.as_deref(),
            Some("conflicting_terminal_event")
        );

        // Edge case: Head/Tail truncation on oversized content
        let small_projection = SessionEventProjection::new(SessionEventProjectionLimits {
            max_items: 8,
            max_events: 16,
            max_bytes: 400,
        });
        let huge_text = "A".repeat(1000);
        let apply_result = small_projection.apply(event(
            1,
            "assistant_huge",
            SessionEventKind::AssistantTextDelta { text: huge_text },
            SessionEventDelivery::Live,
        ));
        assert_eq!(apply_result, SessionEventApplyResult::Applied);
        let small_snapshot = small_projection.snapshot();
        assert_eq!(small_snapshot.items.len(), 1);
        let huge_item = &small_snapshot.items[0];
        assert!(huge_item.truncation.is_some());
        let trunc = huge_item.truncation.as_ref().unwrap();
        assert_eq!(trunc.original_bytes, 1000);
        assert_eq!(trunc.strategy, "headTail");
        assert!(huge_item.text.as_ref().unwrap().contains("[truncated]"));
    }

    fn event(
        sequence: u64,
        item_id: &str,
        kind: SessionEventKind,
        delivery: SessionEventDelivery,
    ) -> SessionEvent {
        event_for_execution("execution-a", sequence, item_id, kind, delivery)
    }

    fn event_for_execution(
        execution_id: &str,
        sequence: u64,
        item_id: &str,
        kind: SessionEventKind,
        delivery: SessionEventDelivery,
    ) -> SessionEvent {
        SessionEvent {
            identity: SessionEventIdentity {
                session_id: "session".to_string(),
                member_id: "member".to_string(),
                execution_id: execution_id.to_string(),
                turn_id: "turn".to_string(),
                item_id: item_id.to_string(),
                event_id: format!("event-{execution_id}-{item_id}-{sequence}"),
            },
            sequence,
            delivery,
            kind,
            truncation: None,
        }
    }
}
