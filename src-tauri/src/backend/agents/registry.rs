use std::{collections::HashMap, fmt, process::Command, sync::RwLock, time::Duration};

use crate::backend::host_process::{
    resolve_host_executable, run_command_with_timeout, HostProcessError, HostProcessOutput,
};

use super::types::{
    AgentCommandDefinition, AgentDefinition, AgentDefinitionError, AgentId, AgentProtocol,
    DeclaredAgentCapabilities,
};

#[derive(Debug)]
pub(crate) struct AgentRegistry {
    definitions: HashMap<AgentId, AgentDefinition>,
    observations: RwLock<HashMap<AgentId, AgentAvailability>>,
}

const AVAILABILITY_TIMEOUT: Duration = Duration::from_secs(8);
const PROBE_STDOUT_CAP: usize = 1024 * 1024;
const PROBE_STDERR_CAP: usize = 256 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentAvailability {
    pub(crate) available: bool,
    pub(crate) version: Option<String>,
    pub(crate) error: Option<AgentProbeError>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AgentProbeError {
    AgentNotFound {
        agent_id: AgentId,
    },
    ProbeNotConfigured {
        agent_id: AgentId,
        kind: &'static str,
    },
    ExecutableNotFound {
        command_name: String,
    },
    Timeout {
        kind: &'static str,
    },
    SpawnFailed {
        kind: &'static str,
    },
    OutputFailed {
        kind: &'static str,
    },
    OutputLimit {
        kind: &'static str,
    },
    ProbeFailed {
        kind: &'static str,
        code: Option<i32>,
    },
}

impl AgentRegistry {
    pub(crate) fn builtin() -> Result<Self, AgentRegistryError> {
        let opencode = AgentDefinition {
            id: AgentId::parse("opencode").map_err(AgentRegistryError::BuiltinDefinition)?,
            display_name: "OpenCode".to_string(),
            protocol: AgentProtocol::Acp,
            command: "opencode".to_string(),
            args: vec!["acp".to_string()],
            env: Vec::new(),
            declared_capabilities: DeclaredAgentCapabilities::acp_text(),
            availability_probe: Some(AgentCommandDefinition::new(["--version"])),
            model_discovery: Some(AgentCommandDefinition::new(["models"])),
        };

        Self::from_definitions([opencode])
    }

    pub(crate) fn from_definitions<I>(definitions: I) -> Result<Self, AgentRegistryError>
    where
        I: IntoIterator<Item = AgentDefinition>,
    {
        let mut registry = Self {
            definitions: HashMap::new(),
            observations: RwLock::new(HashMap::new()),
        };

        for definition in definitions {
            definition
                .validate()
                .map_err(|source| AgentRegistryError::InvalidDefinition {
                    agent_id: definition.id.clone(),
                    source,
                })?;

            let agent_id = definition.id.clone();
            if registry
                .definitions
                .insert(agent_id.clone(), definition)
                .is_some()
            {
                return Err(AgentRegistryError::DuplicateId { agent_id });
            }
        }

        Ok(registry)
    }

    /// Returns an immutable definition owned by this registry.
    ///
    /// Definitions are fixed when the registry is constructed, so callers do
    /// not need to clone values merely to inspect routing or launch metadata.
    pub(crate) fn get(&self, agent_id: &AgentId) -> Option<&AgentDefinition> {
        self.definitions.get(agent_id)
    }

    pub(crate) fn len(&self) -> usize {
        self.definitions.len()
    }

    pub(crate) fn check_availability(&self, agent_id: &AgentId) -> AgentAvailability {
        let availability = match self.probe(agent_id, ProbeKind::Availability) {
            Ok(output) => AgentAvailability {
                available: true,
                version: first_nonempty_line(&output.stdout)
                    .or_else(|| first_nonempty_line(&output.stderr)),
                error: None,
            },
            Err(error) => AgentAvailability {
                available: false,
                version: None,
                error: Some(error),
            },
        };
        if let Ok(mut observations) = self.observations.write() {
            observations.insert(agent_id.clone(), availability.clone());
        }
        availability
    }

    pub(crate) fn discover_models(
        &self,
        agent_id: &AgentId,
        timeout: Duration,
    ) -> Result<Vec<u8>, AgentProbeError> {
        self.execute_probe(agent_id, ProbeKind::ModelDiscovery, timeout)
            .map(|output| output.stdout)
    }

    #[cfg(test)]
    fn observation(&self, agent_id: &AgentId) -> Option<AgentAvailability> {
        self.observations
            .read()
            .ok()
            .and_then(|observations| observations.get(agent_id).cloned())
    }

    fn probe(
        &self,
        agent_id: &AgentId,
        kind: ProbeKind,
    ) -> Result<HostProcessOutput, AgentProbeError> {
        self.execute_probe(agent_id, kind, AVAILABILITY_TIMEOUT)
    }

    fn execute_probe(
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
        let program = resolve_host_executable(&definition.command).ok_or_else(|| {
            AgentProbeError::ExecutableNotFound {
                command_name: definition.command.clone(),
            }
        })?;
        let mut command = Command::new(program);
        command.args(&probe.args).envs(
            definition
                .env
                .iter()
                .map(|entry| (&entry.name, &entry.value)),
        );
        let output =
            run_command_with_timeout(&mut command, timeout, PROBE_STDOUT_CAP, PROBE_STDERR_CAP)
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
        Ok(output)
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
        HostProcessError::Spawn(_) => AgentProbeError::SpawnFailed {
            kind: kind.as_str(),
        },
        HostProcessError::Output(_) => AgentProbeError::OutputFailed {
            kind: kind.as_str(),
        },
        HostProcessError::Cancelled { .. } => AgentProbeError::OutputFailed {
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

impl fmt::Display for AgentProbeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AgentNotFound { agent_id } => {
                write!(formatter, "agent '{agent_id}' was not found")
            }
            Self::ProbeNotConfigured { agent_id, kind } => {
                write!(formatter, "agent '{agent_id}' has no {kind} probe")
            }
            Self::ExecutableNotFound { command_name } => {
                write!(formatter, "{command_name} was not found on this host")
            }
            Self::Timeout { kind } => write!(formatter, "agent {kind} probe timed out"),
            Self::SpawnFailed { kind } => write!(formatter, "agent {kind} probe could not start"),
            Self::OutputFailed { kind } => {
                write!(formatter, "agent {kind} probe output could not be read")
            }
            Self::OutputLimit { kind } => {
                write!(formatter, "agent {kind} probe exceeded its output limit")
            }
            Self::ProbeFailed {
                kind,
                code: Some(code),
            } => {
                write!(formatter, "agent {kind} probe exited with code {code}")
            }
            Self::ProbeFailed { kind, code: None } => {
                write!(formatter, "agent {kind} probe exited unsuccessfully")
            }
        }
    }
}

impl std::error::Error for AgentProbeError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AgentRegistryError {
    BuiltinDefinition(AgentDefinitionError),
    InvalidDefinition {
        agent_id: AgentId,
        source: AgentDefinitionError,
    },
    DuplicateId {
        agent_id: AgentId,
    },
}

impl fmt::Display for AgentRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BuiltinDefinition(source) => {
                write!(formatter, "invalid builtin agent definition: {source}")
            }
            Self::InvalidDefinition { agent_id, source } => {
                write!(
                    formatter,
                    "invalid definition for agent '{agent_id}': {source}"
                )
            }
            Self::DuplicateId { agent_id } => {
                write!(formatter, "duplicate agent id '{agent_id}'")
            }
        }
    }
}

