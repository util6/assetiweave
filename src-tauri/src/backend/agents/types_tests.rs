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

#[test]
fn team_capability_gap_is_reported_in_stable_order() {
    let capabilities = DeclaredAgentCapabilities {
        resume: false,
        history_replay: true,
        live_events: false,
        ..DeclaredAgentCapabilities::default()
    };

    assert_eq!(
        capabilities.missing_team_capabilities(),
        vec!["resume", "live_events"]
    );
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
