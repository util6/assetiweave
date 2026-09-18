use crate::backend::ai_execution::{SessionEventProjection, SessionSnapshot};
use crate::backend::dto::{
    AgentInfoView, AgentSessionCapabilitiesView, AgentSessionContextView, AgentSessionRef,
    AgentSessionTerminalView, AgentSessionView, SessionRetentionView,
};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;

/// The identity used to address a process-local member Session projection.
///
/// The Team and tenant are part of the key even though the execution id is
/// currently globally generated. This keeps the read boundary explicit and
/// prevents a future caller from accidentally treating an execution id as a
/// cross-tenant capability.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SessionStreamKey {
    pub(crate) tenant_id: String,
    pub(crate) team_id: String,
    pub(crate) member_id: String,
    pub(crate) execution_id: String,
}

#[derive(Clone, Debug)]
pub(crate) struct AgentSessionMetadata {
    pub(crate) session_ref: AgentSessionRef,
    pub(crate) execution_id: String,
    pub(crate) purpose: String,
    pub(crate) mode: String,
    pub(crate) tenant_id: Option<String>,
    pub(crate) agent: AgentInfoView,
    pub(crate) context: AgentSessionContextView,
    pub(crate) allow_stop: bool,
}

#[derive(Clone)]
pub(crate) struct AgentSessionEntrySnapshot {
    pub(crate) projection: Arc<SessionEventProjection>,
    pub(crate) active: bool,
    pub(crate) metadata: AgentSessionMetadata,
    pub(crate) terminal_info: Option<AgentSessionTerminalView>,
}

