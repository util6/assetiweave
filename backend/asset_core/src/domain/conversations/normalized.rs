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
    /// 宿主侧的临时轮次源键。
    ///
    /// 仅在单次会话导入期间用于在内存里把子代理轮次的 `parent_external_id`
    /// 解析成父轮次主键，不落库；宿主没有天然消息 ID 时留空即可。
    #[serde(default)]
    pub external_id: Option<String>,
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
            external_id: Some(external_id.into()),
            turn_index,
            user_text: user_text.into(),
            ..Default::default()
        }
    }

    /// 供没有天然轮次源键的宿主构造输入。
    pub fn positional(turn_index: i64, user_text: impl Into<String>) -> Self {
        Self {
            turn_index,
            user_text: user_text.into(),
            ..Default::default()
        }
    }

    /// 归一后的临时源键：空白字符串视同没有源键。
    pub fn source_key(&self) -> Option<&str> {
        self.external_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
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

/// 轮次开始时间的兜底链。
///
/// 轮次原生时间 → 会话更新时间 → 会话开始时间 → 本次导入时间，确保落库值恒为非空：
/// 消费方（如 Memory）需要靠它识别回滚重聊后新产生的轮次。
pub fn resolve_turn_started_at(
    turn_started_at: Option<&str>,
    session_updated_at: Option<&str>,
    session_started_at: Option<&str>,
    imported_at: &str,
) -> String {
    [
        turn_started_at,
        session_updated_at,
        session_started_at,
        Some(imported_at),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|value| !value.is_empty())
    .unwrap_or(imported_at)
    .to_string()
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
