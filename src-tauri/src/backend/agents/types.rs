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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct DeclaredAgentCapabilities {
    pub(crate) text_prompt: bool,
}

impl DeclaredAgentCapabilities {
    pub(crate) fn acp_text() -> Self {
        Self { text_prompt: true }
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AgentDefinitionError {
    InvalidId(String),
    InvalidDisplayName(String),
    InvalidCommand(String),
    InvalidArgument {
        field: &'static str,
        index: usize,
        message: String,
    },
    InvalidEnvironment {
        index: usize,
        message: String,
    },
}

impl fmt::Display for AgentDefinitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(message)
            | Self::InvalidDisplayName(message)
            | Self::InvalidCommand(message) => formatter.write_str(message),
            Self::InvalidArgument {
                field,
                index,
                message,
            } => write!(
                formatter,
                "invalid {field} argument at index {index}: {message}"
            ),
            Self::InvalidEnvironment { index, message } => {
                write!(
                    formatter,
                    "invalid environment entry at index {index}: {message}"
                )
            }
        }
    }
}

impl std::error::Error for AgentDefinitionError {}

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
mod tests {
    use super::*;

    #[test]
    fn agent_id_accepts_the_documented_identifier_alphabet() {
        let id = AgentId::parse("open_code-2").expect("valid agent id");

        assert_eq!(id.as_str(), "open_code-2");
    }

    #[test]
    fn agent_id_rejects_empty_uppercase_path_and_oversized_values() {
        for value in [
            "",
            "OpenCode",
            "../opencode",
            "open code",
            "a/child",
            &"a".repeat(65),
        ] {
            assert!(
                AgentId::parse(value).is_err(),
                "accepted invalid id: {value}"
            );
        }
    }

    #[test]
    fn definition_rejects_an_empty_command() {
        let definition = definition_with_command("   ");

        assert!(matches!(
            definition.validate(),
            Err(AgentDefinitionError::InvalidCommand(_))
        ));
    }

    #[test]
    fn definition_rejects_nul_in_arguments() {
        let mut definition = definition_with_command("opencode");
        definition.args = vec!["acp\0unexpected".to_string()];

        assert!(matches!(
            definition.validate(),
            Err(AgentDefinitionError::InvalidArgument { index: 0, .. })
        ));
    }

    #[test]
    fn definition_rejects_invalid_environment_entries() {
        for entry in [
            AgentEnvEntry::new("", "value"),
            AgentEnvEntry::new("BAD=KEY", "value"),
            AgentEnvEntry::new("BAD\0KEY", "value"),
            AgentEnvEntry::new("GOOD_KEY", "bad\0value"),
        ] {
            let mut definition = definition_with_command("opencode");
            definition.env = vec![entry];
            assert!(matches!(
                definition.validate(),
                Err(AgentDefinitionError::InvalidEnvironment { index: 0, .. })
            ));
        }
    }

    #[test]
    fn session_cleanup_accepts_exactly_one_standalone_session_id_token() {
        let mut definition = definition_with_command("opencode");
        definition.session_cleanup = Some(AgentCommandDefinition::new([
            "session",
            "delete",
            "{session_id}",
        ]));

        definition
            .validate()
            .expect("a standalone session id token is a valid cleanup argv entry");
    }

    #[test]
    fn session_cleanup_rejects_unknown_embedded_missing_and_duplicate_tokens() {
        for args in [
            vec!["session", "delete", "{workspace}"],
            vec!["session", "delete", "session={session_id}"],
            vec!["session", "delete"],
            vec!["session", "delete", "{session_id}", "{session_id}"],
        ] {
            let mut definition = definition_with_command("opencode");
            definition.session_cleanup = Some(AgentCommandDefinition::new(args));

            assert!(matches!(
                definition.validate(),
                Err(AgentDefinitionError::InvalidArgument {
                    field: "session_cleanup",
                    ..
                })
            ));
        }
    }

    #[test]
    fn session_cleanup_rejects_a_shell_command_override() {
        let mut definition = definition_with_command("opencode");
        definition.session_cleanup = Some(AgentCommandDefinition::with_command(
            "sh",
            ["-c", "opencode session delete {session_id}"],
        ));

        assert!(definition.validate().is_err());
    }

    #[test]
    fn valid_acp_definition_passes_validation() {
        let definition = definition_with_command("opencode");

        definition.validate().expect("valid definition");
        assert_eq!(definition.protocol, AgentProtocol::Acp);
        assert!(definition.declared_capabilities.text_prompt);
    }

    fn definition_with_command(command: &str) -> AgentDefinition {
        AgentDefinition {
            id: AgentId::parse("opencode").unwrap(),
            installation_id: None,
            display_name: "OpenCode".to_string(),
            protocol: AgentProtocol::Acp,
            command: command.to_string(),
            args: vec!["acp".to_string()],
            env: Vec::new(),
            declared_capabilities: DeclaredAgentCapabilities::acp_text(),
            availability_probe: Some(AgentCommandDefinition::new(["--version"])),
            model_discovery: Some(AgentCommandDefinition::new(["models"])),
            session_cleanup: None,
            session_cleanup_not_found_markers: Vec::new(),
        }
    }
}
