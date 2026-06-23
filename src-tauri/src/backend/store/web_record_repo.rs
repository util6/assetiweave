use crate::backend::dto::{
    AppResult, ConversationExportContentFilter, ConversationQuestionDetail,
    ConversationSessionDetail, ConversationSessionListItem,
};
use crate::backend::models::{
    conversation_turn_fingerprint, group_turn_ids_by_question, ConversationPart,
    ConversationSession, ConversationSource, ConversationSyncRun, ConversationSyncStatus,
    ConversationTurn, NormalizedConversationSession,
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::{Row as SqlxRow, Sqlite, SqlitePool, Transaction};
use std::collections::{BTreeMap, BTreeSet};

use super::{
    codec::encode_enum,
    conversation_repo::{
        append_declared_card_to_question_aggregate, map_sqlx_conversation_part,
        map_sqlx_conversation_question, map_sqlx_conversation_session, map_sqlx_conversation_turn,
        render_session_detail_markdown, ConversationImportResult, CONVERSATION_IMPORT_BATCH_SIZE,
    },
};

pub(crate) async fn import_web_record_sessions_sqlx(
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
    let incoming_session_ids = sessions
        .iter()
        .map(|session| stable_id("web-record-session", &[&source.id, &session.external_id]))
        .collect::<BTreeSet<_>>();
    {
        let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
        clear_legacy_conversation_records_for_source_sqlx_tx(&mut tx, &source.id).await?;
        prune_missing_web_record_sessions_sqlx_tx(&mut tx, &source.id, &incoming_session_ids)
            .await?;
        tx.commit().await.map_err(|error| error.to_string())?;
    }

    let mut warning_count = 0usize;
    let mut skipped_session_count = 0usize;
    for batch in sessions.chunks(CONVERSATION_IMPORT_BATCH_SIZE) {
        let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
        for normalized in batch {
            let session = web_record_session_from_normalized(source, normalized, &now);
            if web_record_session_is_unchanged_sqlx_tx(&mut tx, &session, normalized).await? {
                skipped_session_count += 1;
                continue;
            }
            delete_web_record_session_sqlx_tx(&mut tx, &session.id).await?;
            insert_web_record_session_sqlx_tx(&mut tx, &session).await?;

            let mut stored_turns = Vec::new();
            for turn in &normalized.turns {
                if turn.user_text.trim().is_empty() {
                    warning_count += 1;
                    continue;
                }
                let stored_turn = web_record_turn_from_normalized(&session.id, turn, &now);
                insert_web_record_turn_sqlx_tx(&mut tx, &stored_turn).await?;
                insert_web_record_parts_sqlx_tx(&mut tx, &stored_turn.id, &turn.parts).await?;
                stored_turns.push(stored_turn);
            }
            insert_web_record_questions_sqlx_tx(&mut tx, &session.id, &stored_turns, &now).await?;
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
            id: stable_id("web-record-sync", &[&source.id, &now]),
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
        warnings: Vec::new(),
    })
}

pub(crate) async fn list_web_record_sessions_sqlx(
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
                   FROM web_record_questions q
                   WHERE q.session_id = s.id
               ) AS question_count,
               (
                   SELECT COUNT(*)
                   FROM web_record_turns t
                   WHERE t.session_id = s.id
               ) AS turn_count
        FROM web_record_sessions s
        WHERE (?1 IS NULL OR s.adapter_id = ?1)
          AND (?2 IS NULL OR s.source_id = ?2)
          AND (
              ?3 IS NULL
              OR instr(lower(s.title), ?3) > 0
              OR instr(lower(COALESCE(s.project_path, '')), ?3) > 0
              OR instr(lower(s.external_id), ?3) > 0
              OR EXISTS (
                  SELECT 1
                  FROM web_record_questions q
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
    .bind(i64::try_from(limit).map_err(|_| format!("invalid web record limit: {limit}"))?)
    .bind(i64::try_from(offset).map_err(|_| format!("invalid web record offset: {offset}"))?)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;

    rows.iter()
        .map(|row| {
            let question_count = usize::try_from(
                row.try_get::<i64, _>(13)
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|_| "invalid web record question count".to_string())?;
            let turn_count = usize::try_from(
                row.try_get::<i64, _>(14)
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|_| "invalid web record turn count".to_string())?;
            Ok(ConversationSessionListItem {
                session: map_sqlx_conversation_session(row)?,
                question_count,
                turn_count,
            })
        })
        .collect()
}

pub(crate) async fn load_web_record_session_detail_sqlx(
    pool: &SqlitePool,
    session_id: &str,
) -> AppResult<ConversationSessionDetail> {
    let session_row = sqlx::query(
        r#"
        SELECT id, source_id, adapter_id, external_id, title, project_path,
               started_at, updated_at, source_locator, source_fingerprint,
               missing, created_at, imported_at
        FROM web_record_sessions
        WHERE id = ?1
        "#,
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| format!("web record session not found: {session_id}"))?;
    let session = map_sqlx_conversation_session(&session_row)?;

    let question_rows = sqlx::query(
        r#"
        SELECT id, session_id, question_index, title, question_text, answer_text,
               code_text, command_text, grouping_origin, created_at, updated_at
        FROM web_record_questions
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
        FROM web_record_question_turns qt
        JOIN web_record_turns t ON t.id = qt.turn_id
        JOIN web_record_questions q ON q.id = qt.question_id
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
        let question_id = row.try_get(11).map_err(|error| error.to_string())?;
        turns_by_question
            .entry(question_id)
            .or_default()
            .push(map_sqlx_conversation_turn(row)?);
    }

    let part_rows = sqlx::query(
        r#"
        SELECT p.id, p.turn_id, p.part_index, p.role, p.kind, p.text, p.language,
               p.command, p.cwd, p.status, p.exit_code, p.metadata_json
        FROM web_record_parts p
        JOIN web_record_turns t ON t.id = p.turn_id
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

    let question_details = questions
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
        .collect();
    Ok(ConversationSessionDetail {
        session,
        questions: question_details,
    })
}

pub(crate) fn render_web_record_detail_markdown_with_filter(
    detail: &ConversationSessionDetail,
    question_ids: &[String],
    content_filter: &ConversationExportContentFilter,
) -> AppResult<String> {
    let selection = (!question_ids.is_empty()).then_some(question_ids);
    render_session_detail_markdown(detail, selection, content_filter)
}

fn web_record_session_from_normalized(
    source: &ConversationSource,
    normalized: &NormalizedConversationSession,
    now: &str,
) -> ConversationSession {
    ConversationSession {
        id: stable_id("web-record-session", &[&source.id, &normalized.external_id]),
        source_id: source.id.clone(),
        adapter_id: source.adapter_id.clone(),
        external_id: normalized.external_id.clone(),
        title: normalized
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Untitled web conversation")
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

fn web_record_turn_from_normalized(
    session_id: &str,
    normalized: &crate::backend::models::NormalizedConversationTurn,
    now: &str,
) -> ConversationTurn {
    ConversationTurn {
        id: stable_id("web-record-turn", &[session_id, &normalized.external_id]),
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

struct QuestionAggregate {
    question_text: String,
    answer_text: String,
    code_text: String,
    command_text: String,
}

async fn prune_missing_web_record_sessions_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    source_id: &str,
    incoming_session_ids: &BTreeSet<String>,
) -> AppResult<()> {
    let existing_ids =
        sqlx::query_scalar::<_, String>("SELECT id FROM web_record_sessions WHERE source_id = ?1")
            .bind(source_id)
            .fetch_all(&mut **tx)
            .await
            .map_err(|error| error.to_string())?;
    for session_id in existing_ids {
        if !incoming_session_ids.contains(&session_id) {
            delete_web_record_session_sqlx_tx(tx, &session_id).await?;
        }
    }
    Ok(())
}

async fn delete_web_record_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    session_id: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"
        DELETE FROM web_record_question_turns
        WHERE question_id IN (
            SELECT id FROM web_record_questions WHERE session_id = ?1
        )
        "#,
    )
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM web_record_questions WHERE session_id = ?1")
        .bind(session_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(
        r#"
        DELETE FROM web_record_parts
        WHERE turn_id IN (
            SELECT id FROM web_record_turns WHERE session_id = ?1
        )
        "#,
    )
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM web_record_turns WHERE session_id = ?1")
        .bind(session_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM web_record_sessions WHERE id = ?1")
        .bind(session_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn web_record_session_is_unchanged_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    session: &ConversationSession,
    normalized: &NormalizedConversationSession,
) -> AppResult<bool> {
    let Some(source_fingerprint) = session.source_fingerprint.as_deref() else {
        return Ok(false);
    };
    let Some(row) = sqlx::query(
        r#"
        SELECT title, project_path, started_at, updated_at, source_locator,
               source_fingerprint, missing
        FROM web_record_sessions
        WHERE id = ?1
        "#,
    )
    .bind(&session.id)
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
        && missing == 0
        && web_record_session_turns_are_unchanged_sqlx_tx(tx, &session.id, normalized).await?)
}

async fn web_record_session_turns_are_unchanged_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    session_id: &str,
    normalized: &NormalizedConversationSession,
) -> AppResult<bool> {
    let rows = sqlx::query(
        r#"
        SELECT external_id, fingerprint, missing
        FROM web_record_turns
        WHERE session_id = ?1
        ORDER BY turn_index ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if rows.len() != normalized.turns.len() {
        return Ok(false);
    }
    for (row, turn) in rows.iter().zip(&normalized.turns) {
        let external_id: String = row.try_get(0).map_err(|error| error.to_string())?;
        let fingerprint: String = row.try_get(1).map_err(|error| error.to_string())?;
        let missing: i64 = row.try_get(2).map_err(|error| error.to_string())?;
        if external_id != turn.external_id
            || fingerprint != conversation_turn_fingerprint(turn)
            || missing != 0
        {
            return Ok(false);
        }
    }
    Ok(true)
}

async fn clear_legacy_conversation_records_for_source_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    source_id: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"
        DELETE FROM conversation_question_fts
        WHERE session_id IN (SELECT id FROM conversation_sessions WHERE source_id = ?1)
        "#,
    )
    .bind(source_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        r#"
        DELETE FROM conversation_question_turns
        WHERE question_id IN (
            SELECT q.id
            FROM conversation_questions q
            JOIN conversation_sessions s ON s.id = q.session_id
            WHERE s.source_id = ?1
        )
        "#,
    )
    .bind(source_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        r#"
        DELETE FROM conversation_questions
        WHERE session_id IN (SELECT id FROM conversation_sessions WHERE source_id = ?1)
        "#,
    )
    .bind(source_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        r#"
        DELETE FROM conversation_parts
        WHERE turn_id IN (
            SELECT t.id
            FROM conversation_turns t
            JOIN conversation_sessions s ON s.id = t.session_id
            WHERE s.source_id = ?1
        )
        "#,
    )
    .bind(source_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        r#"
        DELETE FROM conversation_turns
        WHERE session_id IN (SELECT id FROM conversation_sessions WHERE source_id = ?1)
        "#,
    )
    .bind(source_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM conversation_sessions WHERE source_id = ?1")
        .bind(source_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn insert_web_record_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    session: &ConversationSession,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO web_record_sessions (
            id, source_id, adapter_id, external_id, title, project_path, started_at,
            updated_at, source_locator, source_fingerprint, missing, created_at, imported_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
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

async fn insert_web_record_turn_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    turn: &ConversationTurn,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO web_record_turns (
            id, session_id, external_id, turn_index, user_text, title, started_at,
            ended_at, fingerprint, missing, imported_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
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

async fn insert_web_record_parts_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    turn_id: &str,
    parts: &[crate::backend::models::NormalizedConversationPart],
) -> AppResult<()> {
    for (index, part) in parts.iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO web_record_parts (
                id, turn_id, part_index, role, kind, text, language, command,
                cwd, status, exit_code, metadata_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
        )
        .bind(stable_id("web-record-part", &[turn_id, &index.to_string()]))
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

async fn insert_web_record_questions_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    session_id: &str,
    turns: &[ConversationTurn],
    now: &str,
) -> AppResult<()> {
    let groups = group_turn_ids_by_question(
        turns
            .iter()
            .map(|turn| (turn.id.clone(), turn.user_text.clone()))
            .collect::<Vec<_>>(),
    );
    for (index, group) in groups.into_iter().enumerate() {
        let first_turn_id = group
            .turn_ids
            .first()
            .ok_or_else(|| "empty web record question group".to_string())?;
        let question_id = stable_id("web-record-question", &[session_id, first_turn_id]);
        for (order, turn_id) in group.turn_ids.iter().enumerate() {
            sqlx::query(
                r#"
                INSERT INTO web_record_question_turns (question_id, turn_id, turn_order)
                VALUES (?1, ?2, ?3)
                "#,
            )
            .bind(&question_id)
            .bind(turn_id)
            .bind(order as i64)
            .execute(&mut **tx)
            .await
            .map_err(|error| error.to_string())?;
        }
        let aggregate = build_question_aggregate_sqlx_tx(tx, &group.turn_ids).await?;
        sqlx::query(
            r#"
            INSERT INTO web_record_questions (
                id, session_id, question_index, title, question_text, answer_text,
                code_text, command_text, grouping_origin, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
            "#,
        )
        .bind(&question_id)
        .bind(session_id)
        .bind(index as i64)
        .bind(first_line(&aggregate.question_text))
        .bind(&aggregate.question_text)
        .bind(&aggregate.answer_text)
        .bind(&aggregate.code_text)
        .bind(&aggregate.command_text)
        .bind(encode_enum(group.origin)?)
        .bind(now)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn build_question_aggregate_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    turn_ids: &[String],
) -> AppResult<QuestionAggregate> {
    let mut question_text = Vec::new();
    let mut answer_text = Vec::new();
    let mut code_text = Vec::new();
    let mut command_text = Vec::new();
    for turn_id in turn_ids {
        let user_text: String =
            sqlx::query_scalar::<_, String>("SELECT user_text FROM web_record_turns WHERE id = ?1")
                .bind(turn_id)
                .fetch_one(&mut **tx)
                .await
                .map_err(|error| error.to_string())?;
        question_text.push(user_text);
        for part in load_web_record_parts_sqlx_tx(tx, turn_id).await? {
            append_declared_card_to_question_aggregate(
                &part,
                &mut answer_text,
                &mut code_text,
                &mut command_text,
            );
        }
    }
    Ok(QuestionAggregate {
        question_text: question_text.join("\n\n"),
        answer_text: answer_text.join("\n\n"),
        code_text: code_text.join("\n\n"),
        command_text: command_text.join("\n\n"),
    })
}

async fn load_web_record_parts_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    turn_id: &str,
) -> AppResult<Vec<ConversationPart>> {
    let rows = sqlx::query(
        r#"
        SELECT id, turn_id, part_index, role, kind, text, language, command,
               cwd, status, exit_code, metadata_json
        FROM web_record_parts
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
        ConversationPartKind, ConversationPartRole, ConversationSourceKind,
        NormalizedConversationPart, NormalizedConversationTurn,
    };
    use crate::backend::store::Database;
    use uuid::Uuid;

    #[test]
    fn sqlx_web_records_use_independent_tables_and_remove_legacy_session_rows() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-web-record-import-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let source = fixture_source();

        let (legacy_count_before_import, legacy_count_after_import, sessions, detail) = database
            .block_on(async {
                super::super::conversation_repo::upsert_conversation_source_sqlx(
                    database.pool(),
                    &source,
                )
                .await?;
                super::super::conversation_repo::import_conversation_sessions_sqlx(
                    database.pool(),
                    &source,
                    &[fixture_session()],
                    false,
                )
                .await?;
                let legacy_count_before_import =
                    count_legacy_conversation_sessions_sqlx(database.pool(), &source.id).await?;
                import_web_record_sessions_sqlx(
                    database.pool(),
                    &source,
                    &[fixture_session()],
                    false,
                )
                .await?;
                let legacy_count_after_import =
                    count_legacy_conversation_sessions_sqlx(database.pool(), &source.id).await?;
                let sessions = list_web_record_sessions_sqlx(
                    database.pool(),
                    None,
                    Some(&source.id),
                    None,
                    20,
                    0,
                )
                .await?;
                let detail =
                    load_web_record_session_detail_sqlx(database.pool(), &sessions[0].session.id)
                        .await?;
                AppResult::Ok((
                    legacy_count_before_import,
                    legacy_count_after_import,
                    sessions,
                    detail,
                ))
            })
            .expect("import and read web records through SQLx");

        assert_eq!(legacy_count_before_import, 1);
        assert_eq!(legacy_count_after_import, 0);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].question_count, 1);
        assert_eq!(sessions[0].turn_count, 1);
        assert_eq!(detail.questions.len(), 1);
        assert_eq!(detail.questions[0].turns[0].user_text, "Hello from the web");
        assert_eq!(detail.questions[0].question.answer_text, "Web answer");

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_web_record_import_skips_unchanged_fingerprinted_sessions() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-web-record-import-skip-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let source = fixture_source();
        let mut session = fixture_session();
        session.source_fingerprint = Some("unchanged".to_string());

        let imported_at = database
            .block_on(async {
                super::super::conversation_repo::upsert_conversation_source_sqlx(
                    database.pool(),
                    &source,
                )
                .await?;
                import_web_record_sessions_sqlx(
                    database.pool(),
                    &source,
                    &[session.clone()],
                    false,
                )
                .await?;
                sqlx::query(
                    "UPDATE web_record_sessions SET imported_at = 'preserved' WHERE source_id = ?1",
                )
                .bind(&source.id)
                .execute(database.pool())
                .await
                .map_err(|error| error.to_string())?;
                import_web_record_sessions_sqlx(database.pool(), &source, &[session], false)
                    .await?;
                sqlx::query_scalar::<_, String>(
                    "SELECT imported_at FROM web_record_sessions WHERE source_id = ?1",
                )
                .bind(&source.id)
                .fetch_one(database.pool())
                .await
                .map_err(|error| error.to_string())
            })
            .expect("import unchanged fingerprinted web session through SQLx");

        assert_eq!(imported_at, "preserved");

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_web_record_import_rewrites_when_normalized_parts_change() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-web-record-import-refresh-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let source = fixture_source();
        let mut old_session = fixture_session();
        old_session.source_fingerprint = Some("same-source".to_string());
        old_session.turns[0].parts[0].metadata_json = None;
        let mut refreshed_session = fixture_session();
        refreshed_session.source_fingerprint = Some("same-source".to_string());
        refreshed_session.turns[0].parts[0].metadata_json = content_card_metadata("answer");

        let (result, imported_at, metadata_json) = database
            .block_on(async {
                super::super::conversation_repo::upsert_conversation_source_sqlx(
                    database.pool(),
                    &source,
                )
                .await?;
                import_web_record_sessions_sqlx(database.pool(), &source, &[old_session], false)
                    .await?;
                sqlx::query(
                    "UPDATE web_record_sessions SET imported_at = 'preserved' WHERE source_id = ?1",
                )
                .bind(&source.id)
                .execute(database.pool())
                .await
                .map_err(|error| error.to_string())?;
                let result = import_web_record_sessions_sqlx(
                    database.pool(),
                    &source,
                    &[refreshed_session],
                    false,
                )
                .await?;
                let imported_at = sqlx::query_scalar::<_, String>(
                    "SELECT imported_at FROM web_record_sessions WHERE source_id = ?1",
                )
                .bind(&source.id)
                .fetch_one(database.pool())
                .await
                .map_err(|error| error.to_string())?;
                let metadata_json = sqlx::query_scalar::<_, Option<String>>(
                    r#"
                    SELECT p.metadata_json
                    FROM web_record_parts p
                    JOIN web_record_turns t ON t.id = p.turn_id
                    JOIN web_record_sessions s ON s.id = t.session_id
                    WHERE s.source_id = ?1
                    ORDER BY p.part_index ASC
                    LIMIT 1
                    "#,
                )
                .bind(&source.id)
                .fetch_one(database.pool())
                .await
                .map_err(|error| error.to_string())?;
                AppResult::Ok((result, imported_at, metadata_json))
            })
            .expect("refresh normalized web parts through SQLx");

        assert_eq!(result.skipped_session_count, 0);
        assert_ne!(imported_at, "preserved");
        assert!(metadata_json
            .as_deref()
            .unwrap_or("")
            .contains(r#""content_card""#));

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_web_record_reads_filter_detail_and_render_markdown() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-web-record-read-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let source = fixture_source();
        let first = fixture_session();
        let mut second = fixture_session();
        second.external_id = "web-session-2".to_string();
        second.title = Some("SQLx migration notes".to_string());
        second.project_path = Some("/tmp/sqlx-project".to_string());
        second.turns[0].external_id = "turn-2".to_string();
        second.turns[0].user_text = "How is the read path migrated?".to_string();
        second.turns[0].parts[0].text = Some("Loaded through SQLx answer".to_string());

        let (sessions, detail, markdown) = database
            .block_on(async {
                super::super::conversation_repo::upsert_conversation_source_sqlx(
                    database.pool(),
                    &source,
                )
                .await?;
                import_web_record_sessions_sqlx(database.pool(), &source, &[first, second], false)
                    .await?;
                let sessions = list_web_record_sessions_sqlx(
                    database.pool(),
                    None,
                    Some(&source.id),
                    Some("sqlx answer"),
                    20,
                    0,
                )
                .await?;
                let detail =
                    load_web_record_session_detail_sqlx(database.pool(), &sessions[0].session.id)
                        .await?;
                let markdown = render_web_record_detail_markdown_with_filter(
                    &detail,
                    &[detail.questions[0].question.id.clone()],
                    &ConversationExportContentFilter::default(),
                )?;
                AppResult::Ok((sessions, detail, markdown))
            })
            .expect("read web records through SQLx");

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session.title, "SQLx migration notes");
        assert_eq!(sessions[0].question_count, 1);
        assert_eq!(sessions[0].turn_count, 1);
        assert_eq!(detail.questions.len(), 1);
        assert_eq!(detail.questions[0].turns.len(), 1);
        assert_eq!(detail.questions[0].parts.len(), 1);
        assert!(markdown.contains("## 1. How is the read path migrated?"));
        assert!(markdown.contains("Loaded through SQLx answer"));

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_web_record_aggregates_only_declared_content_cards() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-web-record-declared-cards-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let source = fixture_source();
        let mut session = fixture_session();
        session.turns[0].parts[0].text = Some("undeclared web answer".to_string());
        session.turns[0].parts[0].metadata_json = None;
        session.turns.push(NormalizedConversationTurn {
            external_id: "turn-2".to_string(),
            turn_index: 1,
            user_text: "Second web question".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some("declared web answer".to_string()),
                language: None,
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
                metadata_json: content_card_metadata("answer"),
            }],
        });

        let detail = database
            .block_on(async {
                super::super::conversation_repo::upsert_conversation_source_sqlx(
                    database.pool(),
                    &source,
                )
                .await?;
                import_web_record_sessions_sqlx(database.pool(), &source, &[session], false)
                    .await?;
                let sessions = list_web_record_sessions_sqlx(
                    database.pool(),
                    None,
                    Some(&source.id),
                    None,
                    20,
                    0,
                )
                .await?;
                load_web_record_session_detail_sqlx(database.pool(), &sessions[0].session.id).await
            })
            .expect("aggregate declared web content cards through SQLx");

        assert_eq!(detail.questions[0].question.answer_text, "");
        assert_eq!(
            detail.questions[1].question.answer_text,
            "declared web answer"
        );

        drop(database);
        cleanup_database(&db_path);
    }

    async fn count_legacy_conversation_sessions_sqlx(
        pool: &SqlitePool,
        source_id: &str,
    ) -> AppResult<i64> {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM conversation_sessions WHERE source_id = ?1",
        )
        .bind(source_id)
        .fetch_one(pool)
        .await
        .map_err(|error| error.to_string())
    }

    fn fixture_source() -> ConversationSource {
        let now = Utc::now().to_rfc3339();
        ConversationSource {
            id: "qwen-web-export".to_string(),
            adapter_id: "qwen-web".to_string(),
            name: "Qwen Web".to_string(),
            kind: ConversationSourceKind::Directory,
            location: "/tmp/qwen".to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    fn fixture_session() -> NormalizedConversationSession {
        NormalizedConversationSession {
            external_id: "web-session-1".to_string(),
            title: Some("Web session".to_string()),
            project_path: None,
            started_at: None,
            updated_at: None,
            source_locator: None,
            source_fingerprint: None,
            turns: vec![NormalizedConversationTurn {
                external_id: "turn-1".to_string(),
                turn_index: 0,
                user_text: "Hello from the web".to_string(),
                title: None,
                started_at: None,
                ended_at: None,
                parts: vec![NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Text,
                    text: Some("Web answer".to_string()),
                    language: None,
                    command: None,
                    cwd: None,
                    status: None,
                    exit_code: None,
                    metadata_json: content_card_metadata("answer"),
                }],
            }],
        }
    }

    fn content_card_metadata(card_type: &str) -> Option<String> {
        Some(format!(
            r#"{{"content_card":{{"type":"{card_type}","format":"markdown"}}}}"#
        ))
    }

    fn cleanup_database(db_path: &std::path::Path) {
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    }
}
