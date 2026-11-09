//! Turn 级大纲行的位置契约：按 `detail_level` 决定携带的列，时间戳固定压在行尾。
//!
//! ADR-0022 之后子代理不再以伪轮次形式出现在父会话大纲里，`turn_kind` /
//! `agent_role` / `parent_turn_id` 三列已被物理删除；子代理的下钻入口改由
//! 委派锚点卡片与 `activities` 摘要承载。

use serde::{Deserialize, Serialize};

use super::outline::ConversationOutlineDetailLevel;

/// 会话级大纲行末尾追加的元数据列名。
pub const TURN_ROW_TRAILING_COLUMNS: &[&str] = &["started_at"];

/// 会话级大纲中的单行，按 `detail_level` 决定携带的列。
///
/// 序列化为位置数组（CLI 表格契约依赖列序）：Questions 4 列、Qa 5 列、
/// Default 6 列、Full 7 列，列名与 [`SessionTurnRow::columns`] 一一对应。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionTurnRow {
    Default {
        index: usize,
        turn_id: String,
        question: String,
        activities: Vec<String>,
        cards: usize,
        started_at: Option<String>,
    },
    Questions {
        index: usize,
        turn_id: String,
        question: String,
        started_at: Option<String>,
    },
    Qa {
        index: usize,
        turn_id: String,
        question: String,
        answer: Option<String>,
        started_at: Option<String>,
    },
    Full {
        index: usize,
        turn_id: String,
        question: String,
        activities: Vec<String>,
        cards: usize,
        answer: Option<String>,
        started_at: Option<String>,
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

    /// 轮次起始时间戳（RFC3339 字符串），空表示历史数据未记录。
    pub fn started_at(&self) -> Option<&str> {
        match self {
            Self::Default { started_at, .. }
            | Self::Questions { started_at, .. }
            | Self::Qa { started_at, .. }
            | Self::Full { started_at, .. } => started_at.as_deref(),
        }
    }
}

/// 构造会话级大纲行所需的 Turn 事实，与存储层结构解耦。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTurnFacts {
    pub turn_id: String,
    pub question: String,
    pub activities: Vec<String>,
    pub cards: usize,
    pub answer: Option<String>,
    pub started_at: Option<String>,
}

impl SessionTurnRow {
    /// 会话级大纲的列名单：时间戳固定追加在末尾，既有列序保持不变。
    pub fn columns(detail_level: ConversationOutlineDetailLevel) -> Vec<String> {
        let carries_process = matches!(
            detail_level,
            ConversationOutlineDetailLevel::Default | ConversationOutlineDetailLevel::Full
        );
        let carries_answer = matches!(
            detail_level,
            ConversationOutlineDetailLevel::Qa
                | ConversationOutlineDetailLevel::QaDigest
                | ConversationOutlineDetailLevel::Full
        );
        let mut columns: Vec<String> = vec!["index".into(), "turn_id".into(), "question".into()];
        if carries_process {
            columns.push("activities".into());
            columns.push("cards".into());
        }
        if carries_answer {
            columns.push("answer".into());
        }
        columns.extend(TURN_ROW_TRAILING_COLUMNS.iter().map(|&s| s.to_string()));
        columns
    }

