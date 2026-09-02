use crate::backend::ai_execution::{SessionEventProjection, SessionSnapshot};
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

struct SessionStreamEntry {
    projection: Arc<SessionEventProjection>,
    active: bool,
}

/// Bounded process-local storage for active and recently completed member
/// Session projections. It intentionally has no persistence or serialization
/// path; application shutdown clears the registry explicitly.
pub(crate) struct SessionStreamRegistry {
    entries: Mutex<HashMap<SessionStreamKey, SessionStreamEntry>>,
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
        let mut order = self.order.lock().expect("session stream order lock");
        self.evict_for_insert(&mut entries, &mut order);
        let projection = Arc::new(SessionEventProjection::default());
        entries.insert(
            key.clone(),
            SessionStreamEntry {
                projection: projection.clone(),
                active: true,
            },
        );
        order.push_back(key);
        projection
    }

    pub(crate) fn get(&self, key: &SessionStreamKey) -> Option<Arc<SessionEventProjection>> {
        self.entries
            .lock()
            .ok()
            .and_then(|entries| entries.get(key).map(|entry| entry.projection.clone()))
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

    pub(crate) fn mark_terminal(&self, key: &SessionStreamKey) {
        if let Ok(mut entries) = self.entries.lock() {
            if let Some(entry) = entries.get_mut(key) {
                entry.active = false;
            }
        }
    }

    pub(crate) fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
        if let Ok(mut order) = self.order.lock() {
            order.clear();
        }
    }

    fn evict_for_insert(
        &self,
        entries: &mut HashMap<SessionStreamKey, SessionStreamEntry>,
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
                    entries.remove(&candidate);
                    evicted = true;
                    break;
                }
            }
            if !evicted {
                if let Some(candidate) = order.pop_front() {
                    entries.remove(&candidate);
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
}
