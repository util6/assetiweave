use crate::backend::{agents::types::AgentId, runtime::AppError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Eq, Hash, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct ActionId(String);

impl ActionId {
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExecutionPolicyClass {
    InteractiveAssist,
    Diagnostics,
    BatchTransform,
    BackgroundAnalysis,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ActionRegistration {
    pub(crate) id: &'static str,
    pub(crate) policy: ExecutionPolicyClass,
    pub(crate) description: &'static str,
}

static ACTIONS: &[ActionRegistration] = &[
    ActionRegistration {
        id: "translation",
        policy: ExecutionPolicyClass::InteractiveAssist,
        description: "Translate a conversation card",
    },
    ActionRegistration {
        id: "translation.connection_test",
        policy: ExecutionPolicyClass::Diagnostics,
        description: "Check translation connectivity",
    },
    ActionRegistration {
        id: "translation.model_discovery",
        policy: ExecutionPolicyClass::Diagnostics,
        description: "Discover translation models",
    },
    ActionRegistration {
        id: "memory.extraction",
        policy: ExecutionPolicyClass::BackgroundAnalysis,
        description: "Extract memory candidates",
    },
    ActionRegistration {
        id: "memory.dream",
        policy: ExecutionPolicyClass::BackgroundAnalysis,
        description: "Generate memory dream notes",
    },
    ActionRegistration {
        id: "prompt_optimization",
        policy: ExecutionPolicyClass::InteractiveAssist,
        description: "Optimize a prompt",
    },
];

pub(crate) fn action_registrations() -> &'static [ActionRegistration] {
    ACTIONS
}

pub(crate) fn resolve_action(id: &ActionId) -> Result<&'static ActionRegistration, AppError> {
    ACTIONS
        .iter()
        .find(|registration| registration.id == id.as_str())
        .ok_or_else(|| AppError::Validation(format!("unknown action: {}", id.as_str())))
}

pub(crate) fn resolve_agent_for(action: &ActionId) -> Result<(AgentId, Option<String>), AppError> {
    resolve_action(action)?;
    let settings =
        crate::backend::app_settings::read_app_settings_value().map_err(AppError::Legacy)?;
    let runtime = settings.get("aiRuntime").and_then(Value::as_object);
    let assignments = settings
        .get("agentCapabilityAssignments")
        .and_then(Value::as_object);
    let configured = assignments
        .and_then(|values| values.get(action.as_str()))
        .or_else(|| {
            action
                .as_str()
                .starts_with("memory.")
                .then(|| assignments.and_then(|values| values.get("memory")))
                .flatten()
        })
        .or_else(|| assignments.and_then(|values| values.get("cardTranslation")))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let legacy_agent_id = match runtime
        .and_then(|value| value.get("cli"))
        .and_then(Value::as_str)
    {
        Some("gemini") => "gemini",
        _ => "opencode",
    };
    let configured_agent_id = configured.unwrap_or(legacy_agent_id);
    let agent_id = AgentId::parse(configured_agent_id)
        .map_err(|error| AppError::Validation(error.to_string()))?;
    let model = settings
        .get("agentModels")
        .and_then(Value::as_object)
        .and_then(|models| models.get(agent_id.as_str()))
        .and_then(Value::as_str)
        .or_else(|| {
            if configured.is_none() {
                runtime
                    .and_then(|value| value.get("model"))
                    .and_then(Value::as_str)
            } else {
                None
            }
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Ok((agent_id, model))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_action_fails_closed() {
        assert!(matches!(
            resolve_action(&ActionId::new("unknown")),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn registrations_are_unique() {
        let mut ids = ACTIONS.iter().map(|action| action.id).collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), ACTIONS.len());
    }
}
