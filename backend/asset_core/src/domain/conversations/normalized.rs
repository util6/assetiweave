use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::serde_utils::deserialize_optional_metadata_json;
use super::{ConversationContentCardDescriptor, ConversationPartKind, ConversationPartRole};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct NormalizedConversationSession {
    pub external_id: String,
    pub title: Option<String>,
    pub project_path: Option<String>,
    pub started_at: Option<String>,
    pub updated_at: Option<String>,
    pub source_locator: Option<String>,
    pub source_fingerprint: Option<String>,
    pub turns: Vec<NormalizedConversationTurn>,
    #[serde(default)]
    pub execution_origin: Option<String>,
    #[serde(default)]
    pub execution_purpose: Option<String>,
    #[serde(default)]
    pub user_visible: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct NormalizedConversationTurn {
    pub external_id: String,
    pub turn_index: i64,
    pub user_text: String,
    pub title: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    pub parts: Vec<NormalizedConversationPart>,
    #[serde(default, alias = "execution_origin")]
    pub turn_kind: Option<String>,
    #[serde(default, alias = "subagent_name")]
    pub agent_role: Option<String>,
    #[serde(default, alias = "parent_turn_id", alias = "parent_external_id")]
    pub parent_external_id: Option<String>,
}

impl NormalizedConversationTurn {
    pub fn new(
        external_id: impl Into<String>,
        turn_index: i64,
        user_text: impl Into<String>,
    ) -> Self {
        Self {
            external_id: external_id.into(),
            turn_index,
            user_text: user_text.into(),
            ..Default::default()
        }
    }

    pub fn is_subagent(&self) -> bool {
        matches!(self.turn_kind.as_deref(), Some("subagent") | Some("agent"))
    }

    pub fn is_continuation(&self) -> bool {
        matches!(
            self.turn_kind.as_deref(),
            Some("continuation") | Some("main_continuation")
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NormalizedConversationPart {
    pub role: ConversationPartRole,
    pub kind: ConversationPartKind,
    pub text: Option<String>,
    pub language: Option<String>,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub status: Option<String>,
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub command_label: Option<String>,
    #[serde(default)]
    pub source_execution_id: Option<String>,
    #[serde(default, alias = "contentCard")]
    pub content_card: Option<ConversationContentCardDescriptor>,
    #[serde(default, deserialize_with = "deserialize_optional_metadata_json")]
    pub metadata_json: Option<String>,
}
