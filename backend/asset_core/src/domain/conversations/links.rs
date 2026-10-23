use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ConversationContentCardDescriptor {
    #[serde(alias = "schemaVersion")]
    pub schema_version: u32,
    pub kind: String,
    #[serde(default, alias = "semanticRole")]
    pub semantic_role: Option<String>,
    #[serde(default)]
    pub renderer: Option<String>,
}
