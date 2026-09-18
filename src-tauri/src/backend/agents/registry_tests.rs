use super::*;
use crate::backend::agents::types::{
    AgentCommandDefinition, AgentDefinition, AgentId, AgentProtocol, DeclaredAgentCapabilities,
};

impl AgentRegistry {
    pub(crate) fn builtin() -> Result<Self, AgentRegistryError> {
        Self::from_definitions([
            builtin_agent(
                "opencode",
                "OpenCode",
                AgentProtocol::Acp,
                "opencode",
                ["acp"],
                "opencode",
            ),
            builtin_agent(
                "gemini",
                "Gemini CLI",
                AgentProtocol::Acp,
                "gemini",
                ["--acp"],
                "gemini",
            ),
            builtin_agent(
                "kiro",
                "Kiro",
                AgentProtocol::Acp,
                "kiro-cli-chat",
                ["acp"],
                "kiro-cli-chat",
            ),
            builtin_agent(
                "antigravity",
                "Antigravity",
                AgentProtocol::Acp,
                "antigravity-acp",
                [],
                "antigravity-acp",
            ),
            builtin_agent(
                "claude",
                "Claude Code",
                AgentProtocol::Acp,
                "npx",
                ["-y", "@agentclientprotocol/claude-agent-acp@0.58.1"],
                "claude",
            ),
            builtin_agent(
                "codex",
                "Codex CLI",
                AgentProtocol::Acp,
                "npx",
                ["-y", "@agentclientprotocol/codex-acp@1.1.2"],
                "codex",
            ),
            builtin_agent(
                "hermes",
                "Hermes",
                AgentProtocol::Acp,
                "hermes",
                ["acp"],
                "hermes",
            ),
            builtin_agent(
                "pi",
                "Pi",
                AgentProtocol::Acp,
                "npx",
                ["-y", "pi-acp@0.0.33"],
                "pi",
            ),
            builtin_agent(
                "qoder",
                "Qoder",
                AgentProtocol::Acp,
                "qodercli",
                ["--acp"],
                "qodercli",
            ),
        ])
    }
}

fn builtin_agent<const N: usize>(
    id: &str,
    display_name: &str,
    protocol: AgentProtocol,
    command: &str,
    args: [&str; N],
    availability_command: &str,
) -> AgentDefinition {
    AgentDefinition {
        id: AgentId::parse(id).expect("builtin agent ids are valid"),
        installation_id: None,
        display_name: display_name.to_string(),
        protocol,
        command: command.to_string(),
        args: args.into_iter().map(str::to_string).collect(),
        env: Vec::new(),
        declared_capabilities: DeclaredAgentCapabilities::acp_text(),
        availability_probe: Some(AgentCommandDefinition::with_command(
            availability_command,
            ["--version"],
        )),
        model_discovery: (id == "opencode").then(|| AgentCommandDefinition::new(["models"])),
        session_cleanup: (id == "opencode")
            .then(|| AgentCommandDefinition::new(["session", "delete", "{session_id}"])),
        session_cleanup_not_found_markers: (id == "opencode")
            .then(|| vec!["Session not found:".to_string()])
            .unwrap_or_default(),
    }
}

