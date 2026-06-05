use crate::command_registry::{CommandRisk, CommandSpec};
use globset::Glob;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{env, fs, path::Path};

const POLICY_VERSION: u32 = 1;
const DIAGNOSTIC_METHODS: &[&str] = &["system.version", "schema.list", "schema.get", "doctor.run"];

#[derive(Debug)]
pub(crate) struct PolicyFailure {
    pub(crate) kind: &'static str,
    pub(crate) message: String,
    pub(crate) details: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyDocument {
    version: u32,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    allow: Vec<String>,
    #[serde(default)]
    deny: Vec<String>,
    #[serde(default)]
    max_risk: Option<CommandRisk>,
}

pub(crate) fn authorize(spec: &CommandSpec) -> Result<(), PolicyFailure> {
    if DIAGNOSTIC_METHODS.contains(&spec.method) {
        return Ok(());
    }
    let Some(path) = env::var_os("ASSETIWEAVE_POLICY_PATH") else {
        return Ok(());
    };
    let path = Path::new(&path);
    let content =
        fs::read_to_string(path).map_err(|error| invalid_policy(path, error.to_string()))?;
    let policy: PolicyDocument =
        serde_json::from_str(&content).map_err(|error| invalid_policy(path, error.to_string()))?;
    if policy.version != POLICY_VERSION {
        return Err(invalid_policy(
            path,
            format!(
                "unsupported policy version {}; expected {POLICY_VERSION}",
                policy.version
            ),
        ));
    }
    validate_patterns(&policy, path)?;
    evaluate(&policy, spec, path)
}

fn validate_patterns(policy: &PolicyDocument, path: &Path) -> Result<(), PolicyFailure> {
    for pattern in policy.allow.iter().chain(&policy.deny) {
        Glob::new(pattern)
            .map_err(|error| invalid_policy(path, format!("invalid glob {pattern:?}: {error}")))?;
    }
    Ok(())
}

fn evaluate(policy: &PolicyDocument, spec: &CommandSpec, path: &Path) -> Result<(), PolicyFailure> {
    if let Some(pattern) = matching_pattern(&policy.deny, spec, path)? {
        return Err(denied(
            policy,
            spec,
            path,
            "deny_match",
            format!("command matched deny pattern {pattern}"),
        ));
    }
    if !policy.allow.is_empty() && matching_pattern(&policy.allow, spec, path)?.is_none() {
        return Err(denied(
            policy,
            spec,
            path,
            "not_allowed",
            "command did not match any allow pattern".to_string(),
        ));
    }
    if policy
        .max_risk
        .is_some_and(|max_risk| risk_rank(spec.risk) > risk_rank(max_risk))
    {
        return Err(denied(
            policy,
            spec,
            path,
            "risk_exceeds_max",
            format!(
                "command risk {} exceeds policy maximum {}",
                spec.risk.as_str(),
                policy.max_risk.expect("checked max risk").as_str()
            ),
        ));
    }
    Ok(())
}

fn matching_pattern(
    patterns: &[String],
    spec: &CommandSpec,
    path: &Path,
) -> Result<Option<String>, PolicyFailure> {
    for pattern in patterns {
        let matcher = Glob::new(pattern)
            .map_err(|error| invalid_policy(path, format!("invalid glob {pattern:?}: {error}")))?
            .compile_matcher();
        if matcher.is_match(spec.method) || matcher.is_match(spec.canonical_method) {
            return Ok(Some(pattern.clone()));
        }
    }
    Ok(None)
}

fn risk_rank(risk: CommandRisk) -> u8 {
    match risk {
        CommandRisk::Read => 0,
        CommandRisk::Write => 1,
        CommandRisk::HighRiskWrite => 2,
    }
}

fn denied(
    policy: &PolicyDocument,
    spec: &CommandSpec,
    path: &Path,
    reason_code: &'static str,
    reason: String,
) -> PolicyFailure {
    PolicyFailure {
        kind: "command_denied",
        message: format!("command denied by policy: {}", spec.method),
        details: json!({
            "method": spec.method,
            "canonical_method": spec.canonical_method,
            "risk": spec.risk,
            "policy_path": path,
            "policy_name": policy.name,
            "reason_code": reason_code,
            "reason": reason
        }),
    }
}

fn invalid_policy(path: &Path, message: String) -> PolicyFailure {
    PolicyFailure {
        kind: "policy_invalid",
        message: "command policy is invalid; refusing to run command".to_string(),
        details: json!({
            "policy_path": path,
            "reason_code": "policy_invalid",
            "reason": message
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_registry;
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
}
