use crate::models::{
    conversation_turn_fingerprint, group_turn_ids_by_question, ConversationAdapter,
    ConversationAdapterKind, ConversationAdapterTrustState, ConversationGroupingOrigin,
    ConversationPart, ConversationPartKind, ConversationPartRole, ConversationQuestion,
    ConversationSession, ConversationSource, ConversationSourceKind, ConversationSyncRun,
    ConversationSyncStatus, ConversationTurn, NormalizedConversationSession,
};
use crate::types::AppResult;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Row};
use schemars::JsonSchema;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

use super::codec::{db_error, decode_enum, decode_json, encode_enum, encode_json, to_sql_error};

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ConversationImportResult {
    pub(crate) source_id: String,
    pub(crate) adapter_id: String,
    pub(crate) dry_run: bool,
    pub(crate) session_count: usize,
    pub(crate) turn_count: usize,
    pub(crate) warning_count: usize,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, JsonSchema)]
pub(crate) struct ConversationExportContentFilter {
    #[serde(default = "default_true")]
    pub(crate) answer: bool,
    #[serde(default = "default_true")]
    pub(crate) tool: bool,
    #[serde(default = "default_true")]
    pub(crate) command: bool,
    #[serde(default = "default_true")]
    pub(crate) code: bool,
    #[serde(default = "default_true")]
    pub(crate) result: bool,
}

