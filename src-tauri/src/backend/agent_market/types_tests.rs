use super::*;

fn distribution() -> Distribution {
    Distribution::Npx {
        id: "npx-test".to_string(),
        priority: 30,
        package: "@scope/agent".to_string(),
        version: "1.2.3".to_string(),
        bin: "agent".to_string(),
        launch_args: vec!["acp".to_string()],
        node_range: Some(">=20".to_string()),
        model_discovery_args: None,
        session_cleanup_args: None,
        session_cleanup_not_found_markers: Vec::new(),
    }
}

fn item() -> CatalogItem {
    CatalogItem {
        id: "agent".to_string(),
        display_name: "Agent".to_string(),
        description: "A curated agent".to_string(),
        protocol: AgentMarketProtocol::Acp,
        version: "1.2.3".to_string(),
        core_compatibility: CoreCompatibility {
            min: "0.5.0".to_string(),
            max_exclusive: "0.6.0".to_string(),
        },
        capabilities: CatalogCapabilities {
            purposes: vec!["card_translation".to_string()],
            text_prompt: true,
            model_discovery: false,
            ..CatalogCapabilities::default()
        },
        verification: Verification {
            status: VerificationStatus::Tested,
            tested_at: "2026-08-16T00:00:00Z".to_string(),
            evidence_id: Some("fixture".to_string()),
        },
        upstream: UpstreamSource {
            registry_id: "agent".to_string(),
            homepage: "https://example.com".to_string(),
            license: "MIT".to_string(),
        },
        distributions: vec![distribution()],
    }
}

#[test]
fn protocol_and_distribution_are_independent_and_fixed() {
    let item = item();
    item.validate_basic().expect("valid catalog item");
    assert_eq!(item.protocol, AgentMarketProtocol::Acp);
    assert_eq!(
        item.distributions[0].distribution_type(),
        DistributionType::Npx
    );
    assert_eq!(item.distributions[0].ownership(), Ownership::Managed);
}

#[test]
fn catalog_capabilities_project_to_runtime_without_losing_richness() {
    let capabilities = CatalogCapabilities {
        text_prompt: true,
        resume: true,
        history_replay: true,
        live_events: true,
        rich_history_replay: true,
        team_tools: true,
        resume_args: Some(vec![
            "--conversation".to_string(),
            "{session_id}".to_string(),
        ]),
        ..CatalogCapabilities::default()
    };

    let declared = capabilities.to_declared_agent_capabilities(&AgentMarketProtocol::Native);

    assert!(declared.resume);
    assert!(declared.history_replay);
    assert!(declared.live_events);
    assert!(declared.rich_history_replay);
    assert_eq!(declared.resume_args, capabilities.resume_args);
}

#[test]
fn verification_status_uses_the_shared_trust_gate_without_collapsing_domain_states() {
    use crate::backend::extension_kernel::TrustGate;

    assert!(VerificationStatus::Tested.can_enable());
    assert!(!VerificationStatus::Tested.needs_confirmation());
    assert!(VerificationStatus::Experimental.can_enable());
    assert!(VerificationStatus::Experimental.needs_confirmation());
    assert!(!VerificationStatus::Experimental.integrity_changed());
}

#[test]
fn catalog_accepts_observed_versions_but_rejects_unsafe_paths() {
    let mut item = item();
    item.version = "release-2026.08-current".to_string();
    item.validate_basic()
        .expect("Agent version metadata is observational");
    item.distributions = vec![Distribution::Binary {
        id: "binary".to_string(),
        priority: 20,
        target: Target {
            os: "darwin".to_string(),
            arch: "aarch64".to_string(),
        },
        archive: "none".to_string(),
        url: "https://example.com/agent".to_string(),
        sha256: "a".repeat(64),
        size: None,
        executable: "../agent".to_string(),
        launch_args: Vec::new(),
        model_discovery_args: None,
        session_cleanup_args: None,
        session_cleanup_not_found_markers: Vec::new(),
    }];
    assert!(item.validate_basic().is_err());
}

