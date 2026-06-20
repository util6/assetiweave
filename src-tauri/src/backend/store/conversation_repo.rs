use crate::backend::dto::{
    AppResult, ConversationExportContentFilter, ConversationMutationResult,
    ConversationQuestionDetail, ConversationRecordKind, ConversationSearchCardType,
    ConversationSearchHit, ConversationSearchPage, ConversationSessionDetail,
    ConversationSessionListItem,
};
use crate::backend::models::{
    conversation_turn_fingerprint, group_turn_ids_by_question, ConversationAdapter,
    ConversationAdapterKind, ConversationAdapterTrustState, ConversationGroupingOrigin,
    ConversationPart, ConversationPartKind, ConversationPartRole, ConversationQuestion,
    ConversationSession, ConversationSource, ConversationSourceKind, ConversationSyncRun,
    ConversationSyncStatus, ConversationTurn, NormalizedConversationSession,
};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use sha2::{Digest, Sha256};
use sqlx::{sqlite::SqliteRow, AssertSqlSafe, Row as SqlxRow, Sqlite, SqlitePool, Transaction};
use std::collections::{BTreeMap, BTreeSet};

use super::codec::{db_error, decode_enum, decode_json, encode_enum, encode_json, to_sql_error};

pub(super) const CONVERSATION_IMPORT_BATCH_SIZE: usize = 8;

const LIST_CONVERSATION_ADAPTERS_SQL: &str = r#"
    SELECT id, name, kind, version, enabled, manifest_path, executable_path,
           content_hash, trusted_hash, trust_state, protocol_version,
           capabilities, input_kinds, created_at, updated_at
    FROM conversation_adapters
    ORDER BY kind ASC, name ASC
    "#;

const LOAD_CONVERSATION_ADAPTER_SQL: &str = r#"
    SELECT id, name, kind, version, enabled, manifest_path, executable_path,
           content_hash, trusted_hash, trust_state, protocol_version,
           capabilities, input_kinds, created_at, updated_at
    FROM conversation_adapters
    WHERE id = ?1
    "#;

const UPSERT_CONVERSATION_ADAPTER_SQL: &str = r#"
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
    "#;

const DELETE_CONVERSATION_ADAPTER_SQL: &str = "DELETE FROM conversation_adapters WHERE id = ?1";

const DISABLE_CONVERSATION_SOURCES_BY_ADAPTER_SQL: &str =
    "UPDATE conversation_sources SET enabled = 0, updated_at = ?1 WHERE adapter_id = ?2";

const LIST_CONVERSATION_SOURCES_SQL: &str = r#"
    SELECT id, adapter_id, name, kind, location, config_json, enabled,
           last_synced_at, last_sync_status, created_at, updated_at
    FROM conversation_sources
    ORDER BY adapter_id ASC, name ASC
    "#;

const LOAD_CONVERSATION_SOURCE_SQL: &str = r#"
    SELECT id, adapter_id, name, kind, location, config_json, enabled,
           last_synced_at, last_sync_status, created_at, updated_at
    FROM conversation_sources
    WHERE id = ?1
    "#;

