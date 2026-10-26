use super::*;

pub async fn update_conversation_part_translation_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    part_id: &str,
    translated_text: &str,
) -> StoreResult<()> {
    let id_key = crate::backend::domain::derive_part_id_key(part_id)
        .ok_or_else(|| StoreError::NotFound(format!("invalid conversation part id: {part_id}")))?;
    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    let result = sqlx::query(
        r#"
        UPDATE conversation_parts
        SET translated_text = ?1
        WHERE tenant_id = ?2 AND id_key = ?3
        "#,
    )
    .bind(translated_text)
    .bind(tenant_id)
    .bind(id_key.as_slice())
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;

    if result.rows_affected() == 0 {
        let updated = triad_locator::update_triad_part_translation_tx(
            &mut tx,
            tenant_id,
            ConversationRecordKind::Session,
            part_id,
            translated_text,
        )
        .await?;
        if !updated {
            return Err(StoreError::NotFound(format!(
                "conversation part not found: {part_id}"
            )));
        }
    }

    super::bump_conversation_search_source_revision_sqlx_tx(&mut *tx, tenant_id).await?;
    tx.commit().await.map_err(StoreError::external)?;
    Ok(())
}

/// 轮次上的分组两列（`question_id` / `question_origin`），读路径的唯一分组事实源。
#[derive(Debug, FromRow)]
pub(crate) struct ConversationTurnQuestionRow {
    id: String,
    session_id: String,
    external_id: String,
    turn_index: i64,
    prompt: String,
    process: Option<String>,
    response: Option<String>,
    title: Option<String>,
    started_at: Option<String>,
    ended_at: Option<String>,
    model: Option<String>,
    fingerprint: String,
    missing: i64,
    imported_at: String,
    question_id: String,
    question_origin: String,
}

impl ConversationTurnQuestionRow {
    pub(crate) fn into_grouped_turn(
        self,
    ) -> (String, ConversationGroupingOrigin, ConversationTurn) {
        let question_id = self.question_id;
        let origin = decode_turn_question_origin(&self.question_origin);
        let turn = ConversationTurn {
            id: self.id,
            session_id: self.session_id,
            external_id: self.external_id,
            turn_index: self.turn_index,
            prompt: self.prompt,
            title: self.title,
            started_at: self.started_at,
            ended_at: self.ended_at,
            fingerprint: self.fingerprint,
            missing: self.missing == 1,
            imported_at: self.imported_at,
            model: self.model,
            process: self.process,
            response: self.response,
        };
        (question_id, origin, turn)
    }
}

/// 由轮次列即时派生问题详情：分组顺序取组内首轮位置，成员顺序取轮次顺序。
///
/// 返回结构（问题 + 成员 + 轮次 + 零件 + 投影节点）与成员表时代的对外契约逐字段一致，
/// 成员部分不再持久化，而是按 `turn_order` 从轮次序列重放：`turn_order` 即组内位次，
/// 成员的时间戳取轮次自身的导入时刻，问题时间取首轮 `COALESCE(started_at, imported_at)`。
pub(crate) fn build_question_details_from_turn_columns(
    turns: Vec<(String, ConversationGroupingOrigin, ConversationTurn)>,
    mut parts_by_turn: BTreeMap<String, Vec<ConversationPart>>,
    adapter_id: &str,
    card_kinds: &[ConversationCardKindDefinition],
) -> StoreResult<Vec<ConversationQuestionDetail>> {
    let mut group_order: Vec<String> = Vec::new();
    let mut grouped: BTreeMap<String, Vec<(ConversationGroupingOrigin, ConversationTurn)>> =
        BTreeMap::new();
    for (question_id, origin, turn) in turns {
        if !grouped.contains_key(&question_id) {
            group_order.push(question_id.clone());
        }
        grouped.entry(question_id).or_default().push((origin, turn));
    }

    let mut details = Vec::with_capacity(group_order.len());
    for question_id in group_order {
        let members = grouped.remove(&question_id).unwrap_or_default();
        let mut question_turns = Vec::with_capacity(members.len());
        let mut group_turns = Vec::with_capacity(members.len());
        for (turn_order, (origin, turn)) in members.into_iter().enumerate() {
            question_turns.push(ConversationQuestionTurn {
                question_id: question_id.clone(),
                turn_id: turn.id.clone(),
                turn_order: turn_order as i64,
                assignment_origin: origin,
                assigned_at: turn.imported_at.clone(),
                updated_at: turn.imported_at.clone(),
            });
            group_turns.push(turn);
        }

        let created_at = group_turns
            .first()
            .map(turn_relation_time)
            .unwrap_or_default();
        let updated_at = group_turns
            .last()
            .map(turn_relation_time)
            .unwrap_or_else(|| created_at.clone());
        let question = ConversationQuestion {
            id: question_id.clone(),
            session_id: group_turns
                .first()
                .map(|turn| turn.session_id.clone())
                .unwrap_or_default(),
            title: None,
            created_at,
            updated_at,
        };

        let mut parts = Vec::new();
        for turn in &group_turns {
            parts.extend(parts_by_turn.remove(&turn.id).unwrap_or_default());
        }
        let projected_content_nodes = project_question_content_nodes(
            &question_id,
            &question_turns,
            &parts,
            adapter_id,
            card_kinds,
        )?;
        details.push(ConversationQuestionDetail {
            question: project_question_title(question, &group_turns),
            question_turns,
            turns: group_turns,
            parts,
            projected_content_nodes,
        });
    }
    Ok(details)
}

