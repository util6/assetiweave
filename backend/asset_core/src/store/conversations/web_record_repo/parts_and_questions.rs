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

    let mut groups: Vec<(String, Vec<String>, ConversationGroupingOrigin)> = Vec::new();
    for assignment in &assignments {
        match groups.last_mut() {
            Some((question_id, turn_ids, origin)) if *question_id == assignment.question_id => {
                turn_ids.push(assignment.turn_id.clone());
                *origin = assignment.origin;
            }
            _ => groups.push((
                assignment.question_id.clone(),
                vec![assignment.turn_id.clone()],
                assignment.origin,
            )),
        }
    }

    for (question_id, turn_ids, origin) in groups {
        for (order, turn_id) in turn_ids.iter().enumerate() {
            sqlx::query(
                r#"
                INSERT INTO web_record_question_turns (
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
            .bind(&question_id)
            .bind(turn_id)
            .bind(order as i64)
            .bind(encode_enum_app(origin)?)
            .bind(now)
            .execute(&mut **tx)
            .await
            .map_err(StoreError::external)?;
        }
        let aggregate =
            build_question_aggregate_sqlx_tx(tx, tenant_id, session_id, &turn_ids).await?;
        sqlx::query(
            r#"
            INSERT INTO web_record_questions (
                tenant_id, id, session_id, title, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?5)
            ON CONFLICT(tenant_id, id) DO UPDATE SET
                title = excluded.title,
                updated_at = excluded.updated_at
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

pub async fn build_question_aggregate_sqlx_tx(
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

pub async fn load_web_record_parts_sqlx_tx(
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
