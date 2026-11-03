use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize},
        Arc,
    },
    time::{Duration, Instant},
};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    domain::{AgentDefinition, AgentId},
    infrastructure::{
        backends::acp_probe_report::{AcpProbeReport, AcpProtocolConnectionOutcome},
        types::{AgentModelsResult, AiExecutionCancellation},
    },
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

impl AgentProbeIdentity {
    pub fn from_definition(definition: &AgentDefinition) -> Self {
        let mut digest = Sha256::new();
        digest.update(definition.id.as_str().as_bytes());
        digest.update([0]);
        if let Some(inst_id) = &definition.installation_id {
            digest.update(inst_id.as_bytes());
        }
        digest.update([0]);
        digest.update(definition.command.as_bytes());
        digest.update([0]);
        for arg in &definition.args {
            digest.update(arg.as_bytes());
            digest.update([0]);
        }
        let executable_present = std::path::Path::new(&definition.command).is_file();
        Self {
            agent_id: definition.id.to_string(),
            installation_id: definition.installation_id.clone().unwrap_or_default(),
            definition_digest: format!("{:x}", digest.finalize()),
            enabled: true,
            executable_present,
        }
    }
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
    pub fn new(agent_id: &str, code: &str, message: &str, retryable: bool) -> Self {
        Self {
            agent_id: agent_id.to_string(),
            code: code.to_string(),
            message: message.to_string(),
            retryable,
        }
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AgentProbeReport {
    pub available: bool,
    pub connected: bool,
    pub connection_method: String,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub models_result: Option<AgentModelsResult>,
}

impl AgentProbeReport {
    pub fn from_acp_report(agent_id: &AgentId, report: &AcpProbeReport) -> Self {
        let connected = report.protocol_connected();
        let (error_code, error_message) = match &report.protocol_connection {
            AcpProtocolConnectionOutcome::Connected => (None, None),
            AcpProtocolConnectionOutcome::Failed {
                error_code,
                error_message,
                ..
            } => (Some(error_code.clone()), Some(error_message.clone())),
            AcpProtocolConnectionOutcome::Cancelled => (
                Some("cancelled".to_string()),
                Some("The ACP probe was cancelled.".to_string()),
            ),
        };
        let models_result = if connected {
            Some(report.to_models_result(agent_id.as_str()))
        } else {
            None
        };
        Self {
            available: connected,
            connected,
            connection_method: "acp".to_string(),
            error_code,
            error_message,
            models_result,
        }
    }

    pub fn unavailable(agent_id: &AgentId, code: &str, message: &str) -> Self {
        Self {
            available: false,
            connected: false,
            connection_method: "acp".to_string(),
            error_code: Some(code.to_string()),
            error_message: Some(message.to_string()),
            models_result: Some(AgentModelsResult {
                agent_id: agent_id.to_string(),
                available: false,
                current_model_id: None,
                models: Vec::new(),
                error_code: Some(code.to_string()),
                error: Some(message.to_string()),
            }),
        }
    }
}