const UPSERT_CONVERSATION_SOURCE_SQL: &str = r#"
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
    "#;

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ConversationImportResult {
    pub(crate) source_id: String,
    pub(crate) adapter_id: String,
    pub(crate) dry_run: bool,
    pub(crate) session_count: usize,
    pub(crate) skipped_session_count: usize,
    pub(crate) turn_count: usize,
    pub(crate) warning_count: usize,
    pub(crate) warnings: Vec<String>,
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

pub(crate) async fn list_conversation_adapters_sqlx(
    pool: &SqlitePool,
) -> AppResult<Vec<ConversationAdapter>> {
    let rows = sqlx::query(LIST_CONVERSATION_ADAPTERS_SQL)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    rows.iter().map(map_sqlx_conversation_adapter).collect()
}

pub(crate) fn upsert_conversation_adapter(
    conn: &Connection,
    adapter: &ConversationAdapter,
) -> AppResult<()> {
    conn.execute(
        UPSERT_CONVERSATION_ADAPTER_SQL,
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

pub(crate) async fn upsert_conversation_adapter_sqlx(
    pool: &SqlitePool,
    adapter: &ConversationAdapter,
) -> AppResult<()> {
    sqlx::query(UPSERT_CONVERSATION_ADAPTER_SQL)
        .bind(&adapter.id)
        .bind(&adapter.name)
        .bind(encode_enum(adapter.kind)?)
        .bind(&adapter.version)
        .bind(if adapter.enabled { 1 } else { 0 })
        .bind(&adapter.manifest_path)
        .bind(&adapter.executable_path)
        .bind(&adapter.content_hash)
        .bind(&adapter.trusted_hash)
        .bind(encode_enum(adapter.trust_state)?)
        .bind(adapter.protocol_version.map(i64::from))
        .bind(encode_json(&adapter.capabilities)?)
        .bind(encode_json(&adapter.input_kinds)?)
        .bind(&adapter.created_at)
        .bind(&adapter.updated_at)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) async fn delete_conversation_adapter_sqlx(
    pool: &SqlitePool,
    adapter_id: &str,
) -> AppResult<ConversationAdapter> {
    let adapter = load_conversation_adapter_sqlx(pool, adapter_id)
        .await?
        .ok_or_else(|| format!("conversation adapter not found: {adapter_id}"))?;
    if adapter.kind != ConversationAdapterKind::External {
        return Err("built-in conversation adapters cannot be unregistered".to_string());
    }
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query(DELETE_CONVERSATION_ADAPTER_SQL)
        .bind(adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(DISABLE_CONVERSATION_SOURCES_BY_ADAPTER_SQL)
        .bind(Utc::now().to_rfc3339())
        .bind(adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(adapter)
}

pub(crate) async fn load_conversation_adapter_sqlx(
    pool: &SqlitePool,
    adapter_id: &str,
) -> AppResult<Option<ConversationAdapter>> {
    sqlx::query(LOAD_CONVERSATION_ADAPTER_SQL)
        .bind(adapter_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(map_sqlx_conversation_adapter)
        .transpose()
}

pub(crate) async fn list_conversation_sources_sqlx(
    pool: &SqlitePool,
) -> AppResult<Vec<ConversationSource>> {
    let rows = sqlx::query(LIST_CONVERSATION_SOURCES_SQL)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    rows.iter().map(map_sqlx_conversation_source).collect()
}

pub(crate) fn load_conversation_source(
    conn: &Connection,
    source_id: &str,
) -> AppResult<Option<ConversationSource>> {
    conn.query_row(
        LOAD_CONVERSATION_SOURCE_SQL,
        params![source_id],
        map_conversation_source,
    )
    .optional()
    .map_err(db_error)
}

pub(crate) async fn load_conversation_source_sqlx(
    pool: &SqlitePool,
    source_id: &str,
) -> AppResult<Option<ConversationSource>> {
    sqlx::query(LOAD_CONVERSATION_SOURCE_SQL)
        .bind(source_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(map_sqlx_conversation_source)
        .transpose()
}

pub(crate) fn upsert_conversation_source(
    conn: &Connection,
    source: &ConversationSource,
) -> AppResult<()> {
    conn.execute(
        UPSERT_CONVERSATION_SOURCE_SQL,
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

pub(crate) async fn upsert_conversation_source_sqlx(
    pool: &SqlitePool,
    source: &ConversationSource,
) -> AppResult<()> {
    sqlx::query(UPSERT_CONVERSATION_SOURCE_SQL)
        .bind(&source.id)
        .bind(&source.adapter_id)
        .bind(&source.name)
        .bind(encode_enum(source.kind)?)
        .bind(&source.location)
        .bind(&source.config_json)
        .bind(if source.enabled { 1 } else { 0 })
        .bind(&source.last_synced_at)
        .bind(&source.last_sync_status)
        .bind(&source.created_at)
        .bind(&source.updated_at)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) async fn disable_conversation_source_sqlx(
    pool: &SqlitePool,
    source_id: &str,
) -> AppResult<ConversationSource> {
    let mut source = load_conversation_source_sqlx(pool, source_id)
        .await?
        .ok_or_else(|| format!("conversation source not found: {source_id}"))?;
    source.enabled = false;
    source.updated_at = Utc::now().to_rfc3339();
    upsert_conversation_source_sqlx(pool, &source).await?;
    Ok(source)
}

#[cfg(test)]
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
            skipped_session_count: 0,
            turn_count,
            warning_count: 0,
            warnings: Vec::new(),
        });
    }

    let now = Utc::now().to_rfc3339();
    let mut warning_count = 0usize;
    let mut skipped_session_count = 0usize;
    let warnings = Vec::new();

    for batch in sessions.chunks(CONVERSATION_IMPORT_BATCH_SIZE) {
        let tx = conn
            .unchecked_transaction()
            .map_err(|error| error.to_string())?;
        for normalized in batch {
            let session = conversation_session_from_normalized(source, normalized, &now);
            if conversation_session_is_unchanged_tx(&tx, &session)? {
                skipped_session_count += 1;
                continue;
            }
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
        tx.commit().map_err(|error| error.to_string())?;
    }

    let tx = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
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
        skipped_session_count,
        turn_count,
        warning_count,
        warnings,
    })
}

pub(crate) async fn import_conversation_sessions_sqlx(
    pool: &SqlitePool,
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
            skipped_session_count: 0,
            turn_count,
            warning_count: 0,
            warnings: Vec::new(),
        });
    }

    let now = Utc::now().to_rfc3339();
    let mut warning_count = 0usize;
    let mut skipped_session_count = 0usize;
    let warnings = Vec::new();

    for batch in sessions.chunks(CONVERSATION_IMPORT_BATCH_SIZE) {
        let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
        for normalized in batch {
            let session = conversation_session_from_normalized(source, normalized, &now);
            if conversation_session_is_unchanged_sqlx_tx(&mut tx, &session).await? {
                skipped_session_count += 1;
                continue;
            }
            upsert_conversation_session_sqlx_tx(&mut tx, &session).await?;
            for turn in &normalized.turns {
                if turn.user_text.trim().is_empty() {
                    warning_count += 1;
                    continue;
                }
                let stored_turn = conversation_turn_from_normalized(&session.id, turn, &now);
                upsert_conversation_turn_sqlx_tx(&mut tx, &stored_turn).await?;
                replace_conversation_parts_sqlx_tx(&mut tx, &stored_turn.id, &turn.parts).await?;
            }
            ensure_question_groups_for_session_sqlx_tx(&mut tx, &session.id, &now).await?;
            rebuild_session_question_aggregates_sqlx_tx(&mut tx, &session.id, &now).await?;
        }
        tx.commit().await.map_err(|error| error.to_string())?;
    }

    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query(
        r#"
        UPDATE conversation_sources
        SET last_synced_at = ?1, last_sync_status = 'completed', updated_at = ?1
        WHERE id = ?2
        "#,
    )
    .bind(&now)
    .bind(&source.id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    insert_sync_run_sqlx_tx(
        &mut tx,
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
    )
    .await?;
    tx.commit().await.map_err(|error| error.to_string())?;

    Ok(ConversationImportResult {
        source_id: source.id.clone(),
        adapter_id: source.adapter_id.clone(),
        dry_run: false,
        session_count: sessions.len(),
        skipped_session_count,
        turn_count,
        warning_count,
        warnings,
    })
}

pub(crate) async fn list_conversation_sessions_sqlx(
    pool: &SqlitePool,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    query: Option<&str>,
    limit: usize,
    offset: usize,
) -> AppResult<Vec<ConversationSessionListItem>> {
    let needle = normalize_query(query);
    let rows = sqlx::query(
        r#"
        SELECT s.id, s.source_id, s.adapter_id, s.external_id, s.title, s.project_path,
               s.started_at, s.updated_at, s.source_locator, s.source_fingerprint,
               s.missing, s.created_at, s.imported_at,
               (
                   SELECT COUNT(*)
                   FROM conversation_questions q
                   WHERE q.session_id = s.id
               ) AS question_count,
               (
                   SELECT COUNT(*)
                   FROM conversation_turns t
                   WHERE t.session_id = s.id
               ) AS turn_count
        FROM conversation_sessions s
        WHERE (?1 IS NULL OR s.adapter_id = ?1)
          AND (?2 IS NULL OR s.source_id = ?2)
          AND (
              ?3 IS NULL
              OR instr(lower(s.title), ?3) > 0
              OR instr(lower(COALESCE(s.project_path, '')), ?3) > 0
              OR instr(lower(s.external_id), ?3) > 0
              OR EXISTS (
                  SELECT 1
                  FROM conversation_questions q
                  WHERE q.session_id = s.id
                    AND (
                        instr(lower(q.question_text), ?3) > 0
                        OR instr(lower(q.answer_text), ?3) > 0
                        OR instr(lower(q.code_text), ?3) > 0
                        OR instr(lower(q.command_text), ?3) > 0
                    )
              )
          )
        ORDER BY COALESCE(s.updated_at, s.imported_at) DESC, s.title ASC
        LIMIT ?4 OFFSET ?5
        "#,
    )
    .bind(adapter_id)
    .bind(source_id)
    .bind(needle.as_deref())
    .bind(i64::try_from(limit).map_err(|_| format!("invalid conversation limit: {limit}"))?)
    .bind(i64::try_from(offset).map_err(|_| format!("invalid conversation offset: {offset}"))?)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;

    rows.iter()
        .map(|row| {
            let question_count = usize::try_from(
                row.try_get::<i64, _>(13)
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|_| "invalid conversation question count".to_string())?;
            let turn_count = usize::try_from(
                row.try_get::<i64, _>(14)
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|_| "invalid conversation turn count".to_string())?;
            Ok(ConversationSessionListItem {
                session: map_sqlx_conversation_session(row)?,
                question_count,
                turn_count,
            })
        })
        .collect()
}

pub(crate) async fn load_conversation_session_detail_sqlx(
    pool: &SqlitePool,
    session_id: &str,
) -> AppResult<ConversationSessionDetail> {
    let session_row = sqlx::query(
        r#"
        SELECT id, source_id, adapter_id, external_id, title, project_path,
               started_at, updated_at, source_locator, source_fingerprint,
               missing, created_at, imported_at
        FROM conversation_sessions
        WHERE id = ?1
        "#,
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| format!("conversation session not found: {session_id}"))?;
    let session = map_sqlx_conversation_session(&session_row)?;
    let questions = load_conversation_question_details_for_session_sqlx(pool, session_id).await?;
    Ok(ConversationSessionDetail { session, questions })
}

pub(crate) async fn list_conversation_question_details_sqlx(
    pool: &SqlitePool,
    session_id: &str,
    query: Option<&str>,
    limit: usize,
    offset: usize,
) -> AppResult<Vec<ConversationQuestionDetail>> {
    let needle = normalize_query(query);
    let details = load_conversation_question_details_for_session_sqlx(pool, session_id).await?;
    Ok(details
        .into_iter()
        .filter(|detail| {
            needle.as_ref().is_none_or(|needle| {
                let question = &detail.question;
                format!(
                    "{}\n{}\n{}\n{}",
                    question.question_text,
                    question.answer_text,
                    question.code_text,
                    question.command_text
                )
                .to_lowercase()
                .contains(needle)
            })
        })
        .skip(offset)
        .take(limit)
        .collect())
}

pub(crate) async fn load_conversation_question_detail_sqlx(
    pool: &SqlitePool,
    question_id: &str,
) -> AppResult<ConversationQuestionDetail> {
    let question_row = sqlx::query(
        r#"
        SELECT id, session_id, question_index, title, question_text, answer_text,
               code_text, command_text, grouping_origin, created_at, updated_at
        FROM conversation_questions
        WHERE id = ?1
        "#,
    )
    .bind(question_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| format!("conversation question not found: {question_id}"))?;
    let question = map_sqlx_conversation_question(&question_row)?;

    let turn_rows = sqlx::query(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at
        FROM conversation_question_turns qt
        JOIN conversation_turns t ON t.id = qt.turn_id
        WHERE qt.question_id = ?1
        ORDER BY qt.turn_order ASC, t.turn_index ASC
        "#,
    )
    .bind(question_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let turns = turn_rows
        .iter()
        .map(map_sqlx_conversation_turn)
        .collect::<AppResult<Vec<_>>>()?;

    let part_rows = sqlx::query(
        r#"
        SELECT p.id, p.turn_id, p.part_index, p.role, p.kind, p.text, p.language,
               p.command, p.cwd, p.status, p.exit_code, p.metadata_json
        FROM conversation_parts p
        JOIN conversation_question_turns qt ON qt.turn_id = p.turn_id
        WHERE qt.question_id = ?1
        ORDER BY qt.turn_order ASC, p.part_index ASC
        "#,
    )
    .bind(question_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let parts = part_rows
        .iter()
        .map(map_sqlx_conversation_part)
        .collect::<AppResult<Vec<_>>>()?;
    Ok(ConversationQuestionDetail {
        question,
        turns,
        parts,
    })
}

pub(crate) async fn merge_conversation_questions_sqlx(
    pool: &SqlitePool,
    question_ids: &[String],
    dry_run: bool,
) -> AppResult<ConversationMutationResult> {
    if question_ids.len() < 2 {
        return Err("at least two question ids are required".to_string());
    }

    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    let mut questions = Vec::with_capacity(question_ids.len());
    for question_id in question_ids {
        questions.push(
            load_conversation_question_sqlx_tx(&mut tx, question_id)
                .await?
                .ok_or_else(|| format!("conversation question not found: {question_id}"))?,
        );
    }
    let session_id = questions[0].session_id.clone();
    if questions
        .iter()
        .any(|question| question.session_id != session_id)
    {
        return Err("questions must belong to the same session".to_string());
    }
    ensure_question_ids_are_adjacent_sqlx_tx(&mut tx, &session_id, question_ids).await?;

    if dry_run {
        tx.rollback().await.map_err(|error| error.to_string())?;
        let mut details = Vec::with_capacity(question_ids.len());
        for question_id in question_ids {
            details.push(load_conversation_question_detail_sqlx(pool, question_id).await?);
        }
        return Ok(ConversationMutationResult {
            dry_run: true,
            session_id,
            affected_question_ids: question_ids.to_vec(),
            questions: details,
        });
    }

    let now = Utc::now().to_rfc3339();
    let survivor_id = question_ids[0].clone();
    for question_id in &question_ids[1..] {
        let next_order = max_question_turn_order_sqlx_tx(&mut tx, &survivor_id).await? + 1;
        let turn_ids = load_question_turn_ids_sqlx_tx(&mut tx, question_id).await?;
        for (offset, turn_id) in turn_ids.iter().enumerate() {
            sqlx::query(
                r#"
                UPDATE conversation_question_turns
                SET question_id = ?1, turn_order = ?2
                WHERE question_id = ?3 AND turn_id = ?4
                "#,
            )
            .bind(&survivor_id)
            .bind(next_order + offset as i64)
            .bind(question_id)
            .bind(turn_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
        }
        sqlx::query("DELETE FROM conversation_questions WHERE id = ?1")
            .bind(question_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
        sqlx::query("DELETE FROM conversation_question_fts WHERE question_id = ?1")
            .bind(question_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
    }
    sqlx::query(
        "UPDATE conversation_questions SET grouping_origin = ?1, updated_at = ?2 WHERE id = ?3",
    )
    .bind(encode_enum(ConversationGroupingOrigin::Manual)?)
    .bind(&now)
    .bind(&survivor_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    renumber_questions_for_session_sqlx_tx(&mut tx, &session_id).await?;
    rebuild_session_question_aggregates_sqlx_tx(&mut tx, &session_id, &now).await?;
    tx.commit().await.map_err(|error| error.to_string())?;

    Ok(ConversationMutationResult {
        dry_run: false,
        session_id,
        affected_question_ids: question_ids.to_vec(),
        questions: vec![load_conversation_question_detail_sqlx(pool, &survivor_id).await?],
    })
}

pub(crate) async fn split_conversation_question_sqlx(
    pool: &SqlitePool,
    question_id: &str,
    before_turn_id: &str,
    dry_run: bool,
) -> AppResult<ConversationMutationResult> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    let question = load_conversation_question_sqlx_tx(&mut tx, question_id)
        .await?
        .ok_or_else(|| format!("conversation question not found: {question_id}"))?;
    let turns = load_question_turns_sqlx_tx(&mut tx, question_id).await?;
    let split_index = turns
        .iter()
        .position(|turn| turn.id == before_turn_id)
        .ok_or_else(|| format!("turn is not in question: {before_turn_id}"))?;
    if split_index == 0 {
        return Err("split turn must not be the first turn in the question".to_string());
    }

    if dry_run {
        tx.rollback().await.map_err(|error| error.to_string())?;
        return Ok(ConversationMutationResult {
            dry_run: true,
            session_id: question.session_id,
            affected_question_ids: vec![question_id.to_string()],
            questions: vec![load_conversation_question_detail_sqlx(pool, question_id).await?],
        });
    }

    let now = Utc::now().to_rfc3339();
    let new_question_id = stable_id(
        "conversation-question",
        &[question_id, before_turn_id, &now],
    );
    sqlx::query(
        r#"
        INSERT INTO conversation_questions (
            id, session_id, question_index, title, question_text, answer_text,
            code_text, command_text, grouping_origin, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, NULL, '', '', '', '', ?4, ?5, ?5)
        "#,
    )
    .bind(&new_question_id)
    .bind(&question.session_id)
    .bind(question.question_index + 1)
    .bind(encode_enum(ConversationGroupingOrigin::Manual)?)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    for (order, turn) in turns.iter().skip(split_index).enumerate() {
        sqlx::query(
            r#"
            UPDATE conversation_question_turns
            SET question_id = ?1, turn_order = ?2
            WHERE question_id = ?3 AND turn_id = ?4
            "#,
        )
        .bind(&new_question_id)
        .bind(order as i64)
        .bind(question_id)
        .bind(&turn.id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    }
    sqlx::query(
        "UPDATE conversation_questions SET grouping_origin = ?1, updated_at = ?2 WHERE id = ?3",
    )
    .bind(encode_enum(ConversationGroupingOrigin::Manual)?)
    .bind(&now)
    .bind(question_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    renumber_question_turns_sqlx_tx(&mut tx, question_id).await?;
    renumber_questions_for_session_sqlx_tx(&mut tx, &question.session_id).await?;
    rebuild_session_question_aggregates_sqlx_tx(&mut tx, &question.session_id, &now).await?;
    tx.commit().await.map_err(|error| error.to_string())?;

    Ok(ConversationMutationResult {
        dry_run: false,
        session_id: question.session_id,
        affected_question_ids: vec![question_id.to_string(), new_question_id.clone()],
        questions: vec![
            load_conversation_question_detail_sqlx(pool, question_id).await?,
            load_conversation_question_detail_sqlx(pool, &new_question_id).await?,
        ],
    })
}

pub(crate) fn render_conversation_detail_markdown_with_filter(
    detail: &ConversationSessionDetail,
    question_ids: &[String],
    content_filter: &ConversationExportContentFilter,
) -> AppResult<String> {
    let selection = (!question_ids.is_empty()).then_some(question_ids);
    render_session_detail_markdown(detail, selection, content_filter)
}

async fn load_conversation_question_details_for_session_sqlx(
    pool: &SqlitePool,
    session_id: &str,
) -> AppResult<Vec<ConversationQuestionDetail>> {
    let question_rows = sqlx::query(
        r#"
        SELECT id, session_id, question_index, title, question_text, answer_text,
               code_text, command_text, grouping_origin, created_at, updated_at
        FROM conversation_questions
        WHERE session_id = ?1
        ORDER BY question_index ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let questions = question_rows
        .iter()
        .map(map_sqlx_conversation_question)
        .collect::<AppResult<Vec<_>>>()?;

    let turn_rows = sqlx::query(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at,
               qt.question_id
        FROM conversation_question_turns qt
        JOIN conversation_turns t ON t.id = qt.turn_id
        JOIN conversation_questions q ON q.id = qt.question_id
        WHERE q.session_id = ?1
        ORDER BY q.question_index ASC, qt.turn_order ASC, t.turn_index ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let mut turns_by_question = BTreeMap::<String, Vec<ConversationTurn>>::new();
    for row in &turn_rows {
        let question_id = row
            .try_get::<String, _>(11)
            .map_err(|error| error.to_string())?;
        turns_by_question
            .entry(question_id)
            .or_default()
            .push(map_sqlx_conversation_turn(row)?);
    }

    let part_rows = sqlx::query(
        r#"
        SELECT p.id, p.turn_id, p.part_index, p.role, p.kind, p.text, p.language,
               p.command, p.cwd, p.status, p.exit_code, p.metadata_json
        FROM conversation_parts p
        JOIN conversation_turns t ON t.id = p.turn_id
        WHERE t.session_id = ?1
        ORDER BY t.turn_index ASC, p.part_index ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let mut parts_by_turn = BTreeMap::<String, Vec<ConversationPart>>::new();
    for row in &part_rows {
        let part = map_sqlx_conversation_part(row)?;
        parts_by_turn
            .entry(part.turn_id.clone())
            .or_default()
            .push(part);
    }

    Ok(questions
        .into_iter()
        .map(|question| {
            let turns = turns_by_question.remove(&question.id).unwrap_or_default();
            let mut parts = Vec::new();
            for turn in &turns {
                parts.extend(parts_by_turn.remove(&turn.id).unwrap_or_default());
            }
            ConversationQuestionDetail {
                question,
                turns,
                parts,
            }
        })
        .collect())
}

#[cfg(test)]
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

#[cfg(test)]
pub(crate) fn load_conversation_session_detail(
    conn: &Connection,
    session_id: &str,
) -> AppResult<ConversationSessionDetail> {
    let session = load_conversation_session(conn, session_id)?
        .ok_or_else(|| format!("conversation session not found: {session_id}"))?;
    let questions = list_conversation_question_details(conn, session_id, None, usize::MAX, 0)?;
    Ok(ConversationSessionDetail { session, questions })
}

#[cfg(test)]
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

#[allow(clippy::too_many_arguments)]
pub(crate) async fn search_conversation_cards_sqlx(
    pool: &SqlitePool,
    record_kind: ConversationRecordKind,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    project_path: Option<&str>,
    query: &str,
    content_types: &[ConversationSearchCardType],
    since: Option<&str>,
    until: Option<&str>,
    timeline: bool,
    limit: usize,
    offset: usize,
) -> AppResult<ConversationSearchPage> {
    let needle = normalize_query(Some(query))
        .ok_or_else(|| "conversation search query is required".to_string())?;
    let project_path = normalize_project_path(project_path);
    let since = parse_search_time_bound(since, SearchTimeBound::Since)?;
    let until = parse_search_time_bound(until, SearchTimeBound::Until)?;
    let allowed_types = content_types.iter().copied().collect::<BTreeSet<_>>();
    let tables = record_kind.tables();
    let mut sessions = load_search_sessions_sqlx(pool, tables, adapter_id, source_id).await?;
    if timeline {
        sessions.sort_by(|left, right| {
            conversation_session_search_time(&left.session)
                .cmp(&conversation_session_search_time(&right.session))
                .then_with(|| left.session.title.cmp(&right.session.title))
        });
    }
    let mut questions_by_session =
        load_search_questions_sqlx(pool, tables, adapter_id, source_id).await?;
    let mut turns_by_question = load_search_turns_sqlx(pool, tables, adapter_id, source_id).await?;
    let mut parts_by_turn = load_search_parts_sqlx(pool, tables, adapter_id, source_id).await?;
    let mut hits = Vec::new();

    for session_item in sessions {
        let session = &session_item.session;
        if let Some(project_path) = project_path.as_deref() {
            let session_project = normalize_project_path(session.project_path.as_deref());
            if session_project.as_deref() != Some(project_path) {
                continue;
            }
        }
        if since.is_some() || until.is_some() {
            let Some(session_time) = conversation_session_search_time(session) else {
                continue;
            };
            if let Some(since) = since.as_ref() {
                if &session_time < since {
                    continue;
                }
            }
            if let Some(until) = until.as_ref() {
                if &session_time > until {
                    continue;
                }
            }
        }

        for question in questions_by_session.remove(&session.id).unwrap_or_default() {
            let question_title = question
                .title
                .clone()
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| first_line(&question.question_text));
            for turn in turns_by_question.remove(&question.id).unwrap_or_default() {
                push_search_hit_if_matching(
                    &mut hits,
                    &needle,
                    &allowed_types,
                    &session_item,
                    &question,
                    &question_title,
                    Some(turn.id.clone()),
                    None,
                    format!("{}-question", turn.id),
                    ConversationSearchCardType::Question,
                    &turn.user_text,
                );

                for part in parts_by_turn.remove(&turn.id).unwrap_or_default() {
                    for entry in search_entries_for_part(&part) {
                        push_search_hit_if_matching(
                            &mut hits,
                            &needle,
                            &allowed_types,
                            &session_item,
                            &question,
                            &question_title,
                            Some(turn.id.clone()),
                            Some(part.id.clone()),
                            entry.block_id,
                            entry.card_type,
                            &entry.text,
                        );
                    }
                }
            }
        }
    }

    let total_count = hits.len();
    Ok(ConversationSearchPage {
        total_count,
        hits: hits.into_iter().skip(offset).take(limit).collect(),
    })
}

#[cfg(test)]
pub(crate) fn search_conversation_cards(
    conn: &Connection,
    record_kind: ConversationRecordKind,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    project_path: Option<&str>,
    query: &str,
    content_types: &[ConversationSearchCardType],
    since: Option<&str>,
    until: Option<&str>,
    timeline: bool,
    limit: usize,
    offset: usize,
) -> AppResult<ConversationSearchPage> {
    let needle = normalize_query(Some(query))
        .ok_or_else(|| "conversation search query is required".to_string())?;
    let project_path = normalize_project_path(project_path);
    let since = parse_search_time_bound(since, SearchTimeBound::Since)?;
    let until = parse_search_time_bound(until, SearchTimeBound::Until)?;
    let allowed_types = content_types.iter().copied().collect::<BTreeSet<_>>();
    let tables = record_kind.tables();
    let mut sessions = load_record_sessions(conn, tables)?;
    if timeline {
        sessions.sort_by(|left, right| {
            conversation_session_search_time(left)
                .cmp(&conversation_session_search_time(right))
                .then_with(|| left.title.cmp(&right.title))
        });
    }
    let mut hits = Vec::new();

    for session in sessions {
        if adapter_id.is_some_and(|value| value != session.adapter_id) {
            continue;
        }
        if source_id.is_some_and(|value| value != session.source_id) {
            continue;
        }
        if let Some(project_path) = project_path.as_deref() {
            let session_project = normalize_project_path(session.project_path.as_deref());
            if session_project.as_deref() != Some(project_path) {
                continue;
            }
        }
        if since.is_some() || until.is_some() {
            let Some(session_time) = conversation_session_search_time(&session) else {
                continue;
            };
            if let Some(since) = since.as_ref() {
                if &session_time < since {
                    continue;
                }
            }
            if let Some(until) = until.as_ref() {
                if &session_time > until {
                    continue;
                }
            }
        }

        let session_item = ConversationSessionListItem {
            question_count: count_record_questions(conn, tables, &session.id)?,
            turn_count: count_record_turns(conn, tables, &session.id)?,
            session: session.clone(),
        };
        for question in list_record_questions(conn, tables, &session.id)? {
            let question_title = question
                .title
                .clone()
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| first_line(&question.question_text));
            for turn in load_record_question_turns(conn, tables, &question.id)? {
                push_search_hit_if_matching(
                    &mut hits,
                    &needle,
                    &allowed_types,
                    &session_item,
                    &question,
                    &question_title,
                    Some(turn.id.clone()),
                    None,
                    format!("{}-question", turn.id),
                    ConversationSearchCardType::Question,
                    &turn.user_text,
                );

                for part in load_record_turn_parts(conn, tables, &turn.id)? {
                    for entry in search_entries_for_part(&part) {
                        push_search_hit_if_matching(
                            &mut hits,
                            &needle,
                            &allowed_types,
                            &session_item,
                            &question,
                            &question_title,
                            Some(turn.id.clone()),
                            Some(part.id.clone()),
                            entry.block_id,
                            entry.card_type,
                            &entry.text,
                        );
                    }
                }
            }
        }
    }

    let total_count = hits.len();
    Ok(ConversationSearchPage {
        total_count,
        hits: hits.into_iter().skip(offset).take(limit).collect(),
    })
}

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

fn map_sqlx_conversation_adapter(row: &SqliteRow) -> AppResult<ConversationAdapter> {
    let protocol_version = row
        .try_get::<Option<i64>, _>(10)
        .map_err(|error| error.to_string())?
        .map(|value| u32::try_from(value).map_err(|_| format!("invalid protocol_version: {value}")))
        .transpose()?;
    Ok(ConversationAdapter {
        id: row.try_get(0).map_err(|error| error.to_string())?,
        name: row.try_get(1).map_err(|error| error.to_string())?,
        kind: decode_enum(
            row.try_get::<String, _>(2)
                .map_err(|error| error.to_string())?,
        )?,
        version: row.try_get(3).map_err(|error| error.to_string())?,
        enabled: row
            .try_get::<i64, _>(4)
            .map_err(|error| error.to_string())?
            == 1,
        manifest_path: row.try_get(5).map_err(|error| error.to_string())?,
        executable_path: row.try_get(6).map_err(|error| error.to_string())?,
        content_hash: row.try_get(7).map_err(|error| error.to_string())?,
        trusted_hash: row.try_get(8).map_err(|error| error.to_string())?,
        trust_state: decode_enum(
            row.try_get::<String, _>(9)
                .map_err(|error| error.to_string())?,
        )?,
        protocol_version,
        capabilities: decode_json(
            row.try_get::<String, _>(11)
                .map_err(|error| error.to_string())?,
        )?,
        input_kinds: decode_json(
            row.try_get::<String, _>(12)
                .map_err(|error| error.to_string())?,
        )?,
        created_at: row.try_get(13).map_err(|error| error.to_string())?,
        updated_at: row.try_get(14).map_err(|error| error.to_string())?,
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

fn map_sqlx_conversation_source(row: &SqliteRow) -> AppResult<ConversationSource> {
    Ok(ConversationSource {
        id: row.try_get(0).map_err(|error| error.to_string())?,
        adapter_id: row.try_get(1).map_err(|error| error.to_string())?,
        name: row.try_get(2).map_err(|error| error.to_string())?,
        kind: decode_enum(
            row.try_get::<String, _>(3)
                .map_err(|error| error.to_string())?,
        )?,
        location: row.try_get(4).map_err(|error| error.to_string())?,
        config_json: row.try_get(5).map_err(|error| error.to_string())?,
        enabled: row
            .try_get::<i64, _>(6)
            .map_err(|error| error.to_string())?
            == 1,
        last_synced_at: row.try_get(7).map_err(|error| error.to_string())?,
        last_sync_status: row.try_get(8).map_err(|error| error.to_string())?,
        created_at: row.try_get(9).map_err(|error| error.to_string())?,
        updated_at: row.try_get(10).map_err(|error| error.to_string())?,
    })
}

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

pub(super) fn map_sqlx_conversation_session(row: &SqliteRow) -> AppResult<ConversationSession> {
    Ok(ConversationSession {
        id: row.try_get(0).map_err(|error| error.to_string())?,
        source_id: row.try_get(1).map_err(|error| error.to_string())?,
        adapter_id: row.try_get(2).map_err(|error| error.to_string())?,
        external_id: row.try_get(3).map_err(|error| error.to_string())?,
        title: row.try_get(4).map_err(|error| error.to_string())?,
        project_path: row.try_get(5).map_err(|error| error.to_string())?,
        started_at: row.try_get(6).map_err(|error| error.to_string())?,
        updated_at: row.try_get(7).map_err(|error| error.to_string())?,
        source_locator: row.try_get(8).map_err(|error| error.to_string())?,
        source_fingerprint: row.try_get(9).map_err(|error| error.to_string())?,
        missing: row
            .try_get::<i64, _>(10)
            .map_err(|error| error.to_string())?
            == 1,
        created_at: row.try_get(11).map_err(|error| error.to_string())?,
        imported_at: row.try_get(12).map_err(|error| error.to_string())?,
    })
}

pub(super) fn map_sqlx_conversation_turn(row: &SqliteRow) -> AppResult<ConversationTurn> {
    Ok(ConversationTurn {
        id: row.try_get(0).map_err(|error| error.to_string())?,
        session_id: row.try_get(1).map_err(|error| error.to_string())?,
        external_id: row.try_get(2).map_err(|error| error.to_string())?,
        turn_index: row.try_get(3).map_err(|error| error.to_string())?,
        user_text: row.try_get(4).map_err(|error| error.to_string())?,
        title: row.try_get(5).map_err(|error| error.to_string())?,
        started_at: row.try_get(6).map_err(|error| error.to_string())?,
        ended_at: row.try_get(7).map_err(|error| error.to_string())?,
        fingerprint: row.try_get(8).map_err(|error| error.to_string())?,
        missing: row
            .try_get::<i64, _>(9)
            .map_err(|error| error.to_string())?
            == 1,
        imported_at: row.try_get(10).map_err(|error| error.to_string())?,
    })
}

pub(super) fn map_sqlx_conversation_part(row: &SqliteRow) -> AppResult<ConversationPart> {
    Ok(ConversationPart {
        id: row.try_get(0).map_err(|error| error.to_string())?,
        turn_id: row.try_get(1).map_err(|error| error.to_string())?,
        part_index: row.try_get(2).map_err(|error| error.to_string())?,
        role: decode_enum(
            row.try_get::<String, _>(3)
                .map_err(|error| error.to_string())?,
        )?,
        kind: decode_enum(
            row.try_get::<String, _>(4)
                .map_err(|error| error.to_string())?,
        )?,
        text: row.try_get(5).map_err(|error| error.to_string())?,
        language: row.try_get(6).map_err(|error| error.to_string())?,
        command: row.try_get(7).map_err(|error| error.to_string())?,
        cwd: row.try_get(8).map_err(|error| error.to_string())?,
        status: row.try_get(9).map_err(|error| error.to_string())?,
        exit_code: row.try_get(10).map_err(|error| error.to_string())?,
        metadata_json: row.try_get(11).map_err(|error| error.to_string())?,
    })
}

pub(super) fn map_sqlx_conversation_question(row: &SqliteRow) -> AppResult<ConversationQuestion> {
    Ok(ConversationQuestion {
        id: row.try_get(0).map_err(|error| error.to_string())?,
        session_id: row.try_get(1).map_err(|error| error.to_string())?,
        question_index: row.try_get(2).map_err(|error| error.to_string())?,
        title: row.try_get(3).map_err(|error| error.to_string())?,
        question_text: row.try_get(4).map_err(|error| error.to_string())?,
        answer_text: row.try_get(5).map_err(|error| error.to_string())?,
        code_text: row.try_get(6).map_err(|error| error.to_string())?,
        command_text: row.try_get(7).map_err(|error| error.to_string())?,
        grouping_origin: decode_enum(
            row.try_get::<String, _>(8)
                .map_err(|error| error.to_string())?,
        )?,
        created_at: row.try_get(9).map_err(|error| error.to_string())?,
        updated_at: row.try_get(10).map_err(|error| error.to_string())?,
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

#[cfg(test)]
fn conversation_session_is_unchanged_tx(
    tx: &rusqlite::Transaction<'_>,
    session: &ConversationSession,
) -> AppResult<bool> {
    let Some(source_fingerprint) = session.source_fingerprint.as_deref() else {
        return Ok(false);
    };
    let existing = tx
        .query_row(
            r#"
            SELECT title, project_path, started_at, updated_at, source_locator,
                   source_fingerprint, missing
            FROM conversation_sessions
            WHERE source_id = ?1 AND external_id = ?2
            "#,
            params![session.source_id, session.external_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        )
        .optional()
        .map_err(db_error)?;

    Ok(existing.is_some_and(
        |(
            title,
            project_path,
            started_at,
            updated_at,
            source_locator,
            existing_fingerprint,
            missing,
        )| {
            title == session.title
                && project_path == session.project_path
                && started_at == session.started_at
                && updated_at == session.updated_at
                && source_locator == session.source_locator
                && existing_fingerprint.as_deref() == Some(source_fingerprint)
                && missing == 0
        },
    ))
}

fn conversation_turn_from_normalized(
    session_id: &str,
    normalized: &crate::backend::models::NormalizedConversationTurn,
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
fn replace_conversation_parts_tx(
    tx: &rusqlite::Transaction<'_>,
    turn_id: &str,
    parts: &[crate::backend::models::NormalizedConversationPart],
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

async fn conversation_session_is_unchanged_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    session: &ConversationSession,
) -> AppResult<bool> {
    let Some(source_fingerprint) = session.source_fingerprint.as_deref() else {
        return Ok(false);
    };
    let Some(row) = sqlx::query(
        r#"
        SELECT title, project_path, started_at, updated_at, source_locator,
               source_fingerprint, missing
        FROM conversation_sessions
        WHERE source_id = ?1 AND external_id = ?2
        "#,
    )
    .bind(&session.source_id)
    .bind(&session.external_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())?
    else {
        return Ok(false);
    };

    let title: String = row.try_get(0).map_err(|error| error.to_string())?;
    let project_path: Option<String> = row.try_get(1).map_err(|error| error.to_string())?;
    let started_at: Option<String> = row.try_get(2).map_err(|error| error.to_string())?;
    let updated_at: Option<String> = row.try_get(3).map_err(|error| error.to_string())?;
    let source_locator: Option<String> = row.try_get(4).map_err(|error| error.to_string())?;
    let existing_fingerprint: Option<String> = row.try_get(5).map_err(|error| error.to_string())?;
    let missing: i64 = row.try_get(6).map_err(|error| error.to_string())?;

    Ok(title == session.title
        && project_path == session.project_path
        && started_at == session.started_at
        && updated_at == session.updated_at
        && source_locator == session.source_locator
        && existing_fingerprint.as_deref() == Some(source_fingerprint)
        && missing == 0)
}

async fn upsert_conversation_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    session: &ConversationSession,
) -> AppResult<()> {
    sqlx::query(
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
    )
    .bind(&session.id)
    .bind(&session.source_id)
    .bind(&session.adapter_id)
    .bind(&session.external_id)
    .bind(&session.title)
    .bind(&session.project_path)
    .bind(&session.started_at)
    .bind(&session.updated_at)
    .bind(&session.source_locator)
    .bind(&session.source_fingerprint)
    .bind(if session.missing { 1_i64 } else { 0_i64 })
    .bind(&session.created_at)
    .bind(&session.imported_at)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn upsert_conversation_turn_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    turn: &ConversationTurn,
) -> AppResult<()> {
    sqlx::query(
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
    )
    .bind(&turn.id)
    .bind(&turn.session_id)
    .bind(&turn.external_id)
    .bind(turn.turn_index)
    .bind(&turn.user_text)
    .bind(&turn.title)
    .bind(&turn.started_at)
    .bind(&turn.ended_at)
    .bind(&turn.fingerprint)
    .bind(if turn.missing { 1_i64 } else { 0_i64 })
    .bind(&turn.imported_at)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn replace_conversation_parts_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    turn_id: &str,
    parts: &[crate::backend::models::NormalizedConversationPart],
) -> AppResult<()> {
    sqlx::query("DELETE FROM conversation_parts WHERE turn_id = ?1")
        .bind(turn_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    for (index, part) in parts.iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO conversation_parts (
                id, turn_id, part_index, role, kind, text, language, command,
                cwd, status, exit_code, metadata_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
        )
        .bind(stable_id(
            "conversation-part",
            &[turn_id, &index.to_string()],
        ))
        .bind(turn_id)
        .bind(index as i64)
        .bind(encode_enum(part.role)?)
        .bind(encode_enum(part.kind)?)
        .bind(&part.text)
        .bind(&part.language)
        .bind(&part.command)
        .bind(&part.cwd)
        .bind(&part.status)
        .bind(part.exit_code)
        .bind(&part.metadata_json)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn ensure_question_groups_for_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    session_id: &str,
    now: &str,
) -> AppResult<()> {
    let turns = load_session_turns_sqlx_tx(tx, session_id).await?;
    let existing_memberships = load_turn_question_memberships_sqlx_tx(tx, session_id).await?;
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
        let previous_question_id =
            previous_question_id_for_turn_sqlx_tx(tx, session_id, first_turn_id).await?;
        let question_id = if group.origin == ConversationGroupingOrigin::AutoMerged {
            previous_question_id
                .unwrap_or_else(|| stable_id("conversation-question", &[session_id, first_turn_id]))
        } else {
            stable_id("conversation-question", &[session_id, first_turn_id])
        };
        if load_conversation_question_sqlx_tx(tx, &question_id)
            .await?
            .is_none()
        {
            let question_index = next_question_index_sqlx_tx(tx, session_id).await?;
            sqlx::query(
                r#"
                INSERT INTO conversation_questions (
                    id, session_id, question_index, title, question_text, answer_text,
                    code_text, command_text, grouping_origin, created_at, updated_at
                )
                VALUES (?1, ?2, ?3, NULL, '', '', '', '', ?4, ?5, ?5)
                "#,
            )
            .bind(&question_id)
            .bind(session_id)
            .bind(question_index)
            .bind(encode_enum(group.origin)?)
            .bind(now)
            .execute(&mut **tx)
            .await
            .map_err(|error| error.to_string())?;
        }
        let start_order = max_question_turn_order_sqlx_tx(tx, &question_id).await? + 1;
        for (offset, turn_id) in group.turn_ids.iter().enumerate() {
            sqlx::query(
                r#"
                INSERT OR IGNORE INTO conversation_question_turns (question_id, turn_id, turn_order)
                VALUES (?1, ?2, ?3)
                "#,
            )
            .bind(&question_id)
            .bind(turn_id)
            .bind(start_order + offset as i64)
            .execute(&mut **tx)
            .await
            .map_err(|error| error.to_string())?;
        }
    }
    renumber_questions_for_session_sqlx_tx(tx, session_id).await?;
    Ok(())
}

async fn rebuild_session_question_aggregates_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    session_id: &str,
    now: &str,
) -> AppResult<()> {
    let question_ids = question_ids_for_session_sqlx_tx(tx, session_id).await?;
    for question_id in question_ids {
        rebuild_question_aggregate_sqlx_tx(tx, &question_id, now).await?;
    }
    Ok(())
}

async fn rebuild_question_aggregate_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    question_id: &str,
    now: &str,
) -> AppResult<()> {
    let turns = load_question_turns_sqlx_tx(tx, question_id).await?;
    let mut question_text = Vec::new();
    let mut answer_text = Vec::new();
    let mut code_text = Vec::new();
    let mut command_text = Vec::new();

    for turn in &turns {
        question_text.push(turn.user_text.clone());
        for part in load_turn_parts_sqlx_tx(tx, &turn.id).await? {
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

    sqlx::query(
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
    )
    .bind(&title)
    .bind(&question_text)
    .bind(&answer_text)
    .bind(&code_text)
    .bind(&command_text)
    .bind(now)
    .bind(question_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    let session_id: String = sqlx::query_scalar::<_, String>(
        "SELECT session_id FROM conversation_questions WHERE id = ?1",
    )
    .bind(question_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM conversation_question_fts WHERE question_id = ?1")
        .bind(question_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(
        r#"
        INSERT INTO conversation_question_fts (
            question_id, session_id, question_text, answer_text, code_text, command_text
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(question_id)
    .bind(&session_id)
    .bind(&question_text)
    .bind(&answer_text)
    .bind(&code_text)
    .bind(&command_text)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn insert_sync_run_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    run: &ConversationSyncRun,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_sync_runs (
            id, source_id, adapter_id, status, started_at, finished_at,
            session_count, turn_count, warning_count, error_message
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
    )
    .bind(&run.id)
    .bind(&run.source_id)
    .bind(&run.adapter_id)
    .bind(encode_enum(run.status)?)
    .bind(&run.started_at)
    .bind(&run.finished_at)
    .bind(run.session_count)
    .bind(run.turn_count)
    .bind(run.warning_count)
    .bind(&run.error_message)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn load_session_turns_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    session_id: &str,
) -> AppResult<Vec<ConversationTurn>> {
    let rows = sqlx::query(
        r#"
        SELECT id, session_id, external_id, turn_index, user_text, title,
               started_at, ended_at, fingerprint, missing, imported_at
        FROM conversation_turns
        WHERE session_id = ?1
        ORDER BY turn_index ASC, imported_at ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    rows.iter().map(map_sqlx_conversation_turn).collect()
}

async fn load_question_turns_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    question_id: &str,
) -> AppResult<Vec<ConversationTurn>> {
    let rows = sqlx::query(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at
        FROM conversation_question_turns qt
        JOIN conversation_turns t ON t.id = qt.turn_id
        WHERE qt.question_id = ?1
        ORDER BY qt.turn_order ASC, t.turn_index ASC
        "#,
    )
    .bind(question_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    rows.iter().map(map_sqlx_conversation_turn).collect()
}

async fn load_turn_parts_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    turn_id: &str,
) -> AppResult<Vec<ConversationPart>> {
    let rows = sqlx::query(
        r#"
        SELECT id, turn_id, part_index, role, kind, text, language, command,
               cwd, status, exit_code, metadata_json
        FROM conversation_parts
        WHERE turn_id = ?1
        ORDER BY part_index ASC
        "#,
    )
    .bind(turn_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    rows.iter().map(map_sqlx_conversation_part).collect()
}

async fn load_turn_question_memberships_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    session_id: &str,
) -> AppResult<BTreeMap<String, String>> {
    let rows = sqlx::query(
        r#"
        SELECT qt.turn_id, qt.question_id
        FROM conversation_question_turns qt
        JOIN conversation_turns t ON t.id = qt.turn_id
        WHERE t.session_id = ?1
        "#,
    )
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    let mut memberships = BTreeMap::new();
    for row in rows {
        memberships.insert(
            row.try_get(0).map_err(|error| error.to_string())?,
            row.try_get(1).map_err(|error| error.to_string())?,
        );
    }
    Ok(memberships)
}

async fn previous_question_id_for_turn_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    session_id: &str,
    turn_id: &str,
) -> AppResult<Option<String>> {
    sqlx::query_scalar::<_, String>(
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
    )
    .bind(session_id)
    .bind(turn_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())
}

async fn next_question_index_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    session_id: &str,
) -> AppResult<i64> {
    let max_index: Option<i64> = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(question_index) FROM conversation_questions WHERE session_id = ?1",
    )
    .bind(session_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(max_index.unwrap_or(-1) + 1)
}

async fn max_question_turn_order_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    question_id: &str,
) -> AppResult<i64> {
    let max_order: Option<i64> = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(turn_order) FROM conversation_question_turns WHERE question_id = ?1",
    )
    .bind(question_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(max_order.unwrap_or(-1))
}

async fn load_conversation_question_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    question_id: &str,
) -> AppResult<Option<ConversationQuestion>> {
    sqlx::query(
        r#"
        SELECT id, session_id, question_index, title, question_text, answer_text,
               code_text, command_text, grouping_origin, created_at, updated_at
        FROM conversation_questions
        WHERE id = ?1
        "#,
    )
    .bind(question_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())?
    .as_ref()
    .map(map_sqlx_conversation_question)
    .transpose()
}

async fn question_ids_for_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    session_id: &str,
) -> AppResult<Vec<String>> {
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT q.id
        FROM conversation_questions q
        WHERE q.session_id = ?1
        ORDER BY q.question_index ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())
}

async fn load_question_turn_ids_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    question_id: &str,
) -> AppResult<Vec<String>> {
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT turn_id
        FROM conversation_question_turns
        WHERE question_id = ?1
        ORDER BY turn_order ASC
        "#,
    )
    .bind(question_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())
}

async fn renumber_question_turns_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    question_id: &str,
) -> AppResult<()> {
    let turn_ids = load_question_turn_ids_sqlx_tx(tx, question_id).await?;
    for (index, turn_id) in turn_ids.iter().enumerate() {
        sqlx::query(
            r#"
            UPDATE conversation_question_turns
            SET turn_order = ?1
            WHERE question_id = ?2 AND turn_id = ?3
            "#,
        )
        .bind(index as i64)
        .bind(question_id)
        .bind(turn_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn ensure_question_ids_are_adjacent_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    session_id: &str,
    question_ids: &[String],
) -> AppResult<()> {
    let ordered = question_ids_for_session_sqlx_tx(tx, session_id).await?;
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

async fn renumber_questions_for_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    session_id: &str,
) -> AppResult<()> {
    let question_ids = sqlx::query_scalar::<_, String>(
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
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;

    for (index, question_id) in question_ids.iter().enumerate() {
        sqlx::query("UPDATE conversation_questions SET question_index = ?1 WHERE id = ?2")
            .bind(1_000_000i64 + index as i64)
            .bind(question_id)
            .execute(&mut **tx)
            .await
            .map_err(|error| error.to_string())?;
    }
    for (index, question_id) in question_ids.iter().enumerate() {
        sqlx::query("UPDATE conversation_questions SET question_index = ?1 WHERE id = ?2")
            .bind(index as i64)
            .bind(question_id)
            .execute(&mut **tx)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
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

#[cfg(test)]
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

#[derive(Debug, Clone, Copy)]
struct ConversationRecordTables {
    sessions: &'static str,
    turns: &'static str,
    parts: &'static str,
    questions: &'static str,
    question_turns: &'static str,
}

impl ConversationRecordKind {
    fn tables(self) -> ConversationRecordTables {
        match self {
            ConversationRecordKind::Session => ConversationRecordTables {
                sessions: "conversation_sessions",
                turns: "conversation_turns",
                parts: "conversation_parts",
                questions: "conversation_questions",
                question_turns: "conversation_question_turns",
            },
            ConversationRecordKind::Web => ConversationRecordTables {
                sessions: "web_record_sessions",
                turns: "web_record_turns",
                parts: "web_record_parts",
                questions: "web_record_questions",
                question_turns: "web_record_question_turns",
            },
        }
    }
}

async fn load_search_sessions_sqlx(
    pool: &SqlitePool,
    tables: ConversationRecordTables,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
) -> AppResult<Vec<ConversationSessionListItem>> {
    let query = format!(
        r#"
        SELECT s.id, s.source_id, s.adapter_id, s.external_id, s.title, s.project_path,
               s.started_at, s.updated_at, s.source_locator, s.source_fingerprint,
               s.missing, s.created_at, s.imported_at,
               (
                   SELECT COUNT(*)
                   FROM {questions} q
                   WHERE q.session_id = s.id
               ) AS question_count,
               (
                   SELECT COUNT(*)
                   FROM {turns} t
                   WHERE t.session_id = s.id
               ) AS turn_count
        FROM {sessions} s
        WHERE (?1 IS NULL OR s.adapter_id = ?1)
          AND (?2 IS NULL OR s.source_id = ?2)
        ORDER BY COALESCE(s.updated_at, s.imported_at) DESC, s.title ASC
        "#,
        sessions = tables.sessions,
        questions = tables.questions,
        turns = tables.turns,
    );
    let rows = sqlx::query(AssertSqlSafe(query))
        .bind(adapter_id)
        .bind(source_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    rows.iter()
        .map(|row| {
            let question_count = usize::try_from(
                row.try_get::<i64, _>(13)
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|_| "invalid conversation search question count".to_string())?;
            let turn_count = usize::try_from(
                row.try_get::<i64, _>(14)
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|_| "invalid conversation search turn count".to_string())?;
            Ok(ConversationSessionListItem {
                session: map_sqlx_conversation_session(row)?,
                question_count,
                turn_count,
            })
        })
        .collect()
}

async fn load_search_questions_sqlx(
    pool: &SqlitePool,
    tables: ConversationRecordTables,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
) -> AppResult<BTreeMap<String, Vec<ConversationQuestion>>> {
    let query = format!(
        r#"
        SELECT q.id, q.session_id, q.question_index, q.title, q.question_text,
               q.answer_text, q.code_text, q.command_text, q.grouping_origin,
               q.created_at, q.updated_at
        FROM {questions} q
        JOIN {sessions} s ON s.id = q.session_id
        WHERE (?1 IS NULL OR s.adapter_id = ?1)
          AND (?2 IS NULL OR s.source_id = ?2)
        ORDER BY q.session_id ASC, q.question_index ASC
        "#,
        questions = tables.questions,
        sessions = tables.sessions,
    );
    let rows = sqlx::query(AssertSqlSafe(query))
        .bind(adapter_id)
        .bind(source_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
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

async fn load_search_turns_sqlx(
    pool: &SqlitePool,
    tables: ConversationRecordTables,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
) -> AppResult<BTreeMap<String, Vec<ConversationTurn>>> {
    let query = format!(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at,
               qt.question_id
        FROM {turns} t
        JOIN {question_turns} qt ON qt.turn_id = t.id
        JOIN {sessions} s ON s.id = t.session_id
        WHERE (?1 IS NULL OR s.adapter_id = ?1)
          AND (?2 IS NULL OR s.source_id = ?2)
        ORDER BY qt.question_id ASC, qt.turn_order ASC, t.turn_index ASC
        "#,
        turns = tables.turns,
        question_turns = tables.question_turns,
        sessions = tables.sessions,
    );
    let rows = sqlx::query(AssertSqlSafe(query))
        .bind(adapter_id)
        .bind(source_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    let mut turns_by_question = BTreeMap::<String, Vec<ConversationTurn>>::new();
    for row in &rows {
        let question_id = row
            .try_get::<String, _>(11)
            .map_err(|error| error.to_string())?;
        turns_by_question
            .entry(question_id)
            .or_default()
            .push(map_sqlx_conversation_turn(row)?);
    }
    Ok(turns_by_question)
}

async fn load_search_parts_sqlx(
    pool: &SqlitePool,
    tables: ConversationRecordTables,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
) -> AppResult<BTreeMap<String, Vec<ConversationPart>>> {
    let query = format!(
        r#"
        SELECT p.id, p.turn_id, p.part_index, p.role, p.kind, p.text, p.language,
               p.command, p.cwd, p.status, p.exit_code, p.metadata_json
        FROM {parts} p
        JOIN {turns} t ON t.id = p.turn_id
        JOIN {sessions} s ON s.id = t.session_id
        WHERE (?1 IS NULL OR s.adapter_id = ?1)
          AND (?2 IS NULL OR s.source_id = ?2)
        ORDER BY p.turn_id ASC, p.part_index ASC
        "#,
        parts = tables.parts,
        turns = tables.turns,
        sessions = tables.sessions,
    );
    let rows = sqlx::query(AssertSqlSafe(query))
        .bind(adapter_id)
        .bind(source_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
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

struct ConversationSearchEntry {
    card_type: ConversationSearchCardType,
    block_id: String,
    text: String,
}

#[cfg(test)]
fn load_record_sessions(
    conn: &Connection,
    tables: ConversationRecordTables,
) -> AppResult<Vec<ConversationSession>> {
    let query = format!(
        r#"
        SELECT id, source_id, adapter_id, external_id, title, project_path,
               started_at, updated_at, source_locator, source_fingerprint,
               missing, created_at, imported_at
        FROM {}
        ORDER BY COALESCE(updated_at, imported_at) DESC, title ASC
        "#,
        tables.sessions
    );
    let mut stmt = conn.prepare(&query).map_err(db_error)?;
    let rows = stmt
        .query_map([], map_conversation_session)
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

#[derive(Clone, Copy)]
enum SearchTimeBound {
    Since,
    Until,
}

fn parse_search_time_bound(
    value: Option<&str>,
    bound: SearchTimeBound,
) -> AppResult<Option<DateTime<Utc>>> {
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
    Err(format!(
        "invalid conversation search time {value:?}; use RFC3339 or YYYY-MM-DD"
    ))
}

fn conversation_session_search_time(session: &ConversationSession) -> Option<DateTime<Utc>> {
    session
        .started_at
        .as_deref()
        .and_then(parse_rfc3339_utc)
        .or_else(|| session.updated_at.as_deref().and_then(parse_rfc3339_utc))
        .or_else(|| parse_rfc3339_utc(&session.imported_at))
}

fn parse_rfc3339_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.trim())
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

#[cfg(test)]
fn list_record_questions(
    conn: &Connection,
    tables: ConversationRecordTables,
    session_id: &str,
) -> AppResult<Vec<ConversationQuestion>> {
    let query = format!(
        r#"
        SELECT id, session_id, question_index, title, question_text, answer_text,
               code_text, command_text, grouping_origin, created_at, updated_at
        FROM {}
        WHERE session_id = ?1
        ORDER BY question_index ASC
        "#,
        tables.questions
    );
    let mut stmt = conn.prepare(&query).map_err(db_error)?;
    let rows = stmt
        .query_map(params![session_id], map_conversation_question)
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

#[cfg(test)]
fn load_record_question_turns(
    conn: &Connection,
    tables: ConversationRecordTables,
    question_id: &str,
) -> AppResult<Vec<ConversationTurn>> {
    let query = format!(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at
        FROM {} t
        INNER JOIN {} qt ON qt.turn_id = t.id
        WHERE qt.question_id = ?1
        ORDER BY qt.turn_order ASC
        "#,
        tables.turns, tables.question_turns
    );
    let mut stmt = conn.prepare(&query).map_err(db_error)?;
    let rows = stmt
        .query_map(params![question_id], map_conversation_turn)
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

#[cfg(test)]
fn load_record_turn_parts(
    conn: &Connection,
    tables: ConversationRecordTables,
    turn_id: &str,
) -> AppResult<Vec<ConversationPart>> {
    let query = format!(
        r#"
        SELECT id, turn_id, part_index, role, kind, text, language, command,
               cwd, status, exit_code, metadata_json
        FROM {}
        WHERE turn_id = ?1
        ORDER BY part_index ASC
        "#,
        tables.parts
    );
    let mut stmt = conn.prepare(&query).map_err(db_error)?;
    let rows = stmt
        .query_map(params![turn_id], map_conversation_part)
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

#[cfg(test)]
fn count_record_questions(
    conn: &Connection,
    tables: ConversationRecordTables,
    session_id: &str,
) -> AppResult<usize> {
    count_record_rows(conn, tables.questions, session_id)
}

#[cfg(test)]
fn count_record_turns(
    conn: &Connection,
    tables: ConversationRecordTables,
    session_id: &str,
) -> AppResult<usize> {
    count_record_rows(conn, tables.turns, session_id)
}

#[cfg(test)]
fn count_record_rows(conn: &Connection, table: &str, session_id: &str) -> AppResult<usize> {
    let query = format!("SELECT COUNT(*) FROM {table} WHERE session_id = ?1");
    let count: i64 = conn
        .query_row(&query, params![session_id], |row| row.get(0))
        .map_err(db_error)?;
    Ok(count as usize)
}

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[allow(clippy::too_many_arguments)]
fn push_search_hit_if_matching(
    hits: &mut Vec<ConversationSearchHit>,
    needle: &str,
    allowed_types: &BTreeSet<ConversationSearchCardType>,
    session: &ConversationSessionListItem,
    question: &ConversationQuestion,
    question_title: &str,
    turn_id: Option<String>,
    part_id: Option<String>,
    block_id: String,
    card_type: ConversationSearchCardType,
    text: &str,
) {
    if !allowed_types.is_empty() && !allowed_types.contains(&card_type) {
        return;
    }
    if !text.to_lowercase().contains(needle) {
        return;
    }

    hits.push(ConversationSearchHit {
        session: session.clone(),
        question_id: question.id.clone(),
        question_index: question.question_index,
        question_title: question_title.to_string(),
        turn_id,
        part_id,
        block_id,
        card_type,
        snippet: search_snippet(text, needle),
        score: match_count(text, needle) * 100,
    });
}

fn search_entries_for_part(part: &ConversationPart) -> Vec<ConversationSearchEntry> {
    match part.kind {
        ConversationPartKind::CodeBlock => search_entry(
            part,
            ConversationSearchCardType::Code,
            part.text.as_deref(),
            "code",
        )
        .into_iter()
        .collect(),
        ConversationPartKind::Command => {
            let command = part
                .command
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    part.text
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                });
            let output = command_card_result_text(part);
            let mut entries = Vec::new();
            if let Some(entry) = search_entry(
                part,
                ConversationSearchCardType::Command,
                command,
                "command",
            ) {
                entries.push(entry);
            }
            if let Some(entry) = search_entry(
                part,
                ConversationSearchCardType::Result,
                output.as_deref(),
                "result",
            ) {
                entries.push(entry);
            }
            entries
        }
        ConversationPartKind::Text => {
            let card_type = if part.role == ConversationPartRole::Tool {
                ConversationSearchCardType::Result
            } else {
                ConversationSearchCardType::Answer
            };
            search_entry(
                part,
                card_type,
                part.text.as_deref(),
                card_type.block_suffix(),
            )
            .into_iter()
            .collect()
        }
        ConversationPartKind::Metadata => search_entry(
            part,
            ConversationSearchCardType::Tool,
            part.text.as_deref().or(part.metadata_json.as_deref()),
            "tool",
        )
        .into_iter()
        .collect(),
        ConversationPartKind::Tool
        | ConversationPartKind::Subagent
        | ConversationPartKind::FileChange => {
            let card_type = if is_search_result_part(part) {
                ConversationSearchCardType::Result
            } else {
                ConversationSearchCardType::Tool
            };
            search_entry(
                part,
                card_type,
                part.text.as_deref().or(part.metadata_json.as_deref()),
                card_type.block_suffix(),
            )
            .into_iter()
            .collect()
        }
    }
}

fn search_entry(
    part: &ConversationPart,
    card_type: ConversationSearchCardType,
    text: Option<&str>,
    suffix: &str,
) -> Option<ConversationSearchEntry> {
    let text = text.map(str::trim).filter(|value| !value.is_empty())?;
    Some(ConversationSearchEntry {
        card_type,
        block_id: format!("{}-{suffix}", part.id),
        text: text.to_string(),
    })
}

impl ConversationSearchCardType {
    fn block_suffix(self) -> &'static str {
        match self {
            ConversationSearchCardType::Question => "question",
            ConversationSearchCardType::Answer => "answer",
            ConversationSearchCardType::Tool => "tool",
            ConversationSearchCardType::Command => "command",
            ConversationSearchCardType::Code => "code",
            ConversationSearchCardType::Result => "result",
        }
    }
}

fn command_card_result_text(part: &ConversationPart) -> Option<String> {
    let text = part.text.as_deref().map(str::trim).filter(|value| {
        !value.is_empty() && part.command.as_deref().map(str::trim) != Some(*value)
    });
    if let Some(text) = text {
        return Some(text.to_string());
    }
    if let Some(status) = part
        .status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(status.to_string());
    }
    part.exit_code
        .map(|exit_code| format!("Exit code {exit_code}"))
}

fn is_search_result_part(part: &ConversationPart) -> bool {
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

fn search_snippet(text: &str, needle: &str) -> String {
    let normalized_text = text.to_lowercase();
    let match_start = normalized_text
        .find(needle)
        .map(|index| normalized_text[..index].chars().count())
        .unwrap_or(0);
    let chars = text.chars().collect::<Vec<_>>();
    let start = match_start.saturating_sub(64);
    let end = (match_start + needle.chars().count() + 96).min(chars.len());
    let prefix = if start > 0 { "..." } else { "" };
    let suffix = if end < chars.len() { "..." } else { "" };
    compact_whitespace(&format!(
        "{prefix}{}{suffix}",
        chars[start..end].iter().collect::<String>()
    ))
}

fn match_count(text: &str, needle: &str) -> usize {
    text.to_lowercase().matches(needle).count().max(1)
}

fn compact_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_query(query: Option<&str>) -> Option<String> {
    query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase)
}

fn normalize_project_path(project_path: Option<&str>) -> Option<String> {
    project_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
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
    use crate::backend::models::{
        ConversationPartRole, NormalizedConversationPart, NormalizedConversationTurn,
    };
    use crate::backend::store::Database;
    use uuid::Uuid;

    #[test]
    fn sqlx_conversation_metadata_round_trips_and_disables_sources() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-metadata-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let builtin_adapter = test_conversation_adapter(
            "metadata-builtin",
            ConversationAdapterKind::Codex,
            ConversationAdapterTrustState::BuiltIn,
        );
        let external_adapter = test_conversation_adapter(
            "metadata-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&external_adapter.id);

        let (
            adapters,
            loaded_adapter,
            sources,
            loaded_source,
            disabled_source,
            builtin_delete_error,
            deleted_adapter,
            source_after_adapter_delete,
            missing_adapter,
        ) = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), &builtin_adapter).await?;
                upsert_conversation_adapter_sqlx(database.pool(), &external_adapter).await?;
                upsert_conversation_source_sqlx(database.pool(), &source).await?;

                let adapters = list_conversation_adapters_sqlx(database.pool()).await?;
                let loaded_adapter =
                    load_conversation_adapter_sqlx(database.pool(), &external_adapter.id).await?;
                let sources = list_conversation_sources_sqlx(database.pool()).await?;
                let loaded_source =
                    load_conversation_source_sqlx(database.pool(), &source.id).await?;
                let disabled_source =
                    disable_conversation_source_sqlx(database.pool(), &source.id).await?;
                upsert_conversation_source_sqlx(database.pool(), &source).await?;
                let builtin_delete_error =
                    delete_conversation_adapter_sqlx(database.pool(), &builtin_adapter.id)
                        .await
                        .expect_err("built-in adapter delete should fail");
                let deleted_adapter =
                    delete_conversation_adapter_sqlx(database.pool(), &external_adapter.id).await?;
                let source_after_adapter_delete =
                    load_conversation_source_sqlx(database.pool(), &source.id)
                        .await?
                        .expect("source is retained after adapter delete");
                let missing_adapter =
                    load_conversation_adapter_sqlx(database.pool(), &external_adapter.id).await?;

                AppResult::Ok((
                    adapters,
                    loaded_adapter,
                    sources,
                    loaded_source,
                    disabled_source,
                    builtin_delete_error,
                    deleted_adapter,
                    source_after_adapter_delete,
                    missing_adapter,
                ))
            })
            .expect("query SQLx conversation metadata repo");

        assert!(adapters.iter().any(|adapter| adapter == &external_adapter));
        assert_eq!(loaded_adapter.as_ref(), Some(&external_adapter));
        assert!(sources.iter().any(|candidate| candidate == &source));
        assert_eq!(loaded_source.as_ref(), Some(&source));
        assert_eq!(disabled_source.id, source.id);
        assert!(!disabled_source.enabled);
        assert!(builtin_delete_error.contains("built-in conversation adapters"));
        assert_eq!(deleted_adapter, external_adapter);
        assert_eq!(source_after_adapter_delete.id, source.id);
        assert!(!source_after_adapter_delete.enabled);
        assert!(missing_adapter.is_none());

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_import_preserves_manual_grouping_across_resync() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-import-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let adapter = test_conversation_adapter(
            "import-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&adapter.id);

        database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), &adapter).await?;
                upsert_conversation_source_sqlx(database.pool(), &source).await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    &source,
                    &[fixture_session("v1")],
                    false,
                )
                .await?;
                AppResult::Ok(())
            })
            .expect("import v1 through SQLx");

        let conn = Connection::open(&db_path).expect("open rusqlite verification connection");
        let sessions =
            list_conversation_sessions(&conn, None, Some(&source.id), None, 20, 0).unwrap();
        let detail = load_conversation_session_detail(&conn, &sessions[0].session.id).unwrap();
        assert_eq!(detail.questions.len(), 2);
        assert_eq!(detail.questions[0].turns.len(), 2);

        let question_ids = detail
            .questions
            .iter()
            .map(|question| question.question.id.clone())
            .collect::<Vec<_>>();
        merge_conversation_questions(&conn, &question_ids, false).unwrap();

        database
            .block_on(async {
                import_conversation_sessions_sqlx(
                    database.pool(),
                    &source,
                    &[fixture_session("v2")],
                    false,
                )
                .await?;
                AppResult::Ok(())
            })
            .expect("import v2 through SQLx");

        let detail = load_conversation_session_detail(&conn, &sessions[0].session.id).unwrap();
        assert_eq!(detail.questions.len(), 1);
        assert_eq!(detail.questions[0].turns.len(), 3);
        assert_eq!(
            detail.questions[0].question.grouping_origin,
            ConversationGroupingOrigin::Manual
        );

        drop(conn);
        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_import_skips_unchanged_fingerprinted_sessions() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-import-skip-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let adapter = test_conversation_adapter(
            "import-skip-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&adapter.id);
        let mut session = fixture_session("v1");
        session.source_fingerprint = Some("unchanged".to_string());

        let imported_at = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), &adapter).await?;
                upsert_conversation_source_sqlx(database.pool(), &source).await?;
                import_conversation_sessions_sqlx(database.pool(), &source, &[session.clone()], false)
                    .await?;
                sqlx::query(
                    "UPDATE conversation_sessions SET imported_at = 'preserved' WHERE source_id = ?1",
                )
                .bind(&source.id)
                .execute(database.pool())
                .await
                .map_err(|error| error.to_string())?;
                import_conversation_sessions_sqlx(database.pool(), &source, &[session], false)
                    .await?;
                sqlx::query_scalar::<_, String>(
                    "SELECT imported_at FROM conversation_sessions WHERE source_id = ?1",
                )
                .bind(&source.id)
                .fetch_one(database.pool())
                .await
                .map_err(|error| error.to_string())
            })
            .expect("import unchanged fingerprinted session through SQLx");

        assert_eq!(imported_at, "preserved");

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_conversation_reads_filter_questions_and_render_markdown() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-read-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let adapter = test_conversation_adapter(
            "read-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&adapter.id);

        let (sessions, detail, filtered_questions, question, markdown) = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), &adapter).await?;
                upsert_conversation_source_sqlx(database.pool(), &source).await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    &source,
                    &[fixture_session("v2")],
                    false,
                )
                .await?;
                let sessions = list_conversation_sessions_sqlx(
                    database.pool(),
                    None,
                    Some(&source.id),
                    Some("answer for t3"),
                    20,
                    0,
                )
                .await?;
                let detail =
                    load_conversation_session_detail_sqlx(database.pool(), &sessions[0].session.id)
                        .await?;
                let filtered_questions = list_conversation_question_details_sqlx(
                    database.pool(),
                    &sessions[0].session.id,
                    Some("answer for t3"),
                    20,
                    0,
                )
                .await?;
                let question = load_conversation_question_detail_sqlx(
                    database.pool(),
                    &filtered_questions[0].question.id,
                )
                .await?;
                let markdown = render_conversation_detail_markdown_with_filter(
                    &detail,
                    &[question.question.id.clone()],
                    &ConversationExportContentFilter::default(),
                )?;
                AppResult::Ok((sessions, detail, filtered_questions, question, markdown))
            })
            .expect("read conversations through SQLx");

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].question_count, 2);
        assert_eq!(sessions[0].turn_count, 3);
        assert_eq!(detail.questions.len(), 2);
        assert_eq!(filtered_questions.len(), 1);
        assert_eq!(filtered_questions[0].question.question_text, "Export it");
        assert_eq!(question.turns.len(), 1);
        assert_eq!(question.parts.len(), 1);
        assert!(markdown.contains("## 1. Export it"));
        assert!(markdown.contains("answer for t3"));
        assert!(!markdown.contains("How does sync work?"));

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_merge_and_split_conversation_questions_preserve_grouping() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-mutation-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let adapter = test_conversation_adapter(
            "mutation-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&adapter.id);

        let detail = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), &adapter).await?;
                upsert_conversation_source_sqlx(database.pool(), &source).await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    &source,
                    &[fixture_session("v1")],
                    false,
                )
                .await?;
                let detail = load_conversation_session_detail_sqlx(
                    database.pool(),
                    &stable_id("conversation-session", &[&source.id, "session-1"]),
                )
                .await?;
                let question_ids = detail
                    .questions
                    .iter()
                    .map(|question| question.question.id.clone())
                    .collect::<Vec<_>>();
                let dry_run =
                    merge_conversation_questions_sqlx(database.pool(), &question_ids, true).await?;
                assert!(dry_run.dry_run);
                assert_eq!(
                    load_conversation_session_detail_sqlx(database.pool(), &detail.session.id)
                        .await?
                        .questions
                        .len(),
                    2
                );
                let merged =
                    merge_conversation_questions_sqlx(database.pool(), &question_ids, false)
                        .await?;
                let first_turn_error = split_conversation_question_sqlx(
                    database.pool(),
                    &merged.questions[0].question.id,
                    &merged.questions[0].turns[0].id,
                    false,
                )
                .await
                .expect_err("split at first turn should fail");
                assert!(first_turn_error.contains("must not be the first turn"));
                let split_turn_id = merged.questions[0].turns[2].id.clone();
                split_conversation_question_sqlx(
                    database.pool(),
                    &merged.questions[0].question.id,
                    &split_turn_id,
                    false,
                )
                .await?;
                load_conversation_session_detail_sqlx(database.pool(), &detail.session.id).await
            })
            .expect("merge and split through SQLx");

        assert_eq!(detail.questions.len(), 2);
        assert_eq!(detail.questions[0].turns.len(), 2);
        assert_eq!(detail.questions[1].turns.len(), 1);
        assert!(detail.questions.iter().all(|question| {
            question.question.grouping_origin == ConversationGroupingOrigin::Manual
        }));
        assert_eq!(
            detail.questions[0].question.question_text,
            "How does sync work?\n\n继续"
        );
        assert_eq!(detail.questions[1].question.question_text, "Export it");

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_searches_session_and_web_conversation_cards() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-search-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let session_adapter = test_conversation_adapter(
            "search-session-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let web_adapter = test_conversation_adapter(
            "search-web-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let session_source = test_conversation_source(&session_adapter.id);
        let mut web_source = test_conversation_source(&web_adapter.id);
        web_source.id = "search-web-source".to_string();
        let mut session = fixture_session("v1");
        session.started_at = Some("2026-03-02T10:00:00Z".to_string());
        let mut web_session = fixture_session("v1");
        web_session.external_id = "web-session".to_string();
        web_session.started_at = Some("2026-04-02T10:00:00Z".to_string());

        let (session_page, web_page) = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), &session_adapter).await?;
                upsert_conversation_adapter_sqlx(database.pool(), &web_adapter).await?;
                upsert_conversation_source_sqlx(database.pool(), &session_source).await?;
                upsert_conversation_source_sqlx(database.pool(), &web_source).await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    &session_source,
                    &[session],
                    false,
                )
                .await?;
                super::super::web_record_repo::import_web_record_sessions_sqlx(
                    database.pool(),
                    &web_source,
                    &[web_session],
                    false,
                )
                .await?;
                let session_page = search_conversation_cards_sqlx(
                    database.pool(),
                    ConversationRecordKind::Session,
                    Some(&session_adapter.id),
                    Some(&session_source.id),
                    Some("/tmp/project"),
                    "answer for t1",
                    &[ConversationSearchCardType::Answer],
                    Some("2026-03-01"),
                    Some("2026-03-31"),
                    true,
                    20,
                    0,
                )
                .await?;
                let web_page = search_conversation_cards_sqlx(
                    database.pool(),
                    ConversationRecordKind::Web,
                    Some(&web_adapter.id),
                    Some(&web_source.id),
                    None,
                    "answer for t3",
                    &[ConversationSearchCardType::Answer],
                    None,
                    None,
                    false,
                    20,
                    0,
                )
                .await?;
                AppResult::Ok((session_page, web_page))
            })
            .expect("search session and web records through SQLx");

        assert_eq!(session_page.total_count, 1);
        assert_eq!(
            session_page.hits[0].session.session.source_id,
            session_source.id
        );
        assert_eq!(
            session_page.hits[0].card_type,
            ConversationSearchCardType::Answer
        );
        assert_eq!(web_page.total_count, 1);
        assert_eq!(web_page.hits[0].session.session.source_id, web_source.id);
        assert_eq!(
            web_page.hits[0].card_type,
            ConversationSearchCardType::Answer
        );

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn imports_turns_and_preserves_manual_grouping_across_resync() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::backend::store::sql::INIT_SCHEMA)
            .unwrap();
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
    fn unchanged_fingerprinted_session_is_not_rewritten() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::backend::store::sql::INIT_SCHEMA)
            .unwrap();
        seed_builtin_conversation_adapters(&conn).unwrap();
        let source = load_conversation_source(&conn, "codex-live")
            .unwrap()
            .unwrap();
        let mut session = fixture_session("v1");
        session.source_fingerprint = Some("unchanged".to_string());

        import_conversation_sessions(&conn, &source, &[session.clone()], false).unwrap();
        conn.execute(
            "UPDATE conversation_sessions SET imported_at = 'preserved' WHERE source_id = ?1",
            params![source.id],
        )
        .unwrap();

        import_conversation_sessions(&conn, &source, &[session], false).unwrap();

        let imported_at: String = conn
            .query_row(
                "SELECT imported_at FROM conversation_sessions WHERE source_id = ?1",
                params![source.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(imported_at, "preserved");
    }

    #[test]
    fn split_rejects_first_turn_and_creates_tail_question() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::backend::store::sql::INIT_SCHEMA)
            .unwrap();
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
        conn.execute_batch(crate::backend::store::sql::INIT_SCHEMA)
            .unwrap();
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
        conn.execute_batch(crate::backend::store::sql::INIT_SCHEMA)
            .unwrap();
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

    #[test]
    fn search_conversation_cards_returns_user_question_hit_locations() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::backend::store::sql::INIT_SCHEMA)
            .unwrap();
        seed_builtin_conversation_adapters(&conn).unwrap();
        let source = load_conversation_source(&conn, "codex-live")
            .unwrap()
            .unwrap();
        import_conversation_sessions(&conn, &source, &[fixture_session("v1")], false).unwrap();

        let page = search_conversation_cards(
            &conn,
            ConversationRecordKind::Session,
            Some("codex"),
            Some("codex-live"),
            None,
            "sync work",
            &[ConversationSearchCardType::Question],
            None,
            None,
            false,
            20,
            0,
        )
        .unwrap();

        assert_eq!(page.total_count, 1);
        let hit = &page.hits[0];
        assert_eq!(hit.card_type, ConversationSearchCardType::Question);
        assert_eq!(hit.question_index, 0);
        assert!(hit.turn_id.is_some());
        assert!(hit.part_id.is_none());
        assert!(hit.block_id.ends_with("-question"));
        assert!(hit.snippet.contains("How does sync work?"));
    }

    #[test]
    fn search_conversation_cards_returns_answer_card_locations() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::backend::store::sql::INIT_SCHEMA)
            .unwrap();
        seed_builtin_conversation_adapters(&conn).unwrap();
        let source = load_conversation_source(&conn, "codex-live")
            .unwrap()
            .unwrap();
        import_conversation_sessions(&conn, &source, &[fixture_session("v1")], false).unwrap();

        let page = search_conversation_cards(
            &conn,
            ConversationRecordKind::Session,
            None,
            None,
            None,
            "answer for t3",
            &[ConversationSearchCardType::Answer],
            None,
            None,
            false,
            20,
            0,
        )
        .unwrap();

        assert_eq!(page.total_count, 1);
        let hit = &page.hits[0];
        assert_eq!(hit.card_type, ConversationSearchCardType::Answer);
        assert_eq!(hit.question_index, 1);
        assert!(hit.turn_id.is_some());
        assert!(hit.part_id.is_some());
        assert!(hit.block_id.ends_with("-answer"));
        assert!(hit.snippet.contains("answer for t3"));
    }

    #[test]
    fn search_conversation_cards_filters_by_project_path() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::backend::store::sql::INIT_SCHEMA)
            .unwrap();
        seed_builtin_conversation_adapters(&conn).unwrap();
        let source = load_conversation_source(&conn, "codex-live")
            .unwrap()
            .unwrap();
        let mut first = fixture_session("v1");
        first.external_id = "session-project".to_string();
        first.project_path = Some("/tmp/project".to_string());
        let mut second = fixture_session("v1");
        second.external_id = "session-other".to_string();
        second.project_path = Some("/tmp/other".to_string());
        import_conversation_sessions(&conn, &source, &[first, second], false).unwrap();

        let page = search_conversation_cards(
            &conn,
            ConversationRecordKind::Session,
            None,
            None,
            Some("/tmp/project"),
            "answer for t1",
            &[ConversationSearchCardType::Answer],
            None,
            None,
            false,
            20,
            0,
        )
        .unwrap();

        assert_eq!(page.total_count, 1);
        assert_eq!(
            page.hits[0].session.session.project_path.as_deref(),
            Some("/tmp/project")
        );
    }

    #[test]
    fn search_conversation_cards_filters_by_time_range_and_timeline() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::backend::store::sql::INIT_SCHEMA)
            .unwrap();
        seed_builtin_conversation_adapters(&conn).unwrap();
        let source = load_conversation_source(&conn, "codex-live")
            .unwrap()
            .unwrap();
        let mut early = fixture_session("v1");
        early.external_id = "session-early".to_string();
        early.started_at = Some("2026-01-02T10:00:00Z".to_string());
        let mut late = fixture_session("v1");
        late.external_id = "session-late".to_string();
        late.started_at = Some("2026-03-02T10:00:00Z".to_string());
        let mut outside = fixture_session("v1");
        outside.external_id = "session-outside".to_string();
        outside.started_at = Some("2026-05-02T10:00:00Z".to_string());
        import_conversation_sessions(&conn, &source, &[late, outside, early], false).unwrap();

        let page = search_conversation_cards(
            &conn,
            ConversationRecordKind::Session,
            None,
            None,
            None,
            "answer for t1",
            &[ConversationSearchCardType::Answer],
            Some("2026-01-01"),
            Some("2026-04-01"),
            true,
            20,
            0,
        )
        .unwrap();

        assert_eq!(page.total_count, 2);
        assert_eq!(page.hits[0].session.session.external_id, "session-early");
        assert_eq!(page.hits[1].session.session.external_id, "session-late");
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

    fn test_conversation_adapter(
        id: &str,
        kind: ConversationAdapterKind,
        trust_state: ConversationAdapterTrustState,
    ) -> ConversationAdapter {
        ConversationAdapter {
            id: id.to_string(),
            name: id.to_string(),
            kind,
            version: "1.0.0".to_string(),
            enabled: true,
            manifest_path: Some(format!("/tmp/{id}/manifest.json")),
            executable_path: Some(format!("/tmp/{id}/adapter")),
            content_hash: Some(format!("{id}-hash")),
            trusted_hash: Some(format!("{id}-hash")),
            trust_state,
            protocol_version: Some(1),
            capabilities: vec!["read".to_string()],
            input_kinds: vec![ConversationSourceKind::Directory],
            created_at: "2026-06-19T00:00:00Z".to_string(),
            updated_at: "2026-06-19T00:00:00Z".to_string(),
        }
    }

    fn test_conversation_source(adapter_id: &str) -> ConversationSource {
        ConversationSource {
            id: format!("{adapter_id}-source"),
            adapter_id: adapter_id.to_string(),
            name: format!("{adapter_id} source"),
            kind: ConversationSourceKind::Directory,
            location: format!("/tmp/{adapter_id}/sessions"),
            config_json: Some("{\"mode\":\"test\"}".to_string()),
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: "2026-06-19T00:00:00Z".to_string(),
            updated_at: "2026-06-19T00:00:00Z".to_string(),
        }
    }

    fn cleanup_database(db_path: &std::path::Path) {
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    }
}
