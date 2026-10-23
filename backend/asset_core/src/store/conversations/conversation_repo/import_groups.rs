use super::*;
use crate::backend::domain::should_merge_into_previous_question;

/// 轮次上已写入的分组（来自 `question_id` / `question_origin` 两列）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TurnQuestionInput {
    pub(crate) turn_id: String,
    pub(crate) user_text: String,
    pub(crate) existing: Option<(String, ConversationGroupingOrigin)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TurnQuestionAssignment {
    pub(crate) turn_id: String,
    pub(crate) question_id: String,
    pub(crate) origin: ConversationGroupingOrigin,
}

pub(crate) fn decode_turn_question_origin(value: &str) -> ConversationGroupingOrigin {
    match value {
        "manual" => ConversationGroupingOrigin::Manual,
        "auto_merged" => ConversationGroupingOrigin::AutoMerged,
        _ => ConversationGroupingOrigin::Imported,
    }
}

pub(crate) fn turn_question_input(
    turn_id: String,
    user_text: String,
    question_id: String,
    question_origin: &str,
) -> TurnQuestionInput {
    let existing = (!question_id.is_empty())
        .then(|| (question_id, decode_turn_question_origin(question_origin)));
    TurnQuestionInput {
        turn_id,
        user_text,
        existing,
    }
}

/// 规划每个轮次的分组标识与来源。
///
/// 已写入的标识即固化：本函数不会为已有标识的轮次重新推导分组，只有尚未分组的
/// 轮次才会按自动归并规则并入上一组或自成一族。`manual` 轮次是自动归并的围栏，
/// 其后的轮次不会并入它。
pub(crate) fn plan_turn_question_assignments(
    question_id_prefix: &str,
    session_id: &str,
    turns: &[TurnQuestionInput],
) -> Vec<TurnQuestionAssignment> {
    let mut assignments = Vec::with_capacity(turns.len());
    let mut auto_merged_groups = BTreeSet::new();
    let mut current_group: Option<String> = None;

    for turn in turns {
        let (question_id, origin) = match &turn.existing {
            Some((question_id, origin)) => {
                if *origin == ConversationGroupingOrigin::Manual {
                    current_group = None;
                } else {
                    if *origin == ConversationGroupingOrigin::AutoMerged {
                        auto_merged_groups.insert(question_id.clone());
                    }
                    current_group = Some(question_id.clone());
                }
                (question_id.clone(), *origin)
            }
            None => {
                let joined_group = current_group
                    .clone()
                    .filter(|_| should_merge_into_previous_question(&turn.user_text));
                match joined_group {
                    Some(group_id) => {
                        auto_merged_groups.insert(group_id.clone());
                        current_group = Some(group_id.clone());
                        (group_id, ConversationGroupingOrigin::AutoMerged)
                    }
                    None => {
                        let question_id =
                            stable_id(question_id_prefix, &[session_id, &turn.turn_id]);
                        current_group = Some(question_id.clone());
                        (question_id, ConversationGroupingOrigin::Imported)
                    }
                }
            }
        };
        assignments.push(TurnQuestionAssignment {
            turn_id: turn.turn_id.clone(),
            question_id,
            origin,
        });
    }

    // 一个分组只要吸收了自动归并的成员，整组来源即升级为 auto_merged；
    // 手动分组保持 manual，不被自动规则改写。
    for assignment in assignments.iter_mut() {
        if assignment.origin != ConversationGroupingOrigin::Manual
            && auto_merged_groups.contains(&assignment.question_id)
        {
            assignment.origin = ConversationGroupingOrigin::AutoMerged;
        }
    }

    assignments
}

pub(super) async fn ensure_question_groups_for_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    now: &str,
) -> StoreResult<()> {
    reject_invalid_conversation_question_turns_for_session_sqlx_tx(tx, tenant_id, session_id)
        .await?;
    let turns = load_session_turn_question_inputs_sqlx_tx(tx, tenant_id, session_id).await?;
    if turns.is_empty() {
        return Ok(());
    }

    let assignments = plan_turn_question_assignments("conversation-question", session_id, &turns);
    write_conversation_turn_question_columns_sqlx_tx(tx, tenant_id, &assignments).await?;
    replace_conversation_question_turn_memberships_sqlx_tx(tx, tenant_id, &assignments, now)
        .await?;
    ensure_conversation_question_rows_sqlx_tx(tx, tenant_id, session_id, &assignments, now).await?;
    prune_orphan_questions_for_session_sqlx_tx(tx, tenant_id, session_id).await?;
    renumber_questions_for_session_sqlx_tx(tx, tenant_id, session_id).await?;
    Ok(())
}

#[derive(Debug, FromRow)]
struct TurnQuestionInputRow {
    id: String,
    user_text: String,
    question_id: String,
    question_origin: String,
}

async fn load_session_turn_question_inputs_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<Vec<TurnQuestionInput>> {
    let rows = sqlx::query_as::<_, TurnQuestionInputRow>(
        r#"
        SELECT id, user_text, question_id, question_origin
        FROM conversation_turns
        WHERE tenant_id = ?1 AND session_id = ?2
        ORDER BY turn_index ASC, id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            turn_question_input(row.id, row.user_text, row.question_id, &row.question_origin)
        })
        .collect())
}

