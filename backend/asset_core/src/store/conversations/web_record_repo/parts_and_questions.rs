use super::*;
use crate::backend::domain::{
    derive_part_id_key, derive_standard_part_id, ConversationPartKind, ConversationPartRole,
};

pub async fn insert_web_record_parts_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    turn_id: &str,
    parts: &[crate::backend::domain::conversations::triad::TriadPart],
    translation_state: &BTreeMap<String, (Option<String>, Option<String>, Option<String>)>,
) -> StoreResult<()> {
    for part in parts {
        let part_id = derive_standard_part_id("web-record-part", turn_id, part.part_index);
        let id_key = derive_part_id_key(&part_id).unwrap_or([0u8; 8]);
        let content = part.content.clone().unwrap_or_default();
        let translated_text = translation_state
            .get(&part_id)
            .filter(|(old_content, _, _)| old_content.as_deref() == Some(&content))
            .and_then(|(_, _, translated_text)| translated_text.as_ref())
            .cloned()
            .or_else(|| part.translated_text.clone());
        sqlx::query(
            r#"
            INSERT INTO web_record_parts (
                tenant_id, turn_id, part_index, id_key, kind, content, translated_text
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
        )
        .bind(tenant_id)
        .bind(turn_id)
        .bind(part.part_index)
        .bind(id_key.to_vec())
        .bind(&part.kind)
        .bind(&content)
        .bind(translated_text)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    }
    // 索引随零件写入增量维护：只重建这个轮次的索引行，不触碰其它轮次。
    replace_turn_part_index_sqlx_tx(tx, tenant_id, turn_id, ConversationRecordKind::Web).await?;
    Ok(())
}

pub async fn load_web_record_part_translation_state_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<BTreeMap<String, (Option<String>, Option<String>, Option<String>)>> {
    let rows = sqlx::query_as::<_, (String, i64, String, Option<String>)>(
        r#"
        SELECT p.turn_id, p.part_index, p.content, p.translated_text
        FROM web_record_parts p
        JOIN web_record_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
        WHERE t.tenant_id = ?1 AND t.session_id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(rows
        .into_iter()
        .map(|(turn_id, part_index, content, translated_text)| {
            let part_id = derive_standard_part_id("web-record-part", &turn_id, part_index);
            (part_id, (Some(content), None, translated_text))
        })
        .collect())
}

pub async fn load_web_record_turn_question_assignments_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<BTreeMap<String, (String, ConversationGroupingOrigin)>> {
    let rows = sqlx::query_as::<_, (String, String, String)>(
        r#"
        SELECT id, question_id, question_origin
        FROM web_record_turns
        WHERE tenant_id = ?1 AND session_id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(rows
        .into_iter()
        .filter(|(_, question_id, _)| !question_id.is_empty())
        .map(|(turn_id, question_id, origin)| {
            (turn_id, (question_id, decode_turn_question_origin(&origin)))
        })
        .collect())
}

pub async fn insert_web_record_questions_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    turns: &[ConversationTurn],
    existing: &BTreeMap<String, (String, ConversationGroupingOrigin)>,
    _now: &str,
) -> StoreResult<()> {
    let mut ordered_turns = turns.to_vec();
    ordered_turns.sort_by(|left, right| {
        left.turn_index
            .cmp(&right.turn_index)
            .then_with(|| left.id.cmp(&right.id))
    });
    let inputs = ordered_turns
        .iter()
        .map(|turn| TurnQuestionInput {
            turn_id: turn.id.clone(),
            user_text: turn.prompt.clone(),
            existing: existing.get(&turn.id).cloned(),
        })
        .collect::<Vec<_>>();
    let assignments = plan_turn_question_assignments("web-record-question", session_id, &inputs);

    for assignment in &assignments {
        sqlx::query(
            r#"
            UPDATE web_record_turns
            SET question_id = ?1, question_origin = ?2
            WHERE tenant_id = ?3 AND id = ?4
            "#,
        )
        .bind(&assignment.question_id)
        .bind(encode_enum_app(assignment.origin)?)
        .bind(tenant_id)
        .bind(&assignment.turn_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    }
    Ok(())
}
