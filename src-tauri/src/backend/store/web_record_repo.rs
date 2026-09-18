use crate::backend::dto::{
    ConversationQuestionDetail, ConversationSessionDetail, ConversationSessionListItem,
};
use crate::backend::models::{
    conversation_turn_fingerprint, group_turn_ids_by_question, ConversationCardKindDefinition,
    ConversationPart, ConversationQuestionTurn, ConversationSession, ConversationSource,
    ConversationSyncRun, ConversationSyncStatus, ConversationTurn, NormalizedConversationSession,
};
use crate::backend::runtime::{AppError, AppResult};
use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};
use std::collections::BTreeMap;

use super::{
    codec::{decode_json_app, encode_enum_app, encode_json_app},
    conversation_repo::{
        append_projected_cards_to_question_aggregate, insert_conversation_sync_delta_sqlx_tx,
        map_sqlx_conversation_part, map_sqlx_conversation_question,
        map_sqlx_conversation_question_turn, map_sqlx_conversation_session,
        project_question_content_nodes, project_question_title, ConversationImportResult,
        CONVERSATION_IMPORT_BATCH_SIZE,
    },
};

pub(crate) async fn import_web_record_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
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
            sync_run_id: None,
            session_count: sessions.len(),
            skipped_session_count: 0,
            changed_session_count: 0,
            turn_count,
            warning_count: 0,
            warnings: Vec::new(),
            failed_session_count: 0,
            session_failures: Vec::new(),
            session_warnings: Vec::new(),
            status: crate::backend::models::ConversationSyncStatus::Completed,
        });
    }

    let now = Utc::now().to_rfc3339();
    let sync_run_id = stable_id("web-record-sync", &[&source.id, &now]);
    {
        let mut tx = pool.begin().await.map_err(AppError::external)?;
        clear_legacy_conversation_records_for_source_sqlx_tx(&mut tx, tenant_id, &source.id)
            .await?;
        tx.commit().await.map_err(AppError::external)?;
    }

    let mut warning_count = 0usize;
    let mut skipped_session_count = 0usize;
    let mut changed_session_count = 0usize;
    for batch in sessions.chunks(CONVERSATION_IMPORT_BATCH_SIZE) {
        let mut tx = pool.begin().await.map_err(AppError::external)?;
        for normalized in batch {
            let session = web_record_session_from_normalized(source, normalized, &now);
            let change_kind =
                if web_record_session_exists_sqlx_tx(&mut tx, tenant_id, &session.id).await? {
                    "updated"
                } else {
                    "new"
                };
            if web_record_session_is_unchanged_sqlx_tx(&mut tx, tenant_id, &session, normalized)
                .await?
            {
                skipped_session_count += 1;
                continue;
            }
            let translation_state =
                load_web_record_part_translation_state_sqlx_tx(&mut tx, tenant_id, &session.id)
                    .await
                    .map_err(AppError::external)?;
            delete_web_record_session_sqlx_tx(&mut tx, tenant_id, &session.id).await?;
            insert_web_record_session_sqlx_tx(&mut tx, tenant_id, &session).await?;

            let mut stored_turns = Vec::new();
            for turn in &normalized.turns {
                if turn.user_text.trim().is_empty() {
                    warning_count += 1;
                    continue;
                }
                let stored_turn = web_record_turn_from_normalized(&session.id, turn, &now);
                insert_web_record_turn_sqlx_tx(&mut tx, tenant_id, &stored_turn).await?;
                insert_web_record_parts_sqlx_tx(
                    &mut tx,
                    tenant_id,
                    &stored_turn.id,
                    &turn.parts,
                    &translation_state,
                )
                .await?;
                stored_turns.push(stored_turn);
            }
            insert_web_record_questions_sqlx_tx(
                &mut tx,
                tenant_id,
                &session.id,
                &stored_turns,
                &now,
            )
            .await?;
            insert_conversation_sync_delta_sqlx_tx(
                &mut tx,
                tenant_id,
                &sync_run_id,
                "web",
                &session.id,
                change_kind,
                &now,
            )
            .await
            .map_err(AppError::external)?;
            changed_session_count += 1;
        }
        tx.commit().await.map_err(AppError::external)?;
    }

    let mut tx = pool.begin().await.map_err(AppError::external)?;
    sqlx::query(
        r#"
        UPDATE conversation_sources
        SET last_synced_at = ?1, last_sync_status = 'completed', updated_at = ?1
        WHERE tenant_id = ?2 AND id = ?3
        "#,
    )
    .bind(&now)
    .bind(tenant_id)
    .bind(&source.id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    insert_sync_run_sqlx_tx(
        &mut tx,
        tenant_id,
        &ConversationSyncRun {
            id: sync_run_id.clone(),
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
    tx.commit().await.map_err(AppError::external)?;

    Ok(ConversationImportResult {
        source_id: source.id.clone(),
        adapter_id: source.adapter_id.clone(),
        dry_run: false,
        sync_run_id: Some(sync_run_id),
        session_count: sessions.len(),
        skipped_session_count,
        changed_session_count,
        turn_count,
        warning_count,
        warnings: Vec::new(),
        failed_session_count: 0,
        session_failures: Vec::new(),
        session_warnings: Vec::new(),
        status: crate::backend::models::ConversationSyncStatus::Completed,
    })
}

#[derive(Debug, FromRow)]
struct WebRecordSessionListItemRow {
    id: String,
    source_id: String,
    adapter_id: String,
    external_id: String,
    title: String,
    project_path: Option<String>,
    started_at: Option<String>,
    updated_at: Option<String>,
    source_locator: Option<String>,
    source_fingerprint: Option<String>,
    missing: i64,
    created_at: String,
    imported_at: String,
    question_count: i64,
    turn_count: i64,
}

pub(crate) async fn list_web_record_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    query: Option<&str>,
    limit: usize,
    offset: usize,
) -> AppResult<Vec<ConversationSessionListItem>> {
    let needle = normalize_query(query);
    let id_needle = query.and_then(crate::backend::models::conversation_id_search_term);
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
            .map_err(AppError::external)?,
    )
    .bind(
        i64::try_from(offset)
            .map_err(|_| format!("invalid web record offset: {offset}"))
            .map_err(AppError::external)?,
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    rows.into_iter()
        .map(|row| {
            let question_count = usize::try_from(row.question_count)
                .map_err(|_| "invalid web record question count".to_string())
                .map_err(AppError::external)?;
            let turn_count = usize::try_from(row.turn_count)
                .map_err(|_| "invalid web record turn count".to_string())
                .map_err(AppError::external)?;
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
) -> AppResult<String> {
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
    .map_err(AppError::external)?;

    match rows.len() {
        0 => Err(AppError::NotFound(format!(
            "no web record session matches prefix \"{input}\""
        ))),
        1 => Ok(rows.into_iter().next().unwrap()),
        n => {
            let preview: Vec<&str> = rows.iter().take(5).map(|s| s.as_str()).collect();
            Err(AppError::Conflict(format!(
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
) -> AppResult<String> {
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
    .map_err(AppError::external)?;

    match rows.len() {
        0 => Err(AppError::NotFound(format!(
            "no web record part matches prefix \"{input}\""
        ))),
        1 => Ok(rows.into_iter().next().unwrap()),
        n => {
            let preview: Vec<&str> = rows.iter().take(5).map(|s| s.as_str()).collect();
            Err(AppError::Conflict(format!(
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
) -> AppResult<ConversationSessionDetail> {
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
    .map_err(AppError::external)?
    .ok_or_else(|| AppError::NotFound(format!("web record session not found: {session_id}")))?;
    let session = map_sqlx_conversation_session(&session_row).map_err(AppError::external)?;

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
    .map_err(AppError::external)?;
    let questions = question_rows
        .iter()
        .map(map_sqlx_conversation_question)
        .collect::<AppResult<Vec<_>>>()?;

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
    .map_err(AppError::external)?;
    let mut question_turns_by_question = BTreeMap::<String, Vec<ConversationQuestionTurn>>::new();
    for row in &question_turn_rows {
        let membership = map_sqlx_conversation_question_turn(row).map_err(AppError::external)?;
        question_turns_by_question
            .entry(membership.question_id.clone())
            .or_default()
            .push(membership);
    }

    #[derive(Debug, FromRow)]
    struct WebRecordDetailTurnRow {
        id: String,
        session_id: String,
        external_id: String,
        turn_index: i64,
        user_text: String,
        title: Option<String>,
        started_at: Option<String>,
        ended_at: Option<String>,
        fingerprint: String,
        missing: i64,
        imported_at: String,
        question_id: String,
    }

    let turn_rows = sqlx::query_as::<_, WebRecordDetailTurnRow>(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at,
               qt.question_id
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
    .map_err(AppError::external)?;
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
    .map_err(AppError::external)?;
    let mut parts_by_turn = BTreeMap::<String, Vec<ConversationPart>>::new();
    for row in &part_rows {
        let part = map_sqlx_conversation_part(row).map_err(AppError::external)?;
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
    .map_err(AppError::external)?
    .unwrap_or_else(|| "[]".to_string());
    let card_kinds: Vec<crate::backend::models::ConversationCardKindDefinition> =
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
) -> AppResult<()> {
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
    .map_err(AppError::external)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "web record part not found: {part_id}"
        )));
    }

    Ok(())
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
        project_path: None,
        started_at: normalized.started_at.clone(),
        updated_at: normalized.updated_at.clone(),
        source_locator: normalized.source_locator.clone(),
        source_fingerprint: normalized.source_fingerprint.clone(),
        missing: false,
        created_at: now.to_string(),
        imported_at: now.to_string(),
        execution_origin: "user".to_string(),
        execution_purpose: None,
        user_visible: true,
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

async fn delete_web_record_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> AppResult<()> {
    sqlx::query("DELETE FROM conversation_question_fts WHERE tenant_id = ?1 AND session_id = ?2")
        .bind(tenant_id)
        .bind(session_id)
        .execute(&mut **tx)
        .await
        .map_err(AppError::external)?;
    sqlx::query(
        r#"
        DELETE FROM web_record_question_turns
        WHERE tenant_id = ?1
          AND question_id IN (
            SELECT id FROM web_record_questions
            WHERE tenant_id = ?1 AND session_id = ?2
        )
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    sqlx::query("DELETE FROM web_record_questions WHERE tenant_id = ?1 AND session_id = ?2")
        .bind(tenant_id)
        .bind(session_id)
        .execute(&mut **tx)
        .await
        .map_err(AppError::external)?;
    sqlx::query(
        r#"
        DELETE FROM web_record_parts
        WHERE tenant_id = ?1
          AND turn_id IN (
            SELECT id FROM web_record_turns
            WHERE tenant_id = ?1 AND session_id = ?2
        )
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    sqlx::query("DELETE FROM web_record_turns WHERE tenant_id = ?1 AND session_id = ?2")
        .bind(tenant_id)
        .bind(session_id)
        .execute(&mut **tx)
        .await
        .map_err(AppError::external)?;
    sqlx::query("DELETE FROM web_record_sessions WHERE tenant_id = ?1 AND id = ?2")
        .bind(tenant_id)
        .bind(session_id)
        .execute(&mut **tx)
        .await
        .map_err(AppError::external)?;
    Ok(())
}

#[derive(Debug, FromRow)]
struct ExistingWebRecordSessionRow {
    title: String,
    started_at: Option<String>,
    updated_at: Option<String>,
    source_locator: Option<String>,
    source_fingerprint: Option<String>,
    missing: i64,
}

async fn web_record_session_is_unchanged_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session: &ConversationSession,
    normalized: &NormalizedConversationSession,
) -> AppResult<bool> {
    let Some(source_fingerprint) = session.source_fingerprint.as_deref() else {
        return Ok(false);
    };
    let Some(row) = sqlx::query_as::<_, ExistingWebRecordSessionRow>(
        r#"
        SELECT title, started_at, updated_at, source_locator, source_fingerprint, missing
        FROM web_record_sessions
        WHERE tenant_id = ?1 AND id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(&session.id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(AppError::external)?
    else {
        return Ok(false);
    };

    Ok(row.title == session.title
        && row.started_at == session.started_at
        && row.updated_at == session.updated_at
        && row.source_locator == session.source_locator
        && row.source_fingerprint.as_deref() == Some(source_fingerprint)
        && row.missing == 0
        && session_turns_match_normalized_sqlx_tx(tx, tenant_id, &session.id, normalized).await?)
}

async fn web_record_session_exists_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> AppResult<bool> {
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM web_record_sessions WHERE tenant_id = ?1 AND id = ?2)",
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(AppError::external)?;
    Ok(exists != 0)
}

#[derive(Debug, FromRow)]
struct ExistingWebRecordTurnRow {
    external_id: String,
    fingerprint: String,
    missing: i64,
}

async fn session_turns_match_normalized_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    normalized: &NormalizedConversationSession,
) -> AppResult<bool> {
    let rows = sqlx::query_as::<_, ExistingWebRecordTurnRow>(
        r#"
        SELECT external_id, fingerprint, missing
        FROM web_record_turns
        WHERE tenant_id = ?1 AND session_id = ?2
        ORDER BY turn_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::external)?;
    if rows.len() != normalized.turns.len() {
        return Ok(false);
    }
    for (row, turn) in rows.iter().zip(&normalized.turns) {
        if row.external_id != turn.external_id
            || row.fingerprint != conversation_turn_fingerprint(turn)
            || row.missing != 0
        {
            return Ok(false);
        }
    }
    Ok(true)
}

async fn clear_legacy_conversation_records_for_source_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    source_id: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"
        DELETE FROM conversation_question_fts
        WHERE tenant_id = ?1
          AND session_id IN (
            SELECT id FROM conversation_sessions
            WHERE tenant_id = ?1 AND source_id = ?2
          )
        "#,
    )
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    sqlx::query(
        r#"
        DELETE FROM conversation_question_fts
        WHERE tenant_id = ?1
          AND session_id IN (
            SELECT id FROM web_record_sessions
            WHERE tenant_id = ?1 AND source_id = ?2
          )
        "#,
    )
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    sqlx::query(
        r#"
        DELETE FROM conversation_question_turns
        WHERE tenant_id = ?1
          AND question_id IN (
            SELECT q.id
            FROM conversation_questions q
            JOIN conversation_sessions s ON s.tenant_id = q.tenant_id AND s.id = q.session_id
            WHERE s.tenant_id = ?1 AND s.source_id = ?2
        )
        "#,
    )
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    sqlx::query(
        r#"
        DELETE FROM conversation_questions
        WHERE tenant_id = ?1
          AND session_id IN (
            SELECT id FROM conversation_sessions
            WHERE tenant_id = ?1 AND source_id = ?2
          )
        "#,
    )
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    sqlx::query(
        r#"
        DELETE FROM conversation_parts
        WHERE tenant_id = ?1
          AND turn_id IN (
            SELECT t.id
            FROM conversation_turns t
            JOIN conversation_sessions s ON s.tenant_id = t.tenant_id AND s.id = t.session_id
            WHERE s.tenant_id = ?1 AND s.source_id = ?2
        )
        "#,
    )
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    sqlx::query(
        r#"
        DELETE FROM conversation_turns
        WHERE tenant_id = ?1
          AND session_id IN (
            SELECT id FROM conversation_sessions
            WHERE tenant_id = ?1 AND source_id = ?2
          )
        "#,
    )
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    sqlx::query("DELETE FROM conversation_sessions WHERE tenant_id = ?1 AND source_id = ?2")
        .bind(tenant_id)
        .bind(source_id)
        .execute(&mut **tx)
        .await
        .map_err(AppError::external)?;
    Ok(())
}

async fn insert_web_record_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session: &ConversationSession,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO web_record_sessions (
            tenant_id, id, source_id, adapter_id, external_id, title, started_at, updated_at,
            source_locator, source_fingerprint, missing, created_at, imported_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
    )
    .bind(tenant_id)
    .bind(&session.id)
    .bind(&session.source_id)
    .bind(&session.adapter_id)
    .bind(&session.external_id)
    .bind(&session.title)
    .bind(&session.started_at)
    .bind(&session.updated_at)
    .bind(&session.source_locator)
    .bind(&session.source_fingerprint)
    .bind(if session.missing { 1_i64 } else { 0_i64 })
    .bind(&session.created_at)
    .bind(&session.imported_at)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    Ok(())
}

async fn insert_web_record_turn_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    turn: &ConversationTurn,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO web_record_turns (
            tenant_id, id, session_id, external_id, turn_index, user_text, title, started_at,
            ended_at, fingerprint, missing, imported_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        "#,
    )
    .bind(tenant_id)
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
    .map_err(AppError::external)?;
    Ok(())
}

async fn insert_web_record_parts_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    turn_id: &str,
    parts: &[crate::backend::models::NormalizedConversationPart],
    translation_state: &BTreeMap<String, (Option<String>, Option<String>, Option<String>)>,
) -> AppResult<()> {
    for (index, part) in parts.iter().enumerate() {
        let part_id = stable_id("web-record-part", &[turn_id, &index.to_string()]);
        let content_card_json = part
            .content_card
            .as_ref()
            .map(encode_json_app)
            .transpose()?;
        let translated_text = translation_state
            .get(&part_id)
            .filter(|(text, command, _)| text == &part.text && command == &part.command)
            .and_then(|(_, _, translated_text)| translated_text.as_ref());
        sqlx::query(
            r#"
            INSERT INTO web_record_parts (
                tenant_id, id, turn_id, part_index, role, kind, text, language, command,
                cwd, status, exit_code, command_label, metadata_json, content_card_json, translated_text,
                source_execution_id
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
            "#,
        )
        .bind(tenant_id)
        .bind(part_id)
        .bind(turn_id)
        .bind(index as i64)
        .bind(encode_enum_app(part.role)?)
        .bind(encode_enum_app(part.kind)?)
        .bind(&part.text)
        .bind(&part.language)
        .bind(&part.command)
        .bind(&part.cwd)
        .bind(&part.status)
        .bind(part.exit_code)
        .bind(&part.command_label)
        .bind(&part.metadata_json)
        .bind(content_card_json)
        .bind(translated_text)
        .bind(&part.source_execution_id)
        .execute(&mut **tx)
        .await
        .map_err(AppError::external)?;
    }
    Ok(())
}

#[derive(Debug, FromRow)]
struct WebRecordPartTranslationRow {
    id: String,
    text: Option<String>,
    command: Option<String>,
    translated_text: Option<String>,
}

async fn load_web_record_part_translation_state_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> AppResult<BTreeMap<String, (Option<String>, Option<String>, Option<String>)>> {
    let rows = sqlx::query_as::<_, WebRecordPartTranslationRow>(
        r#"
        SELECT p.id, p.text, p.command, p.translated_text
        FROM web_record_parts p
        JOIN web_record_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
        WHERE t.tenant_id = ?1 AND t.session_id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::external)?;
    Ok(rows
        .into_iter()
        .map(|row| (row.id, (row.text, row.command, row.translated_text)))
        .collect())
}

async fn insert_web_record_questions_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    turns: &[ConversationTurn],
    now: &str,
) -> AppResult<()> {
    let mut ordered_turns = turns.to_vec();
    ordered_turns.sort_by(|left, right| {
        left.turn_index
            .cmp(&right.turn_index)
            .then_with(|| left.id.cmp(&right.id))
    });
    let groups = group_turn_ids_by_question(
        ordered_turns
            .iter()
            .map(|turn| (turn.id.clone(), turn.user_text.clone()))
            .collect::<Vec<_>>(),
    );
    for (_index, group) in groups.into_iter().enumerate() {
        let first_turn_id = group
            .turn_ids
            .first()
            .ok_or_else(|| AppError::Validation("empty web record question group".to_string()))?;
        let question_id = stable_id("web-record-question", &[session_id, first_turn_id]);
        for (order, turn_id) in group.turn_ids.iter().enumerate() {
            sqlx::query(
                r#"
                INSERT INTO web_record_question_turns (
                    tenant_id, question_id, turn_id, turn_order,
                    assignment_origin, assigned_at, updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                "#,
            )
            .bind(tenant_id)
            .bind(&question_id)
            .bind(turn_id)
            .bind(order as i64)
            .bind(encode_enum_app(group.origin)?)
            .bind(now)
            .execute(&mut **tx)
            .await
            .map_err(AppError::external)?;
        }
        let aggregate =
            build_question_aggregate_sqlx_tx(tx, tenant_id, session_id, &group.turn_ids).await?;
        sqlx::query(
            r#"
            INSERT INTO web_record_questions (
                tenant_id, id, session_id, title, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?5)
            "#,
        )
        .bind(tenant_id)
        .bind(&question_id)
        .bind(session_id)
        .bind(first_line(&aggregate.question_text))
        .bind(now)
        .execute(&mut **tx)
        .await
        .map_err(AppError::external)?;
        sqlx::query(
            "DELETE FROM conversation_question_fts WHERE tenant_id = ?1 AND question_id = ?2",
        )
        .bind(tenant_id)
        .bind(&question_id)
        .execute(&mut **tx)
        .await
        .map_err(AppError::external)?;
        sqlx::query(
            r#"
            INSERT INTO conversation_question_fts (
                tenant_id, question_id, session_id, question_text, answer_text, code_text, command_text
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
        )
        .bind(tenant_id)
        .bind(&question_id)
        .bind(session_id)
        .bind(&aggregate.question_text)
        .bind(&aggregate.answer_text)
        .bind(&aggregate.code_text)
        .bind(&aggregate.command_text)
        .execute(&mut **tx)
        .await
        .map_err(AppError::external)?;
    }
    Ok(())
}

async fn build_question_aggregate_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    turn_ids: &[String],
) -> AppResult<QuestionAggregate> {
    let mut question_text = Vec::new();
    let mut answer_text = Vec::new();
    let mut code_text = Vec::new();
    let mut command_text = Vec::new();
    let adapter_id = sqlx::query_scalar::<_, String>(
        "SELECT adapter_id FROM web_record_sessions WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(AppError::external)?;
    let card_kinds_json = sqlx::query_scalar::<_, String>(
        "SELECT card_kinds_json FROM conversation_adapters WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(&adapter_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(AppError::external)?
    .unwrap_or_else(|| "[]".to_string());
    let card_kinds: Vec<ConversationCardKindDefinition> = decode_json_app(card_kinds_json)?;
    for turn_id in turn_ids {
        let user_text: String = sqlx::query_scalar::<_, String>(
            "SELECT user_text FROM web_record_turns WHERE tenant_id = ?1 AND id = ?2",
        )
        .bind(tenant_id)
        .bind(turn_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(AppError::external)?;
        question_text.push(user_text);
        for part in load_web_record_parts_sqlx_tx(tx, tenant_id, turn_id).await? {
            append_projected_cards_to_question_aggregate(
                &part,
                &adapter_id,
                &card_kinds,
                &mut answer_text,
                &mut code_text,
                &mut command_text,
            )?;
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
    tenant_id: &str,
    turn_id: &str,
) -> AppResult<Vec<ConversationPart>> {
    let rows = sqlx::query(
        r#"
        SELECT id, turn_id, part_index, role, kind, text, language, command,
               cwd, status, exit_code, metadata_json, content_card_json, translated_text,
               source_execution_id, command_label
        FROM web_record_parts
        WHERE tenant_id = ?1 AND turn_id = ?2
        ORDER BY part_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(turn_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::external)?;
    rows.iter().map(map_sqlx_conversation_part).collect()
}

async fn insert_sync_run_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    run: &ConversationSyncRun,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_sync_runs (
            tenant_id, id, source_id, adapter_id, status, started_at, finished_at,
            session_count, turn_count, warning_count, error_message
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        "#,
    )
    .bind(tenant_id)
    .bind(&run.id)
    .bind(&run.source_id)
    .bind(&run.adapter_id)
    .bind(encode_enum_app(run.status)?)
    .bind(&run.started_at)
    .bind(&run.finished_at)
    .bind(run.session_count)
    .bind(run.turn_count)
    .bind(run.warning_count)
    .bind(&run.error_message)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
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
#[path = "web_record_repo_tests.rs"]
mod tests;
