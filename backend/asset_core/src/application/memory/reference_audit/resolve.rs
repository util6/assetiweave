//! 孤儿解析策略：把失效的问题标识映射回其所在会话内最近的有效分组，
//! 或明确判定无法映射。

use crate::backend::application::prelude::*;
use sqlx::{AssertSqlSafe, SqlitePool};

use super::scan::ReferenceRow;
use super::{
    STRATEGY_AMBIGUOUS, STRATEGY_NEAREST_GROUP, STRATEGY_NO_GROUP, STRATEGY_REDIRECT,
    STRATEGY_SINGLE_GROUP, STRATEGY_TURN_MEMBERSHIP,
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

/// 按会话/记录族给出「问题 → 轮次成员」相关表名前缀。
fn question_tables(record_kind: &str) -> (&'static str, &'static str, &'static str) {
    if record_kind == "web" {
        (
            "web_record_questions",
            "web_record_question_turns",
            "web_record_turns",
        )
    } else {
        (
            "conversation_questions",
            "conversation_question_turns",
            "conversation_turns",
        )
    }
}

async fn question_exists(
    pool: &SqlitePool,
    tenant_id: &str,
    questions: &str,
    question_id: &str,
    session_id: &str,
) -> AppResult<bool> {
    let sql = format!(
        "SELECT EXISTS(SELECT 1 FROM {questions} \
         WHERE tenant_id = ?1 AND id = ?2 AND session_id = ?3)"
    );
    Ok(sqlx::query_scalar::<_, i64>(AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(question_id)
        .bind(session_id)
        .fetch_one(pool)
        .await?
        != 0)
}

/// 合并后的旧问题标识通过跳转表墓碑解析到幸存分组。
async fn redirect_target(
    pool: &SqlitePool,
    tenant_id: &str,
    reference: &ReferenceRow,
) -> AppResult<Option<String>> {
    if reference.record_kind != "session" {
        return Ok(None);
    }
    let mut current = reference.question_id.clone();
    let mut visited = HashSet::new();
    loop {
        if !visited.insert(current.clone()) {
            return Ok(None);
        }
        let target = sqlx::query_scalar::<_, String>(
            "SELECT target_question_id FROM conversation_question_redirects \
             WHERE tenant_id = ?1 AND source_question_id = ?2",
        )
        .bind(tenant_id)
        .bind(&current)
        .fetch_optional(pool)
        .await?;
        match target {
            Some(next) => current = next,
            None => return Ok((current != reference.question_id).then_some(current)),
        }
    }
}

async fn turn_question(
    pool: &SqlitePool,
    tenant_id: &str,
    questions: &str,
    question_turns: &str,
    reference: &ReferenceRow,
    turn_id: &str,
) -> AppResult<Option<String>> {
    let sql = format!(
        "SELECT q.id FROM {question_turns} qt \
         JOIN {questions} q ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id \
         WHERE qt.tenant_id = ?1 AND qt.turn_id = ?2 AND q.session_id = ?3 LIMIT 1"
    );
    Ok(sqlx::query_scalar::<_, String>(AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(turn_id)
        .bind(&reference.session_id)
        .fetch_optional(pool)
        .await?)
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
    questions: &str,
    question_turns: &str,
    turns: &str,
    session_id: &str,
    index: i64,
) -> AppResult<Vec<(String, i64)>> {
    let sql = format!(
        "SELECT q.id, MIN(ABS(t.turn_index - ?3)) AS distance \
         FROM {questions} q \
         JOIN {question_turns} qt ON qt.tenant_id = q.tenant_id AND qt.question_id = q.id \
         JOIN {turns} t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id \
         WHERE q.tenant_id = ?1 AND q.session_id = ?2 \
         GROUP BY q.id ORDER BY distance ASC, q.id ASC LIMIT 2"
    );
    Ok(sqlx::query_as::<_, (String, i64)>(AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(session_id)
        .bind(index)
        .fetch_all(pool)
        .await?)
}

async fn session_questions(
    pool: &SqlitePool,
    tenant_id: &str,
    questions: &str,
    session_id: &str,
) -> AppResult<Vec<String>> {
    let sql = format!(
        "SELECT id FROM {questions} WHERE tenant_id = ?1 AND session_id = ?2 \
         ORDER BY created_at, id"
    );
    Ok(sqlx::query_scalar::<_, String>(AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(session_id)
        .fetch_all(pool)
        .await?)
}

/// 修复策略的唯一实现：先墓碑、再轮次归属、再最近分组、最后单分组回退；
/// 全都无法确定时显式失效，绝不删除引用。
pub(super) async fn resolve_orphan(
    pool: &SqlitePool,
    tenant_id: &str,
    reference: &ReferenceRow,
) -> AppResult<Resolution> {
    let (questions, question_turns, turns) = question_tables(&reference.record_kind);
    if let Some(target) = redirect_target(pool, tenant_id, reference).await? {
        if question_exists(pool, tenant_id, questions, &target, &reference.session_id).await? {
            return Ok(Resolution::Remapped {
                question_id: target,
                strategy: STRATEGY_REDIRECT,
            });
        }
    }
    if let Some(turn_id) = reference.turn_id.as_deref() {
        if let Some(question_id) = turn_question(
            pool,
            tenant_id,
            questions,
            question_turns,
            reference,
            turn_id,
        )
        .await?
        {
            return Ok(Resolution::Remapped {
                question_id,
                strategy: STRATEGY_TURN_MEMBERSHIP,
            });
        }
        if let Some(index) =
            turn_index(pool, tenant_id, turns, &reference.session_id, turn_id).await?
        {
            let candidates = nearest_question(
                pool,
                tenant_id,
                questions,
                question_turns,
                turns,
                &reference.session_id,
                index,
            )
            .await?;
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
    let groups = session_questions(pool, tenant_id, questions, &reference.session_id).await?;
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
