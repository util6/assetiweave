//! Turn 级大纲行的语义模型：轮次种类、子代理角色与父轮次指针。

use serde::{Deserialize, Serialize};

use super::outline::ConversationOutlineDetailLevel;

/// 会话级大纲中的单行，按 `detail_level` 决定携带的列。
///
/// 序列化为位置数组（历史快照与 CLI 表格契约都依赖列序），
/// 反序列化同时兼容 3/4/5 元素的历史格式（缺少语义列时给 `None`）。
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

/// 构造会话级大纲行所需的 Turn 事实，与存储层结构解耦。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTurnFacts {
    pub turn_id: String,
    pub question: String,
    pub activities: Vec<String>,
    pub cards: usize,
    pub answer: Option<String>,
    pub turn_kind: Option<String>,
    pub agent_role: Option<String>,
    pub parent_turn_id: Option<String>,
}

impl SessionTurnRow {
    /// 会话级大纲的列名单：语义三列固定追加在末尾，既有列序保持不变。
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
        columns.extend(
            ["turn_kind", "agent_role", "parent_turn_id"]
                .into_iter()
                .map(str::to_string),
        );
        columns
    }

    /// 按 detail_level 把 Turn 事实映射成一行，语义三列只搬运一次。
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
            turn_kind,
            agent_role,
            parent_turn_id,
        } = facts;
        match detail_level {
            ConversationOutlineDetailLevel::Questions => Self::Questions {
                index,
                turn_id,
                question,
                turn_kind,
                agent_role,
                parent_turn_id,
            },
            ConversationOutlineDetailLevel::Qa | ConversationOutlineDetailLevel::QaDigest => {
                Self::Qa {
                    index,
                    turn_id,
                    question,
                    answer,
                    turn_kind,
                    agent_role,
                    parent_turn_id,
                }
            }
            ConversationOutlineDetailLevel::Full => Self::Full {
                index,
                turn_id,
                question,
                activities,
                cards,
                answer,
                turn_kind,
                agent_role,
                parent_turn_id,
            },
            ConversationOutlineDetailLevel::Default => Self::Default {
                index,
                turn_id,
                question,
                activities,
                cards,
                turn_kind,
                agent_role,
                parent_turn_id,
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
        let semantics = row_semantics(&values);
        match values.len() {
            // 历史格式：尚无子代理语义三列
            3 => Ok(Self::Questions {
                index: row_index(&values)?,
                turn_id: row_turn_id(&values)?,
                question: row_question(&values)?,
                turn_kind: None,
                agent_role: None,
                parent_turn_id: None,
            }),
            4 => Ok(Self::Qa {
                index: row_index(&values)?,
                turn_id: row_turn_id(&values)?,
                question: row_question(&values)?,
                answer: values[3].as_str().map(|s| s.to_string()),
                turn_kind: None,
                agent_role: None,
                parent_turn_id: None,
            }),
            5 => Ok(Self::Default {
                index: row_index(&values)?,
                turn_id: row_turn_id(&values)?,
                question: row_question(&values)?,
                activities: serde_json::from_value(values[3].clone())
                    .map_err(serde::de::Error::custom)?,
                cards: row_cards(&values)?,
                turn_kind: None,
                agent_role: None,
                parent_turn_id: None,
            }),
            6 => Ok(Self::Questions {
                index: row_index(&values)?,
                turn_id: row_turn_id(&values)?,
                question: row_question(&values)?,
                turn_kind: semantics.0,
                agent_role: semantics.1,
                parent_turn_id: semantics.2,
            }),
            7 => Ok(Self::Qa {
                index: row_index(&values)?,
                turn_id: row_turn_id(&values)?,
                question: row_question(&values)?,
                answer: values[3].as_str().map(|s| s.to_string()),
                turn_kind: semantics.0,
                agent_role: semantics.1,
                parent_turn_id: semantics.2,
            }),
            8 => Ok(Self::Default {
                index: row_index(&values)?,
                turn_id: row_turn_id(&values)?,
                question: row_question(&values)?,
                activities: serde_json::from_value(values[3].clone())
                    .map_err(serde::de::Error::custom)?,
                cards: row_cards(&values)?,
                turn_kind: semantics.0,
                agent_role: semantics.1,
                parent_turn_id: semantics.2,
            }),
            9 => Ok(Self::Full {
                index: row_index(&values)?,
                turn_id: row_turn_id(&values)?,
                question: row_question(&values)?,
                activities: serde_json::from_value(values[3].clone())
                    .map_err(serde::de::Error::custom)?,
                cards: row_cards(&values)?,
                answer: values[5].as_str().map(|s| s.to_string()),
                turn_kind: semantics.0,
                agent_role: semantics.1,
                parent_turn_id: semantics.2,
            }),
            len => Err(serde::de::Error::custom(format!(
                "invalid element count for session turn row: expected 3 to 9, got {len}"
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
/// 旧格式不足三列时对应位置给 `None`。
fn row_semantics(values: &[serde_json::Value]) -> (Option<String>, Option<String>, Option<String>) {
    if values.len() < 6 {
        return (None, None, None);
    }
    let offset = values.len() - 3;
    let optional = |idx: usize| {
        values
            .get(idx)
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
    };
    (optional(offset), optional(offset + 1), optional(offset + 2))
}

#[cfg(test)]
#[path = "outline_semantics_tests.rs"]
mod tests;
