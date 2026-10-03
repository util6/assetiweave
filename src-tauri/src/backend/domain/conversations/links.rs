use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationPartLink {
    pub part_id: String,
    pub relation: String,
    pub target_kind: String,
    pub target_id: String,
    #[serde(default)]
    pub metadata_json: Option<String>,
}

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
