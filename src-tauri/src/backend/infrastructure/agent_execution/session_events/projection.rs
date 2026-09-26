use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex, MutexGuard},
};

use tokio::sync::broadcast;

use super::{
    truncation::truncate_head_tail,
    types::{
        SessionEvent, SessionEventApplyResult, SessionEventDelivery, SessionEventIdentity,
        SessionEventKind, SessionEventProjectionLimits, SessionEventSink, SessionItemIdentity,
        SessionItemKind, SessionItemSnapshot, SessionItemState, SessionProcessingState,
        SessionSnapshot, SessionTaskStatus, SessionToolState,
        SESSION_EVENT_SNAPSHOT_CHANNEL_CAPACITY,
    },
};

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
        let mut memory_bytes = memory_bytes_of(&event);
        let mut state = self.lock_state();
        if state
            .seen_events
            .contains(&SessionEventDedupKey::from_event(&event))
        {
            return SessionEventApplyResult::Duplicate;
        }
        if memory_bytes > state.limits.max_bytes {
            if let Some(truncated_event) = truncate_event_to_fit(event, state.limits.max_bytes) {
                event = truncated_event;
                memory_bytes = memory_bytes_of(&event);
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
            self.memory_bytes = self.memory_bytes.saturating_sub(memory_bytes_of(&event));
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
            self.memory_bytes = self.memory_bytes.saturating_sub(memory_bytes_of(event));
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
        let mut snapshot = create_initial_item_snapshot(identity);
        for event in self.events.values() {
            apply_event_to_snapshot(&mut snapshot, event);
        }
        snapshot
    }
}

use super::materialize::*;

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

fn memory_bytes_of(event: &SessionEvent) -> usize {
    let identity_bytes = [
        event.identity.session_id.len(),
        event.identity.member_id.len(),
        event.identity.execution_id.len(),
        event.identity.turn_id.len(),
        event.identity.item_id.len(),
        event.identity.event_id.len(),
    ]
    .into_iter()
    .sum::<usize>();
    match &event.kind {
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
        SessionEventKind::TaskResult { detail, .. } | SessionEventKind::Notice { detail, .. } => {
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

fn truncate_event_to_fit(mut event: SessionEvent, max_bytes: usize) -> Option<SessionEvent> {
    let identity_bytes = [
        event.identity.session_id.len(),
        event.identity.member_id.len(),
        event.identity.execution_id.len(),
        event.identity.turn_id.len(),
        event.identity.item_id.len(),
        event.identity.event_id.len(),
    ]
    .into_iter()
    .sum::<usize>();

    if identity_bytes >= max_bytes {
        return None;
    }
    let budget = max_bytes - identity_bytes;

    match &mut event.kind {
        SessionEventKind::AssistantTextDelta { text }
        | SessionEventKind::AssistantTextSnapshot { text }
        | SessionEventKind::ThinkingDelta { text }
        | SessionEventKind::ThinkingSnapshot { text } => {
            let (new_text, info) = truncate_head_tail(text, budget);
            *text = new_text;
            event.truncation = Some(info);
            Some(event)
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
            event.truncation = Some(info);
            Some(event)
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
            event.truncation = Some(info);
            Some(event)
        }
        _ => None,
    }
}
