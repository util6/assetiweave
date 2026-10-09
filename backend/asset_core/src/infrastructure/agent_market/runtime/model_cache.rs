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

pub const MODEL_CACHE_CAPACITY: usize = 64;
pub const MODEL_CACHE_TTL: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct AgentProbeIdentity {
    pub agent_id: String,
    pub installation_id: String,
    pub definition_digest: String,
    pub enabled: bool,
    pub executable_present: bool,
}

#[derive(Clone, Debug)]
pub struct CachedModelProbe {
    pub identity: AgentProbeIdentity,
    pub timestamp: Instant,
    pub last_accessed: Instant,
    pub result: AgentModelsResult,
}

#[derive(Clone, Debug)]
pub struct BoundedModelCache {
    entries: HashMap<String, CachedModelProbe>,
    capacity: usize,
}

impl BoundedModelCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            capacity,
        }
    }

    pub fn get(
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

    pub fn insert(
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

    pub fn remove(&mut self, agent_id: &str) {
        self.entries.remove(agent_id);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Clone, Debug)]
pub struct SharedProbeError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub agent_id: String,
}

impl SharedProbeError {
    pub fn from_error(agent_id: &str, error: &AgentMarketError) -> Self {
        Self {
            code: error.code(),
            message: error.message(),
            retryable: error.retryable(),
            agent_id: agent_id.to_string(),
        }
    }

    pub fn into_error(self) -> AgentMarketError {
        AgentMarketError::new(&self.code, &self.message, self.retryable)
            .with_agent_id(self.agent_id)
    }
}

pub type SharedProbeResult = Result<Arc<AcpProbeReport>, SharedProbeError>;

pub struct ProbeFlight {
    pub result: tokio::sync::Mutex<Option<SharedProbeResult>>,
    pub notify: tokio::sync::Notify,
    pub cancellation: AiExecutionCancellation,
    pub callers: AtomicUsize,
    pub persist_health: AtomicBool,
}
