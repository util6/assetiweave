use super::super::registry as command_registry;
use super::*;
use std::path::PathBuf;

fn policy(json: &str) -> PolicyDocument {
    serde_json::from_str(json).expect("parse policy")
}

#[test]
fn deny_pattern_matches_canonical_method_alias() {
    let spec = command_registry::find("delete_source").expect("delete_source");
    let error = evaluate(
        &policy(r#"{"version":1,"deny":["source.*"]}"#),
        spec,
        &PathBuf::from("policy.json"),
    )
    .expect_err("canonical method should match deny");
    assert_eq!(error.kind, "command_denied");
    assert_eq!(error.details["reason_code"], json!("deny_match"));
}

#[test]
fn allow_list_and_max_risk_fail_closed() {
    let spec = command_registry::find("skill.delete").expect("skill.delete");
    let error = evaluate(
        &policy(r#"{"version":1,"allow":["skill.*"],"max_risk":"write"}"#),
        spec,
        &PathBuf::from("policy.json"),
    )
    .expect_err("high-risk write should exceed policy");
    assert_eq!(error.details["reason_code"], json!("risk_exceeds_max"));
}

#[test]
fn every_policy_pattern_is_validated_before_evaluation() {
    let error = validate_patterns(
        &policy(r#"{"version":1,"deny":["source.*"],"allow":["["]}"#),
        &PathBuf::from("policy.json"),
    )
    .expect_err("invalid unused pattern should fail policy validation");
    assert_eq!(error.kind, "policy_invalid");
}
