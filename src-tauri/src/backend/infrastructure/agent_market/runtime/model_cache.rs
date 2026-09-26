use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize},
        Arc,
    },
    time::{Duration, Instant},
};

use crate::backend::infrastructure::{
    agent_execution::{backends::acp::AcpProbeReport, AgentModelsResult, AiExecutionCancellation},
    agent_market::error::AgentMarketError,
};

pub(crate) const MODEL_CACHE_CAPACITY: usize = 64;
pub(crate) const MODEL_CACHE_TTL: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct AgentProbeIdentity {
    pub(crate) agent_id: String,
    pub(crate) installation_id: String,
    pub(crate) definition_digest: String,
    pub(crate) enabled: bool,
    pub(crate) executable_present: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct CachedModelProbe {
    pub(crate) identity: AgentProbeIdentity,
    pub(crate) timestamp: Instant,
    pub(crate) last_accessed: Instant,
    pub(crate) result: AgentModelsResult,
}

#[derive(Clone, Debug)]
pub(crate) struct BoundedModelCache {
    entries: HashMap<String, CachedModelProbe>,
    capacity: usize,
}

impl BoundedModelCache {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            capacity,
        }
    }

    pub(crate) fn get(
        &mut self,
        agent_id: &str,
        identity: &AgentProbeIdentity,
    ) -> Option<AgentModelsResult> {
        if let Some(entry) = self.entries.get_mut(agent_id) {
            if entry.identity == *identity && entry.timestamp.elapsed() < MODEL_CACHE_TTL {
                entry.last_accessed = Instant::now();
                return Some(entry.result.clone());
            } else if entry.timestamp.elapsed() >= MODEL_CACHE_TTL || entry.identity != *identity {
                self.entries.remove(agent_id);
            }
        }
        None
    }

    pub(crate) fn insert(
        &mut self,
        agent_id: String,
        identity: AgentProbeIdentity,
        result: AgentModelsResult,
    ) {
        let now = Instant::now();
        if self.entries.len() >= self.capacity && !self.entries.contains_key(&agent_id) {
            if let Some(lru_key) = self
                .entries
                .iter()
                .min_by_key(|(_, v)| v.last_accessed)
                .map(|(k, _)| k.clone())
            {
                self.entries.remove(&lru_key);
            }
        }
        self.entries.insert(
            agent_id,
            CachedModelProbe {
                identity,
                timestamp: now,
                last_accessed: now,
                result,
            },
        );
    }

    pub(crate) fn remove(&mut self, agent_id: &str) {
        self.entries.remove(agent_id);
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SharedProbeError {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) retryable: bool,
    pub(crate) agent_id: String,
}

impl SharedProbeError {
    pub(crate) fn from_error(agent_id: &str, error: &AgentMarketError) -> Self {
        Self {
            code: error.code(),
            message: error.message(),
            retryable: error.retryable(),
            agent_id: agent_id.to_string(),
        }
    }

    pub(crate) fn into_error(self) -> AgentMarketError {
        AgentMarketError::new(&self.code, &self.message, self.retryable)
            .with_agent_id(self.agent_id)
    }
}

pub(crate) type SharedProbeResult = Result<Arc<AcpProbeReport>, SharedProbeError>;

pub(crate) struct ProbeFlight {
    pub(crate) result: tokio::sync::Mutex<Option<SharedProbeResult>>,
    pub(crate) notify: tokio::sync::Notify,
    pub(crate) cancellation: AiExecutionCancellation,
    pub(crate) callers: AtomicUsize,
    pub(crate) persist_health: AtomicBool,
}
