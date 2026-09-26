use super::*;

pub(crate) async fn merge_conversation_questions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    question_ids: &[String],
    dry_run: bool,
) -> StoreResult<ConversationMutationResult> {
    if question_ids.len() < 2 {
        return Err(StoreError::Validation(
            "at least two question ids are required".to_string(),
        ));
    }

    audit_invalid_conversation_question_turns_sqlx(pool, tenant_id).await?;
    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    let mut questions = Vec::with_capacity(question_ids.len());
    for question_id in question_ids {
        let resolved_question_id =
            resolve_conversation_question_redirect_sqlx_tx(&mut tx, tenant_id, question_id).await?;
        questions.push(
            load_conversation_question_sqlx_tx(&mut tx, tenant_id, &resolved_question_id)
                .await?
                .ok_or_else(|| {
                    StoreError::external(format!("conversation question not found: {question_id}"))
                })?,
        );
    }
    let session_id = questions[0].session_id.clone();
    if questions
        .iter()
        .any(|question| question.session_id != session_id)
    {
        return Err(StoreError::Validation(
            "questions must belong to the same session".to_string(),
        ));
    }
    let mut canonical_question_ids = Vec::with_capacity(questions.len());
    for question in &questions {
        if !canonical_question_ids.contains(&question.id) {
            canonical_question_ids.push(question.id.clone());
        }
    }
    if canonical_question_ids.len() < 2 {
        let session_id = questions[0].session_id.clone();
        tx.rollback().await.map_err(StoreError::external)?;
        return Ok(ConversationMutationResult {
            dry_run,
            session_id,
            affected_question_ids: question_ids.to_vec(),
            questions: vec![
                load_conversation_question_detail_sqlx(pool, tenant_id, &canonical_question_ids[0])
                    .await?,
            ],
        });
    }
    reject_invalid_conversation_question_turns_sqlx_tx(&mut tx, tenant_id).await?;
    ensure_question_ids_are_adjacent_sqlx_tx(
        &mut tx,
        tenant_id,
        &session_id,
        &canonical_question_ids,
    )
    .await?;

    if dry_run {
        tx.rollback().await.map_err(StoreError::external)?;
        let mut details = Vec::with_capacity(canonical_question_ids.len());
        for question_id in &canonical_question_ids {
            details
                .push(load_conversation_question_detail_sqlx(pool, tenant_id, question_id).await?);
        }
        return Ok(ConversationMutationResult {
            dry_run: true,
            session_id,
            affected_question_ids: question_ids.to_vec(),
            questions: details,
        });
    }

    let now = Utc::now().to_rfc3339();
    let survivor_id = canonical_question_ids[0].clone();
    sqlx::query(
        "UPDATE conversation_question_turns SET assignment_origin = ?1, updated_at = ?2 WHERE tenant_id = ?3 AND question_id = ?4",
    )
    .bind(encode_enum(ConversationGroupingOrigin::Manual)?)
    .bind(&now)
    .bind(tenant_id)
    .bind(&survivor_id)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;
    for question_id in &canonical_question_ids[1..] {
        let next_order =
            max_question_turn_order_sqlx_tx(&mut tx, tenant_id, &survivor_id).await? + 1;
        let turn_ids = load_question_turn_ids_sqlx_tx(&mut tx, tenant_id, question_id).await?;
        for (offset, turn_id) in turn_ids.iter().enumerate() {
            sqlx::query(
                r#"
                UPDATE conversation_question_turns
                SET question_id = ?1,
                    turn_order = ?2,
                    assignment_origin = ?3,
                    updated_at = ?4
                WHERE tenant_id = ?5 AND question_id = ?6 AND turn_id = ?7
                "#,
            )
            .bind(&survivor_id)
            .bind(next_order + offset as i64)
            .bind(encode_enum(ConversationGroupingOrigin::Manual)?)
            .bind(&now)
            .bind(tenant_id)
            .bind(question_id)
            .bind(turn_id)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::external)?;
        }
        sqlx::query(
            r#"
            UPDATE conversation_question_redirects
            SET target_question_id = ?1, updated_at = ?2
            WHERE tenant_id = ?3 AND target_question_id = ?4
            "#,
        )
        .bind(&survivor_id)
        .bind(&now)
        .bind(tenant_id)
        .bind(question_id)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::external)?;
        sqlx::query(
            r#"
            INSERT INTO conversation_question_redirects (
                tenant_id, source_question_id, target_question_id,
                operation_kind, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, 'merge', ?4, ?4)
            ON CONFLICT(tenant_id, source_question_id) DO UPDATE SET
                target_question_id = excluded.target_question_id,
                operation_kind = excluded.operation_kind,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(tenant_id)
        .bind(question_id)
        .bind(&survivor_id)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::external)?;
    }
    for question_id in &canonical_question_ids[1..] {
        sqlx::query("DELETE FROM conversation_questions WHERE tenant_id = ?1 AND id = ?2")
            .bind(tenant_id)
            .bind(question_id)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::external)?;
        sqlx::query(
            "DELETE FROM conversation_question_fts WHERE tenant_id = ?1 AND question_id = ?2",
        )
        .bind(tenant_id)
        .bind(question_id)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::external)?;
    }
    renumber_questions_for_session_sqlx_tx(&mut tx, tenant_id, &session_id).await?;
    rebuild_session_question_aggregates_sqlx_tx(&mut tx, tenant_id, &session_id, &now).await?;
    super::bump_conversation_search_source_revision_sqlx_tx(&mut *tx, tenant_id).await?;
    tx.commit().await.map_err(StoreError::external)?;

    Ok(ConversationMutationResult {
        dry_run: false,
        session_id,
        affected_question_ids: question_ids.to_vec(),
        questions: vec![
            load_conversation_question_detail_sqlx(pool, tenant_id, &survivor_id).await?,
        ],
    })
}

