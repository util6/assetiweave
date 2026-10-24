use super::*;

pub async fn insert_web_record_parts_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    turn_id: &str,
    parts: &[crate::backend::domain::NormalizedConversationPart],
    translation_state: &BTreeMap<String, (Option<String>, Option<String>, Option<String>)>,
) -> StoreResult<()> {
    for (index, part) in parts.iter().enumerate() {
        let part_id = stable_id("web-record-part", &[turn_id, &index.to_string()]);
        let content_card_json = part
            .content_card
            .as_ref()
            .map(encode_json_app)
            .transpose()?;
        let translated_text = translation_state
            .get(&part_id)
            .filter(|(text, command, _)| text == &part.text && command == &part.command)
            .and_then(|(_, _, translated_text)| translated_text.as_ref());
        sqlx::query(
            r#"
            INSERT INTO web_record_parts (
                tenant_id, id, turn_id, part_index, role, kind, text, language, command,
                cwd, status, exit_code, command_label, metadata_json, content_card_json, translated_text,
                source_execution_id
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
            "#,
        )
        .bind(tenant_id)
        .bind(part_id)
        .bind(turn_id)
        .bind(index as i64)
        .bind(encode_enum_app(part.role)?)
        .bind(encode_enum_app(part.kind)?)
        .bind(&part.text)
        .bind(&part.language)
        .bind(&part.command)
        .bind(&part.cwd)
        .bind(&part.status)
        .bind(part.exit_code)
        .bind(&part.command_label)
        .bind(&part.metadata_json)
        .bind(content_card_json)
        .bind(translated_text)
        .bind(&part.source_execution_id)
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
    let rows = sqlx::query_as::<_, WebRecordPartTranslationRow>(
        r#"
        SELECT p.id, p.text, p.command, p.translated_text
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
        .map(|row| (row.id, (row.text, row.command, row.translated_text)))
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
    now: &str,
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
            user_text: turn.user_text.clone(),
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

pub async fn load_web_record_parts_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    turn_id: &str,
) -> StoreResult<Vec<ConversationPart>> {
    let rows = sqlx::query(
        r#"
        SELECT p.turn_id, p.part_index, p.role, p.kind,
               json_extract(p.content_card_json, '$.kind') AS card_kind,
               COALESCE(p.command, p.text) AS content,
               NULLIF(json_patch('{}', json_object(
                   'language', p.language,
                   'cwd', p.cwd,
                   'status', p.status,
                   'exit_code', p.exit_code,
                   'command_label', p.command_label,
                   'exec_id', p.source_execution_id,
                   'summary', (CASE WHEN p.kind = 'command' THEN p.text ELSE NULL END),
                   'renderer', json_extract(p.content_card_json, '$.renderer'),
                   'semantic_role', json_extract(p.content_card_json, '$.semantic_role')
               )), '{}') AS attrs_json,
               p.metadata_json, p.translated_text
        FROM web_record_parts p
        WHERE p.tenant_id = ?1 AND p.turn_id = ?2
        ORDER BY p.part_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(turn_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    rows.iter().map(map_sqlx_conversation_part).collect()
}
