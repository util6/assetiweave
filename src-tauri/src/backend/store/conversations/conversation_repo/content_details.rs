use super::*;

pub(crate) async fn list_conversation_question_details_sqlx(
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
                    .chain(detail.turns.iter().map(|turn| turn.user_text.clone()))
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

pub(crate) async fn load_conversation_question_detail_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    question_id: &str,
) -> StoreResult<ConversationQuestionDetail> {
    let question_row = sqlx::query(
        r#"
        SELECT id, session_id, title, created_at, updated_at
        FROM conversation_questions
        WHERE tenant_id = ?1 AND id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?
    .ok_or_else(|| {
        StoreError::external(format!("conversation question not found: {question_id}"))
    })?;
    let question = map_sqlx_conversation_question(&question_row)?;
    let question_turns = load_question_turn_memberships_sqlx(pool, tenant_id, question_id).await?;

    let turn_rows = sqlx::query(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at, t.model
        FROM conversation_question_turns qt
        JOIN conversation_turns t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        JOIN conversation_questions q
          ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        WHERE qt.tenant_id = ?1
          AND qt.question_id = ?2
          AND q.session_id = t.session_id
        ORDER BY qt.turn_order ASC, t.turn_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    let turns = turn_rows
        .iter()
        .map(map_sqlx_conversation_turn)
        .collect::<StoreResult<Vec<_>>>()?;

    let part_rows = sqlx::query(
        r#"
        SELECT p.id, p.turn_id, p.part_index, p.role, p.kind, p.text, p.language,
               p.command, p.cwd, p.status, p.exit_code, p.metadata_json,
               p.content_card_json, p.translated_text, p.source_execution_id, p.command_label
        FROM conversation_parts p
        JOIN conversation_question_turns qt ON qt.tenant_id = p.tenant_id AND qt.turn_id = p.turn_id
        JOIN conversation_questions q
          ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        JOIN conversation_turns t
          ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
        WHERE qt.tenant_id = ?1
          AND qt.question_id = ?2
          AND q.session_id = t.session_id
        ORDER BY qt.turn_order ASC, p.part_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    let parts = part_rows
        .iter()
        .map(map_sqlx_conversation_part)
        .collect::<StoreResult<Vec<_>>>()?;
    let (adapter_id, card_kinds) =
        load_conversation_card_projection_context_sqlx(pool, tenant_id, &question.session_id)
            .await?;
    let projected_content_nodes = project_question_content_nodes(
        &question.id,
        &question_turns,
        &parts,
        &adapter_id,
        &card_kinds,
    )?;
    Ok(ConversationQuestionDetail {
        question: project_question_title(question, &turns),
        question_turns,
        turns,
        parts,
        projected_content_nodes,
    })
}

pub(crate) async fn list_conversation_block_locators_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    question_id: &str,
) -> StoreResult<Vec<ConversationBlockLocator>> {
    let tables = record_kind.tables();
    let session_id = sqlx::query_scalar::<_, String>(AssertSqlSafe(format!(
        "SELECT session_id FROM {} WHERE tenant_id = ?1 AND id = ?2",
        tables.questions
    )))
    .bind(tenant_id)
    .bind(question_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?
    .ok_or_else(|| {
        StoreError::external(format!("conversation question not found: {question_id}"))
    })?;

    let turn_rows = sqlx::query(AssertSqlSafe(format!(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at
        FROM {question_turns} qt
        JOIN {turns} t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1 AND qt.question_id = ?2
        ORDER BY qt.turn_order ASC, t.turn_index ASC
        "#,
        question_turns = tables.question_turns,
        turns = tables.turns,
    )))
    .bind(tenant_id)
    .bind(question_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    let turns = turn_rows
        .iter()
        .map(map_sqlx_conversation_turn)
        .collect::<StoreResult<Vec<_>>>()?;

    let part_rows = sqlx::query(AssertSqlSafe(format!(
        r#"
        SELECT p.id, p.turn_id, p.part_index, p.role, p.kind, p.text, p.language,
               p.command, p.cwd, p.status, p.exit_code, p.metadata_json,
               p.content_card_json, p.translated_text, p.source_execution_id, p.command_label
        FROM {question_turns} qt
        JOIN {parts} p ON p.tenant_id = qt.tenant_id AND p.turn_id = qt.turn_id
        WHERE qt.tenant_id = ?1 AND qt.question_id = ?2
        ORDER BY qt.turn_order ASC, p.part_index ASC
        "#,
        question_turns = tables.question_turns,
        parts = tables.parts,
    )))
    .bind(tenant_id)
    .bind(question_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    let parts = part_rows
        .iter()
        .map(map_sqlx_conversation_part)
        .collect::<StoreResult<Vec<_>>>()?;
    let (adapter_id, card_kinds) = load_conversation_card_projection_context_for_record_sqlx(
        pool,
        tenant_id,
        record_kind,
        &session_id,
    )
    .await?;
    let cards = parts.iter().try_fold(Vec::new(), |mut projected, part| {
        projected.extend(
            crate::backend::domain::conversations::projection::project_conversation_content_cards(
                part,
                &adapter_id,
                &card_kinds,
            )?,
        );
        Ok::<_, StoreError>(projected)
    })?;
    let parts_by_id = parts
        .iter()
        .map(|part| (part.id.as_str(), part))
        .collect::<BTreeMap<_, _>>();

    let mut locators = Vec::with_capacity(turns.len() + cards.len());
    for turn in &turns {
        locators.push(conversation_question_block_locator(
            record_kind,
            &session_id,
            question_id,
            turn,
        ));
    }
    for card in &cards {
        if let Some(part) = parts_by_id.get(card.part_id.as_str()) {
            locators.push(conversation_card_block_locator(
                record_kind,
                &session_id,
                question_id,
                part,
                card,
            ));
        }
    }
    Ok(locators)
}

#[derive(Debug, FromRow)]
pub(super) struct ConversationTurnWithQuestionRow {
    id: String,
    session_id: String,
    external_id: String,
    turn_index: i64,
    user_text: String,
    title: Option<String>,
    started_at: Option<String>,
    ended_at: Option<String>,
    model: Option<String>,
    fingerprint: String,
    missing: i64,
    imported_at: String,
    question_id: String,
}

impl ConversationTurnWithQuestionRow {
    pub(super) fn into_turn(self) -> (String, ConversationTurn) {
        let question_id = self.question_id;
        let turn = ConversationTurn {
            id: self.id,
            session_id: self.session_id,
            external_id: self.external_id,
            turn_index: self.turn_index,
            user_text: self.user_text,
            title: self.title,
            started_at: self.started_at,
            ended_at: self.ended_at,
            model: self.model,
            fingerprint: self.fingerprint,
            missing: self.missing == 1,
            imported_at: self.imported_at,
        };
        (question_id, turn)
    }
}

#[derive(Debug, FromRow)]
pub(super) struct ConversationPartDetailRow {
    id: String,
    turn_id: String,
    part_index: i64,
    role: String,
    kind: String,
    text: Option<String>,
    language: Option<String>,
    command: Option<String>,
    cwd: Option<String>,
    status: Option<String>,
    exit_code: Option<i64>,
    metadata_json: Option<String>,
    content_card_json: Option<String>,
    translated_text: Option<String>,
    source_execution_id: Option<String>,
    command_label: Option<String>,
    question_id: String,
    session_id: String,
}

impl ConversationPartDetailRow {
    fn into_part(self) -> StoreResult<(ConversationPart, String, String)> {
        let part = ConversationPart {
            id: self.id,
            turn_id: self.turn_id,
            part_index: self.part_index,
            role: decode_enum(self.role)?,
            kind: decode_enum(self.kind)?,
            text: self.text,
            language: self.language,
            command: self.command,
            cwd: self.cwd,
            status: self.status,
            exit_code: self.exit_code.map(|v| v as i32),
            command_label: self.command_label,
            source_execution_id: self.source_execution_id,
            content_card: self.content_card_json.map(decode_json).transpose()?,
            metadata_json: self.metadata_json,
            translated_text: self.translated_text,
        };
        Ok((part, self.question_id, self.session_id))
    }
}

pub(crate) async fn load_conversation_block_detail_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    block_id: &str,
) -> StoreResult<ConversationBlockDetail> {
    let tables = record_kind.tables();
    if let Some(turn_id) = block_id.strip_suffix("-question") {
        let resolved_turn_id = resolve_conversation_turn_id_prefix_sqlx(pool, tenant_id, turn_id)
            .await
            .unwrap_or_else(|_| turn_id.to_string());
        let row = sqlx::query_as::<_, ConversationTurnWithQuestionRow>(AssertSqlSafe(format!(
            r#"
            SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
                   t.started_at, t.ended_at, t.model, t.fingerprint, t.missing, t.imported_at,
                   qt.question_id
            FROM {turns} t
            JOIN {question_turns} qt ON qt.tenant_id = t.tenant_id AND qt.turn_id = t.id
            WHERE t.tenant_id = ?1 AND t.id = ?2
            "#,
            turns = tables.turns,
            question_turns = tables.question_turns,
        )))
        .bind(tenant_id)
        .bind(&resolved_turn_id)
        .fetch_optional(pool)
        .await
        .map_err(StoreError::external)?
        .ok_or_else(|| {
            StoreError::external(format!("conversation question block not found: {block_id}"))
        })?;
        let (question_id, turn) = row.into_turn();
        let locator =
            conversation_question_block_locator(record_kind, &turn.session_id, &question_id, &turn);
        return Ok(ConversationBlockDetail {
            locator,
            content: turn.user_text,
            translated_content: None,
        });
    }

    let raw_part_id = conversation_part_id_for_block_id(block_id);
    let resolved_part_id = resolve_conversation_part_id_prefix_sqlx(pool, tenant_id, raw_part_id)
        .await
        .unwrap_or_else(|_| raw_part_id.to_string());
    let row = sqlx::query_as::<_, ConversationPartDetailRow>(AssertSqlSafe(format!(
        r#"
        SELECT p.id, p.turn_id, p.part_index, p.role, p.kind, p.text, p.language,
               p.command, p.cwd, p.status, p.exit_code, p.metadata_json,
               p.content_card_json, p.translated_text, p.source_execution_id, p.command_label,
               qt.question_id, t.session_id
        FROM {parts} p
        JOIN {question_turns} qt ON qt.tenant_id = p.tenant_id AND qt.turn_id = p.turn_id
        JOIN {turns} t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
        WHERE p.tenant_id = ?1 AND p.id = ?2
        "#,
        parts = tables.parts,
        question_turns = tables.question_turns,
        turns = tables.turns,
    )))
    .bind(tenant_id)
    .bind(&resolved_part_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?
    .ok_or_else(|| {
        StoreError::external(format!("conversation content block not found: {block_id}"))
    })?;
    let (part, question_id, session_id) = row.into_part()?;
    let (adapter_id, card_kinds) = load_conversation_card_projection_context_for_record_sqlx(
        pool,
        tenant_id,
        record_kind,
        &session_id,
    )
    .await?;
    let cards =
        crate::backend::domain::conversations::projection::project_conversation_content_cards(
            &part,
            &adapter_id,
            &card_kinds,
        )?;
    if cards.is_empty() {
        let content = part
            .text
            .clone()
            .or_else(|| part.command.clone())
            .unwrap_or_default();
        let locator = ConversationBlockLocator {
            record_kind: conversation_record_kind_label(record_kind).to_string(),
            session_id: session_id.clone(),
            question_id: question_id.clone(),
            turn_id: part.turn_id.clone(),
            block_id: block_id.to_string(),
            part_id: Some(part.id.clone()),
            kind: part.kind.as_str().to_string(),
            semantic_role: None,
            renderer: ConversationCardRenderer::Plain,
            role: part.role,
            content_length: content.chars().count(),
            language: part.language.clone(),
            cwd: part.cwd.clone(),
            status: part.status.clone(),
            exit_code: part.exit_code,
        };
        return Ok(ConversationBlockDetail {
            locator,
            content,
            translated_content: part.translated_text.clone(),
        });
    }
    let card = cards
        .iter()
        .find(|card| card.node_id == block_id || card.node_id == resolved_part_id)
        .or_else(|| cards.first())
        .ok_or_else(|| {
            StoreError::external(format!(
                "conversation block is not a readable content card: {block_id}"
            ))
        })?;
    let mut locator =
        conversation_card_block_locator(record_kind, &session_id, &question_id, &part, card);
    if block_id == part.id
        || raw_part_id == resolved_part_id
        || crate::backend::domain::conversation_id_fragment(&part.id) == block_id
    {
        locator.block_id = block_id.to_string();
    }
    Ok(ConversationBlockDetail {
        locator,
        content: card.body.clone(),
        translated_content: card.translated_body.clone(),
    })
}

pub(super) async fn resolve_conversation_question_redirect_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
) -> StoreResult<String> {
    let mut current = question_id.to_string();
    let mut visited = BTreeSet::new();
    loop {
        if !visited.insert(current.clone()) {
            return Err(StoreError::Validation(format!(
                "conversation question redirect cycle: {question_id}"
            )));
        }
        let Some(target) = sqlx::query_scalar::<_, String>(
            "SELECT target_question_id FROM conversation_question_redirects WHERE tenant_id = ?1 AND source_question_id = ?2",
        )
        .bind(tenant_id)
        .bind(&current)
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::external)? else {
            return Ok(current);
        };
        current = target;
    }
}