impl Default for ConversationExportContentFilter {
    fn default() -> Self {
        Self {
            answer: true,
            tool: true,
            command: true,
            code: true,
            result: true,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ConversationSessionListItem {
    #[serde(flatten)]
    pub(crate) session: ConversationSession,
    pub(crate) question_count: usize,
    pub(crate) turn_count: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ConversationQuestionDetail {
    pub(crate) question: ConversationQuestion,
    pub(crate) turns: Vec<ConversationTurn>,
    pub(crate) parts: Vec<ConversationPart>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ConversationSessionDetail {
    pub(crate) session: ConversationSession,
    pub(crate) questions: Vec<ConversationQuestionDetail>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ConversationMutationResult {
    pub(crate) dry_run: bool,
    pub(crate) session_id: String,
    pub(crate) affected_question_ids: Vec<String>,
    pub(crate) questions: Vec<ConversationQuestionDetail>,
}

pub(crate) fn seed_builtin_conversation_adapters(conn: &Connection) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    for adapter in builtin_adapters(&now) {
        upsert_conversation_adapter(conn, &adapter)?;
    }
    for source in builtin_sources(&now) {
        if load_conversation_source(conn, &source.id)?.is_none() {
            upsert_conversation_source(conn, &source)?;
        }
    }
    Ok(())
}

pub(crate) fn list_conversation_adapters(conn: &Connection) -> AppResult<Vec<ConversationAdapter>> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, name, kind, version, enabled, manifest_path, executable_path,
                   content_hash, trusted_hash, trust_state, protocol_version,
                   capabilities, input_kinds, created_at, updated_at
            FROM conversation_adapters
            ORDER BY kind ASC, name ASC
            "#,
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map([], map_conversation_adapter)
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

pub(crate) fn upsert_conversation_adapter(
    conn: &Connection,
    adapter: &ConversationAdapter,
) -> AppResult<()> {
    conn.execute(
        r#"
        INSERT INTO conversation_adapters (
            id, name, kind, version, enabled, manifest_path, executable_path,
            content_hash, trusted_hash, trust_state, protocol_version,
            capabilities, input_kinds, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            kind = excluded.kind,
            version = excluded.version,
            enabled = excluded.enabled,
            manifest_path = excluded.manifest_path,
            executable_path = excluded.executable_path,
            content_hash = excluded.content_hash,
            trusted_hash = excluded.trusted_hash,
            trust_state = excluded.trust_state,
            protocol_version = excluded.protocol_version,
            capabilities = excluded.capabilities,
            input_kinds = excluded.input_kinds,
            updated_at = excluded.updated_at
        "#,
        params![
            adapter.id,
            adapter.name,
            encode_enum(adapter.kind)?,
            adapter.version,
            if adapter.enabled { 1 } else { 0 },
            adapter.manifest_path,
            adapter.executable_path,
            adapter.content_hash,
            adapter.trusted_hash,
            encode_enum(adapter.trust_state)?,
            adapter.protocol_version,
            encode_json(&adapter.capabilities)?,
            encode_json(&adapter.input_kinds)?,
            adapter.created_at,
            adapter.updated_at,
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

pub(crate) fn delete_conversation_adapter(
    conn: &Connection,
    adapter_id: &str,
) -> AppResult<ConversationAdapter> {
    let adapter = load_conversation_adapter(conn, adapter_id)?
        .ok_or_else(|| format!("conversation adapter not found: {adapter_id}"))?;
    if adapter.kind != ConversationAdapterKind::External {
        return Err("built-in conversation adapters cannot be unregistered".to_string());
    }
    conn.execute(
        "DELETE FROM conversation_adapters WHERE id = ?1",
        params![adapter_id],
    )
    .map_err(db_error)?;
    conn.execute(
        "UPDATE conversation_sources SET enabled = 0, updated_at = ?1 WHERE adapter_id = ?2",
        params![Utc::now().to_rfc3339(), adapter_id],
    )
    .map_err(db_error)?;
    Ok(adapter)
}

pub(crate) fn load_conversation_adapter(
    conn: &Connection,
    adapter_id: &str,
) -> AppResult<Option<ConversationAdapter>> {
    conn.query_row(
        r#"
        SELECT id, name, kind, version, enabled, manifest_path, executable_path,
               content_hash, trusted_hash, trust_state, protocol_version,
               capabilities, input_kinds, created_at, updated_at
        FROM conversation_adapters
        WHERE id = ?1
        "#,
        params![adapter_id],
        map_conversation_adapter,
    )
    .optional()
    .map_err(db_error)
}

pub(crate) fn list_conversation_sources(conn: &Connection) -> AppResult<Vec<ConversationSource>> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, adapter_id, name, kind, location, config_json, enabled,
                   last_synced_at, last_sync_status, created_at, updated_at
            FROM conversation_sources
            ORDER BY adapter_id ASC, name ASC
            "#,
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map([], map_conversation_source)
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

pub(crate) fn load_conversation_source(
    conn: &Connection,
    source_id: &str,
) -> AppResult<Option<ConversationSource>> {
    conn.query_row(
        r#"
        SELECT id, adapter_id, name, kind, location, config_json, enabled,
               last_synced_at, last_sync_status, created_at, updated_at
        FROM conversation_sources
        WHERE id = ?1
        "#,
        params![source_id],
        map_conversation_source,
    )
    .optional()
    .map_err(db_error)
}

pub(crate) fn upsert_conversation_source(
    conn: &Connection,
    source: &ConversationSource,
) -> AppResult<()> {
    conn.execute(
        r#"
        INSERT INTO conversation_sources (
            id, adapter_id, name, kind, location, config_json, enabled,
            last_synced_at, last_sync_status, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        ON CONFLICT(id) DO UPDATE SET
            adapter_id = excluded.adapter_id,
            name = excluded.name,
            kind = excluded.kind,
            location = excluded.location,
            config_json = excluded.config_json,
            enabled = excluded.enabled,
            last_synced_at = excluded.last_synced_at,
            last_sync_status = excluded.last_sync_status,
            updated_at = excluded.updated_at
        "#,
        params![
            source.id,
            source.adapter_id,
            source.name,
            encode_enum(source.kind)?,
            source.location,
            source.config_json,
            if source.enabled { 1 } else { 0 },
            source.last_synced_at,
            source.last_sync_status,
            source.created_at,
            source.updated_at,
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

pub(crate) fn disable_conversation_source(
    conn: &Connection,
    source_id: &str,
) -> AppResult<ConversationSource> {
    let mut source = load_conversation_source(conn, source_id)?
        .ok_or_else(|| format!("conversation source not found: {source_id}"))?;
    source.enabled = false;
    source.updated_at = Utc::now().to_rfc3339();
    upsert_conversation_source(conn, &source)?;
    Ok(source)
}

pub(crate) fn import_conversation_sessions(
    conn: &Connection,
    source: &ConversationSource,
    sessions: &[NormalizedConversationSession],
    dry_run: bool,
) -> AppResult<ConversationImportResult> {
    let turn_count = sessions.iter().map(|session| session.turns.len()).sum();
    if dry_run {
        return Ok(ConversationImportResult {
            source_id: source.id.clone(),
            adapter_id: source.adapter_id.clone(),
            dry_run: true,
            session_count: sessions.len(),
            turn_count,
            warning_count: 0,
            warnings: Vec::new(),
        });
    }

    let now = Utc::now().to_rfc3339();
    let tx = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    let mut warning_count = 0usize;
    let warnings = Vec::new();

    for normalized in sessions {
        let session = conversation_session_from_normalized(source, normalized, &now);
        upsert_conversation_session_tx(&tx, &session)?;
        for turn in &normalized.turns {
            if turn.user_text.trim().is_empty() {
                warning_count += 1;
                continue;
            }
            let stored_turn = conversation_turn_from_normalized(&session.id, turn, &now);
            upsert_conversation_turn_tx(&tx, &stored_turn)?;
            replace_conversation_parts_tx(&tx, &stored_turn.id, &turn.parts)?;
        }
        ensure_question_groups_for_session_tx(&tx, &session.id, &now)?;
        rebuild_session_question_aggregates_tx(&tx, &session.id, &now)?;
    }

    tx.execute(
        r#"
        UPDATE conversation_sources
        SET last_synced_at = ?1, last_sync_status = 'completed', updated_at = ?1
        WHERE id = ?2
        "#,
        params![now, source.id],
    )
    .map_err(db_error)?;
    insert_sync_run_tx(
        &tx,
        &ConversationSyncRun {
            id: stable_id("conversation-sync", &[&source.id, &now]),
            source_id: Some(source.id.clone()),
            adapter_id: Some(source.adapter_id.clone()),
            status: ConversationSyncStatus::Completed,
            started_at: now.clone(),
            finished_at: Some(now.clone()),
            session_count: sessions.len() as i64,
            turn_count: turn_count as i64,
            warning_count: warning_count as i64,
            error_message: None,
        },
    )?;
    tx.commit().map_err(|error| error.to_string())?;

    Ok(ConversationImportResult {
        source_id: source.id.clone(),
        adapter_id: source.adapter_id.clone(),
        dry_run: false,
        session_count: sessions.len(),
        turn_count,
        warning_count,
        warnings,
    })
}

pub(crate) fn list_conversation_sessions(
    conn: &Connection,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    query: Option<&str>,
    limit: usize,
    offset: usize,
) -> AppResult<Vec<ConversationSessionListItem>> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, source_id, adapter_id, external_id, title, project_path,
                   started_at, updated_at, source_locator, source_fingerprint,
                   missing, created_at, imported_at
            FROM conversation_sessions
            ORDER BY COALESCE(updated_at, imported_at) DESC, title ASC
            "#,
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map([], map_conversation_session)
        .map_err(db_error)?;
    let needle = normalize_query(query);
    let mut items = Vec::new();
    for row in rows {
        let session = row.map_err(db_error)?;
        if adapter_id.is_some_and(|value| value != session.adapter_id) {
            continue;
        }
        if source_id.is_some_and(|value| value != session.source_id) {
            continue;
        }
        if let Some(needle) = &needle {
            let haystack = format!(
                "{}\n{}\n{}",
                session.title,
                session.project_path.clone().unwrap_or_default(),
                session.external_id
            )
            .to_lowercase();
            if !haystack.contains(needle) && !session_has_question_match(conn, &session.id, needle)?
            {
                continue;
            }
        }
        items.push(ConversationSessionListItem {
            question_count: count_session_questions(conn, &session.id)?,
            turn_count: count_session_turns(conn, &session.id)?,
            session,
        });
    }
    Ok(items.into_iter().skip(offset).take(limit).collect())
}

pub(crate) fn load_conversation_session_detail(
    conn: &Connection,
    session_id: &str,
) -> AppResult<ConversationSessionDetail> {
    let session = load_conversation_session(conn, session_id)?
        .ok_or_else(|| format!("conversation session not found: {session_id}"))?;
    let questions = list_conversation_question_details(conn, session_id, None, usize::MAX, 0)?;
    Ok(ConversationSessionDetail { session, questions })
}

pub(crate) fn list_conversation_question_details(
    conn: &Connection,
    session_id: &str,
    query: Option<&str>,
    limit: usize,
    offset: usize,
) -> AppResult<Vec<ConversationQuestionDetail>> {
    let questions = list_conversation_questions(conn, session_id)?;
    let needle = normalize_query(query);
    let mut details = Vec::new();
    for question in questions {
        if let Some(needle) = &needle {
            let haystack = format!(
                "{}\n{}\n{}\n{}",
                question.question_text,
                question.answer_text,
                question.code_text,
                question.command_text
            )
            .to_lowercase();
            if !haystack.contains(needle) {
                continue;
            }
        }
        details.push(load_conversation_question_detail(conn, &question.id)?);
    }
    Ok(details.into_iter().skip(offset).take(limit).collect())
}

pub(crate) fn load_conversation_question_detail(
    conn: &Connection,
    question_id: &str,
) -> AppResult<ConversationQuestionDetail> {
    let question = load_conversation_question(conn, question_id)?
        .ok_or_else(|| format!("conversation question not found: {question_id}"))?;
    let turns = load_question_turns(conn, question_id)?;
    let mut parts = Vec::new();
    for turn in &turns {
        parts.extend(load_turn_parts(conn, &turn.id)?);
    }
    Ok(ConversationQuestionDetail {
        question,
        turns,
        parts,
    })
}

pub(crate) fn merge_conversation_questions(
    conn: &Connection,
    question_ids: &[String],
    dry_run: bool,
) -> AppResult<ConversationMutationResult> {
    if question_ids.len() < 2 {
        return Err("at least two question ids are required".to_string());
    }
    let questions = question_ids
        .iter()
        .map(|id| {
            load_conversation_question(conn, id)?
                .ok_or_else(|| format!("conversation question not found: {id}"))
        })
        .collect::<AppResult<Vec<_>>>()?;
    let session_id = questions[0].session_id.clone();
    if questions
        .iter()
        .any(|question| question.session_id != session_id)
    {
        return Err("questions must belong to the same session".to_string());
    }
    ensure_question_ids_are_adjacent(conn, &session_id, question_ids)?;

    if dry_run {
        return Ok(ConversationMutationResult {
            dry_run: true,
            session_id,
            affected_question_ids: question_ids.to_vec(),
            questions: questions
                .iter()
                .map(|question| load_conversation_question_detail(conn, &question.id))
                .collect::<AppResult<Vec<_>>>()?,
        });
    }

    let now = Utc::now().to_rfc3339();
    let survivor_id = question_ids[0].clone();
    let tx = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    for question_id in &question_ids[1..] {
        let next_order = max_question_turn_order_tx(&tx, &survivor_id)? + 1;
        let turns = load_question_turn_ids_tx(&tx, question_id)?;
        for (offset, turn_id) in turns.iter().enumerate() {
            tx.execute(
                r#"
                UPDATE conversation_question_turns
                SET question_id = ?1, turn_order = ?2
                WHERE question_id = ?3 AND turn_id = ?4
                "#,
                params![
                    survivor_id,
                    next_order + offset as i64,
                    question_id,
                    turn_id
                ],
            )
            .map_err(db_error)?;
        }
        tx.execute(
            "DELETE FROM conversation_questions WHERE id = ?1",
            params![question_id],
        )
        .map_err(db_error)?;
        tx.execute(
            "DELETE FROM conversation_question_fts WHERE question_id = ?1",
            params![question_id],
        )
        .map_err(db_error)?;
    }
    tx.execute(
        "UPDATE conversation_questions SET grouping_origin = ?1, updated_at = ?2 WHERE id = ?3",
        params![
            encode_enum(ConversationGroupingOrigin::Manual)?,
            now,
            survivor_id
        ],
    )
    .map_err(db_error)?;
    renumber_questions_for_session_tx(&tx, &session_id)?;
    rebuild_session_question_aggregates_tx(&tx, &session_id, &now)?;
    tx.commit().map_err(|error| error.to_string())?;

    Ok(ConversationMutationResult {
        dry_run: false,
        session_id: session_id.clone(),
        affected_question_ids: question_ids.to_vec(),
        questions: vec![load_conversation_question_detail(conn, &survivor_id)?],
    })
}

pub(crate) fn split_conversation_question(
    conn: &Connection,
    question_id: &str,
    before_turn_id: &str,
    dry_run: bool,
) -> AppResult<ConversationMutationResult> {
    let question = load_conversation_question(conn, question_id)?
        .ok_or_else(|| format!("conversation question not found: {question_id}"))?;
    let turns = load_question_turns(conn, question_id)?;
    let split_index = turns
        .iter()
        .position(|turn| turn.id == before_turn_id)
        .ok_or_else(|| format!("turn is not in question: {before_turn_id}"))?;
    if split_index == 0 {
        return Err("split turn must not be the first turn in the question".to_string());
    }

    if dry_run {
        return Ok(ConversationMutationResult {
            dry_run: true,
            session_id: question.session_id,
            affected_question_ids: vec![question_id.to_string()],
            questions: vec![load_conversation_question_detail(conn, question_id)?],
        });
    }

    let now = Utc::now().to_rfc3339();
    let new_question_id = stable_id(
        "conversation-question",
        &[question_id, before_turn_id, &now],
    );
    let tx = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    tx.execute(
        r#"
        INSERT INTO conversation_questions (
            id, session_id, question_index, title, question_text, answer_text,
            code_text, command_text, grouping_origin, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, NULL, '', '', '', '', ?4, ?5, ?5)
        "#,
        params![
            new_question_id,
            question.session_id,
            question.question_index + 1,
            encode_enum(ConversationGroupingOrigin::Manual)?,
            now
        ],
    )
    .map_err(db_error)?;
    for (order, turn) in turns.iter().skip(split_index).enumerate() {
        tx.execute(
            r#"
            UPDATE conversation_question_turns
            SET question_id = ?1, turn_order = ?2
            WHERE question_id = ?3 AND turn_id = ?4
            "#,
            params![new_question_id, order as i64, question_id, turn.id],
        )
        .map_err(db_error)?;
    }
    tx.execute(
        "UPDATE conversation_questions SET grouping_origin = ?1, updated_at = ?2 WHERE id = ?3",
        params![
            encode_enum(ConversationGroupingOrigin::Manual)?,
            now,
            question_id
        ],
    )
    .map_err(db_error)?;
    renumber_question_turns_tx(&tx, question_id)?;
    renumber_questions_for_session_tx(&tx, &question.session_id)?;
    rebuild_session_question_aggregates_tx(&tx, &question.session_id, &now)?;
    tx.commit().map_err(|error| error.to_string())?;

    Ok(ConversationMutationResult {
        dry_run: false,
        session_id: question.session_id,
        affected_question_ids: vec![question_id.to_string(), new_question_id.clone()],
        questions: vec![
            load_conversation_question_detail(conn, question_id)?,
            load_conversation_question_detail(conn, &new_question_id)?,
        ],
    })
}

pub(crate) fn render_session_markdown_with_filter(
    conn: &Connection,
    session_id: &str,
    content_filter: &ConversationExportContentFilter,
) -> AppResult<String> {
    let detail = load_conversation_session_detail(conn, session_id)?;
    render_session_detail_markdown(&detail, None, content_filter)
}

pub(crate) fn render_session_markdown_for_questions_with_filter(
    conn: &Connection,
    session_id: &str,
    question_ids: &[String],
    content_filter: &ConversationExportContentFilter,
) -> AppResult<String> {
    let detail = load_conversation_session_detail(conn, session_id)?;
    render_session_detail_markdown(&detail, Some(question_ids), content_filter)
}

pub(super) fn render_session_detail_markdown(
    detail: &ConversationSessionDetail,
    question_ids: Option<&[String]>,
    content_filter: &ConversationExportContentFilter,
) -> AppResult<String> {
    let selected = question_ids.map(|ids| ids.iter().collect::<BTreeSet<_>>());
    if let Some(selected) = &selected {
        let available = detail
            .questions
            .iter()
            .map(|question| &question.question.id)
            .collect::<BTreeSet<_>>();
        if let Some(missing_id) = selected.iter().find(|id| !available.contains(*id)) {
            return Err(format!(
                "conversation question not found in session: {missing_id}"
            ));
        }
    }

    let mut output = String::new();
    output.push_str(&format!("# {}\n\n", detail.session.title));
    output.push_str("## Session Metadata\n\n");
    output.push_str(&format!("- Adapter: `{}`\n", detail.session.adapter_id));
    output.push_str(&format!("- Source: `{}`\n", detail.session.source_id));
    output.push_str(&format!(
        "- External Session: `{}`\n",
        detail.session.external_id
    ));
    if let Some(project_path) = &detail.session.project_path {
        output.push_str(&format!("- Project: `{project_path}`\n"));
    }
    if let Some(updated_at) = &detail.session.updated_at {
        output.push_str(&format!("- Updated: `{updated_at}`\n"));
    }
    output.push('\n');

    let questions = detail
        .questions
        .iter()
        .filter(|question| {
            selected
                .as_ref()
                .map(|ids| ids.contains(&question.question.id))
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();

    for (index, question) in questions.iter().enumerate() {
        let title = question
            .question
            .title
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| first_line(&question.question.question_text));
        output.push_str(&format!("## {}. {}\n\n", index + 1, title));
        for turn in &question.turns {
            output.push_str("### User\n\n");
            output.push_str(&turn.user_text);
            output.push_str("\n\n");
            for part in question.parts.iter().filter(|part| part.turn_id == turn.id) {
                render_part_markdown(&mut output, part, content_filter);
            }
        }
    }
    Ok(output)
}

fn builtin_adapters(now: &str) -> Vec<ConversationAdapter> {
    vec![
        ConversationAdapter {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            kind: ConversationAdapterKind::Codex,
            version: "1".to_string(),
            enabled: true,
            manifest_path: None,
            executable_path: None,
            content_hash: None,
            trusted_hash: None,
            trust_state: ConversationAdapterTrustState::BuiltIn,
            protocol_version: None,
            capabilities: vec![
                "probe".to_string(),
                "list_sessions".to_string(),
                "read_session".to_string(),
            ],
            input_kinds: vec![ConversationSourceKind::Live, ConversationSourceKind::File],
            created_at: now.to_string(),
            updated_at: now.to_string(),
        },
        ConversationAdapter {
            id: "claude-code".to_string(),
            name: "Claude Code".to_string(),
            kind: ConversationAdapterKind::ClaudeCode,
            version: "1".to_string(),
            enabled: true,
            manifest_path: None,
            executable_path: None,
            content_hash: None,
            trusted_hash: None,
            trust_state: ConversationAdapterTrustState::BuiltIn,
            protocol_version: None,
            capabilities: vec![
                "probe".to_string(),
                "list_sessions".to_string(),
                "read_session".to_string(),
            ],
            input_kinds: vec![
                ConversationSourceKind::Live,
                ConversationSourceKind::Directory,
                ConversationSourceKind::File,
            ],
            created_at: now.to_string(),
            updated_at: now.to_string(),
        },
        ConversationAdapter {
            id: "opencode".to_string(),
            name: "OpenCode".to_string(),
            kind: ConversationAdapterKind::OpenCode,
            version: "1".to_string(),
            enabled: true,
            manifest_path: None,
            executable_path: None,
            content_hash: None,
            trusted_hash: None,
            trust_state: ConversationAdapterTrustState::BuiltIn,
            protocol_version: None,
            capabilities: vec![
                "probe".to_string(),
                "list_sessions".to_string(),
                "read_session".to_string(),
            ],
            input_kinds: vec![ConversationSourceKind::Live, ConversationSourceKind::Sqlite],
            created_at: now.to_string(),
            updated_at: now.to_string(),
        },
    ]
}

fn builtin_sources(now: &str) -> Vec<ConversationSource> {
    vec![
        ConversationSource {
            id: "codex-live".to_string(),
            adapter_id: "codex".to_string(),
            name: "Codex local sessions".to_string(),
            kind: ConversationSourceKind::Live,
            location: "~/.codex".to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        },
        ConversationSource {
            id: "claude-code-live".to_string(),
            adapter_id: "claude-code".to_string(),
            name: "Claude Code local sessions".to_string(),
            kind: ConversationSourceKind::Live,
            location: "~/.claude/projects".to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        },
        ConversationSource {
            id: "opencode-live".to_string(),
            adapter_id: "opencode".to_string(),
            name: "OpenCode local sessions".to_string(),
            kind: ConversationSourceKind::Live,
            location: "~/.local/share/opencode/opencode.db".to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        },
    ]
}

fn map_conversation_adapter(row: &Row<'_>) -> rusqlite::Result<ConversationAdapter> {
    Ok(ConversationAdapter {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: decode_enum(row.get::<_, String>(2)?).map_err(to_sql_error)?,
        version: row.get(3)?,
        enabled: row.get::<_, i64>(4)? == 1,
        manifest_path: row.get(5)?,
        executable_path: row.get(6)?,
        content_hash: row.get(7)?,
        trusted_hash: row.get(8)?,
        trust_state: decode_enum(row.get::<_, String>(9)?).map_err(to_sql_error)?,
        protocol_version: row.get(10)?,
        capabilities: decode_json(row.get::<_, String>(11)?).map_err(to_sql_error)?,
        input_kinds: decode_json(row.get::<_, String>(12)?).map_err(to_sql_error)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

fn map_conversation_source(row: &Row<'_>) -> rusqlite::Result<ConversationSource> {
    Ok(ConversationSource {
        id: row.get(0)?,
        adapter_id: row.get(1)?,
        name: row.get(2)?,
        kind: decode_enum(row.get::<_, String>(3)?).map_err(to_sql_error)?,
        location: row.get(4)?,
        config_json: row.get(5)?,
        enabled: row.get::<_, i64>(6)? == 1,
        last_synced_at: row.get(7)?,
        last_sync_status: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

pub(super) fn map_conversation_session(row: &Row<'_>) -> rusqlite::Result<ConversationSession> {
    Ok(ConversationSession {
        id: row.get(0)?,
        source_id: row.get(1)?,
        adapter_id: row.get(2)?,
        external_id: row.get(3)?,
        title: row.get(4)?,
        project_path: row.get(5)?,
        started_at: row.get(6)?,
        updated_at: row.get(7)?,
        source_locator: row.get(8)?,
        source_fingerprint: row.get(9)?,
        missing: row.get::<_, i64>(10)? == 1,
        created_at: row.get(11)?,
        imported_at: row.get(12)?,
    })
}

pub(super) fn map_conversation_turn(row: &Row<'_>) -> rusqlite::Result<ConversationTurn> {
    Ok(ConversationTurn {
        id: row.get(0)?,
        session_id: row.get(1)?,
        external_id: row.get(2)?,
        turn_index: row.get(3)?,
        user_text: row.get(4)?,
        title: row.get(5)?,
        started_at: row.get(6)?,
        ended_at: row.get(7)?,
        fingerprint: row.get(8)?,
        missing: row.get::<_, i64>(9)? == 1,
        imported_at: row.get(10)?,
    })
}

pub(super) fn map_conversation_part(row: &Row<'_>) -> rusqlite::Result<ConversationPart> {
    Ok(ConversationPart {
        id: row.get(0)?,
        turn_id: row.get(1)?,
        part_index: row.get(2)?,
        role: decode_enum(row.get::<_, String>(3)?).map_err(to_sql_error)?,
        kind: decode_enum(row.get::<_, String>(4)?).map_err(to_sql_error)?,
        text: row.get(5)?,
        language: row.get(6)?,
        command: row.get(7)?,
        cwd: row.get(8)?,
        status: row.get(9)?,
        exit_code: row.get(10)?,
        metadata_json: row.get(11)?,
    })
}

pub(super) fn map_conversation_question(row: &Row<'_>) -> rusqlite::Result<ConversationQuestion> {
    Ok(ConversationQuestion {
        id: row.get(0)?,
        session_id: row.get(1)?,
        question_index: row.get(2)?,
        title: row.get(3)?,
        question_text: row.get(4)?,
        answer_text: row.get(5)?,
        code_text: row.get(6)?,
        command_text: row.get(7)?,
        grouping_origin: decode_enum(row.get::<_, String>(8)?).map_err(to_sql_error)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn conversation_session_from_normalized(
    source: &ConversationSource,
    normalized: &NormalizedConversationSession,
    now: &str,
) -> ConversationSession {
    ConversationSession {
        id: stable_id(
            "conversation-session",
            &[&source.id, &normalized.external_id],
        ),
        source_id: source.id.clone(),
        adapter_id: source.adapter_id.clone(),
        external_id: normalized.external_id.clone(),
        title: normalized
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Untitled session")
            .to_string(),
        project_path: normalized.project_path.clone(),
        started_at: normalized.started_at.clone(),
        updated_at: normalized.updated_at.clone(),
        source_locator: normalized.source_locator.clone(),
        source_fingerprint: normalized.source_fingerprint.clone(),
        missing: false,
        created_at: now.to_string(),
        imported_at: now.to_string(),
    }
}

fn conversation_turn_from_normalized(
    session_id: &str,
    normalized: &crate::models::NormalizedConversationTurn,
    now: &str,
) -> ConversationTurn {
    ConversationTurn {
        id: stable_id("conversation-turn", &[session_id, &normalized.external_id]),
        session_id: session_id.to_string(),
        external_id: normalized.external_id.clone(),
        turn_index: normalized.turn_index,
        user_text: normalized.user_text.trim().to_string(),
        title: normalized.title.clone(),
        started_at: normalized.started_at.clone(),
        ended_at: normalized.ended_at.clone(),
        fingerprint: conversation_turn_fingerprint(normalized),
        missing: false,
        imported_at: now.to_string(),
    }
}

fn upsert_conversation_session_tx(
    tx: &rusqlite::Transaction<'_>,
    session: &ConversationSession,
) -> AppResult<()> {
    tx.execute(
        r#"
        INSERT INTO conversation_sessions (
            id, source_id, adapter_id, external_id, title, project_path, started_at,
            updated_at, source_locator, source_fingerprint, missing, created_at, imported_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        ON CONFLICT(source_id, external_id) DO UPDATE SET
            adapter_id = excluded.adapter_id,
            title = excluded.title,
            project_path = excluded.project_path,
            started_at = excluded.started_at,
            updated_at = excluded.updated_at,
            source_locator = excluded.source_locator,
            source_fingerprint = excluded.source_fingerprint,
            missing = 0,
            imported_at = excluded.imported_at
        "#,
        params![
            session.id,
            session.source_id,
            session.adapter_id,
            session.external_id,
            session.title,
            session.project_path,
            session.started_at,
            session.updated_at,
            session.source_locator,
            session.source_fingerprint,
            if session.missing { 1 } else { 0 },
            session.created_at,
            session.imported_at,
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

fn upsert_conversation_turn_tx(
    tx: &rusqlite::Transaction<'_>,
    turn: &ConversationTurn,
) -> AppResult<()> {
    tx.execute(
        r#"
        INSERT INTO conversation_turns (
            id, session_id, external_id, turn_index, user_text, title, started_at,
            ended_at, fingerprint, missing, imported_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        ON CONFLICT(session_id, external_id) DO UPDATE SET
            turn_index = excluded.turn_index,
            user_text = excluded.user_text,
            title = excluded.title,
            started_at = excluded.started_at,
            ended_at = excluded.ended_at,
            fingerprint = excluded.fingerprint,
            missing = 0,
            imported_at = excluded.imported_at
        "#,
        params![
            turn.id,
            turn.session_id,
            turn.external_id,
            turn.turn_index,
            turn.user_text,
            turn.title,
            turn.started_at,
            turn.ended_at,
            turn.fingerprint,
            if turn.missing { 1 } else { 0 },
            turn.imported_at,
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

fn replace_conversation_parts_tx(
    tx: &rusqlite::Transaction<'_>,
    turn_id: &str,
    parts: &[crate::models::NormalizedConversationPart],
) -> AppResult<()> {
    tx.execute(
        "DELETE FROM conversation_parts WHERE turn_id = ?1",
        params![turn_id],
    )
    .map_err(db_error)?;
    for (index, part) in parts.iter().enumerate() {
        tx.execute(
            r#"
            INSERT INTO conversation_parts (
                id, turn_id, part_index, role, kind, text, language, command,
                cwd, status, exit_code, metadata_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                stable_id("conversation-part", &[turn_id, &index.to_string()]),
                turn_id,
                index as i64,
                encode_enum(part.role)?,
                encode_enum(part.kind)?,
                part.text,
                part.language,
                part.command,
                part.cwd,
                part.status,
                part.exit_code,
                part.metadata_json,
            ],
        )
        .map_err(db_error)?;
    }
    Ok(())
}

fn ensure_question_groups_for_session_tx(
    tx: &rusqlite::Transaction<'_>,
    session_id: &str,
    now: &str,
) -> AppResult<()> {
    let turns = load_session_turns_tx(tx, session_id)?;
    let existing_memberships = load_turn_question_memberships_tx(tx, session_id)?;
    let missing_turns = turns
        .iter()
        .filter(|turn| !existing_memberships.contains_key(&turn.id))
        .map(|turn| (turn.id.clone(), turn.user_text.clone()))
        .collect::<Vec<_>>();
    if missing_turns.is_empty() {
        return Ok(());
    }

    for group in group_turn_ids_by_question(missing_turns) {
        let first_turn_id = group
            .turn_ids
            .first()
            .ok_or_else(|| "empty conversation question group".to_string())?;
        let previous_question_id = previous_question_id_for_turn_tx(tx, session_id, first_turn_id)?;
        let question_id = if group.origin == ConversationGroupingOrigin::AutoMerged {
            previous_question_id
                .unwrap_or_else(|| stable_id("conversation-question", &[session_id, first_turn_id]))
        } else {
            stable_id("conversation-question", &[session_id, first_turn_id])
        };
        if load_conversation_question_tx(tx, &question_id)?.is_none() {
            tx.execute(
                r#"
                INSERT INTO conversation_questions (
                    id, session_id, question_index, title, question_text, answer_text,
                    code_text, command_text, grouping_origin, created_at, updated_at
                )
                VALUES (?1, ?2, ?3, NULL, '', '', '', '', ?4, ?5, ?5)
                "#,
                params![
                    question_id,
                    session_id,
                    next_question_index_tx(tx, session_id)?,
                    encode_enum(group.origin)?,
                    now
                ],
            )
            .map_err(db_error)?;
        }
        let start_order = max_question_turn_order_tx(tx, &question_id)? + 1;
        for (offset, turn_id) in group.turn_ids.iter().enumerate() {
            tx.execute(
                r#"
                INSERT OR IGNORE INTO conversation_question_turns (question_id, turn_id, turn_order)
                VALUES (?1, ?2, ?3)
                "#,
                params![question_id, turn_id, start_order + offset as i64],
            )
            .map_err(db_error)?;
        }
    }
    renumber_questions_for_session_tx(tx, session_id)?;
    Ok(())
}

fn rebuild_session_question_aggregates_tx(
    tx: &rusqlite::Transaction<'_>,
    session_id: &str,
    now: &str,
) -> AppResult<()> {
    let question_ids = question_ids_for_session_tx(tx, session_id)?;
    for question_id in question_ids {
        rebuild_question_aggregate_tx(tx, &question_id, now)?;
    }
    Ok(())
}

fn rebuild_question_aggregate_tx(
    tx: &rusqlite::Transaction<'_>,
    question_id: &str,
    now: &str,
) -> AppResult<()> {
    let turns = load_question_turns_tx(tx, question_id)?;
    let mut question_text = Vec::new();
    let mut answer_text = Vec::new();
    let mut code_text = Vec::new();
    let mut command_text = Vec::new();

    for turn in &turns {
        question_text.push(turn.user_text.clone());
        for part in load_turn_parts_tx(tx, &turn.id)? {
            match (part.role, part.kind) {
                (ConversationPartRole::Assistant, ConversationPartKind::Text)
                | (ConversationPartRole::Tool, ConversationPartKind::Text)
                | (ConversationPartRole::Assistant, ConversationPartKind::Subagent)
                | (ConversationPartRole::Tool, ConversationPartKind::Tool) => {
                    if let Some(text) = part.text {
                        answer_text.push(text);
                    }
                }
                (_, ConversationPartKind::CodeBlock) => {
                    if let Some(text) = part.text {
                        code_text.push(text);
                    }
                }
                (_, ConversationPartKind::Command) => {
                    if let Some(command) = part.command.or(part.text) {
                        command_text.push(command);
                    }
                }
                _ => {}
            }
        }
    }

    let question_text = question_text.join("\n\n");
    let answer_text = answer_text.join("\n\n");
    let code_text = code_text.join("\n\n");
    let command_text = command_text.join("\n\n");
    let title = first_line(&question_text);

    tx.execute(
        r#"
        UPDATE conversation_questions
        SET title = COALESCE(NULLIF(title, ''), ?1),
            question_text = ?2,
            answer_text = ?3,
            code_text = ?4,
            command_text = ?5,
            updated_at = ?6
        WHERE id = ?7
        "#,
        params![
            title,
            question_text,
            answer_text,
            code_text,
            command_text,
            now,
            question_id
        ],
    )
    .map_err(db_error)?;
    let session_id: String = tx
        .query_row(
            "SELECT session_id FROM conversation_questions WHERE id = ?1",
            params![question_id],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    tx.execute(
        "DELETE FROM conversation_question_fts WHERE question_id = ?1",
        params![question_id],
    )
    .map_err(db_error)?;
    tx.execute(
        r#"
        INSERT INTO conversation_question_fts (
            question_id, session_id, question_text, answer_text, code_text, command_text
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![
            question_id,
            session_id,
            question_text,
            answer_text,
            code_text,
            command_text
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

fn insert_sync_run_tx(tx: &rusqlite::Transaction<'_>, run: &ConversationSyncRun) -> AppResult<()> {
    tx.execute(
        r#"
        INSERT INTO conversation_sync_runs (
            id, source_id, adapter_id, status, started_at, finished_at,
            session_count, turn_count, warning_count, error_message
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            run.id,
            run.source_id,
            run.adapter_id,
            encode_enum(run.status)?,
            run.started_at,
            run.finished_at,
            run.session_count,
            run.turn_count,
            run.warning_count,
            run.error_message,
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

fn load_conversation_session(
    conn: &Connection,
    session_id: &str,
) -> AppResult<Option<ConversationSession>> {
    conn.query_row(
        r#"
        SELECT id, source_id, adapter_id, external_id, title, project_path,
               started_at, updated_at, source_locator, source_fingerprint,
               missing, created_at, imported_at
        FROM conversation_sessions
        WHERE id = ?1
        "#,
        params![session_id],
        map_conversation_session,
    )
    .optional()
    .map_err(db_error)
}

fn list_conversation_questions(
    conn: &Connection,
    session_id: &str,
) -> AppResult<Vec<ConversationQuestion>> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, session_id, question_index, title, question_text, answer_text,
                   code_text, command_text, grouping_origin, created_at, updated_at
            FROM conversation_questions
            WHERE session_id = ?1
            ORDER BY question_index ASC
            "#,
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map(params![session_id], map_conversation_question)
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

fn load_conversation_question(
    conn: &Connection,
    question_id: &str,
) -> AppResult<Option<ConversationQuestion>> {
    conn.query_row(
        r#"
        SELECT id, session_id, question_index, title, question_text, answer_text,
               code_text, command_text, grouping_origin, created_at, updated_at
        FROM conversation_questions
        WHERE id = ?1
        "#,
        params![question_id],
        map_conversation_question,
    )
    .optional()
    .map_err(db_error)
}

fn load_conversation_question_tx(
    tx: &rusqlite::Transaction<'_>,
    question_id: &str,
) -> AppResult<Option<ConversationQuestion>> {
    tx.query_row(
        r#"
        SELECT id, session_id, question_index, title, question_text, answer_text,
               code_text, command_text, grouping_origin, created_at, updated_at
        FROM conversation_questions
        WHERE id = ?1
        "#,
        params![question_id],
        map_conversation_question,
    )
    .optional()
    .map_err(db_error)
}

fn load_question_turns(conn: &Connection, question_id: &str) -> AppResult<Vec<ConversationTurn>> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
                   t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at
            FROM conversation_question_turns qt
            JOIN conversation_turns t ON t.id = qt.turn_id
            WHERE qt.question_id = ?1
            ORDER BY qt.turn_order ASC, t.turn_index ASC
            "#,
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map(params![question_id], map_conversation_turn)
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

fn load_turn_parts(conn: &Connection, turn_id: &str) -> AppResult<Vec<ConversationPart>> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, turn_id, part_index, role, kind, text, language, command,
                   cwd, status, exit_code, metadata_json
            FROM conversation_parts
            WHERE turn_id = ?1
            ORDER BY part_index ASC
            "#,
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map(params![turn_id], map_conversation_part)
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

fn load_session_turns_tx(
    tx: &rusqlite::Transaction<'_>,
    session_id: &str,
) -> AppResult<Vec<ConversationTurn>> {
    let mut stmt = tx
        .prepare(
            r#"
            SELECT id, session_id, external_id, turn_index, user_text, title,
                   started_at, ended_at, fingerprint, missing, imported_at
            FROM conversation_turns
            WHERE session_id = ?1
            ORDER BY turn_index ASC, imported_at ASC
            "#,
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map(params![session_id], map_conversation_turn)
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

fn load_question_turns_tx(
    tx: &rusqlite::Transaction<'_>,
    question_id: &str,
) -> AppResult<Vec<ConversationTurn>> {
    let mut stmt = tx
        .prepare(
            r#"
            SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
                   t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at
            FROM conversation_question_turns qt
            JOIN conversation_turns t ON t.id = qt.turn_id
            WHERE qt.question_id = ?1
            ORDER BY qt.turn_order ASC, t.turn_index ASC
            "#,
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map(params![question_id], map_conversation_turn)
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

fn load_turn_parts_tx(
    tx: &rusqlite::Transaction<'_>,
    turn_id: &str,
) -> AppResult<Vec<ConversationPart>> {
    let mut stmt = tx
        .prepare(
            r#"
            SELECT id, turn_id, part_index, role, kind, text, language, command,
                   cwd, status, exit_code, metadata_json
            FROM conversation_parts
            WHERE turn_id = ?1
            ORDER BY part_index ASC
            "#,
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map(params![turn_id], map_conversation_part)
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

fn load_turn_question_memberships_tx(
    tx: &rusqlite::Transaction<'_>,
    session_id: &str,
) -> AppResult<BTreeMap<String, String>> {
    let mut stmt = tx
        .prepare(
            r#"
            SELECT qt.turn_id, qt.question_id
            FROM conversation_question_turns qt
            JOIN conversation_turns t ON t.id = qt.turn_id
            WHERE t.session_id = ?1
            "#,
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map(params![session_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(db_error)?;
    let mut memberships = BTreeMap::new();
    for row in rows {
        let (turn_id, question_id) = row.map_err(db_error)?;
        memberships.insert(turn_id, question_id);
    }
    Ok(memberships)
}

fn previous_question_id_for_turn_tx(
    tx: &rusqlite::Transaction<'_>,
    session_id: &str,
    turn_id: &str,
) -> AppResult<Option<String>> {
    tx.query_row(
        r#"
        SELECT qt.question_id
        FROM conversation_turns current
        JOIN conversation_turns previous
          ON previous.session_id = current.session_id
         AND previous.turn_index < current.turn_index
        JOIN conversation_question_turns qt ON qt.turn_id = previous.id
        WHERE current.session_id = ?1 AND current.id = ?2
        ORDER BY previous.turn_index DESC
        LIMIT 1
        "#,
        params![session_id, turn_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(db_error)
}

fn next_question_index_tx(tx: &rusqlite::Transaction<'_>, session_id: &str) -> AppResult<i64> {
    let max_index: Option<i64> = tx
        .query_row(
            "SELECT MAX(question_index) FROM conversation_questions WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    Ok(max_index.unwrap_or(-1) + 1)
}

fn max_question_turn_order_tx(tx: &rusqlite::Transaction<'_>, question_id: &str) -> AppResult<i64> {
    let max_order: Option<i64> = tx
        .query_row(
            "SELECT MAX(turn_order) FROM conversation_question_turns WHERE question_id = ?1",
            params![question_id],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    Ok(max_order.unwrap_or(-1))
}

fn load_question_turn_ids_tx(
    tx: &rusqlite::Transaction<'_>,
    question_id: &str,
) -> AppResult<Vec<String>> {
    let mut stmt = tx
        .prepare(
            r#"
            SELECT turn_id
            FROM conversation_question_turns
            WHERE question_id = ?1
            ORDER BY turn_order ASC
            "#,
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map(params![question_id], |row| row.get::<_, String>(0))
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

fn question_ids_for_session_tx(
    tx: &rusqlite::Transaction<'_>,
    session_id: &str,
) -> AppResult<Vec<String>> {
    let mut stmt = tx
        .prepare(
            r#"
            SELECT q.id
            FROM conversation_questions q
            WHERE q.session_id = ?1
            ORDER BY q.question_index ASC
            "#,
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map(params![session_id], |row| row.get::<_, String>(0))
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

fn renumber_question_turns_tx(tx: &rusqlite::Transaction<'_>, question_id: &str) -> AppResult<()> {
    let turn_ids = load_question_turn_ids_tx(tx, question_id)?;
    for (index, turn_id) in turn_ids.iter().enumerate() {
        tx.execute(
            r#"
            UPDATE conversation_question_turns
            SET turn_order = ?1
            WHERE question_id = ?2 AND turn_id = ?3
            "#,
            params![index as i64, question_id, turn_id],
        )
        .map_err(db_error)?;
    }
    Ok(())
}

fn renumber_questions_for_session_tx(
    tx: &rusqlite::Transaction<'_>,
    session_id: &str,
) -> AppResult<()> {
    let mut stmt = tx
        .prepare(
            r#"
            SELECT q.id
            FROM conversation_questions q
            JOIN conversation_question_turns qt ON qt.question_id = q.id
            JOIN conversation_turns t ON t.id = qt.turn_id
            WHERE q.session_id = ?1
            GROUP BY q.id
            ORDER BY MIN(t.turn_index) ASC
            "#,
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map(params![session_id], |row| row.get::<_, String>(0))
        .map_err(db_error)?;
    let question_ids = rows.collect::<Result<Vec<_>, _>>().map_err(db_error)?;
    drop(stmt);

    for (index, question_id) in question_ids.iter().enumerate() {
        tx.execute(
            "UPDATE conversation_questions SET question_index = ?1 WHERE id = ?2",
            params![1_000_000i64 + index as i64, question_id],
        )
        .map_err(db_error)?;
    }
    for (index, question_id) in question_ids.iter().enumerate() {
        tx.execute(
            "UPDATE conversation_questions SET question_index = ?1 WHERE id = ?2",
            params![index as i64, question_id],
        )
        .map_err(db_error)?;
    }
    Ok(())
}

fn ensure_question_ids_are_adjacent(
    conn: &Connection,
    session_id: &str,
    question_ids: &[String],
) -> AppResult<()> {
    let ordered = list_conversation_questions(conn, session_id)?
        .into_iter()
        .map(|question| question.id)
        .collect::<Vec<_>>();
    let selected = question_ids.iter().collect::<BTreeSet<_>>();
    let positions = ordered
        .iter()
        .enumerate()
        .filter_map(|(index, id)| selected.contains(id).then_some(index))
        .collect::<Vec<_>>();
    if positions.len() != question_ids.len() {
        return Err("all questions must exist in the session".to_string());
    }
    if positions
        .windows(2)
        .any(|window| window[1] != window[0] + 1)
    {
        return Err("questions must be adjacent".to_string());
    }
    if positions
        .iter()
        .map(|index| &ordered[*index])
        .zip(question_ids.iter())
        .any(|(actual, requested)| actual != requested)
    {
        return Err("question ids must be supplied in session order".to_string());
    }
    Ok(())
}

fn count_session_questions(conn: &Connection, session_id: &str) -> AppResult<usize> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM conversation_questions WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    Ok(count as usize)
}

fn count_session_turns(conn: &Connection, session_id: &str) -> AppResult<usize> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM conversation_turns WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    Ok(count as usize)
}

fn session_has_question_match(conn: &Connection, session_id: &str, query: &str) -> AppResult<bool> {
    let count: i64 = conn
        .query_row(
            r#"
            SELECT COUNT(*)
            FROM conversation_questions
            WHERE session_id = ?1
              AND (
                lower(question_text) LIKE ?2
                OR lower(answer_text) LIKE ?2
                OR lower(code_text) LIKE ?2
                OR lower(command_text) LIKE ?2
              )
            "#,
            params![session_id, format!("%{query}%")],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    Ok(count > 0)
}

fn normalize_query(query: Option<&str>) -> Option<String> {
    query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase)
}

fn first_line(text: &str) -> String {
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("Untitled question");
    let trimmed = line.trim();
    if trimmed.chars().count() > 96 {
        trimmed.chars().take(96).collect()
    } else {
        trimmed.to_string()
    }
}

fn render_part_markdown(
    output: &mut String,
    part: &ConversationPart,
    content_filter: &ConversationExportContentFilter,
) {
    match part.kind {
        ConversationPartKind::Text => {
            if part.role == ConversationPartRole::Tool {
                if content_filter.result {
                    render_text_section(output, "### Result", part.text.as_deref());
                }
            } else if content_filter.answer {
                render_text_section(output, role_heading(part.role), part.text.as_deref());
            }
        }
        ConversationPartKind::CodeBlock => {
            if content_filter.code {
                output.push_str("### Code\n\n```");
                if let Some(language) = &part.language {
                    output.push_str(language);
                }
                output.push('\n');
                if let Some(text) = &part.text {
                    output.push_str(text);
                    output.push('\n');
                }
                output.push_str("```\n\n");
            }
        }
        ConversationPartKind::Command => {
            if content_filter.command {
                output.push_str("### Command\n\n```sh\n");
                if let Some(command) = part.command.as_ref().or(part.text.as_ref()) {
                    output.push_str(command);
                    output.push('\n');
                }
                output.push_str("```\n\n");
            }
            if content_filter.result {
                render_text_section(output, "### Result", command_result_text(part).as_deref());
            }
        }
        ConversationPartKind::Tool
        | ConversationPartKind::Subagent
        | ConversationPartKind::FileChange => {
            let is_result = is_export_result_part(part);
            if (is_result && content_filter.result) || (!is_result && content_filter.tool) {
                output.push_str(&format!("### {:?}\n\n", part.kind));
                if let Some(text) = &part.text {
                    output.push_str(text);
                    output.push_str("\n\n");
                }
            }
        }
        ConversationPartKind::Metadata => {}
    }
}

fn render_text_section(output: &mut String, heading: &str, text: Option<&str>) {
    let Some(text) = text.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    output.push_str(heading);
    output.push_str("\n\n");
    output.push_str(text);
    output.push_str("\n\n");
}

fn command_result_text(part: &ConversationPart) -> Option<String> {
    let mut lines = Vec::new();
    if let Some(text) =
        part.text.as_deref().map(str::trim).filter(|value| {
            !value.is_empty() && part.command.as_deref().map(str::trim) != Some(*value)
        })
    {
        lines.push(text.to_string());
    }
    if let Some(status) = part
        .status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("Status: `{status}`"));
    }
    if let Some(exit_code) = part.exit_code {
        lines.push(format!("Exit code: `{exit_code}`"));
    }
    (!lines.is_empty()).then(|| lines.join("\n\n"))
}

fn is_export_result_part(part: &ConversationPart) -> bool {
    if part.role == ConversationPartRole::Tool && part.kind == ConversationPartKind::Text {
        return true;
    }
    if part.status.is_some() || part.exit_code.is_some() {
        return true;
    }
    let metadata = part.metadata_json.as_deref().unwrap_or("").to_lowercase();
    [
        "tool_result",
        "tool-result",
        "tool_output",
        "tooloutput",
        "function_call_output",
        "\"output\"",
        "\"result\"",
    ]
    .iter()
    .any(|marker| metadata.contains(marker))
}

fn role_heading(role: ConversationPartRole) -> &'static str {
    match role {
        ConversationPartRole::User => "### User",
        ConversationPartRole::Assistant => "### Assistant",
        ConversationPartRole::Tool => "### Tool",
        ConversationPartRole::System => "### System",
    }
}

fn default_true() -> bool {
    true
}

fn stable_id(prefix: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(b"\0");
    }
    format!("{prefix}-{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        ConversationPartRole, NormalizedConversationPart, NormalizedConversationTurn,
    };

    #[test]
    fn imports_turns_and_preserves_manual_grouping_across_resync() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::store::sql::INIT_SCHEMA).unwrap();
        seed_builtin_conversation_adapters(&conn).unwrap();
        let source = load_conversation_source(&conn, "codex-live")
            .unwrap()
            .unwrap();

        import_conversation_sessions(&conn, &source, &[fixture_session("v1")], false).unwrap();
        let sessions =
            list_conversation_sessions(&conn, None, Some("codex-live"), None, 20, 0).unwrap();
        let detail = load_conversation_session_detail(&conn, &sessions[0].session.id).unwrap();
        assert_eq!(detail.questions.len(), 2);
        assert_eq!(detail.questions[0].turns.len(), 2);

        let question_ids = detail
            .questions
            .iter()
            .map(|question| question.question.id.clone())
            .collect::<Vec<_>>();
        merge_conversation_questions(&conn, &question_ids, false).unwrap();
        import_conversation_sessions(&conn, &source, &[fixture_session("v2")], false).unwrap();

        let detail = load_conversation_session_detail(&conn, &sessions[0].session.id).unwrap();
        assert_eq!(detail.questions.len(), 1);
        assert_eq!(detail.questions[0].turns.len(), 3);
        assert_eq!(
            detail.questions[0].question.grouping_origin,
            ConversationGroupingOrigin::Manual
        );
    }

    #[test]
    fn split_rejects_first_turn_and_creates_tail_question() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::store::sql::INIT_SCHEMA).unwrap();
        seed_builtin_conversation_adapters(&conn).unwrap();
        let source = load_conversation_source(&conn, "codex-live")
            .unwrap()
            .unwrap();
        import_conversation_sessions(&conn, &source, &[fixture_session("v1")], false).unwrap();
        let session = list_conversation_sessions(&conn, None, None, None, 20, 0)
            .unwrap()
            .remove(0);
        let detail = load_conversation_session_detail(&conn, &session.session.id).unwrap();
        let question_ids = detail
            .questions
            .iter()
            .map(|question| question.question.id.clone())
            .collect::<Vec<_>>();
        merge_conversation_questions(&conn, &question_ids, false).unwrap();
        let detail = load_conversation_session_detail(&conn, &session.session.id).unwrap();
        let merged = &detail.questions[0];

        let first_turn_id = &merged.turns[0].id;
        assert!(
            split_conversation_question(&conn, &merged.question.id, first_turn_id, false).is_err()
        );

        let second_turn_id = &merged.turns[1].id;
        split_conversation_question(&conn, &merged.question.id, second_turn_id, false).unwrap();
        let detail = load_conversation_session_detail(&conn, &session.session.id).unwrap();
        assert_eq!(detail.questions.len(), 2);
    }

    #[test]
    fn renders_markdown_for_selected_questions_only() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::store::sql::INIT_SCHEMA).unwrap();
        seed_builtin_conversation_adapters(&conn).unwrap();
        let source = load_conversation_source(&conn, "codex-live")
            .unwrap()
            .unwrap();
        import_conversation_sessions(&conn, &source, &[fixture_session("v1")], false).unwrap();
        let session = list_conversation_sessions(&conn, None, None, None, 20, 0)
            .unwrap()
            .remove(0);
        let detail = load_conversation_session_detail(&conn, &session.session.id).unwrap();
        let selected_question_id = detail.questions[1].question.id.clone();

        let markdown = render_session_markdown_for_questions_with_filter(
            &conn,
            &session.session.id,
            &[selected_question_id],
            &ConversationExportContentFilter::default(),
        )
        .unwrap();

        assert!(markdown.contains("## 1. Export it"));
        assert!(markdown.contains("answer for t3"));
        assert!(!markdown.contains("How does sync work?"));
        assert!(!markdown.contains("answer for t1"));
    }

    #[test]
    fn renders_markdown_for_selected_content_types_only() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::store::sql::INIT_SCHEMA).unwrap();
        seed_builtin_conversation_adapters(&conn).unwrap();
        let source = load_conversation_source(&conn, "codex-live")
            .unwrap()
            .unwrap();
        let mut session = fixture_session("v2");
        session.turns[2].parts.push(NormalizedConversationPart {
            role: ConversationPartRole::Tool,
            kind: ConversationPartKind::Command,
            text: Some("tests passed".to_string()),
            language: None,
            command: Some("assetiweave-cli conversation session export".to_string()),
            cwd: Some("/tmp/project".to_string()),
            status: Some("completed".to_string()),
            exit_code: Some(0),
            metadata_json: None,
        });
        import_conversation_sessions(&conn, &source, &[session], false).unwrap();
        let session = list_conversation_sessions(&conn, None, None, None, 20, 0)
            .unwrap()
            .remove(0);
        let detail = load_conversation_session_detail(&conn, &session.session.id).unwrap();
        let selected_question_id = detail.questions[1].question.id.clone();

        let markdown = render_session_markdown_for_questions_with_filter(
            &conn,
            &session.session.id,
            &[selected_question_id],
            &ConversationExportContentFilter {
                answer: false,
                tool: false,
                command: true,
                code: false,
                result: true,
            },
        )
        .unwrap();

        assert!(markdown.contains("### Command"));
        assert!(markdown.contains("assetiweave-cli conversation session export"));
        assert!(markdown.contains("### Result"));
        assert!(markdown.contains("tests passed"));
        assert!(!markdown.contains("answer for t3"));
        assert!(!markdown.contains("cargo test"));
    }

    fn fixture_session(version: &str) -> NormalizedConversationSession {
        let mut turns = vec![
            fixture_turn("t1", 0, "How does sync work?"),
            fixture_turn("t2", 1, "继续"),
            fixture_turn("t3", 2, "Export it"),
        ];
        if version == "v2" {
            turns[0].parts.push(NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::CodeBlock,
                text: Some("cargo test".to_string()),
                language: Some("sh".to_string()),
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
                metadata_json: None,
            });
        }
        NormalizedConversationSession {
            external_id: "session-1".to_string(),
            title: Some("Fixture".to_string()),
            project_path: Some("/tmp/project".to_string()),
            started_at: None,
            updated_at: None,
            source_locator: None,
            source_fingerprint: None,
            turns,
        }
    }

    fn fixture_turn(id: &str, index: i64, user_text: &str) -> NormalizedConversationTurn {
        NormalizedConversationTurn {
            external_id: id.to_string(),
            turn_index: index,
            user_text: user_text.to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some(format!("answer for {id}")),
                language: None,
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
                metadata_json: None,
            }],
        }
    }
}