/// 问题的时间语义取自轮次真实发生时间，而不是导入时刻。
fn turn_relation_time(turn: &ConversationTurn) -> String {
    turn.started_at
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| turn.imported_at.clone())
}

pub(super) async fn load_conversation_question_details_for_session_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<Vec<ConversationQuestionDetail>> {
    let (adapter_id, card_kinds) =
        load_conversation_card_projection_context_sqlx(pool, tenant_id, session_id).await?;

    let turn_rows = sqlx::query_as::<_, ConversationTurnQuestionRow>(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.prompt, t.process, t.response, t.title,
               t.started_at, t.ended_at, t.model, t.fingerprint, t.missing, t.imported_at,
               t.question_id, t.question_origin
        FROM conversation_turns t INDEXED BY idx_conversation_turns_tenant_session
        WHERE t.tenant_id = ?1 AND t.session_id = ?2
        ORDER BY t.turn_index ASC, t.id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    let turns = turn_rows
        .into_iter()
        .map(ConversationTurnQuestionRow::into_grouped_turn)
        .collect::<Vec<_>>();

    let part_rows = sqlx::query_as::<
        _,
        (
            String,
            i64,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        r#"
        SELECT p.turn_id, p.part_index, p.kind, p.content, p.attrs_json, p.translated_text
        FROM conversation_turns t INDEXED BY idx_conversation_turns_tenant_session
        JOIN conversation_parts p
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

    let mut triad_parts_by_turn: BTreeMap<
        String,
        Vec<crate::backend::domain::conversations::triad::TriadPart>,
    > = BTreeMap::new();
    for (turn_id, part_index, kind, content, attrs_json, translated_text) in part_rows {
        triad_parts_by_turn.entry(turn_id).or_default().push(
            crate::backend::domain::conversations::triad::TriadPart {
                part_index,
                kind,
                content,
                attrs_json,
                translated_text,
            },
        );
    }

    let mut parts_by_turn = BTreeMap::<String, Vec<ConversationPart>>::new();
    for (_, _, turn) in &turns {
        let empty_vec = Vec::new();
        let turn_parts = triad_parts_by_turn.get(&turn.id).unwrap_or(&empty_vec);
        let process: Option<crate::backend::domain::conversations::triad::TurnProcess> = turn
            .process
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        let restored = crate::backend::domain::conversations::triad::merge_turn_triad(
            &adapter_id,
            "conversation-part",
            &turn.id,
            &turn.prompt,
            process.as_ref(),
            turn.response.as_deref(),
            turn_parts,
        );
        parts_by_turn.insert(turn.id.clone(), restored);
    }

    build_question_details_from_turn_columns(turns, parts_by_turn, &adapter_id, &card_kinds)
}

pub fn project_question_title(
    mut question: ConversationQuestion,
    turns: &[ConversationTurn],
) -> ConversationQuestion {
    if question
        .title
        .as_deref()
        .is_none_or(|title| title.trim().is_empty())
    {
        if let Some(turn) = turns.iter().find(|turn| !turn.prompt.trim().is_empty()) {
            question.title = Some(first_line(&turn.prompt));
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
        label: None,
        semantic_role: None,
        renderer: ConversationCardRenderer::Plain,
        role: crate::backend::domain::ConversationPartRole::User,
        content_length: turn.prompt.chars().count(),
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
        label: part.command_label.clone(),
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

pub fn conversation_part_id_for_block_id(block_id: &str) -> &str {
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

pub fn project_question_content_nodes(
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
