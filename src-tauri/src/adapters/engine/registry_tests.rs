use super::*;
use std::collections::BTreeSet;

#[test]
fn registry_methods_are_unique() {
    let methods = command_specs()
        .iter()
        .map(|spec| spec.method)
        .collect::<BTreeSet<_>>();
    assert_eq!(methods.len(), command_specs().len());
}

#[test]
fn team_member_transport_methods_are_registered_with_shared_app_authority() {
    for (method, risk) in [
        ("team.member.turn.start", CommandRisk::Write),
        ("team.member.replay.start", CommandRisk::Read),
        ("team.member.stream.snapshot", CommandRisk::Read),
        ("team.member.task.get", CommandRisk::Read),
        ("team.member.tasks.list", CommandRisk::Read),
        ("team.member.turn.cancel", CommandRisk::Write),
    ] {
        let spec = find(method).unwrap_or_else(|| panic!("missing {method}"));
        assert_eq!(spec.exposure, CommandExposure::App);
        assert_eq!(spec.risk, risk);
    }
}

#[test]
fn system_version_is_registered_as_system_command() {
    let spec = find("system.version").expect("system.version spec");
    assert_eq!(spec.exposure, CommandExposure::System);
    assert_eq!(spec.risk, CommandRisk::Read);
}

#[test]
fn high_risk_schema_includes_confirmation() {
    let contract = schema_get("delete_source");
    assert_eq!(contract["risk"], json!("high-risk-write"));
    assert_eq!(
        contract["params_schema"]["properties"]["yes"]["type"],
        json!("boolean")
    );
}

#[test]
fn dry_run_bypasses_high_risk_confirmation() {
    let spec = find("source.remove").expect("source.remove spec");
    assert!(!requires_confirmation(spec, &json!({ "dry_run": true })));
    assert!(requires_confirmation(spec, &json!({ "id": "source-id" })));
    assert!(!requires_confirmation(
        spec,
        &json!({ "id": "source-id", "yes": true })
    ));
}

#[test]
fn unsupported_dry_run_cannot_bypass_high_risk_confirmation() {
    let spec = find("delete_source").expect("delete_source spec");
    assert!(!spec.supports_dry_run);
    assert!(requires_confirmation(
        spec,
        &json!({ "id": "source-id", "dry_run": true })
    ));
}

#[test]
fn runtime_validation_accepts_aliases_and_rejects_unknown_or_invalid_values() {
    let spec = find("source.add").expect("source.add spec");
    let normalized = validate_params(
        spec,
        &json!({
            "name": "skills",
            "kind": "local",
            "rootPath": "/tmp/skills",
            "includeGlobs": [],
            "excludeGlobs": [],
            "enabled": true,
            "priority": 1
        }),
    )
    .expect("aliases should validate");
    assert_eq!(normalized["root_path"], json!("/tmp/skills"));
    assert!(normalized.get("rootPath").is_none());

    let violations = validate_params(
        spec,
        &json!({
            "name": "skills",
            "root_path": "/tmp/skills",
            "priority": "first",
            "typo": true
        }),
    )
    .expect_err("invalid params should fail");
    assert!(violations
        .iter()
        .any(|violation| violation.code == "unknown_param"));
    assert!(violations
        .iter()
        .any(|violation| violation.code == "invalid_type"));
}

#[test]
fn runtime_validation_accepts_implicit_confirmation_param_once() {
    let spec = find("delete_source").expect("delete_source spec");
    assert!(validate_params(spec, &json!({ "id": "source-id", "yes": true })).is_ok());
}

#[test]
fn source_add_contract_required_fields_match_deserialization_type() {
    let contract = schema_get("source.add");
    let required = contract["params_schema"]["required"]
        .as_array()
        .expect("source.add required fields");

    for field in [
        "name",
        "kind",
        "root_path",
        "include_globs",
        "exclude_globs",
        "enabled",
        "priority",
    ] {
        assert!(
            required.contains(&json!(field)),
            "source.add contract omitted required serde field {field}"
        );
    }
}

#[test]
fn deployment_strategy_contract_matches_backend_model() {
    let contract = schema_get("set_asset_mount");
    assert_eq!(
        contract["params_schema"]["properties"]["strategy"]["enum"],
        json!([
            "symlink_to_source",
            "copy_to_target",
            "render",
            "append",
            "config_merge"
        ])
    );
}

#[test]
fn canonical_translation_method_keeps_request_contract_and_shared_handler() {
    let spec = find("conversation.card.translation.run").expect("translation method");
    assert_eq!(spec.canonical_method, "conversation.card.translation.run");
    assert_eq!(spec.risk, CommandRisk::Write);
    let contract = schema_get("conversation.card.translation.run");
    let required = contract["params_schema"]["required"]
        .as_array()
        .expect("translation required fields");
    for field in ["provider", "model", "prompt"] {
        assert!(required.contains(&json!(field)), "missing field {field}");
    }
    assert!(
        !required.contains(&json!("cli")),
        "legacy cli should remain optional when an Agent is selected"
    );
    assert_eq!(
        contract["params_schema"]["properties"]["agent_id"]["type"],
        json!(["string", "null"])
    );
    assert_eq!(
        contract["params_schema"]["properties"]["cli"]["enum"],
        json!(["opencode", "gemini"])
    );
}

#[test]
fn prompt_optimization_has_a_dedicated_public_contract() {
    let spec = find("prompt.optimization.run").expect("prompt optimization method");
    assert_eq!(spec.canonical_method, "prompt.optimization.run");
    assert_eq!(spec.risk, CommandRisk::Write);
    assert_eq!(
        find("optimize_prompt")
            .expect("prompt optimization Tauri method")
            .canonical_method,
        "prompt.optimization.run"
    );
    assert_eq!(
        find("check_prompt_optimization_availability")
            .expect("prompt optimization availability Tauri method")
            .canonical_method,
        "prompt.optimization.availability"
    );
    let contract = schema_get("prompt.optimization.run");
    let required = contract["params_schema"]["required"]
        .as_array()
        .expect("prompt optimization required fields");
    for field in ["provider", "model", "prompt"] {
        assert!(required.contains(&json!(field)), "missing field {field}");
    }
}

#[test]
fn committed_cli_contract_matches_registry() {
    let committed: Value = serde_json::from_str(include_str!(
        "../../../../cli/internal/schema/contract.json"
    ))
    .expect("parse committed CLI contract");
    assert_eq!(
        committed,
        schema_index(),
        "CLI contract drifted; run `pnpm cli:contract`"
    );
}
