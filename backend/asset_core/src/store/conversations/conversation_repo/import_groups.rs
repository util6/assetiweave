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
    _now: &str,
) -> StoreResult<()> {
    let turns = load_session_turn_question_inputs_sqlx_tx(tx, tenant_id, session_id).await?;
    if turns.is_empty() {
        return Ok(());
    }

    let assignments = plan_turn_question_assignments("conversation-question", session_id, &turns);
    write_conversation_turn_question_columns_sqlx_tx(tx, tenant_id, &assignments).await?;
    Ok(())
}

#[derive(Debug, FromRow)]
struct TurnQuestionInputRow {
    id: String,
    prompt: String,
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
        SELECT id, prompt, question_id, question_origin
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
        .map(|row| turn_question_input(row.id, row.prompt, row.question_id, &row.question_origin))
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