impl AgentSessionEntrySnapshot {
    pub(crate) fn to_view(&self) -> AgentSessionView {
        let snap = self.projection.snapshot();
        let now = chrono::Utc::now().to_rfc3339();
        AgentSessionView {
            schema_version: 1,
            session_ref: self.metadata.session_ref.clone(),
            execution_id: self.metadata.execution_id.clone(),
            purpose: self.metadata.purpose.clone(),
            mode: self.metadata.mode.clone(),
            tenant_id: self.metadata.tenant_id.clone(),
            agent: self.metadata.agent.clone(),
            context: self.metadata.context.clone(),
            state: if self.active {
                "active".to_string()
            } else {
                "terminal".to_string()
            },
            terminal: self.terminal_info.clone(),
            capabilities: AgentSessionCapabilitiesView {
                read: true,
                send: false,
                stop: self.metadata.allow_stop && self.active,
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
            items: snap.items,
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
            finished_at: if self.active { None } else { Some(now) },
        }
    }
}

struct SessionStreamEntry {
    projection: Arc<SessionEventProjection>,
    active: bool,
    metadata: Option<AgentSessionMetadata>,
    terminal_info: Option<AgentSessionTerminalView>,
}

/// Bounded process-local storage for active and recently completed member
/// Session projections. It intentionally has no persistence or serialization
/// path; application shutdown clears the registry explicitly.
pub(crate) struct SessionStreamRegistry {
    entries: Mutex<HashMap<SessionStreamKey, SessionStreamEntry>>,
    by_ref: Mutex<HashMap<String, SessionStreamKey>>,
    order: Mutex<VecDeque<SessionStreamKey>>,
    capacity: usize,
}

impl Default for SessionStreamRegistry {
    fn default() -> Self {
        Self::new(256)
    }
}

impl SessionStreamRegistry {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            by_ref: Mutex::new(HashMap::new()),
            order: Mutex::new(VecDeque::new()),
            capacity: capacity.max(1),
        }
    }

    pub(crate) fn register(&self, key: SessionStreamKey) -> Arc<SessionEventProjection> {
        if let Ok(entries) = self.entries.lock() {
            if let Some(entry) = entries.get(&key) {
                return entry.projection.clone();
            }
        }

        let mut entries = self.entries.lock().expect("session stream registry lock");
        if let Some(entry) = entries.get(&key) {
            return entry.projection.clone();
        }
        let mut by_ref = self.by_ref.lock().expect("session stream by_ref lock");
        let mut order = self.order.lock().expect("session stream order lock");
        self.evict_for_insert(&mut entries, &mut by_ref, &mut order);
        let projection = Arc::new(SessionEventProjection::default());
        entries.insert(
            key.clone(),
            SessionStreamEntry {
                projection: projection.clone(),
                active: true,
                metadata: None,
                terminal_info: None,
            },
        );
        order.push_back(key);
        projection
    }

    pub(crate) fn register_with_metadata(
        &self,
        key: SessionStreamKey,
        metadata: AgentSessionMetadata,
    ) -> Arc<SessionEventProjection> {
        let ref_value = metadata.session_ref.value.clone();
        let mut entries = self.entries.lock().expect("session stream registry lock");
        let mut by_ref = self.by_ref.lock().expect("session stream by_ref lock");
        if let Some(entry) = entries.get_mut(&key) {
            entry.metadata = Some(metadata);
            entry.active = true;
            entry.terminal_info = None;
            by_ref.insert(ref_value, key.clone());
            return entry.projection.clone();
        }
        let mut order = self.order.lock().expect("session stream order lock");
        self.evict_for_insert(&mut entries, &mut by_ref, &mut order);
        let projection = Arc::new(SessionEventProjection::default());
        entries.insert(
            key.clone(),
            SessionStreamEntry {
                projection: projection.clone(),
                active: true,
                metadata: Some(metadata),
                terminal_info: None,
            },
        );
        by_ref.insert(ref_value, key.clone());
        order.push_back(key);
        projection
    }

    pub(crate) fn get(&self, key: &SessionStreamKey) -> Option<Arc<SessionEventProjection>> {
        self.entries
            .lock()
            .ok()
            .and_then(|entries| entries.get(key).map(|entry| entry.projection.clone()))
    }

    pub(crate) fn get_by_ref(&self, session_ref_val: &str) -> Option<AgentSessionEntrySnapshot> {
        let key = {
            let by_ref = self.by_ref.lock().ok()?;
            by_ref.get(session_ref_val)?.clone()
        };
        let entries = self.entries.lock().ok()?;
        let entry = entries.get(&key)?;
        let metadata = entry.metadata.clone()?;
        Some(AgentSessionEntrySnapshot {
            projection: entry.projection.clone(),
            active: entry.active,
            metadata,
            terminal_info: entry.terminal_info.clone(),
        })
    }

    pub(crate) fn snapshot(&self, key: &SessionStreamKey) -> Option<SessionSnapshot> {
        self.get(key).map(|projection| projection.snapshot())
    }

    pub(crate) fn subscribe(
        &self,
        key: &SessionStreamKey,
    ) -> Option<broadcast::Receiver<SessionSnapshot>> {
        self.get(key).map(|projection| projection.subscribe())
    }

    pub(crate) fn subscribe_by_ref(
        &self,
        session_ref_val: &str,
    ) -> Option<broadcast::Receiver<SessionSnapshot>> {
        let key = {
            let by_ref = self.by_ref.lock().ok()?;
            by_ref.get(session_ref_val)?.clone()
        };
        self.subscribe(&key)
    }

    pub(crate) fn mark_terminal(&self, key: &SessionStreamKey) {
        if let Ok(mut entries) = self.entries.lock() {
            if let Some(entry) = entries.get_mut(key) {
                entry.active = false;
            }
        }
    }

    pub(crate) fn mark_terminal_by_ref(
        &self,
        session_ref_val: &str,
        terminal: Option<AgentSessionTerminalView>,
    ) {
        let key = {
            if let Ok(by_ref) = self.by_ref.lock() {
                by_ref.get(session_ref_val).cloned()
            } else {
                None
            }
        };
        if let Some(key) = key {
            if let Ok(mut entries) = self.entries.lock() {
                if let Some(entry) = entries.get_mut(&key) {
                    entry.active = false;
                    entry.terminal_info = terminal;
                }
            }
        }
    }

    pub(crate) fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
        if let Ok(mut by_ref) = self.by_ref.lock() {
            by_ref.clear();
        }
        if let Ok(mut order) = self.order.lock() {
            order.clear();
        }
    }

    fn evict_for_insert(
        &self,
        entries: &mut HashMap<SessionStreamKey, SessionStreamEntry>,
        by_ref: &mut HashMap<String, SessionStreamKey>,
        order: &mut VecDeque<SessionStreamKey>,
    ) {
        while entries.len() >= self.capacity {
            let order_len = order.len();
            let mut evicted = false;
            for _ in 0..order_len {
                let Some(candidate) = order.pop_front() else {
                    break;
                };
                let is_active = entries.get(&candidate).is_some_and(|entry| entry.active);
                if is_active {
                    order.push_back(candidate);
                } else {
                    if let Some(removed) = entries.remove(&candidate) {
                        if let Some(meta) = removed.metadata {
                            by_ref.remove(&meta.session_ref.value);
                        }
                    }
                    evicted = true;
                    break;
                }
            }
            if !evicted {
                if let Some(candidate) = order.pop_front() {
                    if let Some(removed) = entries.remove(&candidate) {
                        if let Some(meta) = removed.metadata {
                            by_ref.remove(&meta.session_ref.value);
                        }
                    }
                } else {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "session_streams_tests.rs"]
mod tests;
