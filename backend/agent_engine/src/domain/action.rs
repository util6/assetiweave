use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, Hash, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActionId(String);

impl ActionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionRegistration {
    pub id: &'static str,
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
    ActionRegistration {
        id: "memory.generation",
    },
    ActionRegistration {
        id: "memory.project",
    },
    ActionRegistration {
        id: "memory.global",
    },
    ActionRegistration {
        id: "memory.recall",
    },
    ActionRegistration {
        id: "prompt.optimization",
    },
];

pub fn resolve_action(id: &ActionId) -> Result<&'static ActionRegistration, String> {
    ACTIONS
        .iter()
        .find(|registration| registration.id == id.as_str())
        .ok_or_else(|| format!("unknown action: {}", id.as_str()))
}

#[cfg(test)]
#[path = "action_tests.rs"]
mod tests;
