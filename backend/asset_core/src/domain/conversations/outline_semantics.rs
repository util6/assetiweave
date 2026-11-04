//! Turn 级大纲行的语义模型：轮次种类、子代理角色与父轮次指针。

use serde::{Deserialize, Serialize};

use super::outline::ConversationOutlineDetailLevel;

/// 会话级大纲行末尾追加的语义元数据列名。
pub const TURN_ROW_SEMANTIC_COLUMNS: &[&str] =
    &["turn_kind", "agent_role", "parent_turn_id", "started_at"];

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
        started_at: Option<String>,
    },
    Questions {
        index: usize,
        turn_id: String,
        question: String,
        turn_kind: Option<String>,
        agent_role: Option<String>,
        parent_turn_id: Option<String>,
        started_at: Option<String>,
    },
    Qa {
        index: usize,
        turn_id: String,
        question: String,
        answer: Option<String>,
        turn_kind: Option<String>,
        agent_role: Option<String>,
        parent_turn_id: Option<String>,
        started_at: Option<String>,
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
    pub turn_kind: Option<String>,
    pub agent_role: Option<String>,
    pub parent_turn_id: Option<String>,
    pub started_at: Option<String>,
}

impl SessionTurnRow {
    /// 会话级大纲的列名单：语义列与时间戳固定追加在末尾，既有列序保持不变。
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
        columns.extend(TURN_ROW_SEMANTIC_COLUMNS.iter().map(|&s| s.to_string()));
        columns
    }

    /// 按 detail_level 把 Turn 事实映射成一行，语义列只搬运一次。
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
            started_at,
        } = facts;
        match detail_level {
            ConversationOutlineDetailLevel::Questions => Self::Questions {
                index,
                turn_id,
                question,
                turn_kind,
                agent_role,
                parent_turn_id,
                started_at,
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
                turn_kind,
                agent_role,
                parent_turn_id,
                started_at,
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
                started_at,
            },
        }
    }
}

macro_rules! serialize_elements {
    ($serializer:expr, $len:expr, $($elem:expr),+) => {{
        let mut seq = $serializer.serialize_seq(Some($len))?;
        $(seq.serialize_element($elem)?;)+
        seq.end()
    }};
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
                started_at,
            } => serialize_elements!(
                serializer,
                7,
                index,
                turn_id,
                question,
                turn_kind,
                agent_role,
                parent_turn_id,
                started_at
            ),
            Self::Qa {
                index,
                turn_id,
                question,
                answer,
                turn_kind,
                agent_role,
                parent_turn_id,
                started_at,
            } => serialize_elements!(
                serializer,
                8,
                index,
                turn_id,
                question,
                answer,
                turn_kind,
                agent_role,
                parent_turn_id,
                started_at
            ),
            Self::Default {
                index,
                turn_id,
                question,
                activities,
                cards,
                turn_kind,
                agent_role,
                parent_turn_id,
                started_at,
            } => serialize_elements!(
                serializer,
                9,
                index,
                turn_id,
                question,
                activities,
                cards,
                turn_kind,
                agent_role,
                parent_turn_id,
                started_at
            ),
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
                started_at,
            } => serialize_elements!(
                serializer,
                10,
                index,
                turn_id,
                question,
                activities,
                cards,
                answer,
                turn_kind,
                agent_role,
                parent_turn_id,
                started_at
            ),
        }
    }
}

impl<'de> Deserialize<'de> for SessionTurnRow {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let values = Vec::<serde_json::Value>::deserialize(deserializer)?;
        let index = row_index(&values)?;
        let turn_id = row_turn_id(&values)?;
        let question = row_question(&values)?;

