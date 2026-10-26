use super::*;

pub async fn list_conversation_question_details_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
    query: Option<&str>,
    limit: usize,
    offset: usize,
) -> StoreResult<Vec<ConversationQuestionDetail>> {
    let needle = normalize_query(query);
    let details =
        load_conversation_question_details_for_session_sqlx(pool, tenant_id, session_id).await?;
    Ok(details
        .into_iter()
        .filter(|detail| {
            needle.as_ref().is_none_or(|needle| {
                let question = &detail.question;
                std::iter::once(question.title.clone().unwrap_or_default())
                    .chain(detail.turns.iter().map(|turn| turn.prompt.clone()))
                    .chain(
                        detail
                            .projected_content_nodes
                            .iter()
                            .map(|node| node.content.clone()),
                    )
                    .collect::<Vec<_>>()
                    .join("\n")
                    .to_lowercase()
                    .contains(needle)
            })
        })
        .skip(offset)
        .take(limit)
        .collect())
}

/// 单问题详情：纯函数动态分组投影，完全解耦持久化 question_id 列与跳转表。
pub async fn load_conversation_question_detail_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    question_or_turn_id: &str,
) -> StoreResult<ConversationQuestionDetail> {
    let direct_turn_id = question_or_turn_id;
    let turn_from_question =
        crate::backend::domain::conversations::derive_turn_id_from_question_id(question_or_turn_id);
    let stripped = question_or_turn_id
        .strip_prefix("conversation-question-")
        .or_else(|| question_or_turn_id.strip_prefix("web-record-question-"));

    let mut candidate_ids = vec![direct_turn_id.to_string()];
    if let Some(t) = turn_from_question.as_ref() {
        candidate_ids.push(t.clone());
    }
    if let Some(s) = stripped {
        candidate_ids.push(s.to_string());
    }

    // Try finding in conversation_turns
    for candidate in &candidate_ids {
        if let Some(session_id) = sqlx::query_scalar::<_, String>(
            "SELECT session_id FROM conversation_turns WHERE tenant_id = ?1 AND id = ?2 LIMIT 1",
        )
        .bind(tenant_id)
        .bind(candidate)
        .fetch_optional(pool)
        .await
        .map_err(StoreError::external)?
        {
            let details =
                load_conversation_question_details_for_session_sqlx(pool, tenant_id, &session_id)
                    .await?;
            if let Some(detail) = details.into_iter().find(|d| {
                d.question.id == question_or_turn_id
                    || d.turns
                        .iter()
                        .any(|t| t.id == *candidate || t.id == question_or_turn_id)
            }) {
                return Ok(detail);
            }
        }
    }

    // Try finding in web_record_turns
    for candidate in &candidate_ids {
        if let Some(session_id) = sqlx::query_scalar::<_, String>(
            "SELECT session_id FROM web_record_turns WHERE tenant_id = ?1 AND id = ?2 LIMIT 1",
        )
        .bind(tenant_id)
        .bind(candidate)
        .fetch_optional(pool)
        .await
        .map_err(StoreError::external)?
        {
            let session_detail = crate::backend::store::load_web_record_session_detail_sqlx(
                pool,
                tenant_id,
                &session_id,
            )
            .await?;
            if let Some(detail) = session_detail.questions.into_iter().find(|d| {
                d.question.id == question_or_turn_id
                    || d.turns
                        .iter()
                        .any(|t| t.id == *candidate || t.id == question_or_turn_id)
            }) {
                return Ok(detail);
            }
        }
    }

    Err(StoreError::NotFound(format!(
        "conversation question not found: {question_or_turn_id}"
    )))
}