#[test]
fn readiness_separates_installed_connected_and_execution_ready() {
    let mut installation = AgentInstallation {
        agent_id: "agent".to_string(),
        installation_id: "installation".to_string(),
        display_name: "Agent".to_string(),
        catalog_item_version: "1.0.0".to_string(),
        agent_version: "1.0.0".to_string(),
        protocol: AgentMarketProtocol::Acp,
        distribution_id: "system".to_string(),
        distribution_type: DistributionType::System,
        ownership: Ownership::System,
        install_dir: None,
        resolved_program: PathBuf::from("/usr/bin/agent"),
        args: Vec::new(),
        definition_json: serde_json::json!({}),
        integrity_json: None,
        source_registry: "agent".to_string(),
        catalog_version: "2026.08.16.1".to_string(),
        enabled: true,
        installation_status: InstallationStatus::Ready,
        runtime_status: RuntimeStatus::Ready,
        runtime_error_code: None,
        runtime_error_message: None,
        runtime_checked_at: None,
        protocol_status: ProtocolStatus::Failed,
        protocol_error_code: Some("acp_probe_failed".to_string()),
        protocol_error_message: None,
        protocol_checked_at: None,
        model_status: None,
        model_error_code: None,
        model_checked_at: None,
        installed_at: "now".to_string(),
        updated_at: "now".to_string(),
    };
    assert!(installation.installed());
    assert!(!installation.connected());
    assert!(!installation.execution_ready());

    installation.protocol_status = ProtocolStatus::AuthRequired;
    assert!(!installation.connected());
    assert!(!installation.execution_ready());

    installation.protocol_status = ProtocolStatus::Ready;
    assert!(installation.connected());
    assert!(installation.execution_ready());

    // Model status does not affect connected or execution_ready
    installation.model_status = Some("unsupported".to_string());
    assert!(installation.connected());
    assert!(installation.execution_ready());

    installation.model_status = Some("failed".to_string());
    assert!(installation.connected());
    assert!(installation.execution_ready());

    installation.model_status = Some("ready".to_string());
    assert!(installation.connected());
    assert!(installation.execution_ready());

    // Incompatible installation status falsifies connected and execution_ready
    installation.installation_status = InstallationStatus::Incompatible;
    assert!(!installation.connected());
    assert!(!installation.execution_ready());

    // Disabled installation falsifies connected and execution_ready
    installation.installation_status = InstallationStatus::Ready;
    installation.enabled = false;
    assert!(!installation.connected());
    assert!(!installation.execution_ready());
}

#[test]
fn package_manifest_projects_agent_invocation_and_probe_contracts() {
    let installation = AgentInstallation {
        agent_id: "agent".to_string(),
        installation_id: "installation".to_string(),
        display_name: "Agent".to_string(),
        catalog_item_version: "1.2.3".to_string(),
        agent_version: "release-2026.08-current".to_string(),
        protocol: AgentMarketProtocol::Acp,
        distribution_id: "binary".to_string(),
        distribution_type: DistributionType::Binary,
        ownership: Ownership::Managed,
        install_dir: Some(PathBuf::from("/tmp/agent-install")),
        resolved_program: PathBuf::from("/tmp/agent-install/bin/agent"),
        args: vec!["acp".to_string()],
        definition_json: serde_json::json!({
            "env": [{ "name": "TOKEN", "value": "fixture" }],
            "modelDiscoveryArgs": ["--models"],
            "capabilities": {
                "textPrompt": true,
                "resume": true,
                "historyReplay": true,
                "liveEvents": true,
                "richHistoryReplay": true,
                "teamTools": true
            }
        }),
        integrity_json: Some(serde_json::json!({ "sha256": "fixture" })),
        source_registry: "fixture".to_string(),
        catalog_version: "catalog-v1".to_string(),
        enabled: true,
        installation_status: InstallationStatus::Ready,
        runtime_status: RuntimeStatus::Ready,
        runtime_error_code: None,
        runtime_error_message: None,
        runtime_checked_at: None,
        protocol_status: ProtocolStatus::Ready,
        protocol_error_code: None,
        protocol_error_message: None,
        protocol_checked_at: None,
        model_status: None,
        model_error_code: None,
        model_checked_at: None,
        installed_at: "now".to_string(),
        updated_at: "now".to_string(),
    };

    let manifest = installation
        .package_manifest()
        .expect("agent package manifest");
    assert_eq!(manifest.identity.package_id, "agent");
    assert_eq!(manifest.identity.version, Version::new(0, 0, 0));
    assert_eq!(manifest.invocation.args, vec!["acp"]);
    assert_eq!(manifest.invocation.version_req, None);
    assert_eq!(manifest.invocation.env[0].key, "TOKEN");
    assert_eq!(manifest.availability_probe.args, vec!["--version"]);
    assert!(installation.catalog_capabilities().rich_history_replay);
    assert_eq!(
        manifest
            .model_discovery_probe
            .as_ref()
            .expect("model probe")
            .args,
        vec!["--models"]
    );
}