impl std::error::Error for AgentRegistryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::BuiltinDefinition(source) | Self::InvalidDefinition { source, .. } => {
                Some(source)
            }
            Self::DuplicateId { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::agents::types::{
        AgentCommandDefinition, AgentDefinition, AgentId, AgentProtocol, DeclaredAgentCapabilities,
    };

    #[test]
    fn builtin_registry_contains_only_the_phase_one_opencode_definition() {
        let registry = AgentRegistry::builtin().expect("valid builtin registry");
        let definition = registry
            .get(&AgentId::parse("opencode").unwrap())
            .expect("OpenCode definition");

        assert_eq!(registry.len(), 1);
        assert_eq!(definition.command, "opencode");
        assert_eq!(definition.args, ["acp"]);
        assert_eq!(definition.protocol, AgentProtocol::Acp);
        assert!(definition.declared_capabilities.text_prompt);
    }

    #[test]
    fn unknown_agent_lookup_returns_none() {
        let registry = AgentRegistry::builtin().expect("valid builtin registry");

        assert!(registry
            .get(&AgentId::parse("missing-agent").unwrap())
            .is_none());
    }

    #[test]
    fn duplicate_agent_ids_fail_registry_construction() {
        let definition = definition("duplicate", AgentProtocol::Acp);

        let error = AgentRegistry::from_definitions([definition.clone(), definition])
            .expect_err("duplicate id must fail");

        assert!(matches!(error, AgentRegistryError::DuplicateId { .. }));
    }

    #[test]
    fn protocol_is_definition_data_and_not_inferred_from_a_vendor_id() {
        let registry = AgentRegistry::from_definitions([
            definition("alternate-acp", AgentProtocol::Acp),
            definition("opencode-native", AgentProtocol::Native),
        ])
        .expect("valid registry");

        assert_eq!(
            registry
                .get(&AgentId::parse("alternate-acp").unwrap())
                .unwrap()
                .protocol,
            AgentProtocol::Acp
        );
        assert_eq!(
            registry
                .get(&AgentId::parse("opencode-native").unwrap())
                .unwrap()
                .protocol,
            AgentProtocol::Native
        );
    }

    #[test]
    fn reg_08_missing_executable_is_classified_as_not_found_and_observed() {
        let definition = probe_definition(
            "missing",
            "assetiweave-command-that-does-not-exist-019ff902",
            ["--version"],
            ["models"],
        );
        let registry = AgentRegistry::from_definitions([definition]).unwrap();
        let agent_id = AgentId::parse("missing").unwrap();

        let availability = registry.check_availability(&agent_id);

        assert!(!availability.available);
        assert!(matches!(
            availability.error,
            Some(AgentProbeError::ExecutableNotFound { .. })
        ));
        assert_eq!(registry.observation(&agent_id), Some(availability));
    }

    #[test]
    #[cfg(unix)]
    fn reg_09_probe_timeout_and_failure_have_distinct_classifications() {
        let timeout = probe_definition(
            "timeout",
            "/bin/sh",
            ["-c", "printf version"],
            ["-c", "sleep 1"],
        );
        let failed = probe_definition("failed", "/bin/sh", ["-c", "exit 7"], ["-c", "exit 8"]);
        let registry = AgentRegistry::from_definitions([timeout, failed]).unwrap();

        let timeout_error = registry
            .discover_models(
                &AgentId::parse("timeout").unwrap(),
                Duration::from_millis(25),
            )
            .unwrap_err();
        let failure = registry.check_availability(&AgentId::parse("failed").unwrap());

        assert!(matches!(timeout_error, AgentProbeError::Timeout { .. }));
        assert!(matches!(
            failure.error,
            Some(AgentProbeError::ProbeFailed { code: Some(7), .. })
        ));
    }

    #[test]
    #[cfg(unix)]
    fn reg_10_model_discovery_executes_definition_arguments() {
        let definition = probe_definition(
            "discovery",
            "/bin/sh",
            ["-c", "printf version"],
            ["-c", "printf 'model/z\\nmodel/a\\n'"],
        );
        let registry = AgentRegistry::from_definitions([definition]).unwrap();

        let output = registry
            .discover_models(
                &AgentId::parse("discovery").unwrap(),
                Duration::from_secs(1),
            )
            .unwrap();

        assert_eq!(String::from_utf8(output).unwrap(), "model/z\nmodel/a\n");
    }

    fn definition(id: &str, protocol: AgentProtocol) -> AgentDefinition {
        AgentDefinition {
            id: AgentId::parse(id).unwrap(),
            display_name: id.to_string(),
            protocol,
            command: "agent-command".to_string(),
            args: vec!["serve".to_string()],
            env: Vec::new(),
            declared_capabilities: DeclaredAgentCapabilities::acp_text(),
            availability_probe: Some(AgentCommandDefinition::new(["--version"])),
            model_discovery: None,
        }
    }

    fn probe_definition<const A: usize, const M: usize>(
        id: &str,
        command: &str,
        availability_args: [&str; A],
        model_args: [&str; M],
    ) -> AgentDefinition {
        AgentDefinition {
            id: AgentId::parse(id).unwrap(),
            display_name: id.to_string(),
            protocol: AgentProtocol::Acp,
            command: command.to_string(),
            args: Vec::new(),
            env: Vec::new(),
            declared_capabilities: DeclaredAgentCapabilities::acp_text(),
            availability_probe: Some(AgentCommandDefinition::new(availability_args)),
            model_discovery: Some(AgentCommandDefinition::new(model_args)),
        }
    }
}
