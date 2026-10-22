use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatrixView<T> {
    pub columns: Vec<String>,
    pub rows: Vec<T>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTurnRow {
    pub index: usize,
    pub turn_id: String,
    pub question: String,
    pub activities: Vec<String>,
    pub cards: usize,
    pub answer_id: Option<String>,
}

impl Serialize for SessionTurnRow {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(6))?;
        seq.serialize_element(&self.index)?;
        seq.serialize_element(&self.turn_id)?;
        seq.serialize_element(&self.question)?;
        seq.serialize_element(&self.activities)?;
        seq.serialize_element(&self.cards)?;
        seq.serialize_element(&self.answer_id)?;
        seq.end()
    }
}

impl<'de> Deserialize<'de> for SessionTurnRow {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = SessionTurnRow;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a 6-element session turn row array")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let index = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                let turn_id = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;
                let question = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(2, &self))?;
                let activities = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(3, &self))?;
                let cards = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(4, &self))?;
                let answer_id = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(5, &self))?;
                Ok(SessionTurnRow {
                    index,
                    turn_id,
                    question,
                    activities,
                    cards,
                    answer_id,
                })
            }
        }
        deserializer.deserialize_seq(Visitor)
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
