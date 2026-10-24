use super::*;

pub async fn list_web_record_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    query: Option<&str>,
    limit: usize,
    offset: usize,
) -> StoreResult<Vec<ConversationSessionListItem>> {
    let needle = normalize_query(query);
    let id_needle = query.and_then(crate::backend::domain::conversation_id_search_term);
    // 正文索引重建期间正文匹配不可用，列表搜索降级为标题级，避免给出误导性的空结果。
    let body_search_ready = conversation_part_index_ready_sqlx(pool, tenant_id).await?;
    let rows = sqlx::query_as::<_, WebRecordSessionListItemRow>(
        r#"
        SELECT s.id, s.source_id, s.adapter_id, s.external_id, s.title, NULL AS project_path,
               s.started_at, s.updated_at, s.source_locator, s.source_fingerprint,
               s.missing, s.created_at, s.imported_at,
               (
                   SELECT COUNT(DISTINCT t.question_id)
                   FROM web_record_turns t
                   WHERE t.tenant_id = s.tenant_id AND t.session_id = s.id
               ) AS question_count,
               (
                   SELECT COUNT(*)
                   FROM web_record_turns t
                   WHERE t.tenant_id = s.tenant_id AND t.session_id = s.id
               ) AS turn_count
        FROM web_record_sessions s
        WHERE s.tenant_id = ?1
          AND (?2 IS NULL OR s.adapter_id = ?2)
          AND (?3 IS NULL OR s.source_id = ?3)
          AND (
              ?4 IS NULL
              OR instr(lower(s.title), ?4) > 0
              OR instr(lower(s.external_id), ?4) > 0
              OR (?5 IS NOT NULL AND instr(lower(s.id), ?5) > 0)
              OR (?8 = 1 AND ?4 IS NOT NULL AND EXISTS (
                  SELECT 1
                  FROM conversation_part_fts f
                  WHERE f.tenant_id = s.tenant_id
                    AND f.session_id = s.id
                    AND instr(lower(f.text), ?4) > 0
              ))
              OR (?8 = 1 AND ?4 IS NOT NULL AND EXISTS (
                  SELECT 1
                  FROM web_record_turns t
                  WHERE t.tenant_id = s.tenant_id
                    AND t.session_id = s.id
                    AND instr(lower(t.user_text), ?4) > 0
              ))
          )
        ORDER BY COALESCE(s.updated_at, s.imported_at) DESC, s.title ASC
        LIMIT ?6 OFFSET ?7
        "#,
    )
    .bind(tenant_id)
    .bind(adapter_id)
    .bind(source_id)
    .bind(needle.as_deref())
    .bind(id_needle.as_deref())
    .bind(
        i64::try_from(limit)
            .map_err(|_| format!("invalid web record limit: {limit}"))
            .map_err(StoreError::external)?,
    )
    .bind(
        i64::try_from(offset)
            .map_err(|_| format!("invalid web record offset: {offset}"))
            .map_err(StoreError::external)?,
    )
    .bind(i64::from(body_search_ready))
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    rows.into_iter()
        .map(|row| {
            let question_count = usize::try_from(row.question_count)
                .map_err(|_| "invalid web record question count".to_string())
                .map_err(StoreError::external)?;
            let turn_count = usize::try_from(row.turn_count)
                .map_err(|_| "invalid web record turn count".to_string())
                .map_err(StoreError::external)?;
            Ok(ConversationSessionListItem {
                session: ConversationSession {
                    id: row.id,
                    source_id: row.source_id,
                    adapter_id: row.adapter_id,
                    external_id: row.external_id,
                    title: row.title,
                    project_path: row.project_path,
                    started_at: row.started_at,
                    updated_at: row.updated_at,
                    source_locator: row.source_locator,
                    source_fingerprint: row.source_fingerprint,
                    missing: row.missing == 1,
                    created_at: row.created_at,
                    imported_at: row.imported_at,
                    execution_origin: "user".to_string(),
                    execution_purpose: None,
                    user_visible: true,
                },
                question_count,
                turn_count,
                latest_user_prompt: None,
                window_turn_count: None,
                window_start_turn_index: None,
                window_start_user_prompt: None,
                window_end_turn_index: None,
                window_end_user_prompt: None,
            })
        })
        .collect()
}