async fn write_conversation_turn_question_columns_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    assignments: &[TurnQuestionAssignment],
) -> StoreResult<()> {
    for assignment in assignments {
        sqlx::query(
            r#"
            UPDATE conversation_turns
            SET question_id = ?1, question_origin = ?2
            WHERE tenant_id = ?3 AND id = ?4
            "#,
        )
        .bind(&assignment.question_id)
        .bind(encode_enum(assignment.origin)?)
        .bind(tenant_id)
        .bind(&assignment.turn_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    }
    Ok(())
}

async fn replace_conversation_question_turn_memberships_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    assignments: &[TurnQuestionAssignment],
    now: &str,
) -> StoreResult<()> {
    let mut group_orders: BTreeMap<&str, i64> = BTreeMap::new();
    for assignment in assignments {
        let turn_order = group_orders
            .entry(assignment.question_id.as_str())
            .or_insert(0);
        sqlx::query(
            r#"
            INSERT INTO conversation_question_turns (
                tenant_id, question_id, turn_id, turn_order,
                assignment_origin, assigned_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
            ON CONFLICT(tenant_id, turn_id) DO UPDATE SET
                question_id = excluded.question_id,
                turn_order = excluded.turn_order,
                assignment_origin = excluded.assignment_origin,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(tenant_id)
        .bind(&assignment.question_id)
        .bind(&assignment.turn_id)
        .bind(*turn_order)
        .bind(encode_enum(assignment.origin)?)
        .bind(now)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
        *turn_order += 1;
    }
    Ok(())
}

async fn ensure_conversation_question_rows_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    assignments: &[TurnQuestionAssignment],
    now: &str,
) -> StoreResult<()> {
    let mut seen = BTreeSet::new();
    for assignment in assignments {
        if !seen.insert(assignment.question_id.as_str()) {
            continue;
        }
        match load_conversation_question_sqlx_tx(tx, tenant_id, &assignment.question_id).await? {
            Some(question) if question.session_id != session_id => {
                return Err(StoreError::Validation(format!(
                    "question id belongs to another session: {}",
                    assignment.question_id
                )));
            }
            Some(_) => {}
            None => {
                sqlx::query(
                    r#"
                    INSERT INTO conversation_questions (
                        tenant_id, id, session_id, title, created_at, updated_at
                    )
                    VALUES (?1, ?2, ?3, NULL, ?4, ?4)
                    "#,
                )
                .bind(tenant_id)
                .bind(&assignment.question_id)
                .bind(session_id)
                .bind(now)
                .execute(&mut **tx)
                .await
                .map_err(StoreError::external)?;
            }
        }
    }
    Ok(())
}

#[derive(Debug, FromRow)]
pub(super) struct InvalidConversationQuestionTurnRow {
    question_id: String,
    turn_id: String,
    reason: String,
}

pub(super) async fn reject_invalid_conversation_question_turns_for_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<()> {
    let row = sqlx::query_as::<_, InvalidConversationQuestionTurnRow>(
        r#"
        SELECT qt.question_id, qt.turn_id,
               CASE
                   WHEN q.id IS NULL THEN 'missing_question'
                   WHEN t.id IS NULL THEN 'missing_turn'
                   ELSE 'cross_session'
               END AS reason
        FROM conversation_question_turns qt
        LEFT JOIN conversation_questions q
          ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        LEFT JOIN conversation_turns t
          ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1
          AND (q.session_id = ?2 OR t.session_id = ?2)
          AND (q.id IS NULL OR t.id IS NULL OR q.session_id <> t.session_id)
        ORDER BY qt.question_id ASC, qt.turn_id ASC
        LIMIT 1
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    if let Some(first) = row {
        return Err(StoreError::Validation(format!(
            "invalid question turn membership ({}): question={}, turn={}",
            first.reason, first.question_id, first.turn_id
        )));
    }
    Ok(())
}

pub(super) async fn prune_orphan_questions_for_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<()> {
    let orphan_ids = sqlx::query_scalar::<_, String>(
        r#"
        SELECT q.id
        FROM conversation_questions q
        LEFT JOIN conversation_question_turns qt
          ON qt.tenant_id = q.tenant_id AND qt.question_id = q.id
        WHERE q.tenant_id = ?1 AND q.session_id = ?2
        GROUP BY q.id
        HAVING COUNT(qt.turn_id) = 0
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;

    if orphan_ids.is_empty() {
        return Ok(());
    }

    for orphan_id in &orphan_ids {
        sqlx::query(
            r#"
            DELETE FROM conversation_questions
            WHERE tenant_id = ?1 AND id = ?2
            "#,
        )
        .bind(tenant_id)
        .bind(orphan_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    }

    Ok(())
}

pub(super) async fn audit_invalid_conversation_question_turns_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<()> {
    let rows = sqlx::query_as::<_, InvalidConversationQuestionTurnRow>(
        r#"
        SELECT qt.question_id, qt.turn_id,
               CASE
                   WHEN q.id IS NULL THEN 'missing_question'
                   WHEN t.id IS NULL THEN 'missing_turn'
                   ELSE 'cross_session'
               END AS reason
        FROM conversation_question_turns qt
        LEFT JOIN conversation_questions q
          ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        LEFT JOIN conversation_turns t
          ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1
          AND (q.id IS NULL OR t.id IS NULL OR q.session_id <> t.session_id)
        ORDER BY qt.question_id ASC, qt.turn_id ASC
        "#,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    if rows.is_empty() {
        return Ok(());
    }

    let detected_at = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    for row in &rows {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO conversation_question_turn_audits (
                tenant_id, record_kind, question_id, turn_id, reason, detected_at
            )
            VALUES (?1, 'session', ?2, ?3, ?4, ?5)
            "#,
        )
        .bind(tenant_id)
        .bind(&row.question_id)
        .bind(&row.turn_id)
        .bind(&row.reason)
        .bind(&detected_at)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::external)?;
    }
    tx.commit().await.map_err(StoreError::external)
}