    /// 按 detail_level 把 Turn 事实映射成一行。
    pub fn from_facts(
        detail_level: ConversationOutlineDetailLevel,
        index: usize,
        facts: SessionTurnFacts,
    ) -> Self {
        let SessionTurnFacts {
            turn_id,
            question,
            activities,
            cards,
            answer,
            started_at,
        } = facts;
        match detail_level {
            ConversationOutlineDetailLevel::Questions => Self::Questions {
                index,
                turn_id,
                question,
                started_at,
            },
            ConversationOutlineDetailLevel::Qa | ConversationOutlineDetailLevel::QaDigest => {
                Self::Qa {
                    index,
                    turn_id,
                    question,
                    answer,
                    started_at,
                }
            }
            ConversationOutlineDetailLevel::Full => Self::Full {
                index,
                turn_id,
                question,
                activities,
                cards,
                answer,
                started_at,
            },
            ConversationOutlineDetailLevel::Default => Self::Default {
                index,
                turn_id,
                question,
                activities,
                cards,
                started_at,
            },
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
                started_at,
            } => {
                let mut seq = serializer.serialize_seq(Some(4))?;
                seq.serialize_element(index)?;
                seq.serialize_element(turn_id)?;
                seq.serialize_element(question)?;
                seq.serialize_element(started_at)?;
                seq.end()
            }
            Self::Qa {
                index,
                turn_id,
                question,
                answer,
                started_at,
            } => {
                let mut seq = serializer.serialize_seq(Some(5))?;
                seq.serialize_element(index)?;
                seq.serialize_element(turn_id)?;
                seq.serialize_element(question)?;
                seq.serialize_element(answer)?;
                seq.serialize_element(started_at)?;
                seq.end()
            }
            Self::Default {
                index,
                turn_id,
                question,
                activities,
                cards,
                started_at,
            } => {
                let mut seq = serializer.serialize_seq(Some(6))?;
                seq.serialize_element(index)?;
                seq.serialize_element(turn_id)?;
                seq.serialize_element(question)?;
                seq.serialize_element(activities)?;
                seq.serialize_element(cards)?;
                seq.serialize_element(started_at)?;
                seq.end()
            }
            Self::Full {
                index,
                turn_id,
                question,
                activities,
                cards,
                answer,
                started_at,
            } => {
                let mut seq = serializer.serialize_seq(Some(7))?;
                seq.serialize_element(index)?;
                seq.serialize_element(turn_id)?;
                seq.serialize_element(question)?;
                seq.serialize_element(activities)?;
                seq.serialize_element(cards)?;
                seq.serialize_element(answer)?;
                seq.serialize_element(started_at)?;
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
        if !matches!(values.len(), 3..=7) {
            return Err(serde::de::Error::custom(format!(
                "invalid element count for session turn row: expected 3 to 7, got {}",
                values.len()
            )));
        }
        let index = row_index(&values)?;
        let turn_id = row_turn_id(&values)?;
        let question = row_question(&values)?;
        let tail = values.last().and_then(|value| value.as_str());

        match values.len() {
            3 => Ok(Self::Questions {
                index,
                turn_id,
                question,
                started_at: None,
            }),
            4 => Ok(Self::Questions {
                index,
                turn_id,
                question,
                started_at: tail.map(str::to_string),
            }),
            5 => Ok(Self::Qa {
                index,
                turn_id,
                question,
                answer: values[3].as_str().map(str::to_string),
                started_at: tail.map(str::to_string),
            }),
            6 => Ok(Self::Default {
                index,
                turn_id,
                question,
                activities: row_activities(&values)?,
                cards: row_cards(&values)?,
                started_at: tail.map(str::to_string),
            }),
            _ => Ok(Self::Full {
                index,
                turn_id,
                question,
                activities: row_activities(&values)?,
                cards: row_cards(&values)?,
                answer: values[5].as_str().map(str::to_string),
                started_at: tail.map(str::to_string),
            }),
        }
    }
}

fn row_activities<E: serde::de::Error>(values: &[serde_json::Value]) -> Result<Vec<String>, E> {
    serde_json::from_value(values[3].clone()).map_err(E::custom)
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
        .map(str::to_string)
        .ok_or_else(|| E::custom("expected string for turn_id"))
}

fn row_question<E: serde::de::Error>(values: &[serde_json::Value]) -> Result<String, E> {
    values[2]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| E::custom("expected string for question"))
}

fn row_cards<E: serde::de::Error>(values: &[serde_json::Value]) -> Result<usize, E> {
    values[4]
        .as_u64()
        .map(|value| value as usize)
        .ok_or_else(|| E::custom("expected integer for cards"))
}

#[cfg(test)]
#[path = "outline_semantics_tests.rs"]
mod tests;