#[test]
fn builtin_registry_contains_the_requested_acp_agent_definitions() {
    let registry = AgentRegistry::builtin().expect("valid builtin registry");
    let definition = registry
        .get(&AgentId::parse("opencode").unwrap())
        .expect("OpenCode definition");

    assert_eq!(registry.len(), 9);
    assert_eq!(definition.command, "opencode");
    assert_eq!(definition.args, ["acp"]);
    assert_eq!(definition.protocol, AgentProtocol::Acp);
    assert!(definition.declared_capabilities.text_prompt);
    assert_eq!(
        definition
            .session_cleanup
            .as_ref()
            .expect("OpenCode must declare OneShot session cleanup")
            .args,
        ["session", "delete", "{session_id}"]
    );

    for (id, command, args, protocol) in [
        ("gemini", "gemini", vec!["--acp"], AgentProtocol::Acp),
        ("kiro", "kiro-cli-chat", vec!["acp"], AgentProtocol::Acp),
        ("antigravity", "antigravity-acp", vec![], AgentProtocol::Acp),
        (
            "claude",
            "npx",
            vec!["-y", "@agentclientprotocol/claude-agent-acp@0.58.1"],
            AgentProtocol::Acp,
        ),
        (
            "codex",
            "npx",
            vec!["-y", "@agentclientprotocol/codex-acp@1.1.2"],
            AgentProtocol::Acp,
        ),
        ("hermes", "hermes", vec!["acp"], AgentProtocol::Acp),
        ("pi", "npx", vec!["-y", "pi-acp@0.0.33"], AgentProtocol::Acp),
        ("qoder", "qodercli", vec!["--acp"], AgentProtocol::Acp),
    ] {
        let definition = registry
            .get(&AgentId::parse(id).unwrap())
            .unwrap_or_else(|| panic!("missing builtin Agent {id}"));
        assert_eq!(definition.command, command);
        assert_eq!(definition.args, args);
        assert_eq!(definition.protocol, protocol);
    }

    assert!(registry
        .catalog()
        .iter()
        .all(|entry| entry.protocol == "acp"));
    let antigravity_def = registry
        .get(&AgentId::parse("antigravity").unwrap())
        .expect("Antigravity definition");
    assert_eq!(antigravity_def.protocol, AgentProtocol::Acp);
    assert_eq!(antigravity_def.command, "antigravity-acp");
    assert!(antigravity_def.model_discovery.is_none());
    assert!(!antigravity_def.declared_capabilities.team_tools);
    assert!(antigravity_def.declared_capabilities.history_replay);
    assert!(antigravity_def.declared_capabilities.resume);
    assert!(antigravity_def.declared_capabilities.text_prompt);
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

#[tokio::test]
async fn reg_08_missing_executable_is_classified_as_not_found_and_observed() {
    let definition = probe_definition(
        "missing",
        "assetiweave-command-that-does-not-exist-019ff902",
        ["--version"],
        ["models"],
    );
    let registry = AgentRegistry::from_definitions([definition]).unwrap();
    let agent_id = AgentId::parse("missing").unwrap();

    let availability = registry.check_availability(&agent_id).await;

    assert!(!availability.available);
    assert!(!availability.installed);
    assert!(matches!(
        availability.error,
        Some(AgentProbeError::ExecutableNotFound { .. })
    ));
    assert_eq!(registry.observation(&agent_id), Some(availability));
}

#[tokio::test]
#[cfg(unix)]
async fn reg_09_probe_timeout_and_failure_have_distinct_classifications() {
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
        .await
        .unwrap_err();
    let failure = registry
        .check_availability(&AgentId::parse("failed").unwrap())
        .await;

    assert!(matches!(timeout_error, AgentProbeError::Timeout { .. }));
    assert!(matches!(
        failure.error,
        Some(AgentProbeError::ProbeFailed { code: Some(7), .. })
    ));
    assert!(failure.installed);
}

#[tokio::test]
#[cfg(unix)]
async fn reg_10_model_discovery_executes_definition_arguments() {
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
        .await
        .unwrap();

    assert_eq!(String::from_utf8(output).unwrap(), "model/z\nmodel/a\n");
}

fn definition(id: &str, protocol: AgentProtocol) -> AgentDefinition {
    AgentDefinition {
        id: AgentId::parse(id).unwrap(),
        installation_id: None,
        display_name: id.to_string(),
        protocol,
        command: "agent-command".to_string(),
        args: vec!["serve".to_string()],
        env: Vec::new(),
        declared_capabilities: DeclaredAgentCapabilities::acp_text(),
        availability_probe: Some(AgentCommandDefinition::new(["--version"])),
        model_discovery: None,
        session_cleanup: None,
        session_cleanup_not_found_markers: Vec::new(),
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
        installation_id: None,
        display_name: id.to_string(),
        protocol: AgentProtocol::Acp,
        command: command.to_string(),
        args: Vec::new(),
        env: Vec::new(),
        declared_capabilities: DeclaredAgentCapabilities::acp_text(),
        availability_probe: Some(AgentCommandDefinition::new(availability_args)),
        model_discovery: Some(AgentCommandDefinition::new(model_args)),
        session_cleanup: None,
        session_cleanup_not_found_markers: Vec::new(),
    }
}
