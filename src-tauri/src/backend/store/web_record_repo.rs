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
use sqlx::{Row as SqlxRow, Sqlite, SqlitePool, Transaction};
use std::collections::BTreeMap;

use super::{
    codec::{decode_json_app, encode_enum_app, encode_json_app},
    conversation_repo::{
        append_projected_cards_to_question_aggregate, insert_conversation_sync_delta_sqlx_tx,
        map_sqlx_conversation_part, map_sqlx_conversation_question,
        map_sqlx_conversation_question_turn, map_sqlx_conversation_session,
        map_sqlx_conversation_turn, project_question_content_nodes, project_question_title,
        ConversationImportResult, CONVERSATION_IMPORT_BATCH_SIZE,
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
            super::memory_repo::reconcile_memory_evidence_for_session_tx(
                &mut tx,
                tenant_id,
                crate::backend::models::MemoryEvidenceRecordKind::Web,
                &session.id,
                &[],
                &[],
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
    })
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
    let rows = sqlx::query(
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

    rows.iter()
        .map(|row| {
            let question_count =
                usize::try_from(row.try_get::<i64, _>(13).map_err(AppError::external)?)
                    .map_err(|_| "invalid web record question count".to_string())
                    .map_err(AppError::external)?;
            let turn_count =
                usize::try_from(row.try_get::<i64, _>(14).map_err(AppError::external)?)
                    .map_err(|_| "invalid web record turn count".to_string())
                    .map_err(AppError::external)?;
            Ok(ConversationSessionListItem {
                session: map_sqlx_conversation_session(row).map_err(AppError::external)?,
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

    let turn_rows = sqlx::query(
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
    for row in &turn_rows {
        let question_id = row.try_get(11).map_err(AppError::external)?;
        turns_by_question
            .entry(question_id)
            .or_default()
            .push(map_sqlx_conversation_turn(row).map_err(AppError::external)?);
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
    super::memory_repo::mark_memory_evidence_source_unavailable_for_session_tx(
        tx,
        tenant_id,
        crate::backend::models::MemoryEvidenceRecordKind::Web,
        session_id,
    )
    .await?;
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

async fn web_record_session_is_unchanged_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session: &ConversationSession,
    normalized: &NormalizedConversationSession,
) -> AppResult<bool> {
    let Some(source_fingerprint) = session.source_fingerprint.as_deref() else {
        return Ok(false);
    };
    let Some(row) = sqlx::query(
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

    let title: String = row.try_get(0).map_err(AppError::external)?;
    let started_at: Option<String> = row.try_get(1).map_err(AppError::external)?;
    let updated_at: Option<String> = row.try_get(2).map_err(AppError::external)?;
    let source_locator: Option<String> = row.try_get(3).map_err(AppError::external)?;
    let existing_fingerprint: Option<String> = row.try_get(4).map_err(AppError::external)?;
    let missing: i64 = row.try_get(5).map_err(AppError::external)?;

    Ok(title == session.title
        && started_at == session.started_at
        && updated_at == session.updated_at
        && source_locator == session.source_locator
        && existing_fingerprint.as_deref() == Some(source_fingerprint)
        && missing == 0
        && web_record_session_turns_are_unchanged_sqlx_tx(tx, tenant_id, &session.id, normalized)
            .await?)
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

async fn web_record_session_turns_are_unchanged_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    normalized: &NormalizedConversationSession,
) -> AppResult<bool> {
    let rows = sqlx::query(
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
        let external_id: String = row.try_get(0).map_err(AppError::external)?;
        let fingerprint: String = row.try_get(1).map_err(AppError::external)?;
        let missing: i64 = row.try_get(2).map_err(AppError::external)?;
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

async fn load_web_record_part_translation_state_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> AppResult<BTreeMap<String, (Option<String>, Option<String>, Option<String>)>> {
    let rows = sqlx::query(
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
    rows.iter()
        .map(|row| {
            Ok((
                row.try_get(0).map_err(AppError::external)?,
                (
                    row.try_get(1).map_err(AppError::external)?,
                    row.try_get(2).map_err(AppError::external)?,
                    row.try_get(3).map_err(AppError::external)?,
                ),
            ))
        })
        .collect()
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
mod tests {
    use super::*;
    use crate::backend::dto::{ConversationRecordKind, ConversationSearchCardType};
    use crate::backend::models::{
        ConversationContentCardDescriptor, ConversationPartKind, ConversationPartRole,
        ConversationSourceKind, NormalizedConversationPart, NormalizedConversationTurn,
    };
    use crate::backend::store::Database;
    use uuid::Uuid;

    const TEST_TENANT_ID: &str = "default";

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
                    TEST_TENANT_ID,
                    &source,
                )
                .await?;
                super::super::conversation_repo::import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[fixture_session()],
                    false,
                )
                .await?;
                let legacy_count_before_import =
                    count_legacy_conversation_sessions_sqlx(database.pool(), &source.id)
                        .await
                        .map_err(AppError::external)?;
                import_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[fixture_session()],
                    false,
                )
                .await?;
                let legacy_count_after_import =
                    count_legacy_conversation_sessions_sqlx(database.pool(), &source.id).await?;
                let sessions = list_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    None,
                    Some(&source.id),
                    None,
                    20,
                    0,
                )
                .await?;
                let detail = load_web_record_session_detail_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &sessions[0].session.id,
                )
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
        assert!(detail.questions[0]
            .projected_content_nodes
            .iter()
            .any(|node| node.content == "Web answer"));

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_web_record_sessions_do_not_store_project_paths() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-web-record-no-project-path-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let source = fixture_source();
        let mut session = fixture_session();
        session.project_path = Some("/tmp/web-project".to_string());

        let (columns, sessions, detail) = database
            .block_on(async {
                super::super::conversation_repo::upsert_conversation_source_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                )
                .await?;
                import_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[session],
                    false,
                )
                .await
                .map_err(AppError::external)?;
                let columns = sqlx::query_scalar::<_, String>(
                    "SELECT name FROM pragma_table_info('web_record_sessions')",
                )
                .fetch_all(database.pool())
                .await
                .map_err(AppError::external)?;
                let sessions = list_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    None,
                    Some(&source.id),
                    None,
                    20,
                    0,
                )
                .await?;
                let detail = load_web_record_session_detail_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &sessions[0].session.id,
                )
                .await?;
                AppResult::Ok((columns, sessions, detail))
            })
            .expect("import web records without persisting project paths");

        assert!(!columns.iter().any(|column| column == "project_path"));
        assert_eq!(sessions[0].session.project_path, None);
        assert_eq!(detail.session.project_path, None);

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_web_record_legacy_cleanup_is_tenant_scoped() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-web-record-legacy-cleanup-tenant-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let tenant_alpha = "tenant-alpha";
        let tenant_beta = "tenant-beta";
        let source = fixture_source();

        let (beta_before, beta_after, alpha_legacy_count) = database
            .block_on(async {
                for tenant_id in [tenant_alpha, tenant_beta] {
                    super::super::conversation_repo::upsert_conversation_source_sqlx(
                        database.pool(),
                        tenant_id,
                        &source,
                    )
                    .await
                    .map_err(AppError::external)?;
                    super::super::conversation_repo::import_conversation_sessions_sqlx(
                        database.pool(),
                        tenant_id,
                        &source,
                        &[fixture_session()],
                        false,
                    )
                    .await
                    .map_err(AppError::external)?;
                }

                let beta_sessions =
                    super::super::conversation_repo::list_conversation_sessions_sqlx(
                        database.pool(),
                        tenant_beta,
                        None,
                        Some(&source.id),
                        None,
                        20,
                        0,
                    )
                    .await
                    .map_err(AppError::external)?;
                let beta_detail =
                    super::super::conversation_repo::load_conversation_session_detail_sqlx(
                        database.pool(),
                        tenant_beta,
                        &beta_sessions[0].session.id,
                    )
                    .await
                    .map_err(AppError::external)?;
                let beta_before = (
                    beta_detail.questions[0].turns.len(),
                    beta_detail.questions[0].parts.len(),
                );

                import_web_record_sessions_sqlx(
                    database.pool(),
                    tenant_alpha,
                    &source,
                    &[fixture_session()],
                    false,
                )
                .await?;

                let beta_detail =
                    super::super::conversation_repo::load_conversation_session_detail_sqlx(
                        database.pool(),
                        tenant_beta,
                        &beta_sessions[0].session.id,
                    )
                    .await
                    .map_err(AppError::external)?;
                let beta_after = (
                    beta_detail.questions[0].turns.len(),
                    beta_detail.questions[0].parts.len(),
                );
                let alpha_legacy_count =
                    super::super::conversation_repo::list_conversation_sessions_sqlx(
                        database.pool(),
                        tenant_alpha,
                        None,
                        Some(&source.id),
                        None,
                        20,
                        0,
                    )
                    .await
                    .map_err(AppError::external)?
                    .len();

                AppResult::Ok((beta_before, beta_after, alpha_legacy_count))
            })
            .expect("web record legacy cleanup stays tenant-scoped");

        assert_eq!(beta_before, (1, 1));
        assert_eq!(beta_after, (1, 1));
        assert_eq!(alpha_legacy_count, 0);

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
                    TEST_TENANT_ID,
                    &source,
                )
                .await
                .map_err(|error| error.to_string())?;
                import_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[session.clone()],
                    false,
                )
                .await
                .map_err(|error| error.to_string())?;
                sqlx::query(
                    "UPDATE web_record_sessions SET imported_at = 'preserved' WHERE source_id = ?1",
                )
                .bind(&source.id)
                .execute(database.pool())
                .await
                .map_err(|error| error.to_string())?;
                import_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[session],
                    false,
                )
                .await
                .map_err(|error| error.to_string())?;
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
    fn conversation_incremental_web_import_retains_sessions_omitted_by_source() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-web-record-import-retain-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let source = fixture_source();
        let current_session = fixture_session();
        let mut archived_session = fixture_session();
        archived_session.external_id = "archived-web-session".to_string();
        archived_session.title = Some("Archived web fixture".to_string());

        let (listed, retained_detail) = database
            .block_on(async {
                super::super::conversation_repo::upsert_conversation_source_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                )
                .await
                .map_err(AppError::external)?;
                import_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[current_session.clone(), archived_session],
                    false,
                )
                .await
                .map_err(AppError::external)?;
                import_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[current_session],
                    false,
                )
                .await?;
                let listed = list_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    None,
                    Some(&source.id),
                    None,
                    20,
                    0,
                )
                .await?;
                let retained_id =
                    stable_id("web-record-session", &[&source.id, "archived-web-session"]);
                let retained_detail = load_web_record_session_detail_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &retained_id,
                )
                .await?;
                AppResult::Ok((listed, retained_detail))
            })
            .expect("retain omitted web record sessions through SQLx");

        assert_eq!(listed.len(), 2);
        assert!(listed
            .iter()
            .any(|item| item.session.external_id == "archived-web-session"));
        assert_eq!(retained_detail.session.external_id, "archived-web-session");
        assert_eq!(retained_detail.questions.len(), 1);

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

        let (result, imported_at, metadata_json, fts_row_count) = database
            .block_on(async {
                super::super::conversation_repo::upsert_conversation_source_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                )
                .await
                .map_err(AppError::external)?;
                import_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[old_session],
                    false,
                )
                .await
                .map_err(AppError::external)?;
                let session_id = stable_id("web-record-session", &[&source.id, "web-session-1"]);
                sqlx::query(
                    r#"
                    INSERT INTO conversation_question_fts (
                        tenant_id, question_id, session_id, question_text, answer_text,
                        code_text, command_text
                    ) VALUES (?1, 'web-record-question-stale', ?2, '', 'stale', '', '')
                    "#,
                )
                .bind(TEST_TENANT_ID)
                .bind(&session_id)
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;
                sqlx::query(
                    "UPDATE web_record_sessions SET imported_at = 'preserved' WHERE source_id = ?1",
                )
                .bind(&source.id)
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;
                let result = import_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
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
                .map_err(AppError::external)?;
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
                .map_err(AppError::external)?;
                let fts_row_count = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM conversation_question_fts WHERE tenant_id = ?1 AND session_id = ?2",
                )
                .bind(TEST_TENANT_ID)
                .bind(&session_id)
                .fetch_one(database.pool())
                .await
                .map_err(AppError::external)?;
                AppResult::Ok((result, imported_at, metadata_json, fts_row_count))
            })
            .expect("refresh normalized web parts through SQLx");

        assert_eq!(result.skipped_session_count, 0);
        assert_ne!(imported_at, "preserved");
        assert!(metadata_json
            .as_deref()
            .unwrap_or("")
            .contains(r#""content_card""#));
        assert_eq!(fts_row_count, 1);

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_web_record_reads_filter_detail() {
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

        let (sessions, detail) = database
            .block_on(async {
                super::super::conversation_repo::upsert_conversation_source_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                )
                .await
                .map_err(AppError::external)?;
                import_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[first, second],
                    false,
                )
                .await
                .map_err(AppError::external)?;
                let sessions = list_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    None,
                    Some(&source.id),
                    Some("sqlx answer"),
                    20,
                    0,
                )
                .await?;
                let detail = load_web_record_session_detail_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &sessions[0].session.id,
                )
                .await?;
                AppResult::Ok((sessions, detail))
            })
            .expect("read web records through SQLx");

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session.title, "SQLx migration notes");
        assert_eq!(sessions[0].question_count, 1);
        assert_eq!(sessions[0].turn_count, 1);
        assert_eq!(detail.questions.len(), 1);
        assert_eq!(detail.questions[0].turns.len(), 1);
        assert_eq!(detail.questions[0].parts.len(), 1);
        assert_eq!(detail.questions[0].question_turns.len(), 1);
        assert_eq!(
            detail.questions[0].question_turns[0].turn_id,
            detail.questions[0].turns[0].id
        );

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_web_record_lists_sessions_by_display_id_fragment() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-web-record-id-fragment-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let source = fixture_source();

        let (fragment_matches, direct_fragment_matches, full_matches) = database
            .block_on(async {
                super::super::conversation_repo::upsert_conversation_source_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                )
                .await
                .map_err(AppError::external)?;
                import_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[fixture_session()],
                    false,
                )
                .await
                .map_err(AppError::external)?;
                let session_id = stable_id("web-record-session", &[&source.id, "web-session-1"]);
                let fragment = crate::backend::models::conversation_id_fragment(&session_id);
                let fragment_matches = list_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    None,
                    Some(&source.id),
                    Some(&fragment),
                    20,
                    0,
                )
                .await?;
                let direct_fragment_matches = super::super::conversation_repo::list_conversation_sessions_by_id_fragment_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    crate::backend::dto::ConversationRecordKind::Web,
                    None,
                    Some(&source.id),
                    &fragment,
                    20,
                    0,
                )
                .await
                .map_err(AppError::external)?;
                let full_matches = list_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    None,
                    Some(&source.id),
                    Some(&session_id),
                    20,
                    0,
                )
                .await?;
                AppResult::Ok((fragment_matches, direct_fragment_matches, full_matches))
            })
            .expect("list web record sessions by display id fragment");

        assert_eq!(fragment_matches.len(), 1);
        assert_eq!(direct_fragment_matches.len(), 1);
        assert_eq!(full_matches.len(), 1);
        assert_eq!(full_matches[0].session.external_id, "web-session-1");

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
                command_label: None,
                source_execution_id: None,
                content_card: None,
                metadata_json: content_card_metadata("answer"),
            }],
        });

        let detail = database
            .block_on(async {
                super::super::conversation_repo::upsert_conversation_source_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                )
                .await
                .map_err(AppError::external)?;
                import_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[session],
                    false,
                )
                .await
                .map_err(AppError::external)?;
                let sessions = list_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    None,
                    Some(&source.id),
                    None,
                    20,
                    0,
                )
                .await?;
                load_web_record_session_detail_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &sessions[0].session.id,
                )
                .await
            })
            .expect("aggregate declared web content cards through SQLx");

        assert!(!detail.questions[0]
            .projected_content_nodes
            .iter()
            .any(|node| node.semantic_role.as_deref() == Some("answer")));
        assert_eq!(
            detail.questions[1]
                .projected_content_nodes
                .iter()
                .find(|node| node.semantic_role.as_deref() == Some("answer"))
                .map(|node| node.content.as_str()),
            Some("declared web answer")
        );

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_web_records_are_isolated_by_tenant() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-web-record-tenant-isolation-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let tenant_alpha = "tenant-alpha";
        let tenant_beta = "tenant-beta";
        let source = fixture_source();
        let mut alpha_session = fixture_session();
        alpha_session.turns[0].parts[0].text = Some("alpha web answer".to_string());
        let mut beta_session = fixture_session();
        beta_session.turns[0].parts[0].text = Some("beta web answer".to_string());

        let (session_id, alpha_detail, beta_detail, alpha_page, beta_page) = database
            .block_on(async {
                for tenant_id in [tenant_alpha, tenant_beta] {
                    super::super::conversation_repo::upsert_conversation_source_sqlx(
                        database.pool(),
                        tenant_id,
                        &source,
                    )
                    .await
                    .map_err(AppError::external)?;
                }
                import_web_record_sessions_sqlx(
                    database.pool(),
                    tenant_alpha,
                    &source,
                    &[alpha_session],
                    false,
                )
                .await
                .map_err(AppError::external)?;
                import_web_record_sessions_sqlx(
                    database.pool(),
                    tenant_beta,
                    &source,
                    &[beta_session],
                    false,
                )
                .await?;

                let alpha_sessions = list_web_record_sessions_sqlx(
                    database.pool(),
                    tenant_alpha,
                    None,
                    Some(&source.id),
                    Some("alpha web"),
                    20,
                    0,
                )
                .await?;
                let beta_sessions = list_web_record_sessions_sqlx(
                    database.pool(),
                    tenant_beta,
                    None,
                    Some(&source.id),
                    Some("beta web"),
                    20,
                    0,
                )
                .await?;
                let session_id = alpha_sessions[0].session.id.clone();
                assert_eq!(beta_sessions[0].session.id, session_id);
                let alpha_detail =
                    load_web_record_session_detail_sqlx(database.pool(), tenant_alpha, &session_id)
                        .await?;
                let beta_detail =
                    load_web_record_session_detail_sqlx(database.pool(), tenant_beta, &session_id)
                        .await?;
                let alpha_page = super::super::conversation_repo::search_conversation_cards_sqlx(
                    database.pool(),
                    tenant_alpha,
                    ConversationRecordKind::Web,
                    Some(&source.adapter_id),
                    Some(&source.id),
                    None,
                    "beta web answer",
                    &[ConversationSearchCardType::answer()],
                    &[],
                    false,
                    true,
                    None,
                    None,
                    false,
                    20,
                    0,
                    None,
                )
                .await
                .map_err(AppError::external)?;
                let beta_page = super::super::conversation_repo::search_conversation_cards_sqlx(
                    database.pool(),
                    tenant_beta,
                    ConversationRecordKind::Web,
                    Some(&source.adapter_id),
                    Some(&source.id),
                    None,
                    "alpha web answer",
                    &[ConversationSearchCardType::answer()],
                    &[],
                    false,
                    true,
                    None,
                    None,
                    false,
                    20,
                    0,
                    None,
                )
                .await
                .map_err(AppError::external)?;
                AppResult::Ok((session_id, alpha_detail, beta_detail, alpha_page, beta_page))
            })
            .expect("isolate web records by tenant");

        assert_eq!(alpha_detail.session.id, session_id);
        assert_eq!(beta_detail.session.id, session_id);
        assert!(alpha_detail.questions[0]
            .projected_content_nodes
            .iter()
            .any(|node| node.content == "alpha web answer"));
        assert!(beta_detail.questions[0]
            .projected_content_nodes
            .iter()
            .any(|node| node.content == "beta web answer"));
        assert_eq!(alpha_page.total_count, 0);
        assert_eq!(beta_page.total_count, 0);

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_web_records_round_trip_structured_cards_and_preserve_translation() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-web-record-card-persistence-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let source = fixture_source();
        let mut first = fixture_session();
        first.turns[0].parts[0].content_card = Some(ConversationContentCardDescriptor {
            schema_version: 1,
            kind: "qwen-web.reasoning".to_string(),
            renderer: Some("markdown".to_string()),
        });
        let mut second = first.clone();
        second.turns[0].parts[0].content_card.as_mut().unwrap().kind =
            "qwen-web.analysis".to_string();

        let (part_id, detail) = database
            .block_on(async {
                super::super::conversation_repo::upsert_conversation_source_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                )
                .await
                .map_err(AppError::external)?;
                import_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[first],
                    false,
                )
                .await
                .map_err(AppError::external)?;
                let session_id = stable_id("web-record-session", &[&source.id, "web-session-1"]);
                let initial = load_web_record_session_detail_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &session_id,
                )
                .await?;
                let part_id = initial.questions[0].parts[0].id.clone();
                update_web_record_part_translation_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &part_id,
                    "网页译文",
                )
                .await?;
                import_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[second],
                    false,
                )
                .await?;
                let detail = load_web_record_session_detail_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &session_id,
                )
                .await?;
                AppResult::Ok((part_id, detail))
            })
            .expect("round trip web record card");

        let part = &detail.questions[0].parts[0];
        assert_eq!(part.id, part_id);
        assert_eq!(part.translated_text.as_deref(), Some("网页译文"));
        assert_eq!(
            part.content_card.as_ref().map(|card| card.kind.as_str()),
            Some("qwen-web.analysis")
        );
        assert_eq!(detail.questions[0].projected_content_nodes.len(), 1);
        assert_eq!(
            detail.questions[0].projected_content_nodes[0].part_id,
            part_id
        );
        assert_eq!(
            detail.questions[0].projected_content_nodes[0].renderer,
            crate::backend::dto::ConversationCardRenderer::Markdown
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
        .map_err(AppError::External)
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
                    command_label: None,
                    source_execution_id: None,
                    content_card: None,
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
