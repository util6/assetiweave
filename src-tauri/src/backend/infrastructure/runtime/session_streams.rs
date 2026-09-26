use crate::backend::domain::agents::{
    AgentInfoView, AgentSessionContextView, AgentSessionRef, AgentSessionTerminalView,
};
use crate::backend::infrastructure::agent_execution::session_events::{
    SessionEventProjection, SessionSnapshot,
};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;

/// The identity used to address a process-local Session projection.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SessionStreamKey {
    pub(crate) tenant_id: String,
    pub(crate) scope_id: String,
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

struct SessionStreamEntry {
    projection: Arc<SessionEventProjection>,
    active: bool,
    replay: bool,
    metadata: Option<AgentSessionMetadata>,
    terminal_info: Option<AgentSessionTerminalView>,
    cancellation: Option<tokio_util::sync::CancellationToken>,
}

/// Bounded process-local storage for active and recently completed member
/// Session projections. It intentionally has no persistence or serialization
/// path; application shutdown clears the registry explicitly.
#[derive(Clone)]
pub(crate) struct SessionStreamRegistry {
    entries: Arc<Mutex<HashMap<SessionStreamKey, SessionStreamEntry>>>,
    by_ref: Arc<Mutex<HashMap<String, SessionStreamKey>>>,
    order: Arc<Mutex<VecDeque<SessionStreamKey>>>,
    capacity: usize,
    updates: broadcast::Sender<SessionStreamKey>,
}

impl Default for SessionStreamRegistry {
    fn default() -> Self {
        Self::new(256)
    }
}

impl SessionStreamRegistry {
    pub(crate) fn new(capacity: usize) -> Self {
        let (updates, _) = broadcast::channel(2048);
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            by_ref: Arc::new(Mutex::new(HashMap::new())),
            order: Arc::new(Mutex::new(VecDeque::new())),
            capacity: capacity.max(1),
            updates,
        }
    }

    pub(crate) fn notify_updated(&self, key: &SessionStreamKey) {
        let _ = self.updates.send(key.clone());
    }

    pub(crate) fn subscribe_updates(&self) -> broadcast::Receiver<SessionStreamKey> {
        self.updates.subscribe()
    }

    pub(crate) fn register_cancellation(
        &self,
        key: &SessionStreamKey,
        cancellation: tokio_util::sync::CancellationToken,
    ) {
        if let Ok(mut entries) = self.entries.lock() {
            if let Some(entry) = entries.get_mut(key) {
                entry.cancellation = Some(cancellation);
            }
        }
    }

    pub(crate) fn cancel(&self, key: &SessionStreamKey) -> bool {
        if let Ok(mut entries) = self.entries.lock() {
            if let Some(entry) = entries.get_mut(key) {
                entry.active = false;
                if let Some(cancel) = entry.cancellation.take() {
                    cancel.cancel();
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn is_active(&self, key: &SessionStreamKey) -> bool {
        self.entries
            .lock()
            .ok()
            .and_then(|entries| entries.get(key).map(|e| e.active))
            .unwrap_or(false)
    }

    pub(crate) fn is_replay(&self, key: &SessionStreamKey) -> bool {
        self.entries
            .lock()
            .ok()
            .and_then(|entries| entries.get(key).map(|e| e.replay))
            .unwrap_or(false)
    }

    pub(crate) fn list_keys_by_scope(
        &self,
        tenant_id: &str,
        scope_id: &str,
    ) -> Vec<SessionStreamKey> {
        if let Ok(entries) = self.entries.lock() {
            entries
                .keys()
                .filter(|k| k.tenant_id == tenant_id && k.scope_id == scope_id)
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    pub(crate) fn register(&self, key: SessionStreamKey) -> Arc<SessionEventProjection> {
        self.register_with_options(key, false)
    }

    pub(crate) fn register_with_options(
        &self,
        key: SessionStreamKey,
        replay: bool,
    ) -> Arc<SessionEventProjection> {
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
                replay,
                metadata: None,
                terminal_info: None,
                cancellation: None,
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
                replay: false,
                metadata: Some(metadata),
                terminal_info: None,
                cancellation: None,
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
                entry.cancellation = None;
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
