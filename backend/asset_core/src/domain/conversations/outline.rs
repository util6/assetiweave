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
    },
    Questions {
        index: usize,
        turn_id: String,
        question: String,
    },
    Qa {
        index: usize,
        turn_id: String,
        question: String,
        answer: Option<String>,
    },
    Full {
        index: usize,
        turn_id: String,
        question: String,
        activities: Vec<String>,
        cards: usize,
        answer: Option<String>,
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
            } => {
                let mut seq = serializer.serialize_seq(Some(3))?;
                seq.serialize_element(index)?;
                seq.serialize_element(turn_id)?;
                seq.serialize_element(question)?;
                seq.end()
            }
            Self::Qa {
                index,
                turn_id,
                question,
                answer,
            } => {
                let mut seq = serializer.serialize_seq(Some(4))?;
                seq.serialize_element(index)?;
                seq.serialize_element(turn_id)?;
                seq.serialize_element(question)?;
                seq.serialize_element(answer)?;
                seq.end()
            }
            Self::Default {
                index,
                turn_id,
                question,
                activities,
                cards,
            } => {
                let mut seq = serializer.serialize_seq(Some(5))?;
                seq.serialize_element(index)?;
                seq.serialize_element(turn_id)?;
                seq.serialize_element(question)?;
                seq.serialize_element(activities)?;
                seq.serialize_element(cards)?;
                seq.end()
            }
            Self::Full {
                index,
                turn_id,
                question,
                activities,
                cards,
                answer,
            } => {
                let mut seq = serializer.serialize_seq(Some(6))?;
                seq.serialize_element(index)?;
                seq.serialize_element(turn_id)?;
                seq.serialize_element(question)?;
                seq.serialize_element(activities)?;
                seq.serialize_element(cards)?;
                seq.serialize_element(answer)?;
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
            3 => {
                let index = values[0]
                    .as_u64()
                    .ok_or_else(|| serde::de::Error::custom("expected integer for index"))?
                    as usize;
                let turn_id = values[1]
                    .as_str()
                    .ok_or_else(|| serde::de::Error::custom("expected string for turn_id"))?
                    .to_string();
                let question = values[2]
                    .as_str()
                    .ok_or_else(|| serde::de::Error::custom("expected string for question"))?
                    .to_string();
                Ok(Self::Questions {
                    index,
                    turn_id,
                    question,
                })
            }
            4 => {
                let index = values[0]
                    .as_u64()
                    .ok_or_else(|| serde::de::Error::custom("expected integer for index"))?
                    as usize;
                let turn_id = values[1]
                    .as_str()
                    .ok_or_else(|| serde::de::Error::custom("expected string for turn_id"))?
                    .to_string();
                let question = values[2]
                    .as_str()
                    .ok_or_else(|| serde::de::Error::custom("expected string for question"))?
                    .to_string();
                let answer = values[3].as_str().map(|s| s.to_string());
                Ok(Self::Qa {
                    index,
                    turn_id,
                    question,
                    answer,
                })
            }
            5 => {
                let index = values[0]
                    .as_u64()
                    .ok_or_else(|| serde::de::Error::custom("expected integer for index"))?
                    as usize;
                let turn_id = values[1]
                    .as_str()
                    .ok_or_else(|| serde::de::Error::custom("expected string for turn_id"))?
                    .to_string();
                let question = values[2]
                    .as_str()
                    .ok_or_else(|| serde::de::Error::custom("expected string for question"))?
                    .to_string();
                let activities: Vec<String> =
                    serde_json::from_value(values[3].clone()).map_err(serde::de::Error::custom)?;
                let cards = values[4]
                    .as_u64()
                    .ok_or_else(|| serde::de::Error::custom("expected integer for cards"))?
                    as usize;
                Ok(Self::Default {
                    index,
                    turn_id,
                    question,
                    activities,
                    cards,
                })
            }
            6 => {
                let index = values[0]
                    .as_u64()
                    .ok_or_else(|| serde::de::Error::custom("expected integer for index"))?
                    as usize;
                let turn_id = values[1]
                    .as_str()
                    .ok_or_else(|| serde::de::Error::custom("expected string for turn_id"))?
                    .to_string();
                let question = values[2]
                    .as_str()
                    .ok_or_else(|| serde::de::Error::custom("expected string for question"))?
                    .to_string();
                let activities: Vec<String> =
                    serde_json::from_value(values[3].clone()).map_err(serde::de::Error::custom)?;
                let cards = values[4]
                    .as_u64()
                    .ok_or_else(|| serde::de::Error::custom("expected integer for cards"))?
                    as usize;
                let answer = values[5].as_str().map(|s| s.to_string());
                Ok(Self::Full {
                    index,
                    turn_id,
                    question,
                    activities,
                    cards,
                    answer,
                })
            }
            len => Err(serde::de::Error::custom(format!(
                "invalid element count for session turn row: expected 3, 4, 5, or 6, got {len}"
            ))),
        }
    }
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
