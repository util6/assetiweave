use super::*;

pub(crate) async fn list_web_record_sessions_sqlx(
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
    let rows = sqlx::query_as::<_, WebRecordSessionListItemRow>(
        r#"
        SELECT s.id, s.source_id, s.adapter_id, s.external_id, s.title, NULL AS project_path,
               s.started_at, s.updated_at, s.source_locator, s.source_fingerprint,
               s.missing, s.created_at, s.imported_at,
               (
                   SELECT COUNT(*)
                   FROM web_record_questions q
                   WHERE q.tenant_id = s.tenant_id AND q.session_id = s.id
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
              OR EXISTS (
                  SELECT 1
                  FROM conversation_question_fts f
                  WHERE f.tenant_id = s.tenant_id
                    AND f.session_id = s.id
                    AND (
                        instr(lower(f.question_text), ?4) > 0
                        OR instr(lower(f.answer_text), ?4) > 0
                        OR instr(lower(f.code_text), ?4) > 0
                        OR instr(lower(f.command_text), ?4) > 0
                    )
              )
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
            })
        })
        .collect()
}

/// Resolve a possibly-short web-record session ID prefix to the full UUID.
/// Same semantics as `resolve_conversation_session_id_prefix_sqlx` but queries
/// the `web_record_sessions` table.
pub(crate) async fn resolve_web_record_session_id_prefix_sqlx(
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

pub(crate) async fn resolve_web_record_part_id_prefix_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    input: &str,
) -> StoreResult<String> {
    if input.len() >= 36 {
        return Ok(input.to_string());
    }
    let clean_prefix = input.strip_prefix("web-record-part-").unwrap_or(input);
    let like_pattern_verbatim = format!("{}%", input);
    let like_pattern_domain = format!("web-record-part-{}%", clean_prefix);

    let rows: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT id FROM web_record_parts
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

pub(crate) async fn load_web_record_session_detail_sqlx(
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

    let question_rows = sqlx::query(
        r#"
        SELECT id, session_id, title, created_at, updated_at
        FROM web_record_questions
        WHERE tenant_id = ?1 AND session_id = ?2
        ORDER BY COALESCE((
            SELECT MIN(t.turn_index)
            FROM web_record_question_turns qt_order
            JOIN web_record_turns t
              ON t.tenant_id = qt_order.tenant_id AND t.id = qt_order.turn_id
            WHERE qt_order.tenant_id = web_record_questions.tenant_id
              AND qt_order.question_id = web_record_questions.id
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
        FROM web_record_question_turns qt
        JOIN web_record_questions q
          ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        JOIN web_record_turns t
          ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1
          AND q.session_id = ?2
          AND q.session_id = t.session_id
        ORDER BY COALESCE((SELECT MIN(t_order.turn_index) FROM web_record_question_turns qt_order JOIN web_record_turns t_order ON t_order.tenant_id = qt_order.tenant_id AND t_order.id = qt_order.turn_id WHERE qt_order.tenant_id = q.tenant_id AND qt_order.question_id = q.id), 9223372036854775807) ASC, qt.turn_order ASC, t.turn_index ASC,
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
        let membership = map_sqlx_conversation_question_turn(row).map_err(StoreError::external)?;
        question_turns_by_question
            .entry(membership.question_id.clone())
            .or_default()
            .push(membership);
    }

    #[derive(Debug, FromRow)]
    pub(crate) struct WebRecordDetailTurnRow {
        pub(crate) id: String,
        pub(crate) session_id: String,
        pub(crate) external_id: String,
        pub(crate) turn_index: i64,
        pub(crate) user_text: String,
        pub(crate) title: Option<String>,
        pub(crate) started_at: Option<String>,
        pub(crate) ended_at: Option<String>,
        pub(crate) fingerprint: String,
        pub(crate) missing: i64,
        pub(crate) imported_at: String,
        pub(crate) question_id: String,
        pub(crate) model: Option<String>,
    }

    let turn_rows = sqlx::query_as::<_, WebRecordDetailTurnRow>(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at,
               qt.question_id, t.model
        FROM web_record_question_turns qt
        JOIN web_record_turns t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        JOIN web_record_questions q ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        WHERE q.tenant_id = ?1
          AND q.session_id = ?2
          AND q.session_id = t.session_id
        ORDER BY COALESCE((SELECT MIN(t_order.turn_index) FROM web_record_question_turns qt_order JOIN web_record_turns t_order ON t_order.tenant_id = qt_order.tenant_id AND t_order.id = qt_order.turn_id WHERE qt_order.tenant_id = q.tenant_id AND qt_order.question_id = q.id), 9223372036854775807) ASC, qt.turn_order ASC, t.turn_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    let mut turns_by_question = BTreeMap::<String, Vec<ConversationTurn>>::new();
    for row in turn_rows {
        let question_id = row.question_id;
        turns_by_question
            .entry(question_id)
            .or_default()
            .push(ConversationTurn {
                id: row.id,
                session_id: row.session_id,
                external_id: row.external_id,
                turn_index: row.turn_index,
                user_text: row.user_text,
                title: row.title,
                started_at: row.started_at,
                ended_at: row.ended_at,
                fingerprint: row.fingerprint,
                missing: row.missing == 1,
                imported_at: row.imported_at,
                model: row.model,
            });
    }

    let part_rows = sqlx::query(
        r#"
        SELECT p.id, p.turn_id, p.part_index, p.role, p.kind, p.text, p.language,
               p.command, p.cwd, p.status, p.exit_code, p.metadata_json,
               p.content_card_json, p.translated_text, p.source_execution_id, p.command_label
        FROM web_record_parts p
        JOIN web_record_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
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
    let mut question_details = Vec::with_capacity(questions.len());
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
            &session.adapter_id,
            &card_kinds,
        )?;
        question_details.push(ConversationQuestionDetail {
            question: project_question_title(question, &turns),
            question_turns,
            turns,
            parts,
            projected_content_nodes,
        });
    }
    Ok(ConversationSessionDetail {
        session,
        questions: question_details,
    })
}

pub(crate) async fn update_web_record_part_translation_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    part_id: &str,
    translated_text: &str,
) -> StoreResult<()> {
    let result = sqlx::query(
        r#"
        UPDATE web_record_parts
        SET translated_text = ?1
        WHERE tenant_id = ?2 AND id = ?3
        "#,
    )
    .bind(translated_text)
    .bind(tenant_id)
    .bind(part_id)
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
