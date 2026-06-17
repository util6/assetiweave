use super::registry as command_registry;
use serde::Serialize;
use serde_json::{json, Value};
use std::time::Instant;

trait Hook {
    fn name(&self) -> &'static str;
    fn before(&self, invocation: &mut Invocation);
    fn after(&self, invocation: &mut Invocation);
}

struct TimingHook;

impl Hook for TimingHook {
    fn name(&self) -> &'static str {
        "runtime.timing"
    }

    fn before(&self, invocation: &mut Invocation) {
        invocation.started_at = Some(Instant::now());
    }

    fn after(&self, invocation: &mut Invocation) {
        invocation.duration_ms = invocation
            .started_at
            .map(|started_at| u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX));
    }
}

pub(crate) struct HookRegistry {
    hooks: Vec<Box<dyn Hook>>,
}

impl HookRegistry {
    fn engine() -> Self {
        Self {
            hooks: vec![Box::new(TimingHook)],
        }
    }

    fn before(&self, invocation: &mut Invocation) {
        for hook in &self.hooks {
            hook.before(invocation);
        }
    }

    fn after(&self, invocation: &mut Invocation) {
        for hook in &self.hooks {
            hook.after(invocation);
        }
    }

    fn names(&self) -> Vec<&'static str> {
        self.hooks.iter().map(|hook| hook.name()).collect()
    }
}

pub(crate) struct Invocation {
    method: String,
    canonical_method: Option<&'static str>,
    risk: Option<&'static str>,
    exposure: Option<&'static str>,
    outcome: &'static str,
    error_type: Option<String>,
    hooks: Vec<&'static str>,
    started_at: Option<Instant>,
    duration_ms: Option<u64>,
}

#[derive(Serialize)]
struct InvocationMeta<'a> {
    method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    canonical_method: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    risk: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exposure: Option<&'static str>,
    outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_type: Option<&'a str>,
    hooks: &'a [&'static str],
    duration_ms: u64,
}

pub(crate) fn before(method: &str) -> (HookRegistry, Invocation) {
    let spec = command_registry::find(method);
    let registry = HookRegistry::engine();
    let mut invocation = Invocation {
        method: method.to_string(),
        canonical_method: spec.map(|spec| spec.canonical_method),
        risk: spec.map(|spec| spec.risk.as_str()),
        exposure: spec.map(|spec| spec.exposure.as_str()),
        outcome: "running",
        error_type: None,
        hooks: registry.names(),
        started_at: None,
        duration_ms: None,
    };
    registry.before(&mut invocation);
    (registry, invocation)
}

pub(crate) fn after(
    registry: &HookRegistry,
    invocation: &mut Invocation,
    error_type: Option<&str>,
) {
    invocation.outcome = if error_type.is_some() {
        "error"
    } else {
        "success"
    };
    invocation.error_type = error_type.map(str::to_string);
    registry.after(invocation);
}

pub(crate) fn response_meta(invocation: &Invocation) -> Value {
    json!(InvocationMeta {
        method: &invocation.method,
        canonical_method: invocation.canonical_method,
        risk: invocation.risk,
        exposure: invocation.exposure,
        outcome: invocation.outcome,
        error_type: invocation.error_type.as_deref(),
        hooks: &invocation.hooks,
        duration_ms: invocation.duration_ms.unwrap_or(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postflight_metadata_is_emitted_for_failed_commands() {
        let (registry, mut invocation) = before("delete_source");
        after(&registry, &mut invocation, Some("command_denied"));
        let meta = response_meta(&invocation);

        assert_eq!(meta["canonical_method"], json!("source.remove"));
        assert_eq!(meta["risk"], json!("high-risk-write"));
        assert_eq!(meta["outcome"], json!("error"));
        assert_eq!(meta["error_type"], json!("command_denied"));
        assert_eq!(meta["hooks"], json!(["runtime.timing"]));
        assert!(meta["duration_ms"].is_u64());
    }
}
