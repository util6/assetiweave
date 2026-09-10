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
mod tests {
    use super::*;

    fn key(id: &str) -> SessionStreamKey {
        SessionStreamKey {
            tenant_id: "tenant".to_string(),
            team_id: "team".to_string(),
            member_id: "member".to_string(),
            execution_id: id.to_string(),
        }
    }

    #[test]
    fn registry_is_bounded_and_keeps_active_projection_until_capacity_is_exhausted() {
        let registry = SessionStreamRegistry::new(2);
        let first = key("first");
        let second = key("second");
        let third = key("third");
        registry.register(first.clone());
        registry.register(second.clone());
        registry.mark_terminal(&first);

        registry.register(third.clone());

        assert!(registry.get(&first).is_none());
        assert!(registry.get(&second).is_some());
        assert!(registry.get(&third).is_some());
    }

    #[test]
    fn clear_drops_all_transient_projections() {
        let registry = SessionStreamRegistry::new(1);
        let key = key("execution");
        registry.register(key.clone());

        registry.clear();

        assert!(registry.get(&key).is_none());
    }

    #[test]
    fn metadata_and_by_ref_lookup_and_eviction_work() {
        let registry = SessionStreamRegistry::new(2);
        let key1 = key("first");
        let ref1 = AgentSessionRef::new("ref-1");
        let meta1 = AgentSessionMetadata {
            session_ref: ref1.clone(),
            execution_id: "first".to_string(),
            purpose: "memory".to_string(),
            mode: "oneshot".to_string(),
            tenant_id: Some("tenant".to_string()),
            agent: AgentInfoView {
                id: "agent-1".to_string(),
                display_name: None,
                model: None,
                protocol: "builtin".to_string(),
            },
            context: AgentSessionContextView {
                team_id: None,
                member_id: None,
                memory_scope: Some("session".to_string()),
                memory_job_id: Some("job-1".to_string()),
                task_id: Some("task-1".to_string()),
            },
            allow_stop: true,
        };

        registry.register_with_metadata(key1.clone(), meta1);
        let snapshot = registry.get_by_ref("ref-1");
        assert!(snapshot.is_some());
        let view = snapshot.unwrap().to_view();
        assert_eq!(view.session_ref.value, "ref-1");
        assert_eq!(view.state, "active");

        registry.mark_terminal_by_ref(
            "ref-1",
            Some(AgentSessionTerminalView {
                state: "succeeded".to_string(),
                code: None,
                message: None,
                retryable: false,
            }),
        );
        let updated = registry.get_by_ref("ref-1").unwrap().to_view();
        assert_eq!(updated.state, "terminal");
        assert_eq!(
            updated.terminal.as_ref().map(|t| t.state.as_str()),
            Some("succeeded")
        );
    }
}
