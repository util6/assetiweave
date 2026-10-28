use super::*;

pub(super) async fn load_search_session_ids_by_id_fragment_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    tables: ConversationRecordTables,
    fragment: &str,
) -> StoreResult<BTreeSet<String>> {
    let (session_lower, session_upper) =
        conversation_id_fragment_range(tables.session_id_prefix, fragment);
    let (turn_lower, turn_upper) = conversation_id_fragment_range(tables.turn_id_prefix, fragment);
    let (part_clause, part_low_bytes, part_high_bytes) =
        if let Some(parsed) = crate::backend::domain::parse_id_key_prefix(fragment) {
            match parsed.lookup {
                crate::backend::domain::IdKeyLookup::Exact(exact) => (
                    "p.id_key = ?6 AND (?7 IS NULL OR 1 = 1)",
                    Some(exact.to_vec()),
                    None,
                ),
                crate::backend::domain::IdKeyLookup::Range {
                    low,
                    high: Some(high),
                } => (
                    "p.id_key >= ?6 AND p.id_key < ?7",
                    Some(low.to_vec()),
                    Some(high.to_vec()),
                ),
                crate::backend::domain::IdKeyLookup::Range { low, high: None } => (
                    "p.id_key >= ?6 AND (?7 IS NULL OR 1 = 1)",
                    Some(low.to_vec()),
                    None,
                ),
            }
        } else {
            (
                "0 = 1 AND (?6 IS NULL OR 1 = 1) AND (?7 IS NULL OR 1 = 1)",
                None,
                None,
            )
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
            WHERE t.tenant_id = ?1 AND s.missing = 0 AND t.id >= ?4 AND t.id < ?5
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
               {session_metadata_expr}
               (
                   SELECT COUNT(*)
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
        session_metadata_expr = tables.session_metadata_expr,
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
pub(super) async fn load_search_question_groups_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    tables: ConversationRecordTables,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    session_ids_json: Option<&str>,
) -> StoreResult<(
    BTreeMap<String, Vec<ConversationQuestion>>,
    BTreeMap<String, Vec<ConversationTurn>>,
)> {
    let query = format!(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.prompt, t.process, t.response, t.title,
               t.started_at, t.ended_at, t.model, t.fingerprint, t.missing, t.imported_at,
               t.turn_kind, t.agent_role, t.parent_turn_id
        FROM {turns} t
        JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id
        WHERE t.tenant_id = ?1
          AND (?2 IS NULL OR s.adapter_id = ?2)
          AND (?3 IS NULL OR s.source_id = ?3)
          AND (?4 IS NULL OR s.id IN (SELECT value FROM json_each(?4)))
        ORDER BY t.session_id ASC, t.turn_index ASC, t.id ASC
        "#,
        turns = tables.turns,
        sessions = tables.sessions,
    );
    let rows = sqlx::query_as::<_, ConversationTurnRow>(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(source_id)
        .bind(session_ids_json)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;

    let mut turns_by_session = BTreeMap::<String, Vec<ConversationTurn>>::new();
    for row in rows {
        let turn = row.into_turn();
        turns_by_session
            .entry(turn.session_id.clone())
            .or_default()
            .push(turn);
    }

    let is_web = tables.turns == "web_record_turns";
    let question_prefix = if is_web {
        "web-record-question"
    } else {
        "conversation-question"
    };

    let mut questions_by_session = BTreeMap::<String, Vec<ConversationQuestion>>::new();
    let mut turns_by_question = BTreeMap::<String, Vec<ConversationTurn>>::new();

    let turn_time = |turn: &ConversationTurn| -> String {
        turn.started_at
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&turn.imported_at)
            .to_string()
    };

    for (session_id, session_turns) in turns_by_session {
        let groups = crate::backend::domain::conversations::group_turns_by_dynamic_projection(
            question_prefix,
            session_turns,
            |turn: &ConversationTurn| &turn.prompt,
            |turn: &ConversationTurn| &turn.id,
        );
        for group in groups {
            let created_at = group.turns.first().map(turn_time).unwrap_or_default();
            let updated_at = group.turns.last().map(turn_time).unwrap_or_default();
            questions_by_session
                .entry(session_id.clone())
                .or_default()
                .push(ConversationQuestion {
                    id: group.question_id.clone(),
                    session_id: session_id.clone(),
                    title: None,
                    created_at,
                    updated_at,
                });
            turns_by_question.insert(group.question_id, group.turns);
        }
    }

    Ok((questions_by_session, turns_by_question))
}

pub(super) async fn load_search_parts_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    tables: ConversationRecordTables,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    session_ids_json: Option<&str>,
) -> StoreResult<BTreeMap<String, Vec<ConversationPart>>> {
    let turn_query = format!(
        r#"
        SELECT t.id, s.adapter_id, t.prompt, t.process, t.response
        FROM {turns} t
        JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id
        WHERE t.tenant_id = ?1
          AND (?2 IS NULL OR s.adapter_id = ?2)
          AND (?3 IS NULL OR s.source_id = ?3)
          AND (?4 IS NULL OR s.id IN (SELECT value FROM json_each(?4)))
        ORDER BY t.session_id ASC, t.turn_index ASC, t.id ASC
        "#,
        turns = tables.turns,
        sessions = tables.sessions,
    );
    let turn_rows = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>)>(
        AssertSqlSafe(turn_query),
    )
    .bind(tenant_id)
    .bind(adapter_id)
    .bind(source_id)
    .bind(session_ids_json)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    let is_web = tables.parts == "web_record_parts";
    let attrs_expr = if is_web { "NULL" } else { "p.attrs_json" };
    let prefix = if is_web {
        "web-record-part"
    } else {
        "conversation-part"
    };

    let part_query = format!(
        r#"
        SELECT p.turn_id, p.part_index, p.kind, p.content, {attrs_expr} AS attrs_json, p.translated_text
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
        attrs_expr = attrs_expr,
    );
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
    >(AssertSqlSafe(part_query))
    .bind(tenant_id)
    .bind(adapter_id)
    .bind(source_id)
    .bind(session_ids_json)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    let mut triad_parts_by_turn =
        BTreeMap::<String, Vec<crate::backend::domain::conversations::triad::TriadPart>>::new();
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
    for (turn_id, turn_adapter_id, prompt, process_json, response) in turn_rows {
        let t_parts = triad_parts_by_turn.remove(&turn_id).unwrap_or_default();
        let process = process_json.as_deref().and_then(|s| {
            serde_json::from_str::<crate::backend::domain::conversations::triad::TurnProcess>(s)
                .ok()
        });
        let restored = crate::backend::domain::conversations::triad::merge_turn_triad(
            &turn_adapter_id,
            prefix,
            &turn_id,
            &prompt,
            process.as_ref(),
            response.as_deref(),
            &t_parts,
        );
        parts_by_turn.insert(turn_id, restored);
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
