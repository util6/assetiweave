//! 孤儿解析策略：把失效的问题标识映射回其所在会话内最近的有效分组，
//! 或明确判定无法映射。

use crate::backend::application::prelude::*;
use sqlx::{AssertSqlSafe, SqlitePool};

use super::scan::ReferenceRow;
use super::{
    STRATEGY_AMBIGUOUS, STRATEGY_NEAREST_GROUP, STRATEGY_NO_GROUP, STRATEGY_SINGLE_GROUP,
    STRATEGY_TURN_MEMBERSHIP,
};

pub(super) enum Resolution {
    Remapped {
        question_id: String,
        strategy: &'static str,
    },
    Invalidated {
        strategy: &'static str,
    },
}

/// 按会话/记录族给出轮次表名。
fn turn_table(record_kind: &str) -> &'static str {
    if record_kind == "web" {
        "web_record_turns"
    } else {
        "conversation_turns"
    }
}

async fn turn_question(
    pool: &SqlitePool,
    tenant_id: &str,
    turns: &str,
    reference: &ReferenceRow,
    turn_id: &str,
) -> AppResult<Option<String>> {
    let sql = format!(
        "SELECT id FROM {turns} \
         WHERE tenant_id = ?1 AND id = ?2 AND session_id = ?3 \
           AND (id LIKE 'conversation-turn-%' OR id LIKE 'web-record-turn-%' OR id LIKE 'conversation-question-%' OR id LIKE 'web-record-question-%' OR id LIKE 'q-%' OR id LIKE 'wq-%') \
         LIMIT 1"
    );
    let matched_id: Option<String> = sqlx::query_scalar::<_, String>(AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(turn_id)
        .bind(&reference.session_id)
        .fetch_optional(pool)
        .await?;
    Ok(matched_id
        .map(|id| crate::backend::domain::conversations::derive_question_id_from_turn(&id)))
}

async fn turn_index(
    pool: &SqlitePool,
    tenant_id: &str,
    turns: &str,
    session_id: &str,
    turn_id: &str,
) -> AppResult<Option<i64>> {
    let sql = format!(
        "SELECT turn_index FROM {turns} WHERE tenant_id = ?1 AND id = ?2 AND session_id = ?3"
    );
    Ok(sqlx::query_scalar::<_, i64>(AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(turn_id)
        .bind(session_id)
        .fetch_optional(pool)
        .await?)
}

/// 按轮次距离取最近的现存分组；距离并列即视为无法判定。
async fn nearest_question(
    pool: &SqlitePool,
    tenant_id: &str,
    turns: &str,
    session_id: &str,
    index: i64,
) -> AppResult<Vec<(String, i64)>> {
    let sql = format!(
        "SELECT id, MIN(ABS(turn_index - ?3)) AS distance \
         FROM {turns} \
         WHERE tenant_id = ?1 AND session_id = ?2 \
           AND (id LIKE 'conversation-turn-%' OR id LIKE 'web-record-turn-%' OR id LIKE 'conversation-question-%' OR id LIKE 'web-record-question-%' OR id LIKE 'q-%' OR id LIKE 'wq-%') \
         GROUP BY id ORDER BY distance ASC, id ASC LIMIT 2"
    );
    let rows = sqlx::query_as::<_, (String, i64)>(AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(session_id)
        .bind(index)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|(id, dist)| {
            (
                crate::backend::domain::conversations::derive_question_id_from_turn(&id),
                dist,
            )
        })
        .collect())
}

async fn session_questions(
    pool: &SqlitePool,
    tenant_id: &str,
    turns: &str,
    session_id: &str,
) -> AppResult<Vec<String>> {
    let sql = format!(
        "SELECT id FROM {turns} \
         WHERE tenant_id = ?1 AND session_id = ?2 AND id <> '' \
         GROUP BY id \
         ORDER BY MIN(turn_index) ASC, id ASC"
    );
    let rows = sqlx::query_scalar::<_, String>(AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(session_id)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|id| crate::backend::domain::conversations::derive_question_id_from_turn(&id))
        .collect())
}

/// 修复策略的唯一实现：先轮次归属、再最近分组、最后单分组回退；
/// 全都无法确定时显式失效，绝不删除引用。
pub(super) async fn resolve_orphan(
    pool: &SqlitePool,
    tenant_id: &str,
    reference: &ReferenceRow,
) -> AppResult<Resolution> {
    let turns = turn_table(&reference.record_kind);
    if let Some(turn_id) = reference.turn_id.as_deref() {
        if let Some(question_id) = turn_question(pool, tenant_id, turns, reference, turn_id).await?
        {
            return Ok(Resolution::Remapped {
                question_id,
                strategy: STRATEGY_TURN_MEMBERSHIP,
            });
        }
        if let Some(index) =
            turn_index(pool, tenant_id, turns, &reference.session_id, turn_id).await?
        {
            let candidates =
                nearest_question(pool, tenant_id, turns, &reference.session_id, index).await?;
            match candidates.as_slice() {
                [(question_id, _)] => {
                    return Ok(Resolution::Remapped {
                        question_id: question_id.clone(),
                        strategy: STRATEGY_NEAREST_GROUP,
                    });
                }
                [(_, first), (_, second), ..] if first == second => {
                    return Ok(Resolution::Invalidated {
                        strategy: STRATEGY_AMBIGUOUS,
                    });
                }
                [(_, _), (_, _), ..] => {
                    return Ok(Resolution::Remapped {
                        question_id: candidates[0].0.clone(),
                        strategy: STRATEGY_NEAREST_GROUP,
                    });
                }
                _ => {}
            }
        }
    }
    let groups = session_questions(pool, tenant_id, turns, &reference.session_id).await?;
    match groups.as_slice() {
        [] => Ok(Resolution::Invalidated {
            strategy: STRATEGY_NO_GROUP,
        }),
        [question_id] => Ok(Resolution::Remapped {
            question_id: question_id.clone(),
            strategy: STRATEGY_SINGLE_GROUP,
        }),
        _ => Ok(Resolution::Invalidated {
            strategy: STRATEGY_AMBIGUOUS,
        }),
    }
}
