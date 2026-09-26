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
fn catalog_item_uses_validator_for_common_fields() {
    let mut item = item();
    item.display_name = "   ".to_string();
    let err = item.validate_basic().unwrap_err();
    assert_eq!(
        err,
        CatalogValidationError::InvalidDisplayName(item.id.clone())
    );

    item.display_name = "Valid Name".to_string();
    item.description = "a".repeat(501);
    let err = item.validate_basic().unwrap_err();
    assert_eq!(
        err,
        CatalogValidationError::InvalidDescription(item.id.clone())
    );

    item.description = "Valid description".to_string();
    item.version = "1.0\0.0".to_string();
    let err = item.validate_basic().unwrap_err();
    assert_eq!(err, CatalogValidationError::InvalidVersion(item.id.clone()));

    item.version = "1.0.0".to_string();
    item.distributions = vec![];
    let err = item.validate_basic().unwrap_err();
    assert_eq!(
        err,
        CatalogValidationError::NoDistributions(item.id.clone())
    );
}

#[test]
fn catalog_item_source_guard_ensures_manual_if_replaced_and_business_rules_preserved() {
    let source = concat!(include_str!("catalog_item.rs"), include_str!("types.rs"));
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
