use std::{fmt, hash::Hash};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const MAX_AGENT_ID_BYTES: usize = 64;
const MAX_DISPLAY_NAME_BYTES: usize = 120;
pub(crate) const SESSION_ID_PLACEHOLDER: &str = "{session_id}";

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct AgentId(String);

impl AgentId {
    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, AgentDefinitionError> {
        let value = value.into();
        let value = value.trim();
        let valid = !value.is_empty()
            && value.len() <= MAX_AGENT_ID_BYTES
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
            });
        if !valid {
            return Err(AgentDefinitionError::InvalidId(
                "agent id must contain 1 to 64 lowercase ASCII letters, digits, '-' or '_'"
                    .to_string(),
            ));
        }
        Ok(Self(value.to_string()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AgentProtocol {
    Acp,
    Native,
}

impl AgentProtocol {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Acp => "acp",
            Self::Native => "native",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentEnvEntry {
    pub(crate) name: String,
    pub(crate) value: String,
}

impl AgentEnvEntry {
    pub(crate) fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }

    fn validate(&self, index: usize) -> Result<(), AgentDefinitionError> {
        if self.name.is_empty() || self.name.contains(['=', '\0']) || self.value.contains('\0') {
            return Err(AgentDefinitionError::InvalidEnvironment {
                index,
                message: "environment names must be non-empty and neither names nor values may contain NUL; names may not contain '='"
                    .to_string(),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct AgentCommandDefinition {
    pub(crate) command: Option<String>,
    pub(crate) args: Vec<String>,
}

impl AgentCommandDefinition {
    pub(crate) fn new<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            command: None,
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    pub(crate) fn with_command<I, S>(command: impl Into<String>, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            command: Some(command.into()),
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    fn validate(&self, field: &'static str) -> Result<(), AgentDefinitionError> {
        if let Some(command) = self.command.as_deref() {
            if command.trim().is_empty() || command.contains('\0') {
                return Err(AgentDefinitionError::InvalidCommand(format!(
                    "{field} command must be non-empty and may not contain NUL"
                )));
            }
        }
        validate_arguments(&self.args, field)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct DeclaredAgentCapabilities {
    pub(crate) text_prompt: bool,
    pub(crate) resume: bool,
    pub(crate) history_replay: bool,
    /// The Agent can publish live Session Events for the current turn.
    pub(crate) live_events: bool,
    /// The Agent can replay thought/tool history with provider fidelity.
    pub(crate) rich_history_replay: bool,
    pub(crate) team_tools: bool,
    pub(crate) resume_args: Option<Vec<String>>,
}

impl DeclaredAgentCapabilities {
    pub(crate) fn acp_text() -> Self {
        Self {
            text_prompt: true,
            resume: true,
            history_replay: true,
            live_events: true,
            rich_history_replay: false,
            team_tools: false,
            resume_args: None,
        }
    }

    pub(crate) fn native_text_with_resume(resume_args: Vec<String>) -> Self {
        Self {
            text_prompt: true,
            resume: true,
            history_replay: false,
            live_events: false,
            rich_history_replay: false,
            team_tools: false,
            resume_args: Some(resume_args),
        }
    }

    pub(crate) fn missing_team_capabilities(&self) -> Vec<&'static str> {
        [
            ("resume", self.resume),
            ("history_replay", self.history_replay),
            ("live_events", self.live_events),
        ]
        .into_iter()
        .filter_map(|(name, available)| (!available).then_some(name))
        .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentDefinition {
    pub(crate) id: AgentId,
    /// Identity of the installation snapshot that produced this definition.
    /// Built-in test definitions may leave this unset; persisted Agent Market
    /// definitions always carry the current installation identity.
    pub(crate) installation_id: Option<String>,
    pub(crate) display_name: String,
    pub(crate) protocol: AgentProtocol,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<AgentEnvEntry>,
    pub(crate) declared_capabilities: DeclaredAgentCapabilities,
    pub(crate) availability_probe: Option<AgentCommandDefinition>,
    pub(crate) model_discovery: Option<AgentCommandDefinition>,
    pub(crate) session_cleanup: Option<AgentCommandDefinition>,
    pub(crate) session_cleanup_not_found_markers: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct AgentCatalogEntry {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) availability_command: String,
    pub(crate) protocol: String,
    pub(crate) capabilities: DeclaredAgentCapabilities,
}

impl AgentCatalogEntry {
    pub(crate) fn from_definition(definition: &AgentDefinition) -> Self {
        let availability_command = definition
            .availability_probe
            .as_ref()
            .and_then(|probe| probe.command.as_deref())
            .unwrap_or(&definition.command);
        Self {
            id: definition.id.to_string(),
            display_name: definition.display_name.clone(),
            command: definition.command.clone(),
            args: definition.args.clone(),
            availability_command: availability_command.to_string(),
            protocol: definition.protocol.as_str().to_string(),
            capabilities: definition.declared_capabilities.clone(),
        }
    }
}

impl AgentDefinition {
    pub(crate) fn validate(&self) -> Result<(), AgentDefinitionError> {
        let display_name = self.display_name.trim();
        if display_name.is_empty()
            || display_name.len() > MAX_DISPLAY_NAME_BYTES
            || display_name.contains('\0')
        {
            return Err(AgentDefinitionError::InvalidDisplayName(
                "agent display name must contain 1 to 120 bytes and may not contain NUL"
                    .to_string(),
            ));
        }
        if self.command.trim().is_empty() || self.command.contains('\0') {
            return Err(AgentDefinitionError::InvalidCommand(
                "agent command must be non-empty and may not contain NUL".to_string(),
            ));
        }
        validate_arguments(&self.args, "args")?;
        for (index, entry) in self.env.iter().enumerate() {
            entry.validate(index)?;
        }
        if let Some(probe) = &self.availability_probe {
            probe.validate("availability_probe")?;
        }
        if let Some(discovery) = &self.model_discovery {
            discovery.validate("model_discovery")?;
        }
        if let Some(resume_args) = &self.declared_capabilities.resume_args {
            let mut placeholder_index = None;
            for (index, arg) in resume_args.iter().enumerate() {
                if arg == SESSION_ID_PLACEHOLDER {
                    if placeholder_index.is_some() {
                        return Err(AgentDefinitionError::InvalidArgument {
                            field: "resume_args",
                            index,
                            message: "must contain exactly one standalone {session_id} argument"
                                .to_string(),
                        });
                    }
                    placeholder_index = Some(index);
                } else if arg.contains(['{', '}']) {
                    return Err(AgentDefinitionError::InvalidArgument {
                        field: "resume_args",
                        index,
                        message: "only a standalone {session_id} placeholder is allowed"
                            .to_string(),
                    });
                }
            }
            if placeholder_index.is_none() {
                return Err(AgentDefinitionError::InvalidArgument {
                    field: "resume_args",
                    index: resume_args.len(),
                    message: "must contain exactly one standalone {session_id} argument"
                        .to_string(),
                });
            }
        }
        if let Some(cleanup) = &self.session_cleanup {
            if cleanup.command.is_some() {
                return Err(AgentDefinitionError::InvalidCommand(
                    "session_cleanup must reuse the Agent command".to_string(),
                ));
            }
            cleanup.validate("session_cleanup")?;
            let mut placeholder_index = None;
            for (index, arg) in cleanup.args.iter().enumerate() {
                if arg == SESSION_ID_PLACEHOLDER {
                    if placeholder_index.is_some() {
                        return Err(AgentDefinitionError::InvalidArgument {
                            field: "session_cleanup",
                            index,
                            message: "must contain exactly one standalone {session_id} argument"
                                .to_string(),
                        });
                    }
                    placeholder_index = Some(index);
                } else if arg.contains(['{', '}']) {
                    return Err(AgentDefinitionError::InvalidArgument {
                        field: "session_cleanup",
                        index,
                        message: "only a standalone {session_id} placeholder is allowed"
                            .to_string(),
                    });
                }
            }
            if placeholder_index.is_none() {
                return Err(AgentDefinitionError::InvalidArgument {
                    field: "session_cleanup",
                    index: cleanup.args.len(),
                    message: "must contain exactly one standalone {session_id} argument"
                        .to_string(),
                });
            }
        }
        if self
            .session_cleanup_not_found_markers
            .iter()
            .any(|marker| marker.is_empty() || marker.contains('\0'))
        {
            return Err(AgentDefinitionError::InvalidCommand(
                "session cleanup not-found markers must be non-empty and may not contain NUL"
                    .to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AgentConnectionCheckMode {
    Installation,
    Connection,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub(crate) struct AgentConnectionCheckRequest {
    pub(crate) agent_id: String,
    pub(crate) mode: AgentConnectionCheckMode,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct AgentConnectionResult {
    pub(crate) agent_id: String,
    pub(crate) available: bool,
    pub(crate) installed: bool,
    pub(crate) connected: bool,
    pub(crate) version: Option<String>,
    pub(crate) connection_method: Option<String>,
    pub(crate) error_code: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) installation_status: Option<String>,
    pub(crate) runtime_status: Option<String>,
    pub(crate) protocol_status: Option<String>,
    pub(crate) execution_ready: bool,
    pub(crate) health_stale: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct AgentModelOption {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) description: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct AgentModelsResult {
    pub(crate) agent_id: String,
    pub(crate) available: bool,
    pub(crate) models: Vec<AgentModelOption>,
    pub(crate) current_model_id: Option<String>,
    pub(crate) error_code: Option<String>,
    pub(crate) error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub(crate) struct AgentModelsRequest {
    pub(crate) agent_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum AgentDefinitionError {
    #[error("{0}")]
    InvalidId(String),
    #[error("{0}")]
    InvalidDisplayName(String),
    #[error("{0}")]
    InvalidCommand(String),
    #[error("invalid {field} argument at index {index}: {message}")]
    InvalidArgument {
        field: &'static str,
        index: usize,
        message: String,
    },
    #[error("invalid environment entry at index {index}: {message}")]
    InvalidEnvironment { index: usize, message: String },
}

fn validate_arguments(args: &[String], field: &'static str) -> Result<(), AgentDefinitionError> {
    for (index, argument) in args.iter().enumerate() {
        if argument.contains('\0') {
            return Err(AgentDefinitionError::InvalidArgument {
                field,
                index,
                message: "arguments may not contain NUL".to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