        match values.len() {
            3 => Ok(Self::Questions {
                index,
                turn_id,
                question,
                turn_kind: None,
                agent_role: None,
                parent_turn_id: None,
                started_at: None,
            }),
            4 => Ok(Self::Qa {
                index,
                turn_id,
                question,
                answer: values[3].as_str().map(|s| s.to_string()),
                turn_kind: None,
                agent_role: None,
                parent_turn_id: None,
                started_at: None,
            }),
            5 => {
                let (activities, cards) = row_activities_cards(&values)?;
                Ok(Self::Default {
                    index,
                    turn_id,
                    question,
                    activities,
                    cards,
                    turn_kind: None,
                    agent_role: None,
                    parent_turn_id: None,
                    started_at: None,
                })
            }
            6 => {
                let (turn_kind, agent_role, parent_turn_id) = row_trailing_three(&values);
                Ok(Self::Questions {
                    index,
                    turn_id,
                    question,
                    turn_kind,
                    agent_role,
                    parent_turn_id,
                    started_at: None,
                })
            }
            7 => {
                let is_legacy_qa = values
                    .get(4)
                    .and_then(|v| v.as_str())
                    .map_or(false, |k| k == "user" || k == "subagent");
                if is_legacy_qa {
                    let (turn_kind, agent_role, parent_turn_id) = row_trailing_three(&values);
                    Ok(Self::Qa {
                        index,
                        turn_id,
                        question,
                        answer: values[3].as_str().map(|s| s.to_string()),
                        turn_kind,
                        agent_role,
                        parent_turn_id,
                        started_at: None,
                    })
                } else {
                    let (turn_kind, agent_role, parent_turn_id, started_at) =
                        row_trailing_four(&values);
                    Ok(Self::Questions {
                        index,
                        turn_id,
                        question,
                        turn_kind,
                        agent_role,
                        parent_turn_id,
                        started_at,
                    })
                }
            }
            8 => {
                if values[3].is_array() {
                    let (activities, cards) = row_activities_cards(&values)?;
                    let (turn_kind, agent_role, parent_turn_id) = row_trailing_three(&values);
                    Ok(Self::Default {
                        index,
                        turn_id,
                        question,
                        activities,
                        cards,
                        turn_kind,
                        agent_role,
                        parent_turn_id,
                        started_at: None,
                    })
                } else {
                    let (turn_kind, agent_role, parent_turn_id, started_at) =
                        row_trailing_four(&values);
                    Ok(Self::Qa {
                        index,
                        turn_id,
                        question,
                        answer: values[3].as_str().map(|s| s.to_string()),
                        turn_kind,
                        agent_role,
                        parent_turn_id,
                        started_at,
                    })
                }
            }
            9 => {
                let (activities, cards) = row_activities_cards(&values)?;
                let is_legacy_full = values
                    .get(6)
                    .and_then(|v| v.as_str())
                    .map_or(false, |k| k == "user" || k == "subagent");
                if is_legacy_full {
                    let (turn_kind, agent_role, parent_turn_id) = row_trailing_three(&values);
                    Ok(Self::Full {
                        index,
                        turn_id,
                        question,
                        activities,
                        cards,
                        answer: values[5].as_str().map(|s| s.to_string()),
                        turn_kind,
                        agent_role,
                        parent_turn_id,
                        started_at: None,
                    })
                } else {
                    let (turn_kind, agent_role, parent_turn_id, started_at) =
                        row_trailing_four(&values);
                    Ok(Self::Default {
                        index,
                        turn_id,
                        question,
                        activities,
                        cards,
                        turn_kind,
                        agent_role,
                        parent_turn_id,
                        started_at,
                    })
                }
            }
            10 => {
                let (activities, cards) = row_activities_cards(&values)?;
                let (turn_kind, agent_role, parent_turn_id, started_at) =
                    row_trailing_four(&values);
                Ok(Self::Full {
                    index,
                    turn_id,
                    question,
                    activities,
                    cards,
                    answer: values[5].as_str().map(|s| s.to_string()),
                    turn_kind,
                    agent_role,
                    parent_turn_id,
                    started_at,
                })
            }
            len => Err(serde::de::Error::custom(format!(
                "invalid element count for session turn row: expected 3 to 10, got {len}"
            ))),
        }
    }
}

fn row_activities_cards<E: serde::de::Error>(
    values: &[serde_json::Value],
) -> Result<(Vec<String>, usize), E> {
    let activities = serde_json::from_value(values[3].clone()).map_err(E::custom)?;
    let cards = row_cards(values)?;
    Ok((activities, cards))
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

/// 读取行尾 4 列的语义：轮次种类、角色、父轮次短 ID、起始时间戳。
fn row_trailing_four(
    values: &[serde_json::Value],
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    if values.len() < 7 {
        return (None, None, None, None);
    }
    let offset = values.len() - 4;
    let optional = |idx: usize| {
        values
            .get(idx)
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
    };
    (
        optional(offset),
        optional(offset + 1),
        optional(offset + 2),
        optional(offset + 3),
    )
}

/// 读取历史 3 列子代理语义：轮次种类、角色、父轮次短 ID。
fn row_trailing_three(
    values: &[serde_json::Value],
) -> (Option<String>, Option<String>, Option<String>) {
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
