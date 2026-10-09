use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionRef {
    pub schema_version: u32,
    pub value: String,
}

impl AgentSessionRef {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            schema_version: 1,
            value: value.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfoView {
    pub id: String,
    pub display_name: Option<String>,
    pub model: Option<String>,
    pub protocol: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionContextView {
    pub memory_scope: Option<String>,
    pub memory_job_id: Option<String>,
    pub task_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionTerminalView {
    pub state: String,
    pub code: Option<String>,
    pub message: Option<String>,
    pub retryable: bool,
}
