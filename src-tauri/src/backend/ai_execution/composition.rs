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

#[derive(Clone, Copy, Debug)]
pub(crate) struct ActionRegistration {
    pub(crate) id: &'static str,
}

static ACTIONS: &[ActionRegistration] = &[
    ActionRegistration {
        id: "translation.card",
    },
    ActionRegistration {
        id: "translation.connection_test",
    },
    ActionRegistration {
        id: "translation.model_discovery",
    },
    ActionRegistration {
        id: "memory.extraction",
    },
    ActionRegistration { id: "memory.dream" },
    ActionRegistration {
        id: "prompt.optimization",
    },
];

pub(crate) fn resolve_action(id: &ActionId) -> Result<&'static ActionRegistration, AppError> {
    ACTIONS
        .iter()
        .find(|registration| registration.id == id.as_str())
        .ok_or_else(|| AppError::Validation(format!("unknown action: {}", id.as_str())))
}

pub(crate) fn resolve_agent_for(
    action: &ActionId,
    settings: &Value,
) -> Result<(AgentId, Option<String>), AppError> {
    resolve_action(action)?;
    let assignments = settings.get("agentAssignments").and_then(Value::as_object);
    let assignment = assignments
        .and_then(|values| values.get(action.as_str()))
        .and_then(Value::as_object)
        .ok_or_else(|| {
            AppError::Validation(format!("missing Agent assignment: {}", action.as_str()))
        })?;
    let configured_agent_id = assignment
        .get("agentId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let agent_id = AgentId::parse(configured_agent_id.ok_or_else(|| {
        AppError::Validation(format!("invalid Agent assignment: {}", action.as_str()))
    })?)
    .map_err(|error| AppError::Validation(error.to_string()))?;
    let model = assignment
        .get("modelId")
        .and_then(Value::as_str)
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
