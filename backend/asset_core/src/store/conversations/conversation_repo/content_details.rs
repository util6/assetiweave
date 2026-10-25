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
                    .chain(detail.turns.iter().map(|turn| turn.prompt.clone()))
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
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.prompt, t.process, t.response, t.title,
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

    let (adapter_id, card_kinds) =
        load_conversation_card_projection_context_sqlx(pool, tenant_id, &session_id).await?;

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
    build_question_details_from_turn_columns(turns, parts_by_turn, &adapter_id, &card_kinds)?
        .into_iter()
        .next()
        .ok_or_else(|| {
            StoreError::external(format!("conversation question not found: {question_id}"))
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
