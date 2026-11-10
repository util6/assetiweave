//! 子代理会话溯源与父轮次委派锚点的纯语义。
//!
//! ADR-0022 之后子代理不再拆成父会话里的伪轮次，而是两份互相引用的记录：
//! - **子代理会话**：独立的 `conversation_sessions` 行（`user_visible = 0`、
//!   `execution_origin = "subagent"`），父子关系放在 `execution_purpose` 的 JSON 里；
//! - **委派锚点**：父轮次内 `kind = "subagent"` 的轻量 Part，结构化属性放在
//!   `attrs_json`（读模型里的 `metadata_json`）里。
//!
//! 本模块只做解析与格式化，不碰存储，也不缓存任何东西——调用方拿到的永远是纯函数结果。

use serde_json::Value;

use super::grouping::conversation_id_fragment;
use super::ConversationSession;

/// 子代理会话在 `conversation_sessions.execution_origin` 上的固定取值。
pub const SUBAGENT_EXECUTION_ORIGIN: &str = "subagent";

/// activities 摘要里最多列出的子代理数量，超出部分折叠成 `...`。
const MAX_LISTED_ANCHORS: usize = 3;

/// 子代理会话的父会话归属，由 `execution_purpose` JSON 解析而来。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentSessionLink {
    pub parent_session_id: String,
    pub agent_role: Option<String>,
    pub parent_turn_index: Option<i64>,
}

impl SubagentSessionLink {
    /// 从会话的落库字段解析父会话归属；非子代理会话或溯源信息缺失时返回 `None`。
    pub fn from_session(execution_origin: &str, execution_purpose: Option<&str>) -> Option<Self> {
        if !execution_origin.eq_ignore_ascii_case(SUBAGENT_EXECUTION_ORIGIN) {
            return None;
        }
        let purpose = parse_object(execution_purpose?)?;
        let object = purpose.as_object()?;
        let parent_session_id = string_field(object, "parent_session_id")?;
        Some(Self {
            parent_session_id,
            agent_role: string_field(object, "agent_role"),
            parent_turn_index: object.get("parent_turn_index").and_then(Value::as_i64),
        })
    }

    /// 父会话的 8 位短 ID，与大纲里的 `session_id` 保持同一种身份形态。
    pub fn short_parent_session_id(&self) -> String {
        conversation_id_fragment(&self.parent_session_id)
    }
}

/// 便捷判定：会话行是子代理会话时返回其父会话归属。
pub fn subagent_session_link(session: &ConversationSession) -> Option<SubagentSessionLink> {
    SubagentSessionLink::from_session(
        &session.execution_origin,
        session.execution_purpose.as_deref(),
    )
}

/// 父轮次里的一个子代理委派锚点，由锚点 Part 的 `attrs_json` 解析而来。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubagentDispatchAnchor {
    pub child_session_id: Option<String>,
    pub agent_role: Option<String>,
    pub title: Option<String>,
}

impl SubagentDispatchAnchor {
    /// 从锚点 Part 的 `attrs_json` 解析委派信息；没有任何可用字段时返回 `None`。
    pub fn from_metadata(metadata_json: Option<&str>) -> Option<Self> {
        let metadata = parse_object(metadata_json?)?;
        let object = metadata.as_object()?;
        let anchor = Self {
            child_session_id: string_field(object, "child_session_id"),
            agent_role: string_field(object, "agent_role"),
            title: string_field(object, "title"),
        };
        (anchor.child_session_id.is_some() || anchor.agent_role.is_some() || anchor.title.is_some())
            .then_some(anchor)
    }

    /// 子会话的 8 位短 ID，供 `outline <child>` 直接下钻。
    pub fn short_child_session_id(&self) -> Option<String> {
        self.child_session_id
            .as_deref()
            .map(conversation_id_fragment)
    }

    /// 锚点在 activities 里的角色与短 ID；缺子会话 ID 的锚点无法下钻，直接跳过。
    fn role_and_id(&self) -> Option<(String, String)> {
        let short_id = self.short_child_session_id()?;
        Some((self.agent_role.clone().unwrap_or_default(), short_id))
    }
}

/// 把一轮内的多个锚点折叠成一段 activities 摘要。
///
/// - 单个：`Subagent(<role> -> <short_id>)`，角色为空时写 `Subagent(-> <short_id>)`
/// - 多个：`Subagent x<N>(<role1> -> <id1>, <role2> -> <id2>, ...)`，最多列 3 个
pub fn subagent_activity_summary(anchors: &[SubagentDispatchAnchor]) -> Option<String> {
    let mut pairs: Vec<(String, String)> = Vec::new();
    for anchor in anchors {
        let Some(pair) = anchor.role_and_id() else {
            continue;
        };
        if !pairs.iter().any(|existing| existing.1 == pair.1) {
            pairs.push(pair);
        }
    }
    match pairs.len() {
        0 => None,
        1 => {
            let (role, short_id) = &pairs[0];
            Some(format!("Subagent({})", pair_label(role, short_id)))
        }
        count => {
            let listed = pairs
                .iter()
                .take(MAX_LISTED_ANCHORS)
                .map(|(role, short_id)| pair_label(role, short_id))
                .collect::<Vec<_>>()
                .join(", ");
            let ellipsis = if count > MAX_LISTED_ANCHORS {
                ", ..."
            } else {
                ""
            };
            Some(format!("Subagent x{count}({listed}{ellipsis})"))
        }
    }
}

fn pair_label(role: &str, short_id: &str) -> String {
    if role.is_empty() {
        format!("-> {short_id}")
    } else {
        format!("{role} -> {short_id}")
    }
}

fn string_field(object: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_object(raw: &str) -> Option<Value> {
    let value = serde_json::from_str::<Value>(raw.trim()).ok()?;
    value.is_object().then_some(value)
}

#[cfg(test)]
#[path = "subagent_tests.rs"]
mod tests;
