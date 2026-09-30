use super::*;

pub(super) async fn load_search_session_ids_by_id_fragment_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    tables: ConversationRecordTables,
    fragment: &str,
) -> StoreResult<BTreeSet<String>> {
    let (session_lower, session_upper) =
        conversation_id_fragment_range(tables.session_id_prefix, fragment);
    let (question_lower, question_upper) =
        conversation_id_fragment_range(tables.question_id_prefix, fragment);
    let (turn_lower, turn_upper) = conversation_id_fragment_range(tables.turn_id_prefix, fragment);
    let (part_lower, part_upper) = conversation_id_fragment_range(tables.part_id_prefix, fragment);
    let query = format!(
        r#"
        SELECT session_id FROM (
            SELECT s.id AS session_id
            FROM {sessions} s
            WHERE s.tenant_id = ?1 AND s.missing = 0 AND s.id >= ?2 AND s.id < ?3
            UNION
            SELECT q.session_id
            FROM {questions} q
            JOIN {sessions} s ON s.tenant_id = q.tenant_id AND s.id = q.session_id
            WHERE q.tenant_id = ?1 AND s.missing = 0 AND q.id >= ?4 AND q.id < ?5
            UNION
            SELECT t.session_id
            FROM {turns} t
            JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id
            WHERE t.tenant_id = ?1 AND s.missing = 0 AND t.id >= ?6 AND t.id < ?7
            UNION
            SELECT t.session_id
            FROM {parts} p
            JOIN {turns} t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
            JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id
            WHERE p.tenant_id = ?1 AND s.missing = 0 AND p.id >= ?8 AND p.id < ?9
        )
        "#,
        sessions = tables.sessions,
        questions = tables.questions,
        turns = tables.turns,
        parts = tables.parts,
    );
    let rows = sqlx::query_scalar::<_, String>(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(session_lower)
        .bind(session_upper)
        .bind(question_lower)
        .bind(question_upper)
        .bind(turn_lower)
        .bind(turn_upper)
        .bind(part_lower)
        .bind(part_upper)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;
    Ok(rows.into_iter().collect())
}

pub(super) async fn load_search_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    tables: ConversationRecordTables,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    session_ids_json: Option<&str>,
) -> StoreResult<Vec<ConversationSessionListItem>> {
    let query = format!(
        r#"
        SELECT s.id, s.source_id, s.adapter_id, s.external_id, s.title, {project_path_expr} AS project_path,
               s.started_at, s.updated_at, s.source_locator, s.source_fingerprint,
               s.missing, s.created_at, s.imported_at,
               (
                   SELECT COUNT(*)
                   FROM {questions} q
                   WHERE q.tenant_id = s.tenant_id AND q.session_id = s.id
               ) AS question_count,
               (
                   SELECT COUNT(*)
                   FROM {turns} t
                   WHERE t.tenant_id = s.tenant_id AND t.session_id = s.id
               ) AS turn_count
        FROM {sessions} s
        WHERE s.tenant_id = ?1
          AND (?2 IS NULL OR s.adapter_id = ?2)
          AND (?3 IS NULL OR s.source_id = ?3)
          AND (?4 IS NULL OR s.id IN (SELECT value FROM json_each(?4)))
        ORDER BY COALESCE(s.updated_at, s.imported_at) DESC, s.title ASC
        "#,
        sessions = tables.sessions,
        project_path_expr = tables.session_project_path_expr,
        questions = tables.questions,
        turns = tables.turns,
    );
    let rows = sqlx::query_as::<_, ConversationSessionListItemRow>(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(source_id)
        .bind(session_ids_json)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;
    rows.into_iter()
        .map(ConversationSessionListItemRow::into_item)
        .collect()
}

pub(super) async fn load_search_questions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    tables: ConversationRecordTables,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    session_ids_json: Option<&str>,
) -> StoreResult<BTreeMap<String, Vec<ConversationQuestion>>> {
    let query = format!(
        r#"
        SELECT q.id, q.session_id, q.title,
               q.created_at, q.updated_at
        FROM {questions} q
        JOIN {sessions} s ON s.tenant_id = q.tenant_id AND s.id = q.session_id
        WHERE q.tenant_id = ?1
          AND (?2 IS NULL OR s.adapter_id = ?2)
          AND (?3 IS NULL OR s.source_id = ?3)
          AND (?4 IS NULL OR s.id IN (SELECT value FROM json_each(?4)))
        ORDER BY q.session_id ASC,
                 COALESCE((SELECT MIN(t.turn_index)
                           FROM {question_turns} qt_order
                           JOIN {turns} t ON t.tenant_id = qt_order.tenant_id AND t.id = qt_order.turn_id
                           WHERE qt_order.tenant_id = q.tenant_id AND qt_order.question_id = q.id), 9223372036854775807),
                 q.created_at ASC, q.id ASC
        "#,
        questions = tables.questions,
        sessions = tables.sessions,
        question_turns = tables.question_turns,
        turns = tables.turns,
    );
    let rows = sqlx::query(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(source_id)
        .bind(session_ids_json)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;
    let mut questions_by_session = BTreeMap::<String, Vec<ConversationQuestion>>::new();
    for row in &rows {
        let question = map_sqlx_conversation_question(row)?;
        questions_by_session
            .entry(question.session_id.clone())
            .or_default()
            .push(question);
    }
    Ok(questions_by_session)
}

pub(super) async fn load_search_turns_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    tables: ConversationRecordTables,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    session_ids_json: Option<&str>,
) -> StoreResult<BTreeMap<String, Vec<ConversationTurn>>> {
    let query = format!(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.model, t.fingerprint, t.missing, t.imported_at,
               qt.question_id
        FROM {turns} t
        JOIN {question_turns} qt ON qt.tenant_id = t.tenant_id AND qt.turn_id = t.id
        JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id
        WHERE t.tenant_id = ?1
          AND (?2 IS NULL OR s.adapter_id = ?2)
          AND (?3 IS NULL OR s.source_id = ?3)
          AND (?4 IS NULL OR s.id IN (SELECT value FROM json_each(?4)))
        ORDER BY qt.question_id ASC, qt.turn_order ASC, t.turn_index ASC
        "#,
        turns = tables.turns,
        question_turns = tables.question_turns,
        sessions = tables.sessions,
    );
    let rows = sqlx::query_as::<_, ConversationTurnWithQuestionRow>(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(source_id)
        .bind(session_ids_json)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;
    let mut turns_by_question = BTreeMap::<String, Vec<ConversationTurn>>::new();
    for row in rows {
        let (question_id, turn) = row.into_turn();
        turns_by_question.entry(question_id).or_default().push(turn);
    }
    Ok(turns_by_question)
}

pub(super) async fn load_search_parts_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    tables: ConversationRecordTables,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    session_ids_json: Option<&str>,
) -> StoreResult<BTreeMap<String, Vec<ConversationPart>>> {
    let query = format!(
        r#"
        SELECT p.id, p.turn_id, p.part_index, p.role, p.kind, p.text, p.language,
               p.command, p.cwd, p.status, p.exit_code, p.metadata_json,
               p.content_card_json, p.translated_text, p.source_execution_id, p.command_label
        FROM {parts} p
        JOIN {turns} t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
        JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id
        WHERE p.tenant_id = ?1
          AND (?2 IS NULL OR s.adapter_id = ?2)
          AND (?3 IS NULL OR s.source_id = ?3)
          AND (?4 IS NULL OR s.id IN (SELECT value FROM json_each(?4)))
        ORDER BY p.turn_id ASC, p.part_index ASC
        "#,
        parts = tables.parts,
        turns = tables.turns,
        sessions = tables.sessions,
    );
    let rows = sqlx::query(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(source_id)
        .bind(session_ids_json)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;
    let mut parts_by_turn = BTreeMap::<String, Vec<ConversationPart>>::new();
    for row in &rows {
        let part = map_sqlx_conversation_part(row)?;
        parts_by_turn
            .entry(part.turn_id.clone())
            .or_default()
            .push(part);
    }
    Ok(parts_by_turn)
}

pub(super) struct ConversationSearchEntry {
    pub(super) card_type: ConversationSearchCardType,
    pub(super) block_id: String,
    pub(super) text: String,
    pub(super) semantic_role: Option<String>,
}

pub(crate) fn append_projected_cards_to_question_aggregate(
    part: &ConversationPart,
    adapter_id: &str,
    card_kinds: &[ConversationCardKindDefinition],
    answer_text: &mut Vec<String>,
    code_text: &mut Vec<String>,
    command_text: &mut Vec<String>,
) -> StoreResult<()> {
    let cards =
        crate::backend::domain::conversations::projection::project_conversation_content_cards(
            part, adapter_id, card_kinds,
        )?;
    for card in cards {
        let semantic_role = card
            .semantic_role
            .as_deref()
            .or_else(|| card.kind.rsplit_once('.').map(|(_, value)| value))
            .unwrap_or(card.kind.as_str());
        match semantic_role {
            "answer" => answer_text.push(card.body),
            "code" => code_text.push(card.body),
            "command" => command_text.push(card.body),
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn content_card_type_value(value: &str) -> Option<ConversationSearchCardType> {
    crate::backend::domain::conversations::projection::is_valid_card_kind(value)
        .then(|| ConversationSearchCardType::new(value))
}

#[derive(Clone, Copy)]
pub(super) enum SearchTimeBound {
    Since,
    Until,
}

pub(super) fn parse_search_time_bound(
    value: Option<&str>,
    bound: SearchTimeBound,
) -> StoreResult<Option<DateTime<Utc>>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Ok(Some(parsed.with_timezone(&Utc)));
    }
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        let time = match bound {
            SearchTimeBound::Since => NaiveTime::from_hms_opt(0, 0, 0),
            SearchTimeBound::Until => NaiveTime::from_hms_nano_opt(23, 59, 59, 999_999_999),
        }
        .expect("valid search time bound");
        return Ok(Some(DateTime::from_naive_utc_and_offset(
            date.and_time(time),
            Utc,
        )));
    }
    Err(StoreError::Validation(format!(
        "invalid conversation search time {value:?}; use RFC3339 or YYYY-MM-DD"
    )))
}

pub(super) fn conversation_session_search_time(
    session: &ConversationSession,
) -> Option<DateTime<Utc>> {
    session
        .started_at
        .as_deref()
        .and_then(crate::backend::domain::parse_conversation_timestamp)
        .or_else(|| {
            session
                .updated_at
                .as_deref()
                .and_then(crate::backend::domain::parse_conversation_timestamp)
        })
        .or_else(|| parse_rfc3339_utc(&session.imported_at))
}

pub(super) fn parse_rfc3339_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.trim())
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

