use super::*;

pub(crate) async fn update_conversation_part_translation_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    part_id: &str,
    translated_text: &str,
) -> StoreResult<()> {
    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    let result = sqlx::query(
        r#"
        UPDATE conversation_parts
        SET translated_text = ?1
        WHERE tenant_id = ?2 AND id = ?3
        "#,
    )
    .bind(translated_text)
    .bind(tenant_id)
    .bind(part_id)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;

    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound(format!(
            "conversation part not found: {part_id}"
        )));
    }

    super::bump_conversation_search_source_revision_sqlx_tx(&mut *tx, tenant_id).await?;
    tx.commit().await.map_err(StoreError::external)?;
    Ok(())
}

pub(super) async fn load_conversation_question_details_for_session_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<Vec<ConversationQuestionDetail>> {
    let (adapter_id, card_kinds) =
        load_conversation_card_projection_context_sqlx(pool, tenant_id, session_id).await?;
    let question_rows = sqlx::query(
        r#"
        SELECT id, session_id, title, created_at, updated_at
        FROM conversation_questions
        WHERE tenant_id = ?1 AND session_id = ?2
        ORDER BY COALESCE((
            SELECT MIN(t.turn_index)
            FROM conversation_question_turns qt_order
            JOIN conversation_turns t
              ON t.tenant_id = qt_order.tenant_id AND t.id = qt_order.turn_id
            WHERE qt_order.tenant_id = conversation_questions.tenant_id
              AND qt_order.question_id = conversation_questions.id
        ), 9223372036854775807) ASC, created_at ASC, id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    let questions = question_rows
        .iter()
        .map(map_sqlx_conversation_question)
        .collect::<StoreResult<Vec<_>>>()?;

    let question_turn_rows = sqlx::query(
        r#"
        SELECT qt.question_id, qt.turn_id, qt.turn_order,
               qt.assignment_origin, qt.assigned_at, qt.updated_at
        FROM conversation_question_turns qt
        JOIN conversation_questions q
          ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        JOIN conversation_turns t
          ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1
          AND q.session_id = ?2
          AND q.session_id = t.session_id
        ORDER BY COALESCE((SELECT MIN(t_order.turn_index) FROM conversation_question_turns qt_order JOIN conversation_turns t_order ON t_order.tenant_id = qt_order.tenant_id AND t_order.id = qt_order.turn_id WHERE qt_order.tenant_id = q.tenant_id AND qt_order.question_id = q.id), 9223372036854775807) ASC, qt.turn_order ASC, t.turn_index ASC,
                 qt.turn_id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    let mut question_turns_by_question = BTreeMap::<String, Vec<ConversationQuestionTurn>>::new();
    for row in &question_turn_rows {
        let membership = map_sqlx_conversation_question_turn(row)?;
        question_turns_by_question
            .entry(membership.question_id.clone())
            .or_default()
            .push(membership);
    }

    let turn_rows = sqlx::query_as::<_, ConversationTurnWithQuestionRow>(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at,
               qt.question_id
        FROM conversation_question_turns qt
        JOIN conversation_turns t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        JOIN conversation_questions q ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        WHERE q.tenant_id = ?1
          AND q.session_id = ?2
          AND q.session_id = t.session_id
        ORDER BY COALESCE((SELECT MIN(t_order.turn_index) FROM conversation_question_turns qt_order JOIN conversation_turns t_order ON t_order.tenant_id = qt_order.tenant_id AND t_order.id = qt_order.turn_id WHERE qt_order.tenant_id = q.tenant_id AND qt_order.question_id = q.id), 9223372036854775807) ASC, qt.turn_order ASC, t.turn_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    let mut turns_by_question = BTreeMap::<String, Vec<ConversationTurn>>::new();
    for row in turn_rows {
        let (question_id, turn) = row.into_turn();
        turns_by_question.entry(question_id).or_default().push(turn);
    }

    let part_rows = sqlx::query(
        r#"
        SELECT p.id, p.turn_id, p.part_index, p.role, p.kind, p.text, p.language,
               p.command, p.cwd, p.status, p.exit_code, p.metadata_json,
               p.content_card_json, p.translated_text, p.source_execution_id, p.command_label
        FROM conversation_turns t INDEXED BY idx_conversation_turns_tenant_session
        JOIN conversation_parts p INDEXED BY idx_conversation_parts_tenant_turn
          ON p.tenant_id = t.tenant_id AND p.turn_id = t.id
        WHERE t.tenant_id = ?1 AND t.session_id = ?2
        ORDER BY t.turn_index ASC, p.part_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    let mut parts_by_turn = BTreeMap::<String, Vec<ConversationPart>>::new();
    for row in &part_rows {
        let part = map_sqlx_conversation_part(row)?;
        parts_by_turn
            .entry(part.turn_id.clone())
            .or_default()
            .push(part);
    }

    let mut details = Vec::with_capacity(questions.len());
    for question in questions {
        let question_turns = question_turns_by_question
            .remove(&question.id)
            .unwrap_or_default();
        let turns = turns_by_question.remove(&question.id).unwrap_or_default();
        let mut parts = Vec::new();
        for turn in &turns {
            parts.extend(parts_by_turn.remove(&turn.id).unwrap_or_default());
        }
        let projected_content_nodes = project_question_content_nodes(
            &question.id,
            &question_turns,
            &parts,
            &adapter_id,
            &card_kinds,
        )?;
        details.push(ConversationQuestionDetail {
            question: project_question_title(question, &turns),
            question_turns,
            turns,
            parts,
            projected_content_nodes,
        });
    }
    Ok(details)
}

pub(super) async fn load_question_turn_memberships_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    question_id: &str,
) -> StoreResult<Vec<ConversationQuestionTurn>> {
    let rows = sqlx::query(
        r#"
        SELECT qt.question_id, qt.turn_id, qt.turn_order,
               qt.assignment_origin, qt.assigned_at, qt.updated_at
        FROM conversation_question_turns qt
        JOIN conversation_questions q
          ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        JOIN conversation_turns t
          ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1
          AND qt.question_id = ?2
          AND q.session_id = t.session_id
        ORDER BY qt.turn_order ASC, t.turn_index ASC, qt.turn_id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    rows.iter()
        .map(map_sqlx_conversation_question_turn)
        .collect()
}

pub(crate) fn project_question_title(
    mut question: ConversationQuestion,
    turns: &[ConversationTurn],
) -> ConversationQuestion {
    if question
        .title
        .as_deref()
        .is_none_or(|title| title.trim().is_empty())
    {
        if let Some(turn) = turns.iter().find(|turn| !turn.user_text.trim().is_empty()) {
            question.title = Some(first_line(&turn.user_text));
        }
    }
    question
}

pub(super) async fn load_conversation_card_projection_context_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<(String, Vec<ConversationCardKindDefinition>)> {
    load_conversation_card_projection_context_for_record_sqlx(
        pool,
        tenant_id,
        ConversationRecordKind::Session,
        session_id,
    )
    .await
}

pub(super) async fn load_conversation_card_projection_context_for_record_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    session_id: &str,
) -> StoreResult<(String, Vec<ConversationCardKindDefinition>)> {
    let tables = record_kind.tables();
    let adapter_id = sqlx::query_scalar::<_, String>(AssertSqlSafe(format!(
        "SELECT adapter_id FROM {} WHERE tenant_id = ?1 AND id = ?2",
        tables.sessions
    )))
    .bind(tenant_id)
    .bind(session_id)
    .fetch_one(pool)
    .await
    .map_err(StoreError::external)?;
    let card_kinds_json = sqlx::query_scalar::<_, String>(
        "SELECT card_kinds_json FROM conversation_adapters WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(&adapter_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?
    .unwrap_or_else(|| "[]".to_string());
    Ok((adapter_id, decode_json(card_kinds_json)?))
}

pub(super) fn conversation_question_block_locator(
    record_kind: ConversationRecordKind,
    session_id: &str,
    question_id: &str,
    turn: &ConversationTurn,
) -> ConversationBlockLocator {
    ConversationBlockLocator {
        record_kind: conversation_record_kind_label(record_kind).to_string(),
        session_id: session_id.to_string(),
        question_id: question_id.to_string(),
        turn_id: turn.id.clone(),
        block_id: format!("{}-question", turn.id),
        part_id: None,
        kind: "question".to_string(),
        semantic_role: None,
        renderer: ConversationCardRenderer::Plain,
        role: crate::backend::domain::ConversationPartRole::User,
        content_length: turn.user_text.chars().count(),
        language: None,
        cwd: None,
        status: None,
        exit_code: None,
    }
}

pub(super) fn conversation_card_block_locator(
    record_kind: ConversationRecordKind,
    session_id: &str,
    question_id: &str,
    part: &ConversationPart,
    card: &ConversationCard,
) -> ConversationBlockLocator {
    ConversationBlockLocator {
        record_kind: conversation_record_kind_label(record_kind).to_string(),
        session_id: session_id.to_string(),
        question_id: question_id.to_string(),
        turn_id: part.turn_id.clone(),
        block_id: card.node_id.clone(),
        part_id: Some(part.id.clone()),
        kind: card.kind.clone(),
        semantic_role: card.semantic_role.clone().or_else(|| {
            card.kind
                .rsplit_once('.')
                .map(|(_, value)| value.to_string())
        }),
        renderer: card.renderer,
        role: card.role.clone(),
        content_length: card.body.chars().count(),
        language: card.language.clone(),
        cwd: card.cwd.clone(),
        status: card.status.clone(),
        exit_code: card.exit_code,
    }
}

pub(crate) fn conversation_part_id_for_block_id(block_id: &str) -> &str {
    block_id
        .rsplit_once("-node-")
        .filter(|(_, order)| !order.is_empty() && order.chars().all(|value| value.is_ascii_digit()))
        .map(|(part_id, _)| part_id)
        .unwrap_or(block_id)
}

pub(super) fn conversation_record_kind_label(record_kind: ConversationRecordKind) -> &'static str {
    match record_kind {
        ConversationRecordKind::Session => "session",
        ConversationRecordKind::Web => "web",
    }
}

pub(crate) fn project_question_content_nodes(
    question_id: &str,
    question_turns: &[ConversationQuestionTurn],
    parts: &[ConversationPart],
    adapter_id: &str,
    card_kinds: &[ConversationCardKindDefinition],
) -> StoreResult<Vec<ConversationContentNode>> {
    Ok(project_conversation_content_nodes(
        question_id,
        question_turns,
        parts,
        |part| {
            crate::backend::domain::conversations::projection::project_conversation_content_cards(
                part, adapter_id, card_kinds,
            )
            .map(|cards| {
                cards
                    .into_iter()
                    .map(ConversationContentNodeCandidate::from)
                    .collect()
            })
        },
    )?)
}