/// Resolve a possibly-short web-record session ID prefix to the full UUID.
/// Same semantics as `resolve_conversation_session_id_prefix_sqlx` but queries
/// the `web_record_sessions` table.
pub async fn resolve_web_record_session_id_prefix_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    input: &str,
) -> StoreResult<String> {
    if input.len() >= 36 {
        return Ok(input.to_string());
    }
    let clean_prefix = input.strip_prefix("web-record-session-").unwrap_or(input);
    let like_pattern_verbatim = format!("{}%", input);
    let like_pattern_domain = format!("web-record-session-{}%", clean_prefix);

    let rows: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT id FROM web_record_sessions
        WHERE tenant_id = ?1 AND (id LIKE ?2 OR id LIKE ?3)
        LIMIT 11
        "#,
    )
    .bind(tenant_id)
    .bind(&like_pattern_verbatim)
    .bind(&like_pattern_domain)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    match rows.len() {
        0 => Err(StoreError::NotFound(format!(
            "no web record session matches prefix \"{input}\""
        ))),
        1 => Ok(rows.into_iter().next().unwrap()),
        n => {
            let preview: Vec<&str> = rows.iter().take(5).map(|s| s.as_str()).collect();
            Err(StoreError::Conflict(format!(
                "ambiguous web record session prefix \"{input}\": {n} sessions match (e.g. {}). Use more characters to narrow down.",
                preview.join(", ")
            )))
        }
    }
}

pub async fn resolve_web_record_part_id_prefix_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    input: &str,
) -> StoreResult<String> {
    if input.len() >= 36 && input.starts_with("web-record-part-") {
        return Ok(input.to_string());
    }
    let clean_prefix = input.strip_prefix("web-record-part-").unwrap_or(input);

    let mut rows: Vec<String> = Vec::new();
    if let Some(parsed) = crate::backend::domain::parse_id_key_prefix(clean_prefix) {
        let part_rows: Vec<(String, i64)> = match parsed.lookup {
            crate::backend::domain::IdKeyLookup::Exact(key) => {
                sqlx::query_as(
                    "SELECT turn_id, part_index FROM web_record_parts WHERE tenant_id = ?1 AND id_key = ?2 LIMIT 11",
                )
                .bind(tenant_id)
                .bind(key.to_vec())
                .fetch_all(pool)
                .await
                .map_err(StoreError::external)?
            }
            crate::backend::domain::IdKeyLookup::Range {
                low,
                high: Some(high),
            } => {
                sqlx::query_as(
                    "SELECT turn_id, part_index FROM web_record_parts WHERE tenant_id = ?1 AND id_key >= ?2 AND id_key < ?3 LIMIT 11",
                )
                .bind(tenant_id)
                .bind(low.to_vec())
                .bind(high.to_vec())
                .fetch_all(pool)
                .await
                .map_err(StoreError::external)?
            }
            crate::backend::domain::IdKeyLookup::Range { low, high: None } => {
                sqlx::query_as(
                    "SELECT turn_id, part_index FROM web_record_parts WHERE tenant_id = ?1 AND id_key >= ?2 LIMIT 11",
                )
                .bind(tenant_id)
                .bind(low.to_vec())
                .fetch_all(pool)
                .await
                .map_err(StoreError::external)?
            }
        };

        for (turn_id, part_index) in part_rows {
            let full_id = crate::backend::domain::derive_standard_part_id(
                "web-record-part",
                &turn_id,
                part_index,
            );
            let hash_part = full_id.strip_prefix("web-record-part-").unwrap_or(&full_id);
            if hash_part.starts_with(&parsed.normalized_hex)
                || full_id.starts_with(input)
                || full_id == input
            {
                if !rows.contains(&full_id) {
                    rows.push(full_id);
                }
            }
        }
    }

    match rows.len() {
        0 => Err(StoreError::NotFound(format!(
            "no web record part matches prefix \"{input}\""
        ))),
        1 => Ok(rows.into_iter().next().unwrap()),
        n => {
            let preview: Vec<&str> = rows.iter().take(5).map(|s| s.as_str()).collect();
            Err(StoreError::Conflict(format!(
                "ambiguous web record part prefix \"{input}\": {n} parts match (e.g. {}). Use more characters to narrow down.",
                preview.join(", ")
            )))
        }
    }
}

