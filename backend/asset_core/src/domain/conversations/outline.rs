use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConversationOutlineDetailLevel {
    #[default]
    Default,
    Questions,
    Qa,
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatrixView<T> {
    pub columns: Vec<String>,
    pub rows: Vec<T>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionTurnRow {
    Default {
        index: usize,
        turn_id: String,
        question: String,
        activities: Vec<String>,
        cards: usize,
        turn_kind: Option<String>,
        agent_role: Option<String>,
        parent_turn_id: Option<String>,
    },
    Questions {
        index: usize,
        turn_id: String,
        question: String,
        turn_kind: Option<String>,
        agent_role: Option<String>,
        parent_turn_id: Option<String>,
    },
    Qa {
        index: usize,
        turn_id: String,
        question: String,
        answer: Option<String>,
        turn_kind: Option<String>,
        agent_role: Option<String>,
        parent_turn_id: Option<String>,
    },
    Full {
        index: usize,
        turn_id: String,
        question: String,
        activities: Vec<String>,
        cards: usize,
        answer: Option<String>,
        turn_kind: Option<String>,
        agent_role: Option<String>,
        parent_turn_id: Option<String>,
    },
}

impl SessionTurnRow {
    pub fn index(&self) -> usize {
        match self {
            Self::Default { index, .. }
            | Self::Questions { index, .. }
            | Self::Qa { index, .. }
            | Self::Full { index, .. } => *index,
        }
    }

    pub fn turn_id(&self) -> &str {
        match self {
            Self::Default { turn_id, .. }
            | Self::Questions { turn_id, .. }
            | Self::Qa { turn_id, .. }
            | Self::Full { turn_id, .. } => turn_id,
        }
    }

    pub fn question(&self) -> &str {
        match self {
            Self::Default { question, .. }
            | Self::Questions { question, .. }
            | Self::Qa { question, .. }
            | Self::Full { question, .. } => question,
        }
    }

    pub fn activities(&self) -> Option<&[String]> {
        match self {
            Self::Default { activities, .. } | Self::Full { activities, .. } => Some(activities),
            _ => None,
        }
    }

    pub fn cards(&self) -> Option<usize> {
        match self {
            Self::Default { cards, .. } | Self::Full { cards, .. } => Some(*cards),
            _ => None,
        }
    }

    pub fn answer(&self) -> Option<&str> {
        match self {
            Self::Qa { answer, .. } | Self::Full { answer, .. } => answer.as_deref(),
            _ => None,
        }
    }

    /// 轮次种类（如 `user`、`subagent`），空表示历史数据未标注。
    pub fn turn_kind(&self) -> Option<&str> {
        match self {
            Self::Default { turn_kind, .. }
            | Self::Questions { turn_kind, .. }
            | Self::Qa { turn_kind, .. }
            | Self::Full { turn_kind, .. } => turn_kind.as_deref(),
        }
    }

    /// 子代理语义角色，空表示主代理轮次或历史数据未标注。
    pub fn agent_role(&self) -> Option<&str> {
        match self {
            Self::Default { agent_role, .. }
            | Self::Questions { agent_role, .. }
            | Self::Qa { agent_role, .. }
            | Self::Full { agent_role, .. } => agent_role.as_deref(),
        }
    }

    /// 派发该轮次的父轮次短 ID，空表示顶层轮次。
    pub fn parent_turn_id(&self) -> Option<&str> {
        match self {
            Self::Default { parent_turn_id, .. }
            | Self::Questions { parent_turn_id, .. }
            | Self::Qa { parent_turn_id, .. }
            | Self::Full { parent_turn_id, .. } => parent_turn_id.as_deref(),
        }
    }
}

impl Serialize for SessionTurnRow {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        match self {
            Self::Questions {
                index,
                turn_id,
                question,
                turn_kind,
                agent_role,
                parent_turn_id,
            } => {
                let mut seq = serializer.serialize_seq(Some(6))?;
                seq.serialize_element(index)?;
                seq.serialize_element(turn_id)?;
                seq.serialize_element(question)?;
                seq.serialize_element(turn_kind)?;
                seq.serialize_element(agent_role)?;
                seq.serialize_element(parent_turn_id)?;
                seq.end()
            }
            Self::Qa {
                index,
                turn_id,
                question,
                answer,
                turn_kind,
                agent_role,
                parent_turn_id,
            } => {
                let mut seq = serializer.serialize_seq(Some(7))?;
                seq.serialize_element(index)?;
                seq.serialize_element(turn_id)?;
                seq.serialize_element(question)?;
                seq.serialize_element(answer)?;
                seq.serialize_element(turn_kind)?;
                seq.serialize_element(agent_role)?;
                seq.serialize_element(parent_turn_id)?;
                seq.end()
            }
            Self::Default {
                index,
                turn_id,
                question,
                activities,
                cards,
                turn_kind,
                agent_role,
                parent_turn_id,
            } => {
                let mut seq = serializer.serialize_seq(Some(8))?;
                seq.serialize_element(index)?;
                seq.serialize_element(turn_id)?;
                seq.serialize_element(question)?;
                seq.serialize_element(activities)?;
                seq.serialize_element(cards)?;
                seq.serialize_element(turn_kind)?;
                seq.serialize_element(agent_role)?;
                seq.serialize_element(parent_turn_id)?;
                seq.end()
            }
            Self::Full {
                index,
                turn_id,
                question,
                activities,
                cards,
                answer,
                turn_kind,
                agent_role,
                parent_turn_id,
            } => {
                let mut seq = serializer.serialize_seq(Some(9))?;
                seq.serialize_element(index)?;
                seq.serialize_element(turn_id)?;
                seq.serialize_element(question)?;
                seq.serialize_element(activities)?;
                seq.serialize_element(cards)?;
                seq.serialize_element(answer)?;
                seq.serialize_element(turn_kind)?;
                seq.serialize_element(agent_role)?;
                seq.serialize_element(parent_turn_id)?;
                seq.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for SessionTurnRow {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let values = Vec::<serde_json::Value>::deserialize(deserializer)?;
        match values.len() {
            6 => {
                let (turn_kind, agent_role, parent_turn_id) = row_semantics(&values, 3)?;
                Ok(Self::Questions {
                    index: row_index(&values)?,
                    turn_id: row_turn_id(&values)?,
                    question: row_question(&values)?,
                    turn_kind,
                    agent_role,
                    parent_turn_id,
                })
            }
            7 => {
                let (turn_kind, agent_role, parent_turn_id) = row_semantics(&values, 4)?;
                Ok(Self::Qa {
                    index: row_index(&values)?,
                    turn_id: row_turn_id(&values)?,
                    question: row_question(&values)?,
                    answer: values[3].as_str().map(|s| s.to_string()),
                    turn_kind,
                    agent_role,
                    parent_turn_id,
                })
            }
            8 => {
                let (turn_kind, agent_role, parent_turn_id) = row_semantics(&values, 5)?;
                Ok(Self::Default {
                    index: row_index(&values)?,
                    turn_id: row_turn_id(&values)?,
                    question: row_question(&values)?,
                    activities: serde_json::from_value(values[3].clone())
                        .map_err(serde::de::Error::custom)?,
                    cards: row_cards(&values)?,
                    turn_kind,
                    agent_role,
                    parent_turn_id,
                })
            }
            9 => {
                let (turn_kind, agent_role, parent_turn_id) = row_semantics(&values, 6)?;
                Ok(Self::Full {
                    index: row_index(&values)?,
                    turn_id: row_turn_id(&values)?,
                    question: row_question(&values)?,
                    activities: serde_json::from_value(values[3].clone())
                        .map_err(serde::de::Error::custom)?,
                    cards: row_cards(&values)?,
                    answer: values[5].as_str().map(|s| s.to_string()),
                    turn_kind,
                    agent_role,
                    parent_turn_id,
                })
            }
            len => Err(serde::de::Error::custom(format!(
                "invalid element count for session turn row: expected 6, 7, 8, or 9, got {len}"
            ))),
        }
    }
}

fn row_index<E: serde::de::Error>(values: &[serde_json::Value]) -> Result<usize, E> {
    values[0]
        .as_u64()
        .map(|value| value as usize)
        .ok_or_else(|| E::custom("expected integer for index"))
}

fn row_turn_id<E: serde::de::Error>(values: &[serde_json::Value]) -> Result<String, E> {
    values[1]
        .as_str()
        .map(|value| value.to_string())
        .ok_or_else(|| E::custom("expected string for turn_id"))
}

fn row_question<E: serde::de::Error>(values: &[serde_json::Value]) -> Result<String, E> {
    values[2]
        .as_str()
        .map(|value| value.to_string())
        .ok_or_else(|| E::custom("expected string for question"))
}

fn row_cards<E: serde::de::Error>(values: &[serde_json::Value]) -> Result<usize, E> {
    values[4]
        .as_u64()
        .map(|value| value as usize)
        .ok_or_else(|| E::custom("expected integer for cards"))
}

/// 读取行尾三列的子代理语义：轮次种类、角色、父轮次短 ID。
fn row_semantics<E: serde::de::Error>(
    values: &[serde_json::Value],
    offset: usize,
) -> Result<(Option<String>, Option<String>, Option<String>), E> {
    let optional = |idx: usize| values[idx].as_str().map(|value| value.to_string());
    Ok((optional(offset), optional(offset + 1), optional(offset + 2)))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnCardRow {
    pub order: usize,
    pub card_id: String,
    pub kind: String,
    pub label: Option<String>,
    pub status: Option<String>,
    pub chars: usize,
}

impl Serialize for TurnCardRow {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(6))?;
        seq.serialize_element(&self.order)?;
        seq.serialize_element(&self.card_id)?;
        seq.serialize_element(&self.kind)?;
        seq.serialize_element(&self.label)?;
        seq.serialize_element(&self.status)?;
        seq.serialize_element(&self.chars)?;
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
                formatter.write_str("a 6-element turn card row array")
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
                Ok(TurnCardRow {
                    order,
                    card_id,
                    kind,
                    label,
                    status,
                    chars,
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
    pub turn_kind: Option<String>,
    #[serde(default)]
    pub agent_role: Option<String>,
    #[serde(default)]
    pub parent_turn_id: Option<String>,
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
