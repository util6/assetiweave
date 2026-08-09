//! Engine 命令执行策略与安全鉴权模块
//!
//! 根据环境变量中指定的策略文件（`ASSETIWEAVE_POLICY_PATH`），对发起的 CLI / Engine 命令进行匹配与风险审查（包含 Glob 白名单、黑名单及最大允许风险等级校验）。

use super::registry::{CommandRisk, CommandSpec};
use globset::Glob;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{env, fs, path::Path};

/// 支持的策略文件版本号
const POLICY_VERSION: u32 = 1;
/// 诊断性免策略鉴权的白名单方法列表
const DIAGNOSTIC_METHODS: &[&str] = &["system.version", "schema.list", "schema.get", "doctor.run"];

/// 策略鉴权失败拒绝的错误详情描述
#[derive(Debug)]
pub(crate) struct PolicyFailure {
    /// 失败类型标识
    pub(crate) kind: &'static str,
    /// 错误可读消息
    pub(crate) message: String,
    /// 附加诊断上下文 JSON 对象
    pub(crate) details: Value,
}

/// 策略配置文档结构定义
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyDocument {
    /// 策略版本号
    version: u32,
    /// 策略名称标识
    #[serde(default)]
    name: Option<String>,
    /// 允许执行的方法通配符规则列表 (Glob)
    #[serde(default)]
    allow: Vec<String>,
    /// 明确拒绝执行的方法通配符规则列表 (Glob)
    #[serde(default)]
    deny: Vec<String>,
    /// 允许的最大风险等级 (CommandRisk)
    #[serde(default)]
    max_risk: Option<CommandRisk>,
}

/// 对指定的命令规范执行策略鉴权
///
/// 若未设置 `ASSETIWEAVE_POLICY_PATH` 环境变量，或方法属于免鉴权诊断方法，则默认放行。
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
}
