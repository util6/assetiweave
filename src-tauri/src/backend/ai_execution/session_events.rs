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
#[path = "session_events_tests.rs"]
mod tests;