pub(crate) async fn split_conversation_question_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    question_id: &str,
    before_turn_id: &str,
    dry_run: bool,
) -> StoreResult<ConversationMutationResult> {
    audit_invalid_conversation_question_turns_sqlx(pool, tenant_id).await?;
    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    let question = load_conversation_question_sqlx_tx(&mut tx, tenant_id, question_id)
        .await?
        .ok_or_else(|| {
            StoreError::external(format!("conversation question not found: {question_id}"))
        })?;
    reject_invalid_conversation_question_turns_sqlx_tx(&mut tx, tenant_id).await?;
    let turns = load_question_turns_sqlx_tx(&mut tx, tenant_id, question_id).await?;
    let new_question_id = stable_id(
        "conversation-question",
        &["split", question_id, before_turn_id],
    );
    let split_index = turns.iter().position(|turn| turn.id == before_turn_id);
    let Some(split_index) = split_index else {
        let existing_question_id = sqlx::query_scalar::<_, String>(
            "SELECT question_id FROM conversation_question_turns WHERE tenant_id = ?1 AND turn_id = ?2",
        )
        .bind(tenant_id)
        .bind(before_turn_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::external)?;
        if existing_question_id.as_deref() == Some(new_question_id.as_str())
            && load_conversation_question_sqlx_tx(&mut tx, tenant_id, &new_question_id)
                .await?
                .is_some()
        {
            tx.rollback().await.map_err(StoreError::external)?;
            return Ok(ConversationMutationResult {
                dry_run,
                session_id: question.session_id.clone(),
                affected_question_ids: vec![question_id.to_string(), new_question_id.clone()],
                questions: vec![
                    load_conversation_question_detail_sqlx(pool, tenant_id, question_id).await?,
                    load_conversation_question_detail_sqlx(pool, tenant_id, &new_question_id)
                        .await?,
                ],
            });
        }
        return Err(StoreError::external(format!(
            "turn is not in question: {before_turn_id}"
        )));
    };
    if split_index == 0 {
        return Err(StoreError::Validation(
            "split turn must not be the first turn in the question".to_string(),
        ));
    }

    if dry_run {
        tx.rollback().await.map_err(StoreError::external)?;
        return Ok(ConversationMutationResult {
            dry_run: true,
            session_id: question.session_id,
            affected_question_ids: vec![question_id.to_string()],
            questions: vec![
                load_conversation_question_detail_sqlx(pool, tenant_id, question_id).await?,
            ],
        });
    }

    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE conversation_question_turns SET assignment_origin = ?1, updated_at = ?2 WHERE tenant_id = ?3 AND question_id = ?4",
    )
    .bind(encode_enum(ConversationGroupingOrigin::Manual)?)
    .bind(&now)
    .bind(tenant_id)
    .bind(question_id)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;
    sqlx::query(
        r#"
        INSERT INTO conversation_questions (
            tenant_id, id, session_id, title, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, NULL, ?4, ?4)
        "#,
    )
    .bind(tenant_id)
    .bind(&new_question_id)
    .bind(&question.session_id)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;
    for (order, turn) in turns.iter().skip(split_index).enumerate() {
        sqlx::query(
            r#"
            UPDATE conversation_question_turns
            SET question_id = ?1,
                turn_order = ?2,
                assignment_origin = ?3,
                updated_at = ?4
            WHERE tenant_id = ?5 AND question_id = ?6 AND turn_id = ?7
            "#,
        )
        .bind(&new_question_id)
        .bind(order as i64)
        .bind(encode_enum(ConversationGroupingOrigin::Manual)?)
        .bind(&now)
        .bind(tenant_id)
        .bind(question_id)
        .bind(&turn.id)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::external)?;
    }
    renumber_question_turns_sqlx_tx(&mut tx, tenant_id, question_id, &now).await?;
    renumber_questions_for_session_sqlx_tx(&mut tx, tenant_id, &question.session_id).await?;
    rebuild_session_question_aggregates_sqlx_tx(&mut tx, tenant_id, &question.session_id, &now)
        .await?;
    super::bump_conversation_search_source_revision_sqlx_tx(&mut *tx, tenant_id).await?;
    tx.commit().await.map_err(StoreError::external)?;

    Ok(ConversationMutationResult {
        dry_run: false,
        session_id: question.session_id,
        affected_question_ids: vec![question_id.to_string(), new_question_id.clone()],
        questions: vec![
            load_conversation_question_detail_sqlx(pool, tenant_id, question_id).await?,
            load_conversation_question_detail_sqlx(pool, tenant_id, &new_question_id).await?,
        ],
    })
}