pub async fn load_web_record_session_detail_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<ConversationSessionDetail> {
    let session_row = sqlx::query(
        r#"
        SELECT id, source_id, adapter_id, external_id, title, NULL AS project_path,
               started_at, updated_at, source_locator, source_fingerprint,
               missing, created_at, imported_at
        FROM web_record_sessions
        WHERE tenant_id = ?1 AND id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?
    .ok_or_else(|| StoreError::NotFound(format!("web record session not found: {session_id}")))?;
    let session = map_sqlx_conversation_session(&session_row).map_err(StoreError::external)?;

    let turn_rows = sqlx::query_as::<_, ConversationTurnQuestionRow>(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.model, t.fingerprint, t.missing, t.imported_at,
               t.question_id, t.question_origin
        FROM web_record_turns t
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

    let part_rows = sqlx::query(
        r#"
        SELECT p.turn_id, p.part_index,
               CASE WHEN p.kind = 'answer' THEN 'assistant' ELSE 'tool' END AS role,
               CASE WHEN p.kind = 'answer' THEN 'text' ELSE 'tool' END AS kind,
               CASE WHEN p.kind = 'answer' THEN s.adapter_id || '.answer' ELSE s.adapter_id || '.result' END AS card_kind,
               p.content,
               NULL AS attrs_json,
               NULL AS metadata_json,
               p.translated_text
        FROM web_record_parts p
        JOIN web_record_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
        JOIN web_record_sessions s ON s.tenant_id = t.tenant_id AND s.id = t.session_id
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
        let part = map_sqlx_conversation_part(row).map_err(StoreError::external)?;
        parts_by_turn
            .entry(part.turn_id.clone())
            .or_default()
            .push(part);
    }

    let card_kinds_json = sqlx::query_scalar::<_, String>(
        "SELECT card_kinds_json FROM conversation_adapters WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(&session.adapter_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?
    .unwrap_or_else(|| "[]".to_string());
    let card_kinds: Vec<crate::backend::domain::ConversationCardKindDefinition> =
        decode_json_app(card_kinds_json)?;
    let question_details = build_question_details_from_turn_columns(
        turns,
        parts_by_turn,
        &session.adapter_id,
        &card_kinds,
    )?;
    Ok(ConversationSessionDetail {
        session,
        questions: question_details,
    })
}

pub async fn update_web_record_part_translation_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    part_id: &str,
    translated_text: &str,
) -> StoreResult<()> {
    let id_key = crate::backend::domain::derive_part_id_key(part_id)
        .ok_or_else(|| StoreError::NotFound(format!("invalid web record part id: {part_id}")))?;
    let result = sqlx::query(
        r#"
        UPDATE web_record_parts
        SET translated_text = ?1
        WHERE tenant_id = ?2 AND id_key = ?3
        "#,
    )
    .bind(translated_text)
    .bind(tenant_id)
    .bind(id_key.to_vec())
    .execute(pool)
    .await
    .map_err(StoreError::external)?;

    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound(format!(
            "web record part not found: {part_id}"
        )));
    }

    Ok(())
}
