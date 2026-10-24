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
    let (part_clause, part_low_bytes, part_high_bytes) =
        if let Some(parsed) = crate::backend::domain::parse_id_key_prefix(fragment) {
            let (low, high) = match parsed.lookup {
                crate::backend::domain::IdKeyLookup::Exact(exact) => (exact, exact),
                crate::backend::domain::IdKeyLookup::Range { low, high } => {
                    (low, high.unwrap_or([0xFF; 8]))
                }
            };
            (
                "p.id_key >= ?8 AND p.id_key <= ?9",
                low.to_vec(),
                high.to_vec(),
            )
        } else {
            ("0 = 1", Vec::new(), Vec::new())
        };
    let query = format!(
        r#"
        SELECT session_id FROM (
            SELECT s.id AS session_id
            FROM {sessions} s
            WHERE s.tenant_id = ?1 AND s.missing = 0 AND s.id >= ?2 AND s.id < ?3
            UNION
            SELECT t.session_id
            FROM {turns} t
            JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id
            WHERE t.tenant_id = ?1 AND s.missing = 0 AND t.question_id >= ?4 AND t.question_id < ?5
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
            WHERE p.tenant_id = ?1 AND s.missing = 0 AND {part_clause}
        )
        "#,
        sessions = tables.sessions,
        turns = tables.turns,
        parts = tables.parts,
        part_clause = part_clause,
    );
    let builder = sqlx::query_scalar::<_, String>(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(session_lower)
        .bind(session_upper)
        .bind(question_lower)
        .bind(question_upper)
        .bind(turn_lower)
        .bind(turn_upper);
    let rows = builder
        .bind(part_low_bytes)
        .bind(part_high_bytes)
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
                   SELECT COUNT(DISTINCT t.question_id)
                   FROM {turns} t
                   WHERE t.tenant_id = s.tenant_id AND t.session_id = s.id
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

/// 会话内的分组由轮次列即时派生：分组身份、组间顺序与问题时间全部来自轮次序列。
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
        SELECT t.session_id AS session_id, t.question_id AS id,
               MIN(t.turn_index) AS first_turn_index,
               (
                   SELECT COALESCE(t_first.started_at, t_first.imported_at)
                   FROM {turns} t_first
                   WHERE t_first.tenant_id = t.tenant_id
                     AND t_first.session_id = t.session_id
                     AND t_first.question_id = t.question_id
                   ORDER BY t_first.turn_index ASC, t_first.id ASC
                   LIMIT 1
               ) AS created_at,
               (
                   SELECT COALESCE(t_last.started_at, t_last.imported_at)
                   FROM {turns} t_last
                   WHERE t_last.tenant_id = t.tenant_id
                     AND t_last.session_id = t.session_id
                     AND t_last.question_id = t.question_id
                   ORDER BY t_last.turn_index DESC, t_last.id DESC
                   LIMIT 1
               ) AS updated_at
        FROM {turns} t
        JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id
        WHERE t.tenant_id = ?1
          AND (?2 IS NULL OR s.adapter_id = ?2)
          AND (?3 IS NULL OR s.source_id = ?3)
          AND (?4 IS NULL OR s.id IN (SELECT value FROM json_each(?4)))
        GROUP BY t.session_id, t.question_id
        ORDER BY t.session_id ASC, first_turn_index ASC, t.question_id ASC
        "#,
        sessions = tables.sessions,
        turns = tables.turns,
    );
    let rows = sqlx::query_as::<_, DerivedQuestionGroupRow>(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(source_id)
        .bind(session_ids_json)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;
    let mut questions_by_session = BTreeMap::<String, Vec<ConversationQuestion>>::new();
    for row in rows {
        questions_by_session
            .entry(row.session_id.clone())
            .or_default()
            .push(ConversationQuestion {
                id: row.id,
                session_id: row.session_id,
                title: None,
                created_at: row.created_at,
                updated_at: row.updated_at,
            });
    }
    Ok(questions_by_session)
}

#[derive(Debug, FromRow)]
struct DerivedQuestionGroupRow {
    session_id: String,
    id: String,
    #[allow(dead_code)]
    first_turn_index: i64,
    created_at: String,
    updated_at: String,
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
               t.question_id
        FROM {turns} t
        JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id
        WHERE t.tenant_id = ?1
          AND (?2 IS NULL OR s.adapter_id = ?2)
          AND (?3 IS NULL OR s.source_id = ?3)
          AND (?4 IS NULL OR s.id IN (SELECT value FROM json_each(?4)))
        ORDER BY t.question_id ASC, t.turn_index ASC, t.id ASC
        "#,
        turns = tables.turns,
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
        SELECT {part_columns}
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
        part_columns = tables.part_columns,
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
