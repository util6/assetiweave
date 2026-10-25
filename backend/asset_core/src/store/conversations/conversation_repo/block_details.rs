use super::*;

pub async fn list_conversation_block_locators_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    question_id: &str,
) -> StoreResult<Vec<ConversationBlockLocator>> {
    let question_id =
        resolve_conversation_question_redirect_pool_sqlx(pool, tenant_id, question_id).await?;
    let detail = load_conversation_question_detail_sqlx(pool, tenant_id, &question_id).await?;
    let (adapter_id, card_kinds) = load_conversation_card_projection_context_for_record_sqlx(
        pool,
        tenant_id,
        record_kind,
        &detail.question.session_id,
    )
    .await?;

    let cards = detail
        .parts
        .iter()
        .try_fold(Vec::new(), |mut projected, part| {
            projected.extend(
                crate::backend::domain::conversations::projection::project_conversation_content_cards(
                    part,
                    &adapter_id,
                    &card_kinds,
                )?,
            );
            Ok::<_, StoreError>(projected)
        })?;
    let parts_by_id = detail
        .parts
        .iter()
        .map(|part| (part.id.as_str(), part))
        .collect::<BTreeMap<_, _>>();

    let mut locators = Vec::with_capacity(detail.turns.len() + cards.len());
    for turn in &detail.turns {
        locators.push(conversation_question_block_locator(
            record_kind,
            &detail.question.session_id,
            &detail.question.id,
            turn,
        ));
    }
    for card in &cards {
        if let Some(part) = parts_by_id.get(card.part_id.as_str()) {
            locators.push(conversation_card_block_locator(
                record_kind,
                &detail.question.session_id,
                &detail.question.id,
                part,
                card,
            ));
        }
    }
    Ok(locators)
}

#[derive(Debug, FromRow)]
pub(super) struct ConversationPartDetailRow {
    turn_id: String,
    part_index: i64,
    kind: String,
    content: Option<String>,
    attrs_json: Option<String>,
    translated_text: Option<String>,
    session_id: String,
}

impl ConversationPartDetailRow {
    fn into_part(self) -> StoreResult<(ConversationPart, String)> {
        let session_id = self.session_id;
        let part_row = ConversationPartRow {
            turn_id: self.turn_id,
            part_index: self.part_index,
            kind: self.kind,
            content: self.content,
            attrs_json: self.attrs_json,
            translated_text: self.translated_text,
        };
        let part = part_row.into_domain()?;
        Ok((part, session_id))
    }
}

pub async fn load_conversation_block_detail_sqlx(
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
        let detail =
            load_conversation_question_detail_sqlx(pool, tenant_id, &resolved_turn_id).await?;
        let turn = detail
            .turns
            .iter()
            .find(|t| t.id == resolved_turn_id)
            .ok_or_else(|| {
                StoreError::external(format!("conversation question block not found: {block_id}"))
            })?;
        let locator = conversation_question_block_locator(
            record_kind,
            &detail.question.session_id,
            &detail.question.id,
            turn,
        );
        return Ok(ConversationBlockDetail {
            locator,
            content: turn.prompt.clone(),
            translated_content: None,
        });
    }

    let raw_part_id = conversation_part_id_for_block_id(block_id);
    let resolved_part_id =
        match resolve_conversation_part_id_prefix_sqlx(pool, tenant_id, raw_part_id).await {
            Ok(id) => id,
            Err(StoreError::Conflict(e)) => return Err(StoreError::Conflict(e)),
            Err(_) => raw_part_id.to_string(),
        };

    let key = crate::backend::domain::derive_part_id_key(&resolved_part_id).unwrap_or([0u8; 8]);
    let query = format!(
        r#"
        SELECT {part_columns}, t.session_id
        FROM {parts} p
        JOIN {turns} t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
        JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id
        WHERE p.tenant_id = ?1 AND p.id_key = ?2
        "#,
        parts = tables.parts,
        turns = tables.turns,
        sessions = tables.sessions,
        part_columns = tables.part_columns,
    );
    let row = sqlx::query_as::<_, ConversationPartDetailRow>(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(key.to_vec())
        .fetch_optional(pool)
        .await
        .map_err(StoreError::external)?;
    let (part, session_id) = match row {
        Some(r) => r.into_part()?,
        None => {
            if let Some(detail) = triad_locator::load_triad_block_detail(
                pool,
                tenant_id,
                record_kind,
                &resolved_part_id,
                block_id,
            )
            .await?
            {
                return Ok(detail);
            }
            return Err(StoreError::external(format!(
                "conversation content block not found: {block_id}"
            )));
        }
    };
    let question_detail =
        load_conversation_question_detail_sqlx(pool, tenant_id, &part.turn_id).await?;
    let question_id = question_detail.question.id;
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
            session_id: session_id.to_string(),
            question_id: question_id.clone(),
            turn_id: part.turn_id.clone(),
            block_id: block_id.to_string(),
            part_id: Some(part.id.clone()),
            kind: part.kind.as_str().to_string(),
            label: part.command_label.clone(),
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