pub(super) fn search_entries_for_part(
    part: &ConversationPart,
    adapter_id: &str,
    card_kinds: &[ConversationCardKindDefinition],
) -> Vec<ConversationSearchEntry> {
    crate::backend::domain::conversations::projection::project_conversation_content_cards(
        part, adapter_id, card_kinds,
    )
    .unwrap_or_default()
    .into_iter()
    .map(|card| {
        let semantic_role = card
            .semantic_role
            .clone()
            .or_else(|| {
                card_kinds
                    .iter()
                    .find(|definition| definition.id == card.kind)
                    .and_then(|definition| definition.semantic_role.clone())
            })
            .or_else(|| {
                card.kind
                    .rsplit_once('.')
                    .map(|(_, value)| value.to_string())
            });
        ConversationSearchEntry {
            card_type: ConversationSearchCardType::new(card.kind),
            block_id: card.node_id,
            text: card.body,
            semantic_role,
        }
    })
    .collect()
}

#[derive(Debug, FromRow)]
pub(super) struct AdapterCardKindsRow {
    id: String,
    card_kinds_json: String,
}

pub(super) async fn load_search_adapter_card_kinds_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<BTreeMap<String, Vec<ConversationCardKindDefinition>>> {
    let rows = sqlx::query_as::<_, AdapterCardKindsRow>(
        "SELECT id, card_kinds_json FROM conversation_adapters WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    rows.into_iter()
        .map(|row| {
            let definitions = decode_json(row.card_kinds_json)?;
            Ok((row.id, definitions))
        })
        .collect()
}
