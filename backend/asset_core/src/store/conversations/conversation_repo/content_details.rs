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

/// 单问题详情：分组身份取自轮次列，成员与顺序由轮次序列重放。
///
/// 与 `load_conversation_question_details_for_session_sqlx` 共用同一套派生实现，
/// 因此问题详情、block locator 与导出对同一分组必然给出一致的定位信息。
pub async fn load_conversation_question_detail_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    question_id: &str,
) -> StoreResult<ConversationQuestionDetail> {
    let question_id =
        resolve_conversation_question_redirect_pool_sqlx(pool, tenant_id, question_id).await?;
    let session_id = load_question_session_id_for_record_sqlx(
        pool,
        tenant_id,
        ConversationRecordKind::Session,
        &question_id,
    )
    .await?
    .ok_or_else(|| {
        StoreError::external(format!("conversation question not found: {question_id}"))
    })?;

    let turn_rows = sqlx::query_as::<_, ConversationTurnQuestionRow>(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.model, t.fingerprint, t.missing, t.imported_at,
               t.question_id, t.question_origin
        FROM conversation_turns t INDEXED BY idx_conversation_turns_tenant_session_question
        WHERE t.tenant_id = ?1 AND t.session_id = ?2 AND t.question_id = ?3
        ORDER BY t.turn_index ASC, t.id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(&session_id)
    .bind(&question_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    let turns = turn_rows
        .into_iter()
        .map(ConversationTurnQuestionRow::into_grouped_turn)
        .collect::<Vec<_>>();

    let part_rows = sqlx::query(
        r#"
        SELECT p.turn_id, p.part_index, p.role, p.kind, p.card_kind,
               p.content, p.attrs_json, p.metadata_json, p.translated_text
        FROM conversation_turns t INDEXED BY idx_conversation_turns_tenant_session_question
        JOIN conversation_parts p
          ON p.tenant_id = t.tenant_id AND p.turn_id = t.id
        WHERE t.tenant_id = ?1 AND t.session_id = ?2 AND t.question_id = ?3
        ORDER BY t.turn_index ASC, p.part_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(&session_id)
    .bind(&question_id)
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

    let (adapter_id, card_kinds) =
        load_conversation_card_projection_context_sqlx(pool, tenant_id, &session_id).await?;
    build_question_details_from_turn_columns(turns, parts_by_turn, &adapter_id, &card_kinds)?
        .into_iter()
        .next()
        .ok_or_else(|| {
            StoreError::external(format!("conversation question not found: {question_id}"))
        })
}

pub async fn list_conversation_block_locators_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    question_id: &str,
) -> StoreResult<Vec<ConversationBlockLocator>> {
    let tables = record_kind.tables();
    let question_id =
        resolve_conversation_question_redirect_pool_sqlx(pool, tenant_id, question_id).await?;
    let session_id =
        load_question_session_id_for_record_sqlx(pool, tenant_id, record_kind, &question_id)
            .await?
            .ok_or_else(|| {
                StoreError::external(format!("conversation question not found: {question_id}"))
            })?;

    let turn_rows = sqlx::query(AssertSqlSafe(format!(
        r#"
        SELECT id, session_id, external_id, turn_index, user_text, title,
               started_at, ended_at, fingerprint, missing, imported_at, model
        FROM {turns}
        WHERE tenant_id = ?1 AND session_id = ?2 AND question_id = ?3
        ORDER BY turn_index ASC, id ASC
        "#,
        turns = tables.turns,
    )))
    .bind(tenant_id)
    .bind(&session_id)
    .bind(&question_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    let turns = turn_rows
        .iter()
        .map(map_sqlx_conversation_turn)
        .collect::<StoreResult<Vec<_>>>()?;

    let part_rows = sqlx::query(AssertSqlSafe(format!(
        r#"
        SELECT {part_columns}
        FROM {turns} t
        JOIN {parts} p ON p.tenant_id = t.tenant_id AND p.turn_id = t.id
        JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id
        WHERE t.tenant_id = ?1 AND t.session_id = ?2 AND t.question_id = ?3
        ORDER BY t.turn_index ASC, p.part_index ASC
        "#,
        turns = tables.turns,
        parts = tables.parts,
        sessions = tables.sessions,
        part_columns = tables.part_columns,
    )))
    .bind(tenant_id)
    .bind(&session_id)
    .bind(&question_id)
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
            &question_id,
            turn,
        ));
    }
    for card in &cards {
        if let Some(part) = parts_by_id.get(card.part_id.as_str()) {
            locators.push(conversation_card_block_locator(
                record_kind,
                &session_id,
                &question_id,
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
    turn_id: String,
    part_index: i64,
    role: String,
    kind: String,
    card_kind: Option<String>,
    content: Option<String>,
    attrs_json: Option<String>,
    metadata_json: Option<String>,
    translated_text: Option<String>,
    question_id: String,
    session_id: String,
}

impl ConversationPartDetailRow {
    fn into_part(self) -> StoreResult<(ConversationPart, String, String)> {
        let question_id = self.question_id;
        let session_id = self.session_id;
        let part_row = ConversationPartRow {
            turn_id: self.turn_id,
            part_index: self.part_index,
            role: self.role,
            kind: self.kind,
            card_kind: self.card_kind,
            content: self.content,
            attrs_json: self.attrs_json,
            metadata_json: self.metadata_json,
            translated_text: self.translated_text,
        };
        let part = part_row.into_domain()?;
        Ok((part, question_id, session_id))
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
        let row = sqlx::query_as::<_, ConversationTurnWithQuestionRow>(AssertSqlSafe(format!(
            r#"
            SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
                   t.started_at, t.ended_at, t.model, t.fingerprint, t.missing, t.imported_at,
                   t.question_id
            FROM {turns} t
            WHERE t.tenant_id = ?1 AND t.id = ?2
            "#,
            turns = tables.turns,
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
        let question_id =
            resolve_conversation_question_redirect_pool_sqlx(pool, tenant_id, &question_id).await?;
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

    let key = crate::backend::domain::derive_part_id_key(&resolved_part_id).unwrap_or([0u8; 8]);
    let query = format!(
        r#"
        SELECT {part_columns}, t.question_id, t.session_id
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

/// 分组身份 → 所属会话。分组标识只落在轮次列上，问题表与成员表不再参与解析。
pub(super) async fn load_question_session_id_for_record_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    question_id: &str,
) -> StoreResult<Option<String>> {
    let tables = record_kind.tables();
    sqlx::query_scalar::<_, String>(AssertSqlSafe(format!(
        "SELECT session_id FROM {} WHERE tenant_id = ?1 AND question_id = ?2 LIMIT 1",
        tables.turns
    )))
    .bind(tenant_id)
    .bind(question_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)
}

pub(super) async fn resolve_conversation_question_redirect_sqlx(
    conn: &mut sqlx::SqliteConnection,
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
        .fetch_optional(&mut *conn)
        .await
        .map_err(StoreError::external)? else {
            return Ok(current);
        };
        current = target;
    }
}

pub(super) async fn resolve_conversation_question_redirect_pool_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    question_id: &str,
) -> StoreResult<String> {
    let mut conn = pool.acquire().await.map_err(StoreError::external)?;
    resolve_conversation_question_redirect_sqlx(&mut conn, tenant_id, question_id).await
}
