use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use super::outline_semantics::{SessionTurnFacts, SessionTurnRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConversationOutlineDetailLevel {
    #[default]
    Default,
    Questions,
    Qa,
    QaDigest,
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatrixView<T> {
    pub columns: Vec<String>,
    pub rows: Vec<T>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnCardRow {
    pub order: usize,
    pub card_id: String,
    pub kind: String,
    pub label: Option<String>,
    pub status: Option<String>,
    pub chars: usize,
    /// 仅 `kind == "subagent"` 的委派锚点非空：子会话的 8 位短 ID，可直接下钻大纲。
    pub child_session_id: Option<String>,
}

impl Serialize for TurnCardRow {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(7))?;
        seq.serialize_element(&self.order)?;
        seq.serialize_element(&self.card_id)?;
        seq.serialize_element(&self.kind)?;
        seq.serialize_element(&self.label)?;
        seq.serialize_element(&self.status)?;
        seq.serialize_element(&self.chars)?;
        seq.serialize_element(&self.child_session_id)?;
        seq.end()
    }
}

impl<'de> Deserialize<'de> for TurnCardRow {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = TurnCardRow;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a 7-element turn card row array")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let order = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                let card_id = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;
                let kind = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(2, &self))?;
                let label = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(3, &self))?;
                let status = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(4, &self))?;
                let chars = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(5, &self))?;
                let child_session_id = seq.next_element()?.unwrap_or(None);
                Ok(TurnCardRow {
                    order,
                    card_id,
                    kind,
                    label,
                    status,
                    chars,
                    child_session_id,
                })
            }
        }
        deserializer.deserialize_seq(Visitor)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeighborhoodCardRow {
    pub order: usize,
    pub card_id: String,
    pub kind: String,
    pub label: Option<String>,
    pub is_target: bool,
}

impl Serialize for NeighborhoodCardRow {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(5))?;
        seq.serialize_element(&self.order)?;
        seq.serialize_element(&self.card_id)?;
        seq.serialize_element(&self.kind)?;
        seq.serialize_element(&self.label)?;
        seq.serialize_element(&self.is_target)?;
        seq.end()
    }
}

impl<'de> Deserialize<'de> for NeighborhoodCardRow {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = NeighborhoodCardRow;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a 5-element neighborhood card row array")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let order = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                let card_id = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;
                let kind = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(2, &self))?;
                let label = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(3, &self))?;
                let is_target = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(4, &self))?;
                Ok(NeighborhoodCardRow {
                    order,
                    card_id,
                    kind,
                    label,
                    is_target,
                })
            }
        }
        deserializer.deserialize_seq(Visitor)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardTargetMeta {
    pub card_id: String,
    pub order: usize,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub chars: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOutline {
    pub session_id: String,
    pub source_id: String,
    pub total_turns: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// 仅当被查看的会话本身是 `Subagent Session` 时输出：父会话的 8 位短 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    /// 仅当被查看的会话本身是 `Subagent Session` 时输出：子代理语义角色。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_role: Option<String>,
    pub turns: MatrixView<SessionTurnRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnOutline {
    pub session_id: String,
    pub turn_id: String,
    pub turn_index: usize,
    pub total_turns: usize,
    pub user_question: String,
    #[serde(default)]
    pub started_at: Option<String>,
    pub cards: MatrixView<TurnCardRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardOutline {
    pub session_id: String,
    pub turn_id: String,
    pub turn_index: usize,
    pub turn_user_question: String,
    pub target: CardTargetMeta,
    pub neighborhood: MatrixView<NeighborhoodCardRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "level", rename_all = "snake_case")]
pub enum ConversationOutline {
    Session(SessionOutline),
    Turn(TurnOutline),
    Card(CardOutline),
    Batch {
        items: Vec<ConversationOutline>,
        truncated: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationCardDetail {
    pub card_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub content: String,
}
