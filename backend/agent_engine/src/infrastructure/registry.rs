use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};

use crate::infrastructure::host_process::{
    resolve_host_executable, run_host_command_async, HostCommandSpec, HostInput, HostProcessError,
    HostProcessOutput,
};
use crate::infrastructure::registry_snapshot::RegistrySnapshot;

use crate::domain::definition::{AgentDefinition, AgentDefinitionError, AgentId};

#[derive(Debug)]
pub struct AgentRegistry {
    definitions: HashMap<AgentId, AgentDefinition>,
    observations: RwLock<HashMap<AgentId, AgentAvailability>>,
}

/// Atomically replaceable immutable registry handle used by executions and lifecycle reloads.
/// A caller always receives a cloned definition from one complete snapshot.
#[derive(Clone, Debug)]
pub struct AgentRegistryHandle {
    snapshot: Arc<RegistrySnapshot<AgentRegistry>>,
    generation: Arc<AtomicU64>,
}

impl Default for AgentRegistryHandle {
    fn default() -> Self {
        Self::from_registry(Arc::new(
            AgentRegistry::from_definitions(Vec::<AgentDefinition>::new())
                .expect("empty agent registry is valid"),
        ))
    }
}

impl AgentRegistryHandle {
    pub fn from_registry(registry: Arc<AgentRegistry>) -> Self {
        Self {
            snapshot: Arc::new(RegistrySnapshot::from_arc(registry)),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn from_snapshot(snapshot: Arc<RegistrySnapshot<AgentRegistry>>) -> Self {
        Self {
            snapshot,
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn bump_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn snapshot(&self) -> Arc<AgentRegistry> {
        self.snapshot.load()
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub fn publish(&self, definitions: Vec<AgentDefinition>) -> Result<u64, String> {
        let next =
            AgentRegistry::from_definitions(definitions).map_err(|error| error.to_string())?;
        self.snapshot.replace(next);
        Ok(self.bump_generation())
    }

    pub fn get(&self, agent_id: &AgentId) -> Option<AgentDefinition> {
        self.snapshot().get(agent_id).cloned()
    }

    pub fn definitions(&self) -> Vec<AgentDefinition> {
        self.snapshot().definitions()
    }

    pub async fn check_availability(&self, agent_id: &AgentId) -> AgentAvailability {
        self.snapshot().check_availability(agent_id).await
    }

    pub fn cached_availability(&self, agent_id: &AgentId) -> AgentAvailability {
        self.snapshot().cached_availability(agent_id)
    }

    pub async fn discover_models(
        &self,
        agent_id: &AgentId,
        timeout: Duration,
    ) -> Result<Vec<u8>, AgentProbeError> {
        self.snapshot().discover_models(agent_id, timeout).await
    }
}

const AVAILABILITY_TIMEOUT: Duration = Duration::from_secs(8);
const PROBE_STDOUT_CAP: usize = 1024 * 1024;
const PROBE_STDERR_CAP: usize = 256 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentAvailability {
    pub available: bool,
    pub installed: bool,
    pub version: Option<String>,
    pub error: Option<AgentProbeError>,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AgentProbeError {
    #[error("agent '{agent_id}' was not found")]
    AgentNotFound { agent_id: AgentId },
    #[error("agent '{agent_id}' has no {kind} probe")]
    ProbeNotConfigured {
        agent_id: AgentId,
        kind: &'static str,
    },
    #[error("{command_name} was not found on this host")]
    ExecutableNotFound { command_name: String },
    #[error("agent {kind} probe timed out")]
    Timeout { kind: &'static str },
    #[error("agent {kind} probe could not start")]
    SpawnFailed { kind: &'static str },
    #[error("agent {kind} probe output could not be read")]
    OutputFailed { kind: &'static str },
    #[error("agent {kind} probe exceeded its output limit")]
    OutputLimit { kind: &'static str },
    #[error("{}", match code { Some(c) => format!("agent {kind} probe exited with code {c}"), None => format!("agent {kind} probe exited unsuccessfully") })]
    ProbeFailed {
        kind: &'static str,
        code: Option<i32>,
    },
}

impl AgentRegistry {
    pub fn from_definitions<I>(definitions: I) -> Result<Self, AgentRegistryError>
    where
        I: IntoIterator<Item = AgentDefinition>,
    {
        let mut registry = Self {
            definitions: HashMap::new(),
            observations: RwLock::new(HashMap::new()),
        };

        let mut initial_observations = HashMap::new();
        for definition in definitions {
            definition
                .validate()
                .map_err(|source| AgentRegistryError::InvalidDefinition {
                    agent_id: definition.id.clone(),
                    source,
                })?;

            let agent_id = definition.id.clone();
            let is_command_available = resolve_host_executable(&definition.command).is_some();
            let is_probe_available = definition
                .availability_probe
                .as_ref()
                .map(|probe| {
                    let cmd = probe.command.as_deref().unwrap_or(&definition.command);
                    resolve_host_executable(cmd).is_some()
                })
                .unwrap_or(true);

            let initial_availability = if !is_command_available {
                AgentAvailability {
                    available: false,
                    installed: false,
                    version: None,
                    error: Some(AgentProbeError::ExecutableNotFound {
                        command_name: definition.command.clone(),
                    }),
                }
            } else if !is_probe_available {
                let cmd = definition
                    .availability_probe
                    .as_ref()
                    .and_then(|p| p.command.clone())
                    .unwrap_or_else(|| definition.command.clone());
                AgentAvailability {
                    available: false,
                    installed: false,
                    version: None,
                    error: Some(AgentProbeError::ExecutableNotFound { command_name: cmd }),
                }
            } else {
                AgentAvailability {
                    available: true,
                    installed: true,
                    version: None,
                    error: None,
                }
            };
            initial_observations.insert(agent_id.clone(), initial_availability);

            if registry
                .definitions
                .insert(agent_id.clone(), definition)
                .is_some()
            {
                return Err(AgentRegistryError::DuplicateId { agent_id });
            }
        }

        if let Ok(mut observations) = registry.observations.write() {
            *observations = initial_observations;
        }

        Ok(registry)
    }

    /// Returns an immutable definition owned by this registry.
    ///
    /// Definitions are fixed when the registry is constructed, so callers do
    /// not need to clone values merely to inspect routing or launch metadata.
    pub fn get(&self, agent_id: &AgentId) -> Option<&AgentDefinition> {
        self.definitions.get(agent_id)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    pub fn definitions(&self) -> Vec<AgentDefinition> {
        let mut definitions = self.definitions.values().cloned().collect::<Vec<_>>();
        definitions.sort_by_key(|definition| definition.id.to_string());
        definitions
    }

    pub async fn check_availability(&self, agent_id: &AgentId) -> AgentAvailability {
        let availability = match self.probe(agent_id, ProbeKind::Availability).await {
            Ok(output) => AgentAvailability {
                available: true,
                installed: true,
                version: first_nonempty_line(&output.stdout)
                    .or_else(|| first_nonempty_line(&output.stderr)),
                error: None,
            },
            Err(error) => AgentAvailability {
                available: false,
                installed: !matches!(error, AgentProbeError::ExecutableNotFound { .. }),
                version: None,
                error: Some(error),
            },
        };
        if let Ok(mut observations) = self.observations.write() {
            observations.insert(agent_id.clone(), availability.clone());
        }
        availability
    }

    pub async fn discover_models(
        &self,
        agent_id: &AgentId,
        timeout: Duration,
    ) -> Result<Vec<u8>, AgentProbeError> {
        self.execute_probe(agent_id, ProbeKind::ModelDiscovery, timeout)
            .await
            .map(|output| output.stdout)
    }

    pub fn observation(&self, agent_id: &AgentId) -> Option<AgentAvailability> {
        self.observations
            .read()
            .ok()
            .and_then(|observations| observations.get(agent_id).cloned())
    }

    pub fn cached_availability(&self, agent_id: &AgentId) -> AgentAvailability {
        if let Some(observation) = self.observation(agent_id) {
            return observation;
        }

        let Some(definition) = self.get(agent_id) else {
            return AgentAvailability {
                available: false,
                installed: false,
                version: None,
                error: Some(AgentProbeError::AgentNotFound {
                    agent_id: agent_id.clone(),
                }),
            };
        };

        if resolve_host_executable(&definition.command).is_none() {
            return AgentAvailability {
                available: false,
                installed: false,
                version: None,
                error: Some(AgentProbeError::ExecutableNotFound {
                    command_name: definition.command.clone(),
                }),
            };
        }

        if let Some(probe) = &definition.availability_probe {
            let command_name = probe.command.as_deref().unwrap_or(&definition.command);
            if resolve_host_executable(command_name).is_none() {
                return AgentAvailability {
                    available: false,
                    installed: false,
                    version: None,
                    error: Some(AgentProbeError::ExecutableNotFound {
                        command_name: command_name.to_string(),
                    }),
                };
            }
        }

        AgentAvailability {
            available: true,
            installed: true,
            version: None,
            error: None,
        }
    }

    async fn probe(
        &self,
        agent_id: &AgentId,
        kind: ProbeKind,
    ) -> Result<HostProcessOutput, AgentProbeError> {
        self.execute_probe(agent_id, kind, AVAILABILITY_TIMEOUT)
            .await
    }

    async fn execute_probe(
        &self,
        agent_id: &AgentId,
        kind: ProbeKind,
        timeout: Duration,
    ) -> Result<HostProcessOutput, AgentProbeError> {
        let definition = self
            .get(agent_id)
            .ok_or_else(|| AgentProbeError::AgentNotFound {
                agent_id: agent_id.clone(),
            })?;
        let probe = match kind {
            ProbeKind::Availability => definition.availability_probe.as_ref(),
            ProbeKind::ModelDiscovery => definition.model_discovery.as_ref(),
        }
        .ok_or_else(|| AgentProbeError::ProbeNotConfigured {
            agent_id: agent_id.clone(),
            kind: kind.as_str(),
        })?;
        if resolve_host_executable(&definition.command).is_none() {
            return Err(AgentProbeError::ExecutableNotFound {
                command_name: definition.command.clone(),
            });
        }
        let command_name = probe.command.as_deref().unwrap_or(&definition.command);
        let program = resolve_host_executable(command_name).ok_or_else(|| {
            AgentProbeError::ExecutableNotFound {
                command_name: command_name.to_string(),
            }
        })?;
        let spec = HostCommandSpec {
            program,
            args: probe.args.clone(),
            env: definition
                .env
                .iter()
                .map(|entry| (entry.name.clone(), entry.value.clone()))
                .collect(),
            working_dir: None,
            stdin: HostInput::Null,
            timeout,
            stdout_limit: PROBE_STDOUT_CAP,
            stderr_limit: PROBE_STDERR_CAP,
        };
        let output = run_host_command_async(spec, None)
            .await
            .map_err(|error| map_host_process_error(kind, error))?;
        if output.stdout_truncated || output.stderr_truncated {
            return Err(AgentProbeError::OutputLimit {
                kind: kind.as_str(),
            });
        }
        if !output.status.success() {
            return Err(AgentProbeError::ProbeFailed {
                kind: kind.as_str(),
                code: output.status.code(),
            });
        }
        Ok(HostProcessOutput {
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
            stdout_truncated: output.stdout_truncated,
            stderr_truncated: output.stderr_truncated,
        })
    }
}

#[derive(Clone, Copy)]
enum ProbeKind {
    Availability,
    ModelDiscovery,
}

impl ProbeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Availability => "availability",
            Self::ModelDiscovery => "model_discovery",
        }
    }
}

fn map_host_process_error(kind: ProbeKind, error: HostProcessError) -> AgentProbeError {
    match error {
        HostProcessError::Timeout { .. } => AgentProbeError::Timeout {
            kind: kind.as_str(),
        },
        HostProcessError::MissingProgram { .. } | HostProcessError::Spawn(_) => {
            AgentProbeError::SpawnFailed {
                kind: kind.as_str(),
            }
        }
        HostProcessError::Output(_)
        | HostProcessError::OutputLimitExceeded { .. }
        | HostProcessError::Cleanup(_) => AgentProbeError::OutputFailed {
            kind: kind.as_str(),
        },
        HostProcessError::Cancelled => AgentProbeError::OutputFailed {
            kind: kind.as_str(),
        },
    }
}

fn first_nonempty_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

impl AgentProbeError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::AgentNotFound { .. } => "agent_not_found",
            Self::ProbeNotConfigured { .. } => "probe_not_configured",
            Self::ExecutableNotFound { .. } => "command_not_found",
            Self::Timeout { .. } => "probe_timeout",
            Self::SpawnFailed { .. } => "spawn_failed",
            Self::OutputFailed { .. } => "probe_output_failed",
            Self::OutputLimit { .. } => "probe_output_limit",
            Self::ProbeFailed { .. } => "probe_failed",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AgentRegistryError {
    #[error("invalid definition for agent '{agent_id}': {source}")]
    InvalidDefinition {
        agent_id: AgentId,
        #[source]
        source: AgentDefinitionError,
    },
    #[error("duplicate agent id '{agent_id}'")]
    DuplicateId { agent_id: AgentId },
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
