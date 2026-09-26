use super::*;

pub(crate) async fn insert_web_record_parts_sqlx_tx(
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
    Ok(())
}

pub(crate) async fn load_web_record_part_translation_state_sqlx_tx(
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

pub(crate) async fn insert_web_record_questions_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    turns: &[ConversationTurn],
    now: &str,
) -> StoreResult<()> {
    let mut ordered_turns = turns.to_vec();
    ordered_turns.sort_by(|left, right| {
        left.turn_index
            .cmp(&right.turn_index)
            .then_with(|| left.id.cmp(&right.id))
    });
    let groups = group_turn_ids_by_question(
        ordered_turns
            .iter()
            .map(|turn| (turn.id.clone(), turn.user_text.clone()))
            .collect::<Vec<_>>(),
    );
    for (_index, group) in groups.into_iter().enumerate() {
        let first_turn_id = group
            .turn_ids
            .first()
            .ok_or_else(|| StoreError::Validation("empty web record question group".to_string()))?;
        let question_id = stable_id("web-record-question", &[session_id, first_turn_id]);
        for (order, turn_id) in group.turn_ids.iter().enumerate() {
            sqlx::query(
                r#"
                INSERT INTO web_record_question_turns (
                    tenant_id, question_id, turn_id, turn_order,
                    assignment_origin, assigned_at, updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                "#,
            )
            .bind(tenant_id)
            .bind(&question_id)
            .bind(turn_id)
            .bind(order as i64)
            .bind(encode_enum_app(group.origin)?)
            .bind(now)
            .execute(&mut **tx)
            .await
            .map_err(StoreError::external)?;
        }
        let aggregate =
            build_question_aggregate_sqlx_tx(tx, tenant_id, session_id, &group.turn_ids).await?;
        sqlx::query(
            r#"
            INSERT INTO web_record_questions (
                tenant_id, id, session_id, title, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?5)
            "#,
        )
        .bind(tenant_id)
        .bind(&question_id)
        .bind(session_id)
        .bind(first_line(&aggregate.question_text))
        .bind(now)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
        sqlx::query(
            "DELETE FROM conversation_question_fts WHERE tenant_id = ?1 AND question_id = ?2",
        )
        .bind(tenant_id)
        .bind(&question_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
        sqlx::query(
            r#"
            INSERT INTO conversation_question_fts (
                tenant_id, question_id, session_id, question_text, answer_text, code_text, command_text
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
        )
        .bind(tenant_id)
        .bind(&question_id)
        .bind(session_id)
        .bind(&aggregate.question_text)
        .bind(&aggregate.answer_text)
        .bind(&aggregate.code_text)
        .bind(&aggregate.command_text)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    }
    Ok(())
}

pub(crate) async fn build_question_aggregate_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    turn_ids: &[String],
) -> StoreResult<QuestionAggregate> {
    let mut question_text = Vec::new();
    let mut answer_text = Vec::new();
    let mut code_text = Vec::new();
    let mut command_text = Vec::new();
    let adapter_id = sqlx::query_scalar::<_, String>(
        "SELECT adapter_id FROM web_record_sessions WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    let card_kinds_json = sqlx::query_scalar::<_, String>(
        "SELECT card_kinds_json FROM conversation_adapters WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(&adapter_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::external)?
    .unwrap_or_else(|| "[]".to_string());
    let card_kinds: Vec<ConversationCardKindDefinition> = decode_json_app(card_kinds_json)?;
    for turn_id in turn_ids {
        let user_text: String = sqlx::query_scalar::<_, String>(
            "SELECT user_text FROM web_record_turns WHERE tenant_id = ?1 AND id = ?2",
        )
        .bind(tenant_id)
        .bind(turn_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(StoreError::external)?;
        question_text.push(user_text);
        for part in load_web_record_parts_sqlx_tx(tx, tenant_id, turn_id).await? {
            append_projected_cards_to_question_aggregate(
                &part,
                &adapter_id,
                &card_kinds,
                &mut answer_text,
                &mut code_text,
                &mut command_text,
            )?;
        }
    }
    Ok(QuestionAggregate {
        question_text: question_text.join("\n\n"),
        answer_text: answer_text.join("\n\n"),
        code_text: code_text.join("\n\n"),
        command_text: command_text.join("\n\n"),
    })
}

pub(crate) async fn load_web_record_parts_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    turn_id: &str,
) -> StoreResult<Vec<ConversationPart>> {
    let rows = sqlx::query(
        r#"
        SELECT id, turn_id, part_index, role, kind, text, language, command,
               cwd, status, exit_code, metadata_json, content_card_json, translated_text,
               source_execution_id, command_label
        FROM web_record_parts
        WHERE tenant_id = ?1 AND turn_id = ?2
        ORDER BY part_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(turn_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    rows.iter().map(map_sqlx_conversation_part).collect()
}