#[test]
fn agent_market_error_view_redacts_infrastructure_diagnostics() {
    let error = AgentMarketError::new(
        "uninstall_failed",
        "failed to remove /Users/util6/private-agent token=secret",
        true,
    )
    .with_details(Some(serde_json::json!({
        "path": "/Users/util6/private-agent",
        "token": "secret",
        "phase": "cleaning_up",
    })));

    let view = AgentMarketErrorView::from(&error);
    let serialized = serde_json::to_string(&view).unwrap();

    assert_eq!(view.message, "The operation failed.");
    assert_eq!(view.details.as_ref().unwrap()["path"], "<redacted>");
    assert!(view.details.as_ref().unwrap().get("token").is_none());
    assert_eq!(view.details.as_ref().unwrap()["phase"], "cleaning_up");
    assert!(!serialized.contains("/Users/util6"));
    assert!(!serialized.contains("secret"));
}

#[test]
fn catalog_item_uses_validator_for_common_fields() {
    let mut item = item();
    item.display_name = "   ".to_string();
    let err = item.validate_basic().unwrap_err();
    assert_eq!(
        err.message(),
        format!("invalid display name for {}", item.id)
    );

    item.display_name = "Valid Name".to_string();
    item.description = "a".repeat(501);
    let err = item.validate_basic().unwrap_err();
    assert_eq!(
        err.message(),
        format!("invalid description for {}", item.id)
    );

    item.description = "Valid description".to_string();
    item.version = "1.0\0.0".to_string();
    let err = item.validate_basic().unwrap_err();
    assert_eq!(
        err.message(),
        format!("invalid observed version for {}", item.id)
    );

    item.version = "1.0.0".to_string();
    item.distributions = vec![];
    let err = item.validate_basic().unwrap_err();
    assert_eq!(
        err.message(),
        format!("catalog item has no distributions: {}", item.id)
    );
}

#[test]
fn catalog_item_source_guard_ensures_manual_if_replaced_and_business_rules_preserved() {
    let source = include_str!("types.rs");
    assert!(!source.contains(concat!(
        "self.display_name",
        ".trim().is_empty() || self.display_name.len() > 120"
    )));
    assert!(!source.contains(concat!(
        "self.description",
        ".trim().is_empty() || self.description.len() > MAX_TEXT_BYTES"
    )));
    assert!(!source.contains(concat!(
        "self.version",
        ".trim().is_empty() || self.version.len() > 120"
    )));
    assert!(source.contains("if let Err(errors) = self.validate()"));
    // business rules preserved:
    assert!(source.contains("if !is_valid_id(&self.id)"));
    assert!(source.contains("duplicate distribution id"));
}
