use crate::backend::models::{
    ConversationCardKindDefinition, MemoryDreamCandidateDraft, MemoryDreamCursor, MemoryDreamNote,
    MemoryDreamNoteDetail, MemoryDreamNoteStatus, MemoryDreamPersistInput,
    MemoryDreamQuestionDeltaRow, MemoryDreamState, MemoryEvidenceRecordKind,
    MemoryEvidenceSnapshot, MemoryExtraction, MemoryExtractionValidationStatus, MemoryItem,
    MemoryItemDetail, MemoryItemFilter, MemoryItemRevision, MemoryRawMemory, MemoryRecallEvidence,
    MemoryRecallQuestionRef, MemoryRevisionChangeKind, MemoryRunKind, MemoryRunTrigger,
    MemoryScope, NewMemoryEvidenceSnapshot, NewMemoryItem,
};
use crate::backend::runtime::{AppError, AppResult};
use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::{
    sqlite::SqliteRow, AssertSqlSafe, QueryBuilder, Row as SqlxRow, Sqlite, SqlitePool, Transaction,
};
use std::collections::{BTreeSet, HashSet};
use uuid::Uuid;

use super::codec::{
    decode_enum, decode_json, decode_optional_enum, encode_enum, encode_json, encode_optional_enum,
};
use super::conversation_repo::map_sqlx_conversation_part;
use crate::backend::projection::conversation_cards::project_conversation_content_cards;

pub(crate) async fn list_memory_recall_question_refs_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    scope: &MemoryScope,
    since: Option<&str>,
    until: Option<&str>,
    include_unavailable: bool,
    limit: usize,
    offset: usize,
) -> AppResult<(usize, Vec<MemoryRecallQuestionRef>)> {
    if scope.project_path.is_some() {
        return list_session_memory_recall_question_refs_sqlx(
            pool,
            tenant_id,
            scope,
            since,
            until,
            include_unavailable,
            limit,
            offset,
        )
        .await;
    }
    const RECALL_COUNT_SQL: &str = r#"
      WITH all_questions AS (
        SELECT 'session' AS record_kind, s.source_id, s.id AS session_id,
               s.title AS session_title, s.project_path, q.id AS question_id,
               ROW_NUMBER() OVER (PARTITION BY q.tenant_id, q.session_id ORDER BY q.created_at, q.id) - 1 AS question_index,
               q.created_at AS sort_time
        FROM conversation_questions q
        JOIN conversation_sessions s ON s.tenant_id=q.tenant_id AND s.id=q.session_id
        JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id
        WHERE q.tenant_id=?1 AND (?2 IS NULL OR s.adapter_id=?2)
          AND (?3 IS NULL OR s.source_id=?3) AND (?4 IS NULL OR s.project_path=?4)
          AND (?5 IS NULL OR s.id=?5) AND (?6=1 OR (s.missing=0 AND source.enabled=1))
          AND (?7 IS NULL OR q.created_at>=?7) AND (?8 IS NULL OR q.created_at<=?8)
        UNION ALL
        SELECT 'web', s.source_id, s.id, s.title, NULL, q.id,
               ROW_NUMBER() OVER (PARTITION BY q.tenant_id, q.session_id ORDER BY q.created_at, q.id) - 1 AS question_index,
               q.created_at
        FROM web_record_questions q
        JOIN web_record_sessions s ON s.tenant_id=q.tenant_id AND s.id=q.session_id
        JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id
        WHERE q.tenant_id=?1 AND (?2 IS NULL OR s.adapter_id=?2)
          AND (?3 IS NULL OR s.source_id=?3) AND ?4 IS NULL
          AND (?5 IS NULL OR s.id=?5) AND (?6=1 OR (s.missing=0 AND source.enabled=1))
          AND (?7 IS NULL OR q.created_at>=?7) AND (?8 IS NULL OR q.created_at<=?8)
      )
      SELECT COUNT(*) FROM all_questions
    "#;
    let total_count: i64 = sqlx::query(RECALL_COUNT_SQL)
        .bind(tenant_id)
        .bind(&scope.app_id)
        .bind(&scope.source_id)
        .bind(&scope.project_path)
        .bind(&scope.session_id)
        .bind(if include_unavailable { 1_i64 } else { 0_i64 })
        .bind(since)
        .bind(until)
        .fetch_one(pool)
        .await
        .map_err(AppError::external)?
        .try_get(0)
        .map_err(AppError::external)?;
    const RECALL_PAGE_SQL: &str = r#"
      WITH all_questions AS (
        SELECT 'session' AS record_kind, s.source_id, s.id AS session_id,
               s.title AS session_title, s.project_path, q.id AS question_id,
               ROW_NUMBER() OVER (PARTITION BY q.tenant_id, q.session_id ORDER BY q.created_at, q.id) - 1 AS question_index,
               q.created_at AS sort_time
        FROM conversation_questions q
        JOIN conversation_sessions s ON s.tenant_id=q.tenant_id AND s.id=q.session_id
        JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id
        WHERE q.tenant_id=?1 AND (?2 IS NULL OR s.adapter_id=?2)
          AND (?3 IS NULL OR s.source_id=?3) AND (?4 IS NULL OR s.project_path=?4)
          AND (?5 IS NULL OR s.id=?5) AND (?6=1 OR (s.missing=0 AND source.enabled=1))
          AND (?7 IS NULL OR q.created_at>=?7) AND (?8 IS NULL OR q.created_at<=?8)
        UNION ALL
        SELECT 'web', s.source_id, s.id, s.title, NULL, q.id,
               ROW_NUMBER() OVER (PARTITION BY q.tenant_id, q.session_id ORDER BY q.created_at, q.id) - 1 AS question_index,
               q.created_at
        FROM web_record_questions q
        JOIN web_record_sessions s ON s.tenant_id=q.tenant_id AND s.id=q.session_id
        JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id
        WHERE q.tenant_id=?1 AND (?2 IS NULL OR s.adapter_id=?2)
          AND (?3 IS NULL OR s.source_id=?3) AND ?4 IS NULL
          AND (?5 IS NULL OR s.id=?5) AND (?6=1 OR (s.missing=0 AND source.enabled=1))
          AND (?7 IS NULL OR q.created_at>=?7) AND (?8 IS NULL OR q.created_at<=?8)
      )
      SELECT record_kind,source_id,session_id,session_title,project_path,question_id,question_index
      FROM all_questions
      ORDER BY sort_time DESC,record_kind,session_id,question_index,question_id
      LIMIT ?9 OFFSET ?10
    "#;
    let rows = sqlx::query(RECALL_PAGE_SQL)
        .bind(tenant_id)
        .bind(&scope.app_id)
        .bind(&scope.source_id)
        .bind(&scope.project_path)
        .bind(&scope.session_id)
        .bind(if include_unavailable { 1_i64 } else { 0_i64 })
        .bind(since)
        .bind(until)
        .bind(
            i64::try_from(limit)
                .map_err(|_| AppError::Validation("invalid Recall limit".to_string()))?,
        )
        .bind(
            i64::try_from(offset)
                .map_err(|_| AppError::Validation("invalid Recall offset".to_string()))?,
        )
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    let total = usize::try_from(total_count)
        .map_err(|_| AppError::Validation("invalid Recall question count".to_string()))?;
    let selected = rows
        .iter()
        .map(map_memory_recall_question_ref)
        .collect::<AppResult<Vec<_>>>()?;
    Ok((total, selected))
}

async fn list_session_memory_recall_question_refs_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    scope: &MemoryScope,
    since: Option<&str>,
    until: Option<&str>,
    include_unavailable: bool,
    limit: usize,
    offset: usize,
) -> AppResult<(usize, Vec<MemoryRecallQuestionRef>)> {
    let mut count = QueryBuilder::<Sqlite>::new(
        "SELECT COUNT(*) FROM conversation_questions q JOIN conversation_sessions s ON s.tenant_id=q.tenant_id AND s.id=q.session_id JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id WHERE q.tenant_id=",
    );
    count.push_bind(tenant_id);
    push_session_recall_scope(&mut count, scope, since, until, include_unavailable);
    let total_count = count
        .build_query_scalar::<i64>()
        .fetch_one(pool)
        .await
        .map_err(AppError::external)?;

    let mut page = QueryBuilder::<Sqlite>::new(
        "SELECT 'session' AS record_kind,s.source_id,s.id AS session_id,s.title AS session_title,s.project_path,q.id AS question_id,ROW_NUMBER() OVER (PARTITION BY q.tenant_id, q.session_id ORDER BY q.created_at, q.id) - 1 AS question_index FROM conversation_questions q JOIN conversation_sessions s ON s.tenant_id=q.tenant_id AND s.id=q.session_id JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id WHERE q.tenant_id=",
    );
    page.push_bind(tenant_id);
    push_session_recall_scope(&mut page, scope, since, until, include_unavailable);
    page.push(" ORDER BY q.created_at DESC,s.id,q.created_at,q.id LIMIT ");
    page.push_bind(
        i64::try_from(limit)
            .map_err(|_| AppError::Validation("invalid Recall limit".to_string()))?,
    );
    page.push(" OFFSET ");
    page.push_bind(
        i64::try_from(offset)
            .map_err(|_| AppError::Validation("invalid Recall offset".to_string()))?,
    );
    let rows = page
        .build()
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    Ok((
        usize::try_from(total_count)
            .map_err(|_| AppError::Validation("invalid Recall question count".to_string()))?,
        rows.iter()
            .map(map_memory_recall_question_ref)
            .collect::<AppResult<Vec<_>>>()?,
    ))
}

fn push_session_recall_scope(
    query: &mut QueryBuilder<Sqlite>,
    scope: &MemoryScope,
    since: Option<&str>,
    until: Option<&str>,
    include_unavailable: bool,
) {
    if let Some(app_id) = &scope.app_id {
        query.push(" AND s.adapter_id=").push_bind(app_id);
    }
    if let Some(source_id) = &scope.source_id {
        query.push(" AND s.source_id=").push_bind(source_id);
    }
    if let Some(project_path) = &scope.project_path {
        query.push(" AND s.project_path=").push_bind(project_path);
    }
    if let Some(session_id) = &scope.session_id {
        query.push(" AND s.id=").push_bind(session_id);
    }
    if !include_unavailable {
        query.push(" AND s.missing=0 AND source.enabled=1");
    }
    if let Some(since) = since {
        query.push(" AND q.created_at>=").push_bind(since);
    }
    if let Some(until) = until {
        query.push(" AND q.created_at<=").push_bind(until);
    }
}

fn map_memory_recall_question_ref(row: &SqliteRow) -> AppResult<MemoryRecallQuestionRef> {
    let kind: String = row.try_get("record_kind").map_err(AppError::external)?;
    Ok(MemoryRecallQuestionRef {
        record_kind: if kind == "web" {
            MemoryEvidenceRecordKind::Web
        } else {
            MemoryEvidenceRecordKind::Session
        },
        source_id: row.try_get("source_id").map_err(AppError::external)?,
        session_id: row.try_get("session_id").map_err(AppError::external)?,
        session_title: row.try_get("session_title").map_err(AppError::external)?,
        project_path: row.try_get("project_path").map_err(AppError::external)?,
        question_id: row.try_get("question_id").map_err(AppError::external)?,
        question_index: row.try_get("question_index").map_err(AppError::external)?,
    })
}

pub(crate) async fn create_memory_recall_run_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
    kind: MemoryRunKind,
    scope: &MemoryScope,
    source_revision: i64,
    provider: &str,
    model: Option<&str>,
    total_count: usize,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    let scope_json = encode_json(scope)?;
    let fingerprint = scope.fingerprint().map_err(AppError::external)?;
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let locked: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM memory_runs WHERE tenant_id=?1 AND scope_fingerprint=?2 AND status IN ('queued','running') AND kind IN ('auto_dream','deep_recall','full_organize'))",
    ).bind(tenant_id).bind(&fingerprint).fetch_one(&mut *tx).await.map_err(AppError::external)?;
    if locked == 1 {
        return Err(AppError::Validation(
            "memory scope is already locked by another running task".to_string(),
        ));
    }
    sqlx::query(
        r#"INSERT INTO memory_runs (
          tenant_id,id,kind,trigger_kind,scope_json,scope_fingerprint,source_revision_start,
          provider,model,prompt_version,phase,processed_count,total_count,skipped_count,
          failed_count,status,started_at,created_at,updated_at
        ) VALUES (?1,?2,?3,'user_question',?4,?5,?6,?7,?8,'memory-recall-v1','phase1',0,?9,0,0,'running',?10,?10,?10)"#,
    ).bind(tenant_id).bind(run_id).bind(encode_enum(kind)?).bind(scope_json).bind(fingerprint)
      .bind(source_revision).bind(provider).bind(model)
      .bind(i64::try_from(total_count).map_err(|_| AppError::Validation("invalid Recall total".to_string()))?)
      .bind(now).execute(&mut *tx).await.map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)
}

pub(crate) async fn persist_memory_extraction_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
    batch_index: usize,
    scope: &MemoryScope,
    raw_memories: &[MemoryRawMemory],
    session_summary: &str,
    question_count: usize,
    input_char_count: usize,
    attempt_count: usize,
    evidence: &[MemoryRecallEvidence],
) -> AppResult<MemoryExtraction> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let now_text = now.to_rfc3339();
    let expires_at = (now + chrono::Duration::days(30)).to_rfc3339();
    let scope_json = encode_json(scope)?;
    let fingerprint = scope.fingerprint().map_err(AppError::external)?;
    let raw_json = serde_json::to_string(raw_memories).map_err(AppError::external)?;
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let running: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM memory_runs WHERE tenant_id=?1 AND id=?2 AND status='running')")
        .bind(tenant_id).bind(run_id).fetch_one(&mut *tx).await.map_err(AppError::external)?;
    if running != 1 {
        return Err(AppError::Conflict(format!(
            "Memory Recall run {run_id} is not running"
        )));
    }
    sqlx::query(
        r#"INSERT INTO memory_extractions (
      tenant_id,id,run_id,batch_index,scope_json,scope_fingerprint,raw_memories_json,
      session_summary,question_count,input_char_count,evidence_count,validation_status,
      attempt_count,expires_at,created_at,updated_at
    ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'valid',?12,?13,?14,?14)"#,
    )
    .bind(tenant_id)
    .bind(&id)
    .bind(run_id)
    .bind(
        i64::try_from(batch_index)
            .map_err(|_| AppError::Validation("invalid extraction batch".to_string()))?,
    )
    .bind(scope_json)
    .bind(fingerprint)
    .bind(raw_json)
    .bind(session_summary)
    .bind(
        i64::try_from(question_count)
            .map_err(|_| AppError::Validation("invalid extraction questions".to_string()))?,
    )
    .bind(
        i64::try_from(input_char_count)
            .map_err(|_| AppError::Validation("invalid extraction input".to_string()))?,
    )
    .bind(
        i64::try_from(evidence.len())
            .map_err(|_| AppError::Validation("invalid extraction evidence".to_string()))?,
    )
    .bind(
        i64::try_from(attempt_count)
            .map_err(|_| AppError::Validation("invalid extraction attempts".to_string()))?,
    )
    .bind(expires_at)
    .bind(&now_text)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    let mut seen = HashSet::new();
    for (sort_order, item) in evidence.iter().enumerate() {
        validate_evidence(&item.snapshot)?;
        let row = sqlx::query(UPSERT_MEMORY_EVIDENCE_SQL)
            .bind(tenant_id)
            .bind(Uuid::new_v4().to_string())
            .bind(encode_enum(item.snapshot.record_kind)?)
            .bind(&item.snapshot.source_id)
            .bind(&item.snapshot.session_id)
            .bind(&item.snapshot.question_id)
            .bind(&item.snapshot.turn_id)
            .bind(&item.snapshot.part_id)
            .bind(&item.snapshot.block_id)
            .bind(&item.snapshot.content_hash)
            .bind(&item.snapshot.excerpt)
            .bind(&item.snapshot.translated_excerpt)
            .bind(&item.snapshot.event_time)
            .bind(item.snapshot.source_revision)
            .bind(bool_value(item.snapshot.source_unavailable))
            .bind(&now_text)
            .fetch_one(&mut *tx)
            .await
            .map_err(AppError::external)?;
        let evidence_id: String = row.try_get(0).map_err(AppError::external)?;
        if !seen.insert(evidence_id.clone()) {
            continue;
        }
        let order = i64::try_from(sort_order)
            .map_err(|_| AppError::Validation("invalid evidence order".to_string()))?;
        sqlx::query("INSERT INTO memory_extraction_evidence (tenant_id,extraction_id,evidence_id,sort_order) VALUES (?1,?2,?3,?4)")
            .bind(tenant_id).bind(&id).bind(&evidence_id).bind(order).execute(&mut *tx).await.map_err(AppError::external)?;
        sqlx::query("INSERT OR IGNORE INTO memory_run_evidence (tenant_id,run_id,evidence_id,sort_order) VALUES (?1,?2,?3,?4)")
            .bind(tenant_id).bind(run_id).bind(evidence_id).bind(order).execute(&mut *tx).await.map_err(AppError::external)?;
    }
    sqlx::query("UPDATE memory_runs SET processed_count=processed_count+?1, updated_at=?2 WHERE tenant_id=?3 AND id=?4")
        .bind(i64::try_from(question_count).map_err(|_| AppError::Validation("invalid processed count".to_string()))?)
        .bind(&now_text).bind(tenant_id).bind(run_id).execute(&mut *tx).await.map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)?;
    Ok(MemoryExtraction {
        id,
        run_id: run_id.to_string(),
        batch_index,
        raw_memories: raw_memories.to_vec(),
        session_summary: session_summary.to_string(),
        question_count,
        input_char_count,
        evidence_count: evidence.len(),
        validation_status: MemoryExtractionValidationStatus::Valid,
        attempt_count,
        error_message: None,
        created_at: now_text.clone(),
        updated_at: now_text,
    })
}

pub(crate) async fn load_memory_run_evidence_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
) -> AppResult<Vec<MemoryEvidenceSnapshot>> {
    let rows = sqlx::query(
        r#"SELECT e.id, e.record_kind, e.source_id, e.session_id, e.question_id, e.turn_id,
          e.part_id, e.block_id, e.content_hash, e.excerpt, e.translated_excerpt,
          e.event_time, e.source_revision, e.source_unavailable, e.created_at, e.updated_at
          FROM memory_evidence_snapshots e
      JOIN memory_run_evidence l ON l.tenant_id=e.tenant_id AND l.evidence_id=e.id
      WHERE l.tenant_id=?1 AND l.run_id=?2 ORDER BY l.sort_order,e.id"#,
    )
    .bind(tenant_id)
    .bind(run_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    rows.iter().map(map_memory_evidence).collect()
}

pub(crate) async fn persist_memory_recall_success_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
    candidates: &[(NewMemoryItem, Vec<String>)],
    result: &serde_json::Value,
    source_revision: i64,
    failed_count: usize,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let running: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM memory_runs WHERE tenant_id=?1 AND id=?2 AND status='running')",
    )
    .bind(tenant_id)
    .bind(run_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::external)?;
    if running != 1 {
        return Err(AppError::Conflict(format!(
            "Memory Recall run {run_id} is not running"
        )));
    }
    for (draft, evidence_ids) in candidates {
        insert_memory_item_tx(&mut tx, tenant_id, draft, evidence_ids).await?;
    }
    let updated = sqlx::query(
        r#"UPDATE memory_runs SET phase='completed',status='completed',source_revision_end=?1,
          failed_count=?2,result_json=?3,finished_at=?4,updated_at=?4
          WHERE tenant_id=?5 AND id=?6 AND status='running'"#,
    )
    .bind(source_revision)
    .bind(
        i64::try_from(failed_count)
            .map_err(|_| AppError::Validation("invalid failure count".to_string()))?,
    )
    .bind(encode_json(result)?)
    .bind(now)
    .bind(tenant_id)
    .bind(run_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    if updated.rows_affected() != 1 {
        return Err(AppError::Conflict(format!(
            "Memory Recall run {run_id} could not be completed"
        )));
    }
    tx.commit().await.map_err(AppError::external)
}

pub(crate) async fn fail_memory_recall_run_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
    message: &str,
    cancelled: bool,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE memory_runs SET phase='completed',status=?1,error_kind=?2,error_message=?3,finished_at=?4,updated_at=?4 WHERE tenant_id=?5 AND id=?6")
      .bind(if cancelled { "cancelled" } else { "failed" })
      .bind(if cancelled { "cancelled" } else { "recall_failed" })
      .bind(message).bind(now).bind(tenant_id).bind(run_id).execute(pool).await.map_err(AppError::external)?;
    Ok(())
}

pub(crate) async fn set_memory_run_phase_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
    phase: &str,
) -> AppResult<()> {
    if !matches!(phase, "phase1" | "phase2" | "finalizing") {
        return Err(AppError::Validation("invalid Memory run phase".to_string()));
    }
    sqlx::query("UPDATE memory_runs SET phase=?1,updated_at=?2 WHERE tenant_id=?3 AND id=?4 AND status='running'")
        .bind(phase).bind(Utc::now().to_rfc3339()).bind(tenant_id).bind(run_id)
        .execute(pool).await.map_err(AppError::external)?;
    Ok(())
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MemoryEvidenceRemapSummary {
    pub(crate) checked: usize,
    pub(crate) remapped: usize,
    pub(crate) audited: usize,
}

pub(crate) async fn reconcile_memory_evidence_for_session_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: MemoryEvidenceRecordKind,
    session_id: &str,
) -> AppResult<MemoryEvidenceRemapSummary> {
    let (questions_table, _, _, _, _) = memory_conversation_tables_full(record_kind);
    let question_ids = sqlx::query_scalar::<_, String>(AssertSqlSafe(format!(
        "SELECT id FROM {questions_table} WHERE tenant_id = ?1 AND session_id = ?2"
    )))
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let summary = reconcile_memory_evidence_for_session_tx(
        &mut tx,
        tenant_id,
        record_kind,
        session_id,
        &question_ids,
        &[],
    )
    .await?;
    tx.commit().await.map_err(AppError::external)?;
    Ok(summary)
}

pub(crate) async fn reconcile_memory_evidence_for_session_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    record_kind: MemoryEvidenceRecordKind,
    session_id: &str,
    affected_question_ids: &[String],
    question_mappings: &[(String, String)],
) -> AppResult<MemoryEvidenceRemapSummary> {
    let record_kind_label = memory_record_kind_label(record_kind);
    let (questions_table, _, turns_table, parts_table, question_turns_table) =
        memory_conversation_tables_full(record_kind);
    let rows = sqlx::query(
        r#"SELECT id, question_id, turn_id, part_id, block_id
           FROM memory_evidence_snapshots
           WHERE tenant_id = ?1 AND record_kind = ?2 AND session_id = ?3
           ORDER BY id"#,
    )
    .bind(tenant_id)
    .bind(record_kind_label)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::external)?;
    let now = Utc::now().to_rfc3339();
    let mut summary = MemoryEvidenceRemapSummary::default();

    for row in rows {
        summary.checked += 1;
        let evidence_id: String = row.try_get("id").map_err(AppError::external)?;
        let previous_question_id: Option<String> =
            row.try_get("question_id").map_err(AppError::external)?;
        let evidence_turn_id: Option<String> =
            row.try_get("turn_id").map_err(AppError::external)?;
        let evidence_part_id: Option<String> =
            row.try_get("part_id").map_err(AppError::external)?;
        let block_id: String = row.try_get("block_id").map_err(AppError::external)?;

        let mut candidates = BTreeSet::new();
        let mut reason = None;
        let mut fact_turn_id = evidence_turn_id.clone();
        if let Some(part_id) = evidence_part_id.as_deref() {
            if !memory_part_block_locator_matches(&block_id, part_id) {
                reason = Some("locator_mismatch");
            }
            let part_turn_id = sqlx::query_scalar::<_, String>(AssertSqlSafe(format!(
                "SELECT t.id FROM {parts_table} p JOIN {turns_table} t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id WHERE p.tenant_id = ?1 AND p.id = ?2 AND t.session_id = ?3"
            )))
            .bind(tenant_id)
            .bind(part_id)
            .bind(session_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(AppError::external)?;
            match part_turn_id {
                Some(part_turn_id) => {
                    if evidence_turn_id
                        .as_deref()
                        .is_some_and(|turn_id| turn_id != part_turn_id)
                    {
                        reason = Some("locator_mismatch");
                    }
                    fact_turn_id = Some(part_turn_id);
                }
                None => reason = Some("source_unavailable"),
            }
        } else if let Some(turn_id) = evidence_turn_id.as_deref() {
            if block_id != format!("{turn_id}-question") {
                reason = Some("locator_mismatch");
            }
        }

        if reason.is_none() {
            if let Some(turn_id) = fact_turn_id.as_deref() {
                let turn_exists = sqlx::query_scalar::<_, i64>(AssertSqlSafe(format!(
                    "SELECT 1 FROM {turns_table} WHERE tenant_id = ?1 AND id = ?2 AND session_id = ?3"
                )))
                .bind(tenant_id)
                .bind(turn_id)
                .bind(session_id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(AppError::external)?
                .is_some();
                if !turn_exists {
                    reason = Some("source_unavailable");
                } else {
                    let question_rows = sqlx::query(AssertSqlSafe(format!(
                        "SELECT qt.question_id FROM {question_turns_table} qt JOIN {questions_table} q ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id WHERE qt.tenant_id = ?1 AND qt.turn_id = ?2 AND q.session_id = ?3 ORDER BY qt.question_id"
                    )))
                    .bind(tenant_id)
                    .bind(turn_id)
                    .bind(session_id)
                    .fetch_all(&mut **tx)
                    .await
                    .map_err(AppError::external)?;
                    for question_row in question_rows {
                        candidates.insert(
                            question_row
                                .try_get::<String, _>(0)
                                .map_err(AppError::external)?,
                        );
                    }
                    if candidates.is_empty() {
                        reason = Some("question_missing");
                    } else if candidates.len() > 1 {
                        reason = Some("ambiguous_question");
                    }
                }
            } else if let Some(question_id) = previous_question_id.as_deref() {
                if let Some((_, target)) = question_mappings
                    .iter()
                    .find(|(source, _)| source == question_id)
                {
                    candidates.insert(target.clone());
                } else {
                    let question_exists = sqlx::query_scalar::<_, i64>(AssertSqlSafe(format!(
                        "SELECT 1 FROM {questions_table} WHERE tenant_id = ?1 AND id = ?2 AND session_id = ?3"
                    )))
                    .bind(tenant_id)
                    .bind(question_id)
                    .bind(session_id)
                    .fetch_optional(&mut **tx)
                    .await
                    .map_err(AppError::external)?
                    .is_some();
                    if question_exists && !affected_question_ids.iter().any(|id| id == question_id)
                    {
                        candidates.insert(question_id.to_string());
                    } else {
                        let current_question_ids = sqlx::query_scalar::<_, String>(AssertSqlSafe(format!(
                            "SELECT id FROM {questions_table} WHERE tenant_id = ?1 AND session_id = ?2 ORDER BY created_at, id"
                        )))
                        .bind(tenant_id)
                        .bind(session_id)
                        .fetch_all(&mut **tx)
                        .await
                        .map_err(AppError::external)?;
                        candidates.extend(current_question_ids);
                        reason = Some(if candidates.is_empty() {
                            "question_missing"
                        } else {
                            "ambiguous_question"
                        });
                    }
                }
            } else {
                reason = Some("locator_missing");
            }
        }

        if reason.is_none() && candidates.len() == 1 {
            let question_id = candidates
                .iter()
                .next()
                .expect("one evidence question candidate");
            sqlx::query(
                "UPDATE memory_evidence_snapshots SET question_id = ?1, source_unavailable = 0, updated_at = ?2 WHERE tenant_id = ?3 AND id = ?4",
            )
            .bind(question_id)
            .bind(&now)
            .bind(tenant_id)
            .bind(&evidence_id)
            .execute(&mut **tx)
            .await
            .map_err(AppError::external)?;
            sqlx::query(
                "UPDATE memory_evidence_remap_audits SET status = 'resolved', resolved_at = ?1 WHERE tenant_id = ?2 AND evidence_id = ?3 AND status = 'open'",
            )
            .bind(&now)
            .bind(tenant_id)
            .bind(&evidence_id)
            .execute(&mut **tx)
            .await
            .map_err(AppError::external)?;
            summary.remapped += 1;
        } else {
            let reason = reason.unwrap_or("locator_mismatch");
            record_memory_evidence_remap_audit_tx(
                tx,
                tenant_id,
                &evidence_id,
                record_kind_label,
                session_id,
                previous_question_id.as_deref(),
                evidence_turn_id.as_deref(),
                evidence_part_id.as_deref(),
                &block_id,
                reason,
                &candidates,
                &now,
            )
            .await?;
            summary.audited += 1;
        }
    }
    Ok(summary)
}

pub(crate) async fn mark_memory_evidence_source_unavailable_for_session_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    record_kind: MemoryEvidenceRecordKind,
    session_id: &str,
) -> AppResult<usize> {
    let record_kind_label = memory_record_kind_label(record_kind);
    let rows = sqlx::query(
        "SELECT id, question_id, turn_id, part_id, block_id FROM memory_evidence_snapshots WHERE tenant_id = ?1 AND record_kind = ?2 AND session_id = ?3",
    )
    .bind(tenant_id)
    .bind(record_kind_label)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::external)?;
    let now = Utc::now().to_rfc3339();
    for row in &rows {
        let evidence_id: String = row.try_get("id").map_err(AppError::external)?;
        let previous_question_id: Option<String> =
            row.try_get("question_id").map_err(AppError::external)?;
        let turn_id: Option<String> = row.try_get("turn_id").map_err(AppError::external)?;
        let part_id: Option<String> = row.try_get("part_id").map_err(AppError::external)?;
        let block_id: String = row.try_get("block_id").map_err(AppError::external)?;
        sqlx::query(
            "UPDATE memory_evidence_snapshots SET source_unavailable = 1, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3",
        )
        .bind(&now)
        .bind(tenant_id)
        .bind(&evidence_id)
        .execute(&mut **tx)
        .await
        .map_err(AppError::external)?;
        let candidates = BTreeSet::new();
        record_memory_evidence_remap_audit_tx(
            tx,
            tenant_id,
            &evidence_id,
            record_kind_label,
            session_id,
            previous_question_id.as_deref(),
            turn_id.as_deref(),
            part_id.as_deref(),
            &block_id,
            "source_unavailable",
            &candidates,
            &now,
        )
        .await?;
    }
    Ok(rows.len())
}

pub(crate) async fn mark_memory_evidence_source_unavailable_for_turns_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    record_kind: MemoryEvidenceRecordKind,
    session_id: &str,
    turn_ids: &[String],
) -> AppResult<usize> {
    let record_kind_label = memory_record_kind_label(record_kind);
    let (_, _, _, parts_table, _) = memory_conversation_tables_full(record_kind);
    let now = Utc::now().to_rfc3339();
    let mut marked = 0;
    for turn_id in turn_ids {
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT id, question_id, turn_id, part_id, block_id FROM memory_evidence_snapshots WHERE tenant_id = ?1 AND record_kind = ?2 AND session_id = ?3 AND (turn_id = ?4 OR part_id IN (SELECT id FROM {parts_table} WHERE tenant_id = ?1 AND turn_id = ?4))"
        )))
        .bind(tenant_id)
        .bind(record_kind_label)
        .bind(session_id)
        .bind(turn_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(AppError::external)?;
        for row in rows {
            let evidence_id: String = row.try_get("id").map_err(AppError::external)?;
            let previous_question_id: Option<String> =
                row.try_get("question_id").map_err(AppError::external)?;
            let evidence_turn_id: Option<String> =
                row.try_get("turn_id").map_err(AppError::external)?;
            let part_id: Option<String> = row.try_get("part_id").map_err(AppError::external)?;
            let block_id: String = row.try_get("block_id").map_err(AppError::external)?;
            sqlx::query(
                "UPDATE memory_evidence_snapshots SET source_unavailable = 1, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3",
            )
            .bind(&now)
            .bind(tenant_id)
            .bind(&evidence_id)
            .execute(&mut **tx)
            .await
            .map_err(AppError::external)?;
            record_memory_evidence_remap_audit_tx(
                tx,
                tenant_id,
                &evidence_id,
                record_kind_label,
                session_id,
                previous_question_id.as_deref(),
                evidence_turn_id.as_deref().or(Some(turn_id.as_str())),
                part_id.as_deref(),
                &block_id,
                "source_unavailable",
                &BTreeSet::new(),
                &now,
            )
            .await?;
            marked += 1;
        }
    }
    Ok(marked)
}

async fn record_memory_evidence_remap_audit_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    evidence_id: &str,
    record_kind: &str,
    session_id: &str,
    previous_question_id: Option<&str>,
    turn_id: Option<&str>,
    part_id: Option<&str>,
    block_id: &str,
    reason: &str,
    candidates: &BTreeSet<String>,
    detected_at: &str,
) -> AppResult<()> {
    let candidates_json = serde_json::to_string(&candidates.iter().collect::<Vec<_>>())
        .map_err(AppError::external)?;
    let updated = sqlx::query(
        r#"
        UPDATE memory_evidence_remap_audits
        SET reason = ?1, candidate_question_ids_json = ?2, detected_at = ?3,
            previous_question_id = ?4, turn_id = ?5, part_id = ?6, block_id = ?7,
            resolved_at = NULL
        WHERE tenant_id = ?8 AND evidence_id = ?9 AND status = 'open'
        "#,
    )
    .bind(reason)
    .bind(&candidates_json)
    .bind(detected_at)
    .bind(previous_question_id)
    .bind(turn_id)
    .bind(part_id)
    .bind(block_id)
    .bind(tenant_id)
    .bind(evidence_id)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    if updated.rows_affected() == 0 {
        sqlx::query(
            r#"
            INSERT INTO memory_evidence_remap_audits (
                tenant_id, id, evidence_id, record_kind, session_id,
                previous_question_id, turn_id, part_id, block_id, reason,
                candidate_question_ids_json, status, detected_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'open', ?12)
            "#,
        )
        .bind(tenant_id)
        .bind(Uuid::new_v4().to_string())
        .bind(evidence_id)
        .bind(record_kind)
        .bind(session_id)
        .bind(previous_question_id)
        .bind(turn_id)
        .bind(part_id)
        .bind(block_id)
        .bind(reason)
        .bind(candidates_json)
        .bind(detected_at)
        .execute(&mut **tx)
        .await
        .map_err(AppError::external)?;
    }
    Ok(())
}

fn memory_record_kind_label(record_kind: MemoryEvidenceRecordKind) -> &'static str {
    match record_kind {
        MemoryEvidenceRecordKind::Session => "session",
        MemoryEvidenceRecordKind::Web => "web",
    }
}

fn memory_part_block_locator_matches(block_id: &str, part_id: &str) -> bool {
    if block_id == part_id {
        return true;
    }
    block_id
        .strip_prefix(&format!("{part_id}-node-"))
        .is_some_and(|order| !order.is_empty() && order.chars().all(|value| value.is_ascii_digit()))
}

fn memory_conversation_tables_full(
    record_kind: MemoryEvidenceRecordKind,
) -> (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
) {
    match record_kind {
        MemoryEvidenceRecordKind::Session => (
            "conversation_questions",
            "conversation_sessions",
            "conversation_turns",
            "conversation_parts",
            "conversation_question_turns",
        ),
        MemoryEvidenceRecordKind::Web => (
            "web_record_questions",
            "web_record_sessions",
            "web_record_turns",
            "web_record_parts",
            "web_record_question_turns",
        ),
    }
}

pub(crate) async fn memory_evidence_stale_reason_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    evidence: &MemoryEvidenceSnapshot,
) -> AppResult<Option<crate::backend::models::MemoryStaleReason>> {
    use crate::backend::models::MemoryStaleReason;

    let session_sql = match evidence.record_kind {
        MemoryEvidenceRecordKind::Session => {
            "SELECT missing FROM conversation_sessions WHERE tenant_id=?1 AND id=?2"
        }
        MemoryEvidenceRecordKind::Web => {
            "SELECT missing FROM web_record_sessions WHERE tenant_id=?1 AND id=?2"
        }
    };
    let missing = sqlx::query_scalar::<_, i64>(session_sql)
        .bind(tenant_id)
        .bind(&evidence.session_id)
        .fetch_optional(pool)
        .await
        .map_err(AppError::external)?;
    match missing {
        None => return Ok(Some(MemoryStaleReason::EvidenceMissing)),
        Some(1) => return Ok(Some(MemoryStaleReason::SourceUnavailable)),
        _ => {}
    }

    let current_values = if evidence.part_id.is_some() {
        memory_evidence_projected_part_values(pool, tenant_id, evidence).await?
    } else if let Some(turn_id) = &evidence.turn_id {
        let sql = match evidence.record_kind {
            MemoryEvidenceRecordKind::Session => {
                "SELECT user_text FROM conversation_turns WHERE tenant_id=?1 AND id=?2"
            }
            MemoryEvidenceRecordKind::Web => {
                "SELECT user_text FROM web_record_turns WHERE tenant_id=?1 AND id=?2"
            }
        };
        sqlx::query_scalar::<_, String>(sql)
            .bind(tenant_id)
            .bind(turn_id)
            .fetch_optional(pool)
            .await
            .map_err(AppError::external)?
            .map(|value| vec![value])
    } else {
        None
    };
    let Some(current_values) = current_values else {
        return Ok(Some(MemoryStaleReason::EvidenceMissing));
    };
    let matches = current_values.into_iter().any(|current| {
        format!("sha256:{:x}", Sha256::digest(current.as_bytes())) == evidence.content_hash
    });
    Ok((!matches).then_some(MemoryStaleReason::EvidenceChanged))
}

async fn memory_evidence_projected_part_values(
    pool: &SqlitePool,
    tenant_id: &str,
    evidence: &MemoryEvidenceSnapshot,
) -> AppResult<Option<Vec<String>>> {
    let Some(part_id) = evidence.part_id.as_deref() else {
        return Ok(None);
    };
    let (_, sessions_table, turns_table, parts_table, _) =
        memory_conversation_tables_full(evidence.record_kind);
    let row = sqlx::query(AssertSqlSafe(format!(
        "SELECT p.id, p.turn_id, p.part_index, p.role, p.kind, p.text, p.language,\
         p.command, p.cwd, p.status, p.exit_code, p.metadata_json, p.content_card_json,\
         p.translated_text, p.source_execution_id, p.command_label\
         FROM {parts_table} p\
         JOIN {turns_table} t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id\
         JOIN {sessions_table} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id\
         WHERE p.tenant_id = ?1 AND p.id = ?2 AND s.id = ?3"
    )))
    .bind(tenant_id)
    .bind(part_id)
    .bind(&evidence.session_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let part = map_sqlx_conversation_part(&row)?;
    let adapter_id = sqlx::query_scalar::<_, String>(AssertSqlSafe(format!(
        "SELECT adapter_id FROM {sessions_table} WHERE tenant_id = ?1 AND id = ?2"
    )))
    .bind(tenant_id)
    .bind(&evidence.session_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::external)?;
    let card_kinds_json = sqlx::query_scalar::<_, String>(
        "SELECT card_kinds_json FROM conversation_adapters WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(&adapter_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?
    .unwrap_or_else(|| "[]".to_string());
    let card_kinds: Vec<ConversationCardKindDefinition> = decode_json(card_kinds_json)?;
    let cards = project_conversation_content_cards(&part, &adapter_id, &card_kinds)
        .map_err(AppError::external)?;
    let values = cards
        .iter()
        .filter(|card| card.node_id == evidence.block_id)
        .map(|card| card.body.clone())
        .collect::<Vec<_>>();
    if values.is_empty() {
        return Ok(None);
    }
    Ok(Some(values))
}

const MEMORY_ITEM_COLUMNS: &str = r#"
    id, kind, status, title, content_markdown, scope_json, scope_fingerprint,
    origin, origin_run_id, origin_dream_note_id, origin_extraction_id,
    confidence, supersedes_item_id, source_revision, verified_revision,
    stale_reason, created_at, updated_at
"#;

const MEMORY_DREAM_NOTE_COLUMNS: &str = r#"
    id, run_id, scope_json, scope_fingerprint, markdown, session_count,
    question_count, evidence_count, source_revision, status, created_at, updated_at
"#;

const UPSERT_MEMORY_EVIDENCE_SQL: &str = r#"
    INSERT INTO memory_evidence_snapshots (
        tenant_id, id, record_kind, source_id, session_id, question_id,
        turn_id, part_id, block_id, content_hash, excerpt,
        translated_excerpt, event_time, source_revision,
        source_unavailable, created_at, updated_at
    ) VALUES (
        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
        ?14, ?15, ?16, ?16
    )
    ON CONFLICT(tenant_id, record_kind, block_id, content_hash) DO UPDATE SET
        source_id = excluded.source_id,
        session_id = excluded.session_id,
        question_id = excluded.question_id,
        turn_id = excluded.turn_id,
        part_id = excluded.part_id,
        excerpt = excluded.excerpt,
        translated_excerpt = COALESCE(excluded.translated_excerpt, memory_evidence_snapshots.translated_excerpt),
        event_time = excluded.event_time,
        source_revision = MAX(memory_evidence_snapshots.source_revision, excluded.source_revision),
        source_unavailable = excluded.source_unavailable,
        updated_at = excluded.updated_at
    RETURNING id, record_kind, source_id, session_id, question_id, turn_id,
              part_id, block_id, content_hash, excerpt, translated_excerpt,
              event_time, source_revision, source_unavailable, created_at,
              updated_at
"#;

const LOAD_MEMORY_ITEM_SQL: &str = r#"
    SELECT id, kind, status, title, content_markdown, scope_json,
           scope_fingerprint, origin, origin_run_id, origin_dream_note_id,
           origin_extraction_id, confidence, supersedes_item_id,
           source_revision, verified_revision, stale_reason, created_at,
           updated_at
    FROM memory_items
    WHERE tenant_id = ?1 AND id = ?2
"#;

const LOAD_MEMORY_ITEM_EVIDENCE_SQL: &str = r#"
    SELECT e.id, e.record_kind, e.source_id, e.session_id, e.question_id,
           e.turn_id, e.part_id, e.block_id, e.content_hash, e.excerpt,
           e.translated_excerpt, e.event_time, e.source_revision,
           e.source_unavailable, e.created_at, e.updated_at
    FROM memory_item_evidence link
    JOIN memory_evidence_snapshots e
      ON e.tenant_id = link.tenant_id AND e.id = link.evidence_id
    WHERE link.tenant_id = ?1 AND link.item_id = ?2
    ORDER BY link.sort_order ASC, e.id ASC
"#;

const LOAD_MEMORY_ITEM_REVISIONS_SQL: &str = r#"
    SELECT id, item_id, revision_number, change_kind, kind, status, title,
           content_markdown, scope_json, scope_fingerprint, origin,
           confidence, supersedes_item_id, source_revision,
           verified_revision, stale_reason, changed_at
    FROM memory_item_revisions
    WHERE tenant_id = ?1 AND item_id = ?2
    ORDER BY revision_number DESC
"#;

pub(crate) async fn load_memory_dream_state_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    scope: &MemoryScope,
) -> AppResult<Option<MemoryDreamState>> {
    let scope_fingerprint = scope.fingerprint().map_err(AppError::external)?;
    let row = sqlx::query(
        r#"
        SELECT s.scope_json, s.scope_fingerprint, s.last_successful_run_id,
               r.finished_at, s.source_revision_cursor, s.session_cursor,
               s.next_gate_at, s.last_error_kind, s.last_error_message, s.updated_at
        FROM memory_dream_states s
        LEFT JOIN memory_runs r
          ON r.tenant_id = s.tenant_id AND r.id = s.last_successful_run_id
        WHERE s.tenant_id = ?1 AND s.scope_fingerprint = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(&scope_fingerprint)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;

    row.map(|row| {
        let stored_scope: MemoryScope =
            decode_json(row.try_get::<String, _>(0).map_err(AppError::external)?)?;
        let session_cursor = row
            .try_get::<Option<String>, _>(5)
            .map_err(AppError::external)?
            .map(|value| {
                serde_json::from_str::<MemoryDreamCursor>(&value).map_err(|error| {
                    AppError::Validation(format!("invalid memory dream cursor: {error}"))
                })
            })
            .transpose()?;
        Ok(MemoryDreamState {
            scope: stored_scope,
            scope_fingerprint: row.try_get(1).map_err(AppError::external)?,
            last_successful_run_id: row.try_get(2).map_err(AppError::external)?,
            last_successful_at: row.try_get(3).map_err(AppError::external)?,
            source_revision_cursor: row.try_get(4).map_err(AppError::external)?,
            session_cursor,
            next_gate_at: row.try_get(6).map_err(AppError::external)?,
            last_error_kind: row.try_get(7).map_err(AppError::external)?,
            last_error_message: row.try_get(8).map_err(AppError::external)?,
            updated_at: row.try_get(9).map_err(AppError::external)?,
        })
    })
    .transpose()
}

pub(crate) async fn load_memory_dream_delta_rows_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    scope: &MemoryScope,
    cursor: Option<&MemoryDreamCursor>,
    stable_before: &str,
    row_limit: usize,
) -> AppResult<Vec<MemoryDreamQuestionDeltaRow>> {
    let cursor_key = cursor.map(|cursor| cursor.session_sort_key.as_str());
    let row_limit = i64::try_from(row_limit)
        .map_err(|_| AppError::Validation("memory dream row limit is invalid".to_string()))?;
    let rows = sqlx::query(
        r#"
        WITH session_candidates AS (
            SELECT 'session' AS record_kind, s.id AS session_id, s.source_id,
                   s.adapter_id, s.project_path, s.title, s.imported_at,
                   s.imported_at || char(31) || 'session' || char(31) || s.id AS session_sort_key
            FROM conversation_sessions s
            WHERE s.tenant_id = ?1 AND s.missing = 0
              AND (?2 IS NULL OR s.adapter_id = ?2)
              AND (?3 IS NULL OR s.source_id = ?3)
              AND (?4 IS NULL OR s.project_path = ?4)
              AND (?5 IS NULL OR s.id = ?5)
              AND s.imported_at <= ?6
            UNION ALL
            SELECT 'web' AS record_kind, s.id AS session_id, s.source_id,
                   s.adapter_id, NULL AS project_path, s.title, s.imported_at,
                   s.imported_at || char(31) || 'web' || char(31) || s.id AS session_sort_key
            FROM web_record_sessions s
            WHERE s.tenant_id = ?1 AND s.missing = 0
              AND (?2 IS NULL OR s.adapter_id = ?2)
              AND (?3 IS NULL OR s.source_id = ?3)
              AND (?4 IS NULL)
              AND (?5 IS NULL OR s.id = ?5)
              AND s.imported_at <= ?6
        ), question_rows AS (
            SELECT 'session' AS record_kind, q.session_id, q.id AS question_id,
                   ROW_NUMBER() OVER (PARTITION BY q.tenant_id, q.session_id ORDER BY q.created_at, q.id) - 1 AS question_index,
                   COALESCE((
                       SELECT SUM(length(t.user_text))
                       FROM conversation_question_turns qt
                       JOIN conversation_turns t
                         ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
                       WHERE qt.tenant_id = q.tenant_id AND qt.question_id = q.id
                   ), 0) + COALESCE((
                       SELECT SUM(length(COALESCE(p.text, '') || COALESCE(p.command, '')))
                       FROM conversation_question_turns qt
                       JOIN conversation_parts p
                         ON p.tenant_id = qt.tenant_id AND p.turn_id = qt.turn_id
                       WHERE qt.tenant_id = q.tenant_id AND qt.question_id = q.id
                   ), 0) AS input_char_count
            FROM conversation_questions q
            JOIN conversation_sessions s
              ON s.tenant_id = q.tenant_id AND s.id = q.session_id
            WHERE q.tenant_id = ?1 AND s.missing = 0
            UNION ALL
            SELECT 'web' AS record_kind, q.session_id, q.id AS question_id,
                   ROW_NUMBER() OVER (PARTITION BY q.tenant_id, q.session_id ORDER BY q.created_at, q.id) - 1 AS question_index,
                   COALESCE((
                       SELECT SUM(length(t.user_text))
                       FROM web_record_question_turns qt
                       JOIN web_record_turns t
                         ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
                       WHERE qt.tenant_id = q.tenant_id AND qt.question_id = q.id
                   ), 0) + COALESCE((
                       SELECT SUM(length(COALESCE(p.text, '') || COALESCE(p.command, '')))
                       FROM web_record_question_turns qt
                       JOIN web_record_parts p
                         ON p.tenant_id = qt.tenant_id AND p.turn_id = qt.turn_id
                       WHERE qt.tenant_id = q.tenant_id AND qt.question_id = q.id
                   ), 0) AS input_char_count
            FROM web_record_questions q
            JOIN web_record_sessions s
              ON s.tenant_id = q.tenant_id AND s.id = q.session_id
            WHERE q.tenant_id = ?1 AND s.missing = 0
        ), joined AS (
            SELECT c.record_kind, c.session_id, c.source_id, c.adapter_id,
                   c.project_path, c.title, c.imported_at, c.session_sort_key,
                   q.question_id, q.question_index, q.input_char_count,
                   COUNT(*) OVER (PARTITION BY c.record_kind, c.session_id) AS available_question_count
            FROM session_candidates c
            JOIN question_rows q
              ON q.record_kind = c.record_kind AND q.session_id = c.session_id
            WHERE (?7 IS NULL OR c.session_sort_key >= ?7)
        )
        SELECT record_kind, session_id, source_id, adapter_id, project_path, title,
               imported_at, session_sort_key, question_id, question_index,
               input_char_count, available_question_count
        FROM joined
        ORDER BY session_sort_key ASC, question_index ASC, question_id ASC
        LIMIT ?8
        "#,
    )
    .bind(tenant_id)
    .bind(scope.app_id.as_deref())
    .bind(scope.source_id.as_deref())
    .bind(scope.project_path.as_deref())
    .bind(scope.session_id.as_deref())
    .bind(stable_before)
    .bind(cursor_key)
    .bind(row_limit)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    rows.into_iter()
        .map(|row| {
            let record_kind = match row
                .try_get::<String, _>(0)
                .map_err(AppError::external)?
                .as_str()
            {
                "session" => MemoryEvidenceRecordKind::Session,
                "web" => MemoryEvidenceRecordKind::Web,
                value => {
                    return Err(AppError::Validation(format!(
                        "invalid memory record kind: {value}"
                    )))
                }
            };
            let input_char_count = usize::try_from(
                row.try_get::<i64, _>(10).map_err(AppError::external)?,
            )
            .map_err(|_| {
                AppError::Validation("invalid memory dream input character count".to_string())
            })?;
            let available_question_count = usize::try_from(
                row.try_get::<i64, _>(11).map_err(AppError::external)?,
            )
            .map_err(|_| AppError::Validation("invalid memory dream question count".to_string()))?;
            Ok(MemoryDreamQuestionDeltaRow {
                record_kind,
                session_id: row.try_get(1).map_err(AppError::external)?,
                source_id: row.try_get(2).map_err(AppError::external)?,
                adapter_id: row.try_get(3).map_err(AppError::external)?,
                project_path: row.try_get(4).map_err(AppError::external)?,
                title: row.try_get(5).map_err(AppError::external)?,
                imported_at: row.try_get(6).map_err(AppError::external)?,
                session_sort_key: row.try_get(7).map_err(AppError::external)?,
                question_id: row.try_get(8).map_err(AppError::external)?,
                question_index: row.try_get(9).map_err(AppError::external)?,
                input_char_count,
                available_question_count,
            })
        })
        .collect()
}

pub(crate) async fn has_active_memory_scope_lock_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    scope_fingerprint: &str,
    exclude_run_id: Option<&str>,
) -> AppResult<bool> {
    let value = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM memory_runs
            WHERE tenant_id = ?1 AND scope_fingerprint = ?2
              AND status IN ('queued', 'running')
              AND (?3 IS NULL OR id <> ?3)
              AND kind IN ('auto_dream', 'deep_recall', 'full_organize')
        )
        "#,
    )
    .bind(tenant_id)
    .bind(scope_fingerprint)
    .bind(exclude_run_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::external)?;
    Ok(value == 1)
}

pub(crate) async fn load_memory_source_revision_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<i64> {
    let state = super::search_index_repo::load_or_create_conversation_search_index_state_sqlx(
        pool, tenant_id,
    )
    .await?;
    Ok(state.source_revision)
}

pub(crate) async fn interrupt_stale_memory_runs_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<u64> {
    let now = Utc::now().to_rfc3339();
    let result = sqlx::query(
        r#"
        UPDATE memory_runs
        SET status = 'interrupted', phase = 'completed',
            error_kind = COALESCE(error_kind, 'process_interrupted'),
            error_message = COALESCE(error_message, 'The host process ended before this Memory run completed.'),
            finished_at = COALESCE(finished_at, ?1), updated_at = ?1
        WHERE tenant_id = ?2 AND status IN ('queued', 'running')
        "#,
    )
    .bind(&now)
    .bind(tenant_id)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    Ok(result.rows_affected())
}

pub(crate) async fn list_memory_dream_notes_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    statuses: &[MemoryDreamNoteStatus],
    scope_fingerprint: Option<&str>,
    limit: usize,
    offset: usize,
) -> AppResult<Vec<MemoryDreamNote>> {
    let mut query = QueryBuilder::<Sqlite>::new(format!(
        "SELECT {MEMORY_DREAM_NOTE_COLUMNS} FROM memory_dream_notes WHERE tenant_id = "
    ));
    query.push_bind(tenant_id);
    if !statuses.is_empty() {
        query.push(" AND status IN (");
        let mut separated = query.separated(", ");
        for status in statuses {
            separated.push_bind(encode_enum(*status)?);
        }
        separated.push_unseparated(")");
    }
    if let Some(scope_fingerprint) = scope_fingerprint {
        query.push(" AND scope_fingerprint = ");
        query.push_bind(scope_fingerprint);
    }
    query.push(" ORDER BY created_at DESC, id ASC LIMIT ");
    query.push_bind(
        i64::try_from(limit)
            .map_err(|_| AppError::Validation("invalid Dream limit".to_string()))?,
    );
    query.push(" OFFSET ");
    query.push_bind(
        i64::try_from(offset)
            .map_err(|_| AppError::Validation("invalid Dream offset".to_string()))?,
    );
    let rows = query
        .build()
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    rows.iter().map(map_memory_dream_note).collect()
}

pub(crate) async fn count_memory_dream_notes_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    statuses: &[MemoryDreamNoteStatus],
    scope_fingerprint: Option<&str>,
) -> AppResult<usize> {
    let mut query =
        QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM memory_dream_notes WHERE tenant_id = ");
    query.push_bind(tenant_id);
    if !statuses.is_empty() {
        query.push(" AND status IN (");
        let mut separated = query.separated(", ");
        for status in statuses {
            separated.push_bind(encode_enum(*status)?);
        }
        separated.push_unseparated(")");
    }
    if let Some(scope_fingerprint) = scope_fingerprint {
        query.push(" AND scope_fingerprint = ");
        query.push_bind(scope_fingerprint);
    }
    let count = query
        .build_query_scalar::<i64>()
        .fetch_one(pool)
        .await
        .map_err(AppError::external)?;
    usize::try_from(count).map_err(|_| AppError::Validation("invalid Dream count".to_string()))
}

pub(crate) async fn load_memory_dream_note_detail_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    note_id: &str,
) -> AppResult<Option<MemoryDreamNoteDetail>> {
    let note_row = sqlx::query(
        r#"
        SELECT id, run_id, scope_json, scope_fingerprint, markdown, session_count,
               question_count, evidence_count, source_revision, status, created_at, updated_at
        FROM memory_dream_notes
        WHERE tenant_id = ?1 AND id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(note_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;
    let Some(note_row) = note_row else {
        return Ok(None);
    };
    let evidence_rows = sqlx::query(
        r#"
        SELECT e.id, e.record_kind, e.source_id, e.session_id, e.question_id,
               e.turn_id, e.part_id, e.block_id, e.content_hash, e.excerpt,
               e.translated_excerpt, e.event_time, e.source_revision,
               e.source_unavailable, e.created_at, e.updated_at
        FROM memory_dream_note_evidence link
        JOIN memory_evidence_snapshots e
          ON e.tenant_id = link.tenant_id AND e.id = link.evidence_id
        WHERE link.tenant_id = ?1 AND link.dream_note_id = ?2
        ORDER BY link.sort_order ASC, e.id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(note_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    Ok(Some(MemoryDreamNoteDetail {
        note: map_memory_dream_note(&note_row)?,
        evidence: evidence_rows
            .iter()
            .map(map_memory_evidence)
            .collect::<AppResult<Vec<_>>>()?,
    }))
}

pub(crate) async fn archive_memory_dream_note_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    note_id: &str,
) -> AppResult<MemoryDreamNoteDetail> {
    let result = sqlx::query(
        "UPDATE memory_dream_notes SET status = 'archived', updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(tenant_id)
    .bind(note_id)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "memory Dream note {note_id} was not found"
        )));
    }
    load_memory_dream_note_detail_sqlx(pool, tenant_id, note_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("memory Dream note {note_id} was not found")))
}

pub(crate) async fn promote_memory_dream_note_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    note_id: &str,
    candidates: &[MemoryDreamCandidateDraft],
) -> AppResult<Vec<MemoryItemDetail>> {
    if candidates.is_empty() {
        return Err(AppError::Validation(
            "memory Dream note contains no promotable bullets".to_string(),
        ));
    }
    for candidate in candidates {
        validate_item_values(
            &candidate.title,
            &candidate.content_markdown,
            0,
            0,
            Some(0.7),
        )?;
    }
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let note_row = sqlx::query(
        r#"
        SELECT run_id, scope_json, scope_fingerprint, source_revision, status
        FROM memory_dream_notes
        WHERE tenant_id = ?1 AND id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(note_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::external)?
    .ok_or_else(|| AppError::NotFound(format!("memory Dream note {note_id} was not found")))?;
    let run_id: String = note_row.try_get(0).map_err(AppError::external)?;
    let scope_json: String = note_row.try_get(1).map_err(AppError::external)?;
    let scope_fingerprint: String = note_row.try_get(2).map_err(AppError::external)?;
    let source_revision: i64 = note_row.try_get(3).map_err(AppError::external)?;
    let status: MemoryDreamNoteStatus = decode_enum(
        note_row
            .try_get::<String, _>(4)
            .map_err(AppError::external)?,
    )?;
    if status == MemoryDreamNoteStatus::Promoted {
        let item_ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM memory_items WHERE tenant_id = ?1 AND origin_dream_note_id = ?2 ORDER BY created_at, id",
        )
        .bind(tenant_id)
        .bind(note_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::external)?;
        tx.commit().await.map_err(AppError::external)?;
        let mut details = Vec::new();
        for item_id in item_ids {
            if let Some(detail) = load_memory_item_detail_sqlx(pool, tenant_id, &item_id).await? {
                details.push(detail);
            }
        }
        return Ok(details);
    }
    if status == MemoryDreamNoteStatus::Archived {
        return Err(AppError::Validation(
            "archived Memory Dream notes cannot be promoted".to_string(),
        ));
    }
    let evidence_ids = sqlx::query_scalar::<_, String>(
        "SELECT evidence_id FROM memory_dream_note_evidence WHERE tenant_id = ?1 AND dream_note_id = ?2 ORDER BY sort_order, evidence_id",
    )
    .bind(tenant_id)
    .bind(note_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(AppError::external)?;
    let mut item_ids = Vec::new();
    for candidate in candidates {
        let item_id = Uuid::new_v4().to_string();
        let revision_id = Uuid::new_v4().to_string();
        let kind = encode_enum(candidate.kind)?;
        sqlx::query(
            r#"
            INSERT INTO memory_items (
                tenant_id, id, kind, status, title, content_markdown, scope_json,
                scope_fingerprint, origin, origin_run_id, origin_dream_note_id,
                confidence, source_revision, verified_revision, created_at, updated_at
            ) VALUES (?1, ?2, ?3, 'candidate', ?4, ?5, ?6, ?7, 'auto_dream',
                      ?8, ?9, 0.7, ?10, ?10, ?11, ?11)
            "#,
        )
        .bind(tenant_id)
        .bind(&item_id)
        .bind(&kind)
        .bind(&candidate.title)
        .bind(&candidate.content_markdown)
        .bind(&scope_json)
        .bind(&scope_fingerprint)
        .bind(&run_id)
        .bind(note_id)
        .bind(source_revision)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
        sqlx::query(
            r#"
            INSERT INTO memory_item_revisions (
                tenant_id, id, item_id, revision_number, change_kind, kind, status,
                title, content_markdown, scope_json, scope_fingerprint, origin,
                confidence, source_revision, verified_revision, changed_at
            ) VALUES (?1, ?2, ?3, 1, 'create', ?4, 'candidate', ?5, ?6, ?7, ?8,
                      'auto_dream', 0.7, ?9, ?9, ?10)
            "#,
        )
        .bind(tenant_id)
        .bind(revision_id)
        .bind(&item_id)
        .bind(kind)
        .bind(&candidate.title)
        .bind(&candidate.content_markdown)
        .bind(&scope_json)
        .bind(&scope_fingerprint)
        .bind(source_revision)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
        for (sort_order, evidence_id) in evidence_ids.iter().enumerate() {
            sqlx::query(
                "INSERT INTO memory_item_evidence (tenant_id, item_id, evidence_id, sort_order) VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(tenant_id)
            .bind(&item_id)
            .bind(evidence_id)
            .bind(i64::try_from(sort_order).map_err(|_| AppError::Validation("invalid evidence order".to_string()))?)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;
        }
        item_ids.push(item_id);
    }
    sqlx::query(
        "UPDATE memory_dream_notes SET status = 'promoted', updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3",
    )
    .bind(&now)
    .bind(tenant_id)
    .bind(note_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)?;

    let mut details = Vec::new();
    for item_id in item_ids {
        details.push(
            load_memory_item_detail_sqlx(pool, tenant_id, &item_id)
                .await?
                .ok_or_else(|| {
                    AppError::NotFound(format!("promoted Memory item {item_id} was not found"))
                })?,
        );
    }
    Ok(details)
}

pub(crate) async fn create_memory_dream_run_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
    scope: &MemoryScope,
    trigger: MemoryRunTrigger,
    source_revision_start: i64,
    provider: &str,
    model: Option<&str>,
    prompt_version: &str,
    total_count: usize,
) -> AppResult<()> {
    let scope_json = encode_json(scope)?;
    let scope_fingerprint = scope.fingerprint().map_err(AppError::external)?;
    let total_count = i64::try_from(total_count)
        .map_err(|_| AppError::Validation("memory dream total count is invalid".to_string()))?;
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let locked = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM memory_runs
            WHERE tenant_id = ?1 AND scope_fingerprint = ?2
              AND status IN ('queued', 'running')
              AND kind IN ('auto_dream', 'deep_recall', 'full_organize')
        )
        "#,
    )
    .bind(tenant_id)
    .bind(&scope_fingerprint)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::external)?;
    if locked == 1 {
        return Err(AppError::Validation(
            "memory scope is already locked by another running task".to_string(),
        ));
    }
    sqlx::query(
        r#"
        INSERT INTO memory_runs (
            tenant_id, id, kind, trigger_kind, scope_json, scope_fingerprint,
            source_revision_start, provider, model, prompt_version, phase,
            processed_count, total_count, skipped_count, failed_count, status,
            created_at, updated_at
        ) VALUES (?1, ?2, 'auto_dream', ?3, ?4, ?5, ?6, ?7, ?8, ?9,
                  'context', 0, ?10, 0, 0, 'running', ?11, ?11)
        "#,
    )
    .bind(tenant_id)
    .bind(run_id)
    .bind(encode_enum(trigger)?)
    .bind(scope_json)
    .bind(scope_fingerprint)
    .bind(source_revision_start)
    .bind(provider)
    .bind(model)
    .bind(prompt_version)
    .bind(total_count)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)
}

pub(crate) async fn finish_memory_dream_error_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
    scope: &MemoryScope,
    source_revision: i64,
    error_kind: &str,
    error_message: &str,
    cancelled: bool,
) -> AppResult<()> {
    let scope_json = encode_json(scope)?;
    let scope_fingerprint = scope.fingerprint().map_err(AppError::external)?;
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    sqlx::query(
        r#"
        UPDATE memory_runs
        SET phase = 'completed', status = ?1, error_kind = ?2,
            error_message = ?3, finished_at = ?4, updated_at = ?4
        WHERE tenant_id = ?5 AND id = ?6
        "#,
    )
    .bind(if cancelled { "cancelled" } else { "failed" })
    .bind(error_kind)
    .bind(error_message)
    .bind(&now)
    .bind(tenant_id)
    .bind(run_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    sqlx::query(
        r#"
        INSERT INTO memory_dream_states (
            tenant_id, scope_fingerprint, scope_json, source_revision_cursor,
            last_error_kind, last_error_message, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(tenant_id, scope_fingerprint) DO UPDATE SET
            last_error_kind = excluded.last_error_kind,
            last_error_message = excluded.last_error_message,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(tenant_id)
    .bind(scope_fingerprint)
    .bind(scope_json)
    .bind(source_revision)
    .bind(error_kind)
    .bind(error_message)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)
}

pub(crate) async fn persist_memory_dream_success_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    input: &MemoryDreamPersistInput,
) -> AppResult<String> {
    if input.markdown.chars().count() > 6144 {
        return Err(AppError::Validation(
            "memory dream note exceeds 6144 characters".to_string(),
        ));
    }
    let scope_json = encode_json(&input.scope)?;
    let scope_fingerprint = input.scope.fingerprint().map_err(AppError::external)?;
    let cursor_json = serde_json::to_string(&input.cursor_end).map_err(AppError::external)?;
    let output_json = serde_json::to_string(&input.output).map_err(AppError::external)?;
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(AppError::external)?;

    let run_exists = sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM memory_runs WHERE tenant_id = ?1 AND id = ?2 AND status = 'running')",
    )
    .bind(tenant_id)
    .bind(&input.run_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::external)?;
    if run_exists != 1 {
        return Err(AppError::Conflict(format!(
            "memory dream run {} is not running",
            input.run_id
        )));
    }

    let mut evidence_ids = Vec::with_capacity(input.evidence.len());
    let mut seen_evidence_ids = HashSet::new();
    for evidence in &input.evidence {
        validate_evidence(&evidence.draft)?;
        let row = sqlx::query(UPSERT_MEMORY_EVIDENCE_SQL)
            .bind(tenant_id)
            .bind(Uuid::new_v4().to_string())
            .bind(encode_enum(evidence.draft.record_kind)?)
            .bind(&evidence.draft.source_id)
            .bind(evidence.draft.session_id.trim())
            .bind(&evidence.draft.question_id)
            .bind(&evidence.draft.turn_id)
            .bind(&evidence.draft.part_id)
            .bind(evidence.draft.block_id.trim())
            .bind(evidence.draft.content_hash.trim())
            .bind(&evidence.draft.excerpt)
            .bind(&evidence.draft.translated_excerpt)
            .bind(&evidence.draft.event_time)
            .bind(evidence.draft.source_revision)
            .bind(bool_value(evidence.draft.source_unavailable))
            .bind(&now)
            .fetch_one(&mut *tx)
            .await
            .map_err(AppError::external)?;
        let evidence_id: String = row.try_get(0).map_err(AppError::external)?;
        if seen_evidence_ids.insert(evidence_id.clone()) {
            evidence_ids.push((evidence.reference.as_str(), evidence_id));
        }
    }

    for (sort_order, (_, evidence_id)) in evidence_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO memory_run_evidence (tenant_id, run_id, evidence_id, sort_order) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(tenant_id)
        .bind(&input.run_id)
        .bind(evidence_id)
        .bind(i64::try_from(sort_order).map_err(|_| AppError::Validation("invalid evidence order".to_string()))?)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
    }

    sqlx::query(
        r#"
        INSERT INTO memory_dream_notes (
            tenant_id, id, run_id, scope_json, scope_fingerprint, markdown,
            session_count, question_count, evidence_count, source_revision,
            status, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'active', ?11, ?11)
        "#,
    )
    .bind(tenant_id)
    .bind(&input.note_id)
    .bind(&input.run_id)
    .bind(&scope_json)
    .bind(&scope_fingerprint)
    .bind(&input.markdown)
    .bind(
        i64::try_from(input.session_count)
            .map_err(|_| AppError::Validation("invalid session count".to_string()))?,
    )
    .bind(
        i64::try_from(input.question_count)
            .map_err(|_| AppError::Validation("invalid question count".to_string()))?,
    )
    .bind(
        i64::try_from(evidence_ids.len())
            .map_err(|_| AppError::Validation("invalid evidence count".to_string()))?,
    )
    .bind(input.source_revision_end)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    for (sort_order, (_, evidence_id)) in evidence_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO memory_dream_note_evidence (tenant_id, dream_note_id, evidence_id, sort_order) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(tenant_id)
        .bind(&input.note_id)
        .bind(evidence_id)
        .bind(i64::try_from(sort_order).map_err(|_| AppError::Validation("invalid evidence order".to_string()))?)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
    }

    sqlx::query(
        r#"
        UPDATE memory_runs
        SET phase = 'completed', status = 'completed', source_revision_end = ?1,
            processed_count = ?2, total_count = ?3, result_json = ?4,
            finished_at = ?5, updated_at = ?5
        WHERE tenant_id = ?6 AND id = ?7
        "#,
    )
    .bind(input.source_revision_end)
    .bind(
        i64::try_from(input.processed_count)
            .map_err(|_| AppError::Validation("invalid processed count".to_string()))?,
    )
    .bind(
        i64::try_from(input.total_count)
            .map_err(|_| AppError::Validation("invalid total count".to_string()))?,
    )
    .bind(output_json)
    .bind(&now)
    .bind(tenant_id)
    .bind(&input.run_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;

    sqlx::query(
        r#"
        INSERT INTO memory_dream_states (
            tenant_id, scope_fingerprint, scope_json, last_successful_run_id,
            source_revision_cursor, session_cursor, next_gate_at,
            last_error_kind, last_error_message, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL, ?8)
        ON CONFLICT(tenant_id, scope_fingerprint) DO UPDATE SET
            scope_json = excluded.scope_json,
            last_successful_run_id = excluded.last_successful_run_id,
            source_revision_cursor = excluded.source_revision_cursor,
            session_cursor = excluded.session_cursor,
            next_gate_at = excluded.next_gate_at,
            last_error_kind = NULL,
            last_error_message = NULL,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(tenant_id)
    .bind(scope_fingerprint)
    .bind(scope_json)
    .bind(&input.run_id)
    .bind(input.source_revision_end)
    .bind(cursor_json)
    .bind(&input.next_gate_at)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;

    tx.commit().await.map_err(AppError::external)?;
    Ok(input.note_id.clone())
}

#[cfg(test)]
pub(crate) async fn upsert_memory_evidence_snapshot_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    draft: &NewMemoryEvidenceSnapshot,
) -> AppResult<MemoryEvidenceSnapshot> {
    validate_evidence(draft)?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let record_kind = encode_enum(draft.record_kind)?;
    let row = sqlx::query(UPSERT_MEMORY_EVIDENCE_SQL)
        .bind(tenant_id)
        .bind(id)
        .bind(record_kind)
        .bind(&draft.source_id)
        .bind(draft.session_id.trim())
        .bind(&draft.question_id)
        .bind(&draft.turn_id)
        .bind(&draft.part_id)
        .bind(draft.block_id.trim())
        .bind(draft.content_hash.trim())
        .bind(&draft.excerpt)
        .bind(&draft.translated_excerpt)
        .bind(&draft.event_time)
        .bind(draft.source_revision)
        .bind(bool_value(draft.source_unavailable))
        .bind(now)
        .fetch_one(pool)
        .await
        .map_err(AppError::external)?;
    map_memory_evidence(&row)
}

pub(crate) async fn create_memory_item_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    draft: &NewMemoryItem,
    evidence_ids: &[String],
) -> AppResult<MemoryItemDetail> {
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let item_id = insert_memory_item_tx(&mut tx, tenant_id, draft, evidence_ids).await?;
    tx.commit().await.map_err(AppError::external)?;
    load_memory_item_detail_sqlx(pool, tenant_id, &item_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("created memory item {item_id} was not found")))
}

async fn insert_memory_item_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    draft: &NewMemoryItem,
    evidence_ids: &[String],
) -> AppResult<String> {
    validate_new_item(draft)?;
    let item_id = Uuid::new_v4().to_string();
    let revision_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let scope_json = encode_json(&draft.scope)?;
    let scope_fingerprint = draft.scope.fingerprint().map_err(AppError::external)?;
    let kind = encode_enum(draft.kind)?;
    let status = encode_enum(draft.status)?;
    let origin = encode_enum(draft.origin)?;
    let stale_reason = encode_optional_enum(draft.stale_reason)?;

    sqlx::query(
        r#"
        INSERT INTO memory_items (
            tenant_id, id, kind, status, title, content_markdown, scope_json,
            scope_fingerprint, origin, origin_run_id, origin_dream_note_id,
            origin_extraction_id, confidence, supersedes_item_id,
            source_revision, verified_revision, stale_reason, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18, ?18
        )
        "#,
    )
    .bind(tenant_id)
    .bind(&item_id)
    .bind(&kind)
    .bind(&status)
    .bind(draft.title.trim())
    .bind(draft.content_markdown.trim())
    .bind(&scope_json)
    .bind(&scope_fingerprint)
    .bind(&origin)
    .bind(&draft.origin_run_id)
    .bind(&draft.origin_dream_note_id)
    .bind(&draft.origin_extraction_id)
    .bind(draft.confidence)
    .bind(&draft.supersedes_item_id)
    .bind(draft.source_revision)
    .bind(draft.verified_revision)
    .bind(&stale_reason)
    .bind(&now)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;

    sqlx::query(
        r#"
        INSERT INTO memory_item_revisions (
            tenant_id, id, item_id, revision_number, change_kind, kind,
            status, title, content_markdown, scope_json, scope_fingerprint,
            origin, confidence, supersedes_item_id, source_revision,
            verified_revision, stale_reason, changed_at
        ) VALUES (
            ?1, ?2, ?3, 1, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
            ?13, ?14, ?15, ?16, ?17
        )
        "#,
    )
    .bind(tenant_id)
    .bind(revision_id)
    .bind(&item_id)
    .bind(encode_enum(MemoryRevisionChangeKind::Create)?)
    .bind(&kind)
    .bind(&status)
    .bind(draft.title.trim())
    .bind(draft.content_markdown.trim())
    .bind(&scope_json)
    .bind(&scope_fingerprint)
    .bind(&origin)
    .bind(draft.confidence)
    .bind(&draft.supersedes_item_id)
    .bind(draft.source_revision)
    .bind(draft.verified_revision)
    .bind(&stale_reason)
    .bind(&now)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;

    let mut seen = HashSet::new();
    for (sort_order, evidence_id) in evidence_ids
        .iter()
        .filter(|id| seen.insert((*id).clone()))
        .enumerate()
    {
        let result = sqlx::query(
            r#"
            INSERT INTO memory_item_evidence (
                tenant_id, item_id, evidence_id, sort_order
            )
            SELECT ?1, ?2, ?3, ?4
            WHERE EXISTS (
                SELECT 1
                FROM memory_evidence_snapshots
                WHERE tenant_id = ?1 AND id = ?3
            )
            "#,
        )
        .bind(tenant_id)
        .bind(&item_id)
        .bind(evidence_id)
        .bind(sort_order as i64)
        .execute(&mut **tx)
        .await
        .map_err(AppError::external)?;
        if result.rows_affected() != 1 {
            return Err(AppError::NotFound(format!(
                "memory evidence {evidence_id} was not found for tenant {tenant_id}"
            )));
        }
    }

    Ok(item_id)
}

pub(crate) async fn load_memory_item_detail_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    item_id: &str,
) -> AppResult<Option<MemoryItemDetail>> {
    let Some(item_row) = sqlx::query(LOAD_MEMORY_ITEM_SQL)
        .bind(tenant_id)
        .bind(item_id)
        .fetch_optional(pool)
        .await
        .map_err(AppError::external)?
    else {
        return Ok(None);
    };
    let evidence_rows = sqlx::query(LOAD_MEMORY_ITEM_EVIDENCE_SQL)
        .bind(tenant_id)
        .bind(item_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    let revision_rows = sqlx::query(LOAD_MEMORY_ITEM_REVISIONS_SQL)
        .bind(tenant_id)
        .bind(item_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;

    Ok(Some(MemoryItemDetail {
        item: map_memory_item(&item_row)?,
        evidence: evidence_rows
            .iter()
            .map(map_memory_evidence)
            .collect::<AppResult<Vec<_>>>()?,
        revisions: revision_rows
            .iter()
            .map(map_memory_revision)
            .collect::<AppResult<Vec<_>>>()?,
    }))
}

pub(crate) async fn list_memory_items_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    filter: &MemoryItemFilter,
) -> AppResult<Vec<MemoryItem>> {
    let mut query = QueryBuilder::<Sqlite>::new(format!(
        "SELECT {MEMORY_ITEM_COLUMNS} FROM memory_items WHERE tenant_id = "
    ));
    query.push_bind(tenant_id);
    push_memory_item_filter(&mut query, filter)?;
    query.push(" ORDER BY updated_at DESC, id ASC LIMIT ");
    query.push_bind(filter.limit.clamp(1, 200) as i64);
    query.push(" OFFSET ");
    query.push_bind(filter.offset as i64);
    let rows = query
        .build()
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    rows.iter().map(map_memory_item).collect()
}

pub(crate) async fn count_memory_items_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    filter: &MemoryItemFilter,
) -> AppResult<usize> {
    let mut query =
        QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM memory_items WHERE tenant_id = ");
    query.push_bind(tenant_id);
    push_memory_item_filter(&mut query, filter)?;
    let row = query
        .build()
        .fetch_one(pool)
        .await
        .map_err(AppError::external)?;
    let count: i64 = row.try_get(0).map_err(AppError::external)?;
    usize::try_from(count).map_err(AppError::external)
}

pub(crate) async fn update_memory_item_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    item: &MemoryItem,
    evidence_ids: Option<&[String]>,
    change_kind: MemoryRevisionChangeKind,
) -> AppResult<MemoryItemDetail> {
    validate_item_values(
        &item.title,
        &item.content_markdown,
        item.source_revision,
        item.verified_revision,
        item.confidence,
    )?;
    let scope_json = encode_json(&item.scope)?;
    let scope_fingerprint = item.scope.fingerprint().map_err(AppError::external)?;
    let kind = encode_enum(item.kind)?;
    let status = encode_enum(item.status)?;
    let origin = encode_enum(item.origin)?;
    let stale_reason = encode_optional_enum(item.stale_reason)?;
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(AppError::external)?;

    let result = sqlx::query(
        r#"
        UPDATE memory_items
        SET kind = ?3,
            status = ?4,
            title = ?5,
            content_markdown = ?6,
            scope_json = ?7,
            scope_fingerprint = ?8,
            origin = ?9,
            origin_run_id = ?10,
            origin_dream_note_id = ?11,
            origin_extraction_id = ?12,
            confidence = ?13,
            supersedes_item_id = ?14,
            source_revision = ?15,
            verified_revision = ?16,
            stale_reason = ?17,
            updated_at = ?18
        WHERE tenant_id = ?1 AND id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(&item.id)
    .bind(&kind)
    .bind(&status)
    .bind(item.title.trim())
    .bind(item.content_markdown.trim())
    .bind(&scope_json)
    .bind(&scope_fingerprint)
    .bind(&origin)
    .bind(&item.origin_run_id)
    .bind(&item.origin_dream_note_id)
    .bind(&item.origin_extraction_id)
    .bind(item.confidence)
    .bind(&item.supersedes_item_id)
    .bind(item.source_revision)
    .bind(item.verified_revision)
    .bind(&stale_reason)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    if result.rows_affected() != 1 {
        return Err(AppError::NotFound(format!(
            "memory item {} was not found",
            item.id
        )));
    }

    let revision_number = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COALESCE(MAX(revision_number), 0) + 1
        FROM memory_item_revisions
        WHERE tenant_id = ?1 AND item_id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(&item.id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::external)?;
    sqlx::query(
        r#"
        INSERT INTO memory_item_revisions (
            tenant_id, id, item_id, revision_number, change_kind, kind,
            status, title, content_markdown, scope_json, scope_fingerprint,
            origin, confidence, supersedes_item_id, source_revision,
            verified_revision, stale_reason, changed_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18
        )
        "#,
    )
    .bind(tenant_id)
    .bind(Uuid::new_v4().to_string())
    .bind(&item.id)
    .bind(revision_number)
    .bind(encode_enum(change_kind)?)
    .bind(&kind)
    .bind(&status)
    .bind(item.title.trim())
    .bind(item.content_markdown.trim())
    .bind(&scope_json)
    .bind(&scope_fingerprint)
    .bind(&origin)
    .bind(item.confidence)
    .bind(&item.supersedes_item_id)
    .bind(item.source_revision)
    .bind(item.verified_revision)
    .bind(&stale_reason)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;

    if let Some(evidence_ids) = evidence_ids {
        sqlx::query("DELETE FROM memory_item_evidence WHERE tenant_id = ?1 AND item_id = ?2")
            .bind(tenant_id)
            .bind(&item.id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;

        let mut seen = HashSet::new();
        for (sort_order, evidence_id) in evidence_ids
            .iter()
            .filter(|id| seen.insert((*id).clone()))
            .enumerate()
        {
            let result = sqlx::query(
                r#"
                INSERT INTO memory_item_evidence (
                    tenant_id, item_id, evidence_id, sort_order
                )
                SELECT ?1, ?2, ?3, ?4
                WHERE EXISTS (
                    SELECT 1
                    FROM memory_evidence_snapshots
                    WHERE tenant_id = ?1 AND id = ?3
                )
                "#,
            )
            .bind(tenant_id)
            .bind(&item.id)
            .bind(evidence_id)
            .bind(sort_order as i64)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;
            if result.rows_affected() != 1 {
                return Err(AppError::NotFound(format!(
                    "memory evidence {evidence_id} was not found for tenant {tenant_id}"
                )));
            }
        }
    }

    tx.commit().await.map_err(AppError::external)?;
    load_memory_item_detail_sqlx(pool, tenant_id, &item.id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("updated memory item {} was not found", item.id)))
}

fn push_enum_filter<T: serde::Serialize + Copy>(
    query: &mut QueryBuilder<Sqlite>,
    column: &str,
    values: &[T],
) -> AppResult<()> {
    if values.is_empty() {
        return Ok(());
    }
    query.push(format!(" AND {column} IN ("));
    let mut separated = query.separated(", ");
    for value in values {
        separated.push_bind(encode_enum(*value)?);
    }
    separated.push_unseparated(")");
    Ok(())
}

fn push_memory_item_filter(
    query: &mut QueryBuilder<Sqlite>,
    filter: &MemoryItemFilter,
) -> AppResult<()> {
    push_enum_filter(query, "kind", &filter.kinds)?;
    push_enum_filter(query, "status", &filter.statuses)?;
    push_enum_filter(query, "origin", &filter.origins)?;
    if let Some(scope_fingerprint) = filter.scope_fingerprint.as_deref() {
        query.push(" AND scope_fingerprint = ");
        query.push_bind(scope_fingerprint);
    }
    if filter.stale_only {
        query.push(" AND stale_reason IS NOT NULL");
    }
    Ok(())
}

fn map_memory_item(row: &SqliteRow) -> AppResult<MemoryItem> {
    Ok(MemoryItem {
        id: row.try_get(0).map_err(AppError::external)?,
        kind: decode_enum(row.try_get(1).map_err(AppError::external)?)?,
        status: decode_enum(row.try_get(2).map_err(AppError::external)?)?,
        title: row.try_get(3).map_err(AppError::external)?,
        content_markdown: row.try_get(4).map_err(AppError::external)?,
        scope: decode_json(row.try_get(5).map_err(AppError::external)?)?,
        scope_fingerprint: row.try_get(6).map_err(AppError::external)?,
        origin: decode_enum(row.try_get(7).map_err(AppError::external)?)?,
        origin_run_id: row.try_get(8).map_err(AppError::external)?,
        origin_dream_note_id: row.try_get(9).map_err(AppError::external)?,
        origin_extraction_id: row.try_get(10).map_err(AppError::external)?,
        confidence: row.try_get(11).map_err(AppError::external)?,
        supersedes_item_id: row.try_get(12).map_err(AppError::external)?,
        source_revision: row.try_get(13).map_err(AppError::external)?,
        verified_revision: row.try_get(14).map_err(AppError::external)?,
        stale_reason: decode_optional_enum(row.try_get(15).map_err(AppError::external)?)?,
        created_at: row.try_get(16).map_err(AppError::external)?,
        updated_at: row.try_get(17).map_err(AppError::external)?,
    })
}

fn map_memory_dream_note(row: &SqliteRow) -> AppResult<MemoryDreamNote> {
    Ok(MemoryDreamNote {
        id: row.try_get(0).map_err(AppError::external)?,
        run_id: row.try_get(1).map_err(AppError::external)?,
        scope: decode_json(row.try_get::<String, _>(2).map_err(AppError::external)?)?,
        scope_fingerprint: row.try_get(3).map_err(AppError::external)?,
        markdown: row.try_get(4).map_err(AppError::external)?,
        session_count: usize::try_from(row.try_get::<i64, _>(5).map_err(AppError::external)?)
            .map_err(|_| AppError::Validation("invalid Dream session count".to_string()))?,
        question_count: usize::try_from(row.try_get::<i64, _>(6).map_err(AppError::external)?)
            .map_err(|_| AppError::Validation("invalid Dream question count".to_string()))?,
        evidence_count: usize::try_from(row.try_get::<i64, _>(7).map_err(AppError::external)?)
            .map_err(|_| AppError::Validation("invalid Dream evidence count".to_string()))?,
        source_revision: row.try_get(8).map_err(AppError::external)?,
        status: decode_enum(row.try_get::<String, _>(9).map_err(AppError::external)?)?,
        created_at: row.try_get(10).map_err(AppError::external)?,
        updated_at: row.try_get(11).map_err(AppError::external)?,
    })
}

fn map_memory_evidence(row: &SqliteRow) -> AppResult<MemoryEvidenceSnapshot> {
    Ok(MemoryEvidenceSnapshot {
        id: row.try_get(0).map_err(AppError::external)?,
        record_kind: decode_enum(row.try_get(1).map_err(AppError::external)?)?,
        source_id: row.try_get(2).map_err(AppError::external)?,
        session_id: row.try_get(3).map_err(AppError::external)?,
        question_id: row.try_get(4).map_err(AppError::external)?,
        turn_id: row.try_get(5).map_err(AppError::external)?,
        part_id: row.try_get(6).map_err(AppError::external)?,
        block_id: row.try_get(7).map_err(AppError::external)?,
        content_hash: row.try_get(8).map_err(AppError::external)?,
        excerpt: row.try_get(9).map_err(AppError::external)?,
        translated_excerpt: row.try_get(10).map_err(AppError::external)?,
        event_time: row.try_get(11).map_err(AppError::external)?,
        source_revision: row.try_get(12).map_err(AppError::external)?,
        source_unavailable: row.try_get::<i64, _>(13).map_err(AppError::external)? == 1,
        created_at: row.try_get(14).map_err(AppError::external)?,
        updated_at: row.try_get(15).map_err(AppError::external)?,
    })
}

fn map_memory_revision(row: &SqliteRow) -> AppResult<MemoryItemRevision> {
    Ok(MemoryItemRevision {
        id: row.try_get(0).map_err(AppError::external)?,
        item_id: row.try_get(1).map_err(AppError::external)?,
        revision_number: row.try_get(2).map_err(AppError::external)?,
        change_kind: decode_enum(row.try_get(3).map_err(AppError::external)?)?,
        kind: decode_enum(row.try_get(4).map_err(AppError::external)?)?,
        status: decode_enum(row.try_get(5).map_err(AppError::external)?)?,
        title: row.try_get(6).map_err(AppError::external)?,
        content_markdown: row.try_get(7).map_err(AppError::external)?,
        scope: decode_json(row.try_get(8).map_err(AppError::external)?)?,
        scope_fingerprint: row.try_get(9).map_err(AppError::external)?,
        origin: decode_enum(row.try_get(10).map_err(AppError::external)?)?,
        confidence: row.try_get(11).map_err(AppError::external)?,
        supersedes_item_id: row.try_get(12).map_err(AppError::external)?,
        source_revision: row.try_get(13).map_err(AppError::external)?,
        verified_revision: row.try_get(14).map_err(AppError::external)?,
        stale_reason: decode_optional_enum(row.try_get(15).map_err(AppError::external)?)?,
        changed_at: row.try_get(16).map_err(AppError::external)?,
    })
}

fn validate_evidence(draft: &NewMemoryEvidenceSnapshot) -> AppResult<()> {
    if draft.session_id.trim().is_empty() {
        return Err(AppError::Validation(
            "memory evidence session_id is required".to_string(),
        ));
    }
    if draft.block_id.trim().is_empty() {
        return Err(AppError::Validation(
            "memory evidence block_id is required".to_string(),
        ));
    }
    if draft
        .question_id
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        return Err(AppError::Validation(
            "memory evidence question_id is required for new evidence".to_string(),
        ));
    }
    if draft.turn_id.is_none() && draft.part_id.is_none() {
        return Err(AppError::Validation(
            "memory evidence requires a turn_id or part_id locator".to_string(),
        ));
    }
    if draft.content_hash.trim().is_empty() {
        return Err(AppError::Validation(
            "memory evidence content_hash is required".to_string(),
        ));
    }
    if draft.excerpt.chars().count() > 8192 {
        return Err(AppError::Validation(
            "memory evidence excerpt exceeds 8192 characters".to_string(),
        ));
    }
    if draft.source_revision < 0 {
        return Err(AppError::Validation(
            "memory evidence source_revision cannot be negative".to_string(),
        ));
    }
    Ok(())
}

fn validate_new_item(draft: &NewMemoryItem) -> AppResult<()> {
    validate_item_values(
        &draft.title,
        &draft.content_markdown,
        draft.source_revision,
        draft.verified_revision,
        draft.confidence,
    )
}

fn validate_item_values(
    title: &str,
    content_markdown: &str,
    source_revision: i64,
    verified_revision: i64,
    confidence: Option<f64>,
) -> AppResult<()> {
    if title.trim().is_empty() {
        return Err(AppError::Validation(
            "memory item title is required".to_string(),
        ));
    }
    if content_markdown.trim().is_empty() {
        return Err(AppError::Validation(
            "memory item content is required".to_string(),
        ));
    }
    if source_revision < 0 || verified_revision < 0 || verified_revision > source_revision {
        return Err(AppError::Validation(
            "memory item revisions are invalid".to_string(),
        ));
    }
    if confidence.is_some_and(|confidence| !(0.0..=1.0).contains(&confidence)) {
        return Err(AppError::Validation(
            "memory item confidence must be between 0 and 1".to_string(),
        ));
    }
    Ok(())
}

fn bool_value(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::models::{
        MemoryEvidenceRecordKind, MemoryItemKind, MemoryItemOrigin, MemoryItemStatus, MemoryScope,
    };
    use uuid::Uuid;

    #[test]
    fn memory_repo_deduplicates_evidence_snapshots() {
        let (database, db_path) = test_database("evidence-dedupe");
        let draft = test_evidence();

        let (first, second, count) = database
            .block_on(async {
                let first =
                    upsert_memory_evidence_snapshot_sqlx(database.pool(), "default", &draft)
                        .await?;
                let second =
                    upsert_memory_evidence_snapshot_sqlx(database.pool(), "default", &draft)
                        .await?;
                let count = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM memory_evidence_snapshots WHERE tenant_id = 'default'",
                )
                .fetch_one(database.pool())
                .await
                .map_err(AppError::external)?;
                Ok::<_, AppError>((first, second, count))
            })
            .expect("deduplicate evidence");

        assert_eq!(first.id, second.id);
        assert_eq!(count, 1);
        cleanup(database, &db_path);
    }

    #[test]
    fn memory_evidence_remap_preserves_facts_and_audits_ambiguous_legacy_rows() {
        let (database, db_path) = test_database("evidence-remap");
        database
            .block_on(async {
                sqlx::query(
                    r#"INSERT INTO conversation_sessions (
                        tenant_id, id, source_id, adapter_id, external_id, title,
                        missing, created_at, imported_at
                    ) VALUES ('default', 'remap-session', 'source', 'codex', 'remap',
                              'Remap', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"#,
                )
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;
                for question_id in ["question-a", "question-b"] {
                    sqlx::query(
                        r#"INSERT INTO conversation_questions (
                            tenant_id, id, session_id, title, created_at, updated_at
                        ) VALUES ('default', ?1, 'remap-session', NULL,
                                  '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"#,
                    )
                    .bind(question_id)
                    .execute(database.pool())
                    .await
                    .map_err(AppError::external)?;
                }
                for (turn_id, question_id, turn_index) in [
                    ("turn-a", "question-a", 0_i64),
                    ("turn-b", "question-b", 1_i64),
                ] {
                    sqlx::query(
                        r#"INSERT INTO conversation_turns (
                            tenant_id, id, session_id, external_id, turn_index, user_text,
                            fingerprint, missing, imported_at
                        ) VALUES ('default', ?1, 'remap-session', ?1, ?2, ?1, 'fingerprint', 0,
                                  '2026-01-01T00:00:00Z')"#,
                    )
                    .bind(turn_id)
                    .bind(turn_index)
                    .execute(database.pool())
                    .await
                    .map_err(AppError::external)?;
                    sqlx::query(
                        r#"INSERT INTO conversation_question_turns (
                            tenant_id, question_id, turn_id, turn_order,
                            assignment_origin, assigned_at, updated_at
                        ) VALUES ('default', ?1, ?2, 0, 'imported',
                                  '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"#,
                    )
                    .bind(question_id)
                    .bind(turn_id)
                    .execute(database.pool())
                    .await
                    .map_err(AppError::external)?;
                }

                let mut evidence = test_evidence();
                evidence.session_id = "remap-session".to_string();
                evidence.question_id = Some("question-b".to_string());
                evidence.turn_id = Some("turn-b".to_string());
                evidence.part_id = None;
                evidence.block_id = "turn-b-question".to_string();
                let snapshot = upsert_memory_evidence_snapshot_sqlx(
                    database.pool(),
                    "default",
                    &evidence,
                )
                .await?;

                sqlx::query(
                    "UPDATE conversation_question_turns SET question_id = 'question-a' WHERE tenant_id = 'default' AND question_id = 'question-b' AND turn_id = 'turn-b'",
                )
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;
                let mut tx = database.pool().begin().await.map_err(AppError::external)?;
                let summary = reconcile_memory_evidence_for_session_tx(
                    &mut tx,
                    "default",
                    MemoryEvidenceRecordKind::Session,
                    "remap-session",
                    &["question-a".to_string(), "question-b".to_string()],
                    &[("question-b".to_string(), "question-a".to_string())],
                )
                .await?;
                tx.commit().await.map_err(AppError::external)?;
                assert_eq!(summary.remapped, 1);
                assert_eq!(
                    sqlx::query_scalar::<_, String>(
                        "SELECT question_id FROM memory_evidence_snapshots WHERE tenant_id = 'default' AND id = ?1",
                    )
                    .bind(&snapshot.id)
                    .fetch_one(database.pool())
                    .await
                    .map_err(AppError::external)?,
                    "question-a"
                );

                let mut split_evidence = test_evidence();
                split_evidence.session_id = "remap-session".to_string();
                split_evidence.question_id = Some("question-a".to_string());
                split_evidence.turn_id = Some("turn-b".to_string());
                split_evidence.part_id = None;
                split_evidence.block_id = "turn-b-question".to_string();
                split_evidence.content_hash = "sha256:split".to_string();
                let split_snapshot = upsert_memory_evidence_snapshot_sqlx(
                    database.pool(),
                    "default",
                    &split_evidence,
                )
                .await?;
                sqlx::query(
                    "UPDATE conversation_question_turns SET question_id = 'question-b' WHERE tenant_id = 'default' AND question_id = 'question-a' AND turn_id = 'turn-b'",
                )
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;
                sqlx::query(
                    r#"INSERT INTO memory_evidence_snapshots (
                        tenant_id, id, record_kind, source_id, session_id, question_id,
                        turn_id, part_id, block_id, content_hash, excerpt,
                        event_time, source_revision, source_unavailable, created_at, updated_at
                    ) VALUES ('default', 'legacy-q-only', 'session', 'source', 'remap-session',
                              'question-a', NULL, NULL, 'legacy-block', 'sha256:legacy',
                              'legacy', NULL, 1, 0, '2026-01-01T00:00:00Z',
                              '2026-01-01T00:00:00Z')"#,
                )
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;

                let first = reconcile_memory_evidence_for_session_sqlx(
                    database.pool(),
                    "default",
                    MemoryEvidenceRecordKind::Session,
                    "remap-session",
                )
                .await?;
                assert_eq!(first.remapped, 2);
                assert_eq!(first.audited, 1);
                assert_eq!(
                    sqlx::query_scalar::<_, String>(
                        "SELECT question_id FROM memory_evidence_snapshots WHERE tenant_id = 'default' AND id = ?1",
                    )
                    .bind(&split_snapshot.id)
                    .fetch_one(database.pool())
                    .await
                    .map_err(AppError::external)?,
                    "question-b"
                );
                let audit_count = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM memory_evidence_remap_audits WHERE tenant_id = 'default' AND evidence_id = 'legacy-q-only' AND status = 'open'",
                )
                .fetch_one(database.pool())
                .await
                .map_err(AppError::external)?;
                assert_eq!(audit_count, 1);

                let second = reconcile_memory_evidence_for_session_sqlx(
                    database.pool(),
                    "default",
                    MemoryEvidenceRecordKind::Session,
                    "remap-session",
                )
                .await?;
                assert_eq!(second.audited, 1);
                assert_eq!(
                    sqlx::query_scalar::<_, i64>(
                        "SELECT COUNT(*) FROM memory_evidence_remap_audits WHERE tenant_id = 'default' AND evidence_id = 'legacy-q-only' AND status = 'open'",
                    )
                    .fetch_one(database.pool())
                    .await
                    .map_err(AppError::external)?,
                    1
                );
                Ok::<_, AppError>(())
            })
            .expect("remap evidence by fact locator");
        cleanup(database, &db_path);
    }

    #[test]
    fn memory_repo_creates_item_revision_and_evidence_atomically() {
        let (database, db_path) = test_database("item-create");

        let detail = database
            .block_on(async {
                let evidence = upsert_memory_evidence_snapshot_sqlx(
                    database.pool(),
                    "default",
                    &test_evidence(),
                )
                .await?;
                create_memory_item_sqlx(database.pool(), "default", &test_item(), &[evidence.id])
                    .await
            })
            .expect("create memory item");

        assert_eq!(detail.item.status, MemoryItemStatus::Active);
        assert_eq!(detail.evidence.len(), 1);
        assert_eq!(detail.revisions.len(), 1);
        assert_eq!(detail.revisions[0].revision_number, 1);
        cleanup(database, &db_path);
    }

    #[test]
    fn memory_repo_rejects_cross_tenant_evidence_and_rolls_back_item() {
        let (database, db_path) = test_database("tenant-boundary");

        let (error, tenant_b_count) = database
            .block_on(async {
                sqlx::query(
                    "INSERT INTO tenants (id, slug, name, kind, status, created_at, updated_at) VALUES ('tenant-b', 'tenant-b', 'Tenant B', 'local_workspace', 'active', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                )
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;
                let evidence = upsert_memory_evidence_snapshot_sqlx(
                    database.pool(),
                    "default",
                    &test_evidence(),
                )
                .await?;
                let error = create_memory_item_sqlx(
                    database.pool(),
                    "tenant-b",
                    &test_item(),
                    &[evidence.id],
                )
                .await
                .expect_err("cross-tenant evidence must fail");
                let tenant_b_count = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM memory_items WHERE tenant_id = 'tenant-b'",
                )
                .fetch_one(database.pool())
                .await
                .map_err(AppError::external)?;
                Ok::<_, AppError>((error, tenant_b_count))
            })
            .expect("verify tenant boundary");

        assert!(error.contains("evidence") || error.contains("FOREIGN KEY"));
        assert_eq!(tenant_b_count, 0);
        cleanup(database, &db_path);
    }

    #[test]
    fn memory_repo_filters_status_scope_and_staleness_with_stable_pagination() {
        let (database, db_path) = test_database("filters");
        let mut candidate = test_item();
        candidate.status = MemoryItemStatus::Candidate;
        candidate.kind = MemoryItemKind::Method;
        let mut stale = test_item();
        stale.title = "Stale decision".to_string();
        stale.stale_reason = Some(crate::backend::models::MemoryStaleReason::EvidenceChanged);

        let items = database
            .block_on(async {
                create_memory_item_sqlx(database.pool(), "default", &test_item(), &[]).await?;
                create_memory_item_sqlx(database.pool(), "default", &candidate, &[]).await?;
                create_memory_item_sqlx(database.pool(), "default", &stale, &[]).await?;
                let filtered = list_memory_items_sqlx(
                    database.pool(),
                    "default",
                    &MemoryItemFilter {
                        statuses: vec![MemoryItemStatus::Active],
                        scope_fingerprint: Some(
                            test_item()
                                .scope
                                .fingerprint()
                                .map_err(AppError::external)?,
                        ),
                        stale_only: true,
                        limit: 10,
                        ..MemoryItemFilter::default()
                    },
                )
                .await?;
                Ok::<_, AppError>(filtered)
            })
            .expect("filter memory items");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Stale decision");
        cleanup(database, &db_path);
    }

    #[test]
    fn memory_repo_accepts_candidate_with_evidence_and_revision_atomically() {
        let (database, db_path) = test_database("candidate-accept");
        let detail = database
            .block_on(async {
                let evidence = upsert_memory_evidence_snapshot_sqlx(
                    database.pool(),
                    "default",
                    &test_evidence(),
                )
                .await?;
                let mut candidate = test_item();
                candidate.status = MemoryItemStatus::Candidate;
                let created = create_memory_item_sqlx(
                    database.pool(),
                    "default",
                    &candidate,
                    std::slice::from_ref(&evidence.id),
                )
                .await?;
                let mut accepted = created.item;
                accepted.status = MemoryItemStatus::Active;
                update_memory_item_sqlx(
                    database.pool(),
                    "default",
                    &accepted,
                    Some(&[evidence.id]),
                    MemoryRevisionChangeKind::Accept,
                )
                .await
            })
            .expect("accept candidate");

        assert_eq!(detail.item.status, MemoryItemStatus::Active);
        assert_eq!(detail.evidence.len(), 1);
        assert_eq!(detail.revisions.len(), 2);
        assert_eq!(
            detail.revisions[0].change_kind,
            MemoryRevisionChangeKind::Accept
        );
        cleanup(database, &db_path);
    }

    #[test]
    fn memory_repo_rolls_back_update_when_replacement_evidence_crosses_tenant() {
        let (database, db_path) = test_database("update-tenant-boundary");
        let detail = database
            .block_on(async {
                sqlx::query(
                    "INSERT INTO tenants (id, slug, name, kind, status, created_at, updated_at) VALUES ('tenant-b', 'tenant-b', 'Tenant B', 'local_workspace', 'active', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                )
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;
                let default_evidence = upsert_memory_evidence_snapshot_sqlx(
                    database.pool(),
                    "default",
                    &test_evidence(),
                )
                .await?;
                let tenant_b_evidence = upsert_memory_evidence_snapshot_sqlx(
                    database.pool(),
                    "tenant-b",
                    &test_evidence(),
                )
                .await?;
                let created = create_memory_item_sqlx(
                    database.pool(),
                    "default",
                    &test_item(),
                    &[default_evidence.id],
                )
                .await?;
                let item_id = created.item.id.clone();
                let mut changed = created.item;
                changed.title = "Must roll back".to_string();
                update_memory_item_sqlx(
                    database.pool(),
                    "default",
                    &changed,
                    Some(&[tenant_b_evidence.id]),
                    MemoryRevisionChangeKind::Update,
                )
                .await
                .expect_err("cross-tenant replacement evidence must fail");
                load_memory_item_detail_sqlx(database.pool(), "default", &item_id)
                    .await?
                    .ok_or_else(|| AppError::external("memory item disappeared"))
            })
            .expect("verify update rollback");

        assert_eq!(detail.item.title, test_item().title);
        assert_eq!(detail.evidence.len(), 1);
        assert_eq!(detail.revisions.len(), 1);
        cleanup(database, &db_path);
    }

    #[test]
    fn memory_dream_run_persists_note_evidence_and_cursor_atomically_and_retries() {
        let (database, db_path) = test_database("dream-atomic");
        let scope = MemoryScope {
            project_path: Some("~/project".to_string()),
            ..MemoryScope::default()
        };
        database
            .block_on(async {
                create_memory_dream_run_sqlx(
                    database.pool(),
                    "default",
                    "dream-run-1",
                    &scope,
                    crate::backend::models::MemoryRunTrigger::Manual,
                    0,
                    "opencode",
                    None,
                    "memory-auto-dream-v1",
                    1,
                )
                .await?;
                persist_memory_dream_success_sqlx(
                    database.pool(),
                    "default",
                    &dream_persist_input("dream-run-1", "dream-note-1", &scope, 1),
                )
                .await?;

                create_memory_dream_run_sqlx(
                    database.pool(),
                    "default",
                    "dream-run-2",
                    &scope,
                    crate::backend::models::MemoryRunTrigger::Manual,
                    1,
                    "opencode",
                    None,
                    "memory-auto-dream-v1",
                    1,
                )
                .await?;
                persist_memory_dream_success_sqlx(
                    database.pool(),
                    "default",
                    &dream_persist_input("dream-run-2", "dream-note-1", &scope, 2),
                )
                .await
                .expect_err("duplicate note id must roll back the success transaction");

                let state_after_failure = load_memory_dream_state_sqlx(
                    database.pool(),
                    "default",
                    &scope,
                )
                .await?
                .ok_or_else(|| AppError::external("Dream state disappeared"))?;
                let run_two_evidence = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM memory_run_evidence WHERE tenant_id = 'default' AND run_id = 'dream-run-2'",
                )
                .fetch_one(database.pool())
                .await
                .map_err(AppError::external)?;
                assert_eq!(
                    state_after_failure
                        .session_cursor
                        .as_ref()
                        .map(|cursor| cursor.question_offset),
                    Some(1)
                );
                assert_eq!(run_two_evidence, 0);

                persist_memory_dream_success_sqlx(
                    database.pool(),
                    "default",
                    &dream_persist_input("dream-run-2", "dream-note-2", &scope, 2),
                )
                .await?;
                let state = load_memory_dream_state_sqlx(database.pool(), "default", &scope)
                    .await?
                    .ok_or_else(|| AppError::external("Dream state was not saved"))?;
                let completed_runs = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM memory_runs WHERE tenant_id = 'default' AND status = 'completed'",
                )
                .fetch_one(database.pool())
                .await
                .map_err(AppError::external)?;
                let notes = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM memory_dream_notes WHERE tenant_id = 'default'",
                )
                .fetch_one(database.pool())
                .await
                .map_err(AppError::external)?;

                assert_eq!(state.last_successful_run_id.as_deref(), Some("dream-run-2"));
                assert_eq!(
                    state
                        .session_cursor
                        .as_ref()
                        .map(|cursor| cursor.question_offset),
                    Some(2)
                );
                assert_eq!(completed_runs, 2);
                assert_eq!(notes, 2);
                Ok::<_, AppError>(())
            })
            .expect("persist and retry Dream transactions");
        cleanup(database, &db_path);
    }

    #[test]
    fn memory_repo_rolls_back_recall_candidates_when_finalization_fails() {
        let (database, db_path) = test_database("recall-finalize-rollback");
        database
            .block_on(async {
                let scope = MemoryScope {
                    project_path: Some("~/assetiweave".to_string()),
                    ..MemoryScope::default()
                };
                create_memory_recall_run_sqlx(
                    database.pool(),
                    "default",
                    "recall-run-1",
                    MemoryRunKind::FullOrganize,
                    &scope,
                    3,
                    "opencode",
                    None,
                    2,
                )
                .await?;
                let mut first = test_item();
                first.status = MemoryItemStatus::Candidate;
                first.origin = MemoryItemOrigin::FullOrganize;
                first.origin_run_id = Some("recall-run-1".to_string());
                let mut second = first.clone();
                second.title = "Second candidate".to_string();
                let error = persist_memory_recall_success_sqlx(
                    database.pool(),
                    "default",
                    "recall-run-1",
                    &[
                        (first, Vec::new()),
                        (second, vec!["missing-evidence".to_string()]),
                    ],
                    &serde_json::json!({"answer_markdown": "answer"}),
                    3,
                    0,
                )
                .await
                .expect_err("invalid candidate evidence must roll back finalization");
                assert!(error.contains("evidence"));
                let item_count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM memory_items WHERE tenant_id='default' AND origin_run_id='recall-run-1'",
                )
                .fetch_one(database.pool())
                .await
                .map_err(AppError::external)?;
                let status: String = sqlx::query_scalar(
                    "SELECT status FROM memory_runs WHERE tenant_id='default' AND id='recall-run-1'",
                )
                .fetch_one(database.pool())
                .await
                .map_err(AppError::external)?;
                assert_eq!(item_count, 0);
                assert_eq!(status, "running");
                Ok::<_, AppError>(())
            })
            .expect("verify Recall finalization rollback");
        cleanup(database, &db_path);
    }

    #[test]
    #[ignore = "100k Recall performance fixture; run from the release checklist"]
    fn memory_recall_100k_scope_first_page_p95_is_below_350ms() {
        let (database, db_path) = test_database("recall-100k");
        database
            .block_on(async {
                sqlx::query(
                    r#"INSERT INTO conversation_sessions (
                      tenant_id,id,source_id,adapter_id,external_id,title,project_path,
                      missing,created_at,imported_at
                    ) VALUES ('default','perf-session','perf-source','codex','perf-session',
                      'Performance fixture','~/performance',0,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')"#,
                )
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;
                sqlx::query(
                    r#"WITH RECURSIVE ids(value) AS (
                      VALUES(0) UNION ALL SELECT value+1 FROM ids WHERE value<99999
                    )
                    INSERT INTO conversation_questions (
                      tenant_id,id,session_id,title,created_at,updated_at
                    ) SELECT 'default','perf-question-'||value,'perf-session',
                      'Question',
                      printf('2026-01-%02dT00:00:00Z',(value % 28)+1),
                      '2026-01-01T00:00:00Z' FROM ids"#,
                )
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;
                let scope = MemoryScope {
                    app_id: Some("codex".to_string()),
                    project_path: Some("~/performance".to_string()),
                    ..MemoryScope::default()
                };
                let _ = list_memory_recall_question_refs_sqlx(
                    database.pool(),
                    "default",
                    &scope,
                    None,
                    None,
                    false,
                    200,
                    0,
                )
                .await?;
                let mut samples = Vec::new();
                for _ in 0..20 {
                    let started = std::time::Instant::now();
                    let (total, page) = list_memory_recall_question_refs_sqlx(
                        database.pool(),
                        "default",
                        &scope,
                        None,
                        None,
                        false,
                        200,
                        0,
                    )
                    .await?;
                    samples.push(started.elapsed());
                    assert_eq!(total, 100_000);
                    assert_eq!(page.len(), 200);
                }
                samples.sort();
                let p95 = samples[samples.len() - 1];
                assert!(
                    p95 < std::time::Duration::from_millis(350),
                    "100k Recall page p95 was {p95:?}"
                );
                Ok::<_, AppError>(())
            })
            .expect("measure 100k Recall page");
        cleanup(database, &db_path);
    }

    #[test]
    fn memory_freshness_distinguishes_changed_missing_and_unavailable_evidence() {
        let (database, db_path) = test_database("freshness-reasons");
        database
            .block_on(async {
                sqlx::query(
                    r#"INSERT INTO conversation_sessions (
                      tenant_id,id,source_id,adapter_id,external_id,title,missing,created_at,imported_at
                    ) VALUES ('default','fresh-session','source','codex','fresh-session','Freshness',0,
                      '2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')"#,
                )
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;
                sqlx::query(
                    r#"INSERT INTO conversation_turns (
                      tenant_id,id,session_id,external_id,turn_index,user_text,fingerprint,missing,imported_at
                    ) VALUES ('default','fresh-turn','fresh-session','fresh-turn',0,'original','hash',0,
                      '2026-01-01T00:00:00Z')"#,
                )
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;
                let snapshot = upsert_memory_evidence_snapshot_sqlx(
                    database.pool(),
                    "default",
                    &NewMemoryEvidenceSnapshot {
                        record_kind: MemoryEvidenceRecordKind::Session,
                        source_id: Some("source".to_string()),
                        session_id: "fresh-session".to_string(),
                        question_id: Some("fresh-question".to_string()),
                        turn_id: Some("fresh-turn".to_string()),
                        part_id: None,
                        block_id: "fresh-turn-question".to_string(),
                        content_hash: format!("sha256:{:x}", Sha256::digest(b"original")),
                        excerpt: "original".to_string(),
                        translated_excerpt: None,
                        event_time: None,
                        source_revision: 1,
                        source_unavailable: false,
                    },
                )
                .await?;
                assert_eq!(
                    memory_evidence_stale_reason_sqlx(database.pool(), "default", &snapshot).await?,
                    None
                );
                sqlx::query("UPDATE conversation_turns SET user_text='changed' WHERE tenant_id='default' AND id='fresh-turn'")
                    .execute(database.pool()).await.map_err(AppError::external)?;
                assert_eq!(
                    memory_evidence_stale_reason_sqlx(database.pool(), "default", &snapshot).await?,
                    Some(crate::backend::models::MemoryStaleReason::EvidenceChanged)
                );
                sqlx::query("DELETE FROM conversation_turns WHERE tenant_id='default' AND id='fresh-turn'")
                    .execute(database.pool()).await.map_err(AppError::external)?;
                assert_eq!(
                    memory_evidence_stale_reason_sqlx(database.pool(), "default", &snapshot).await?,
                    Some(crate::backend::models::MemoryStaleReason::EvidenceMissing)
                );
                sqlx::query("UPDATE conversation_sessions SET missing=1 WHERE tenant_id='default' AND id='fresh-session'")
                    .execute(database.pool()).await.map_err(AppError::external)?;
                assert_eq!(
                    memory_evidence_stale_reason_sqlx(database.pool(), "default", &snapshot).await?,
                    Some(crate::backend::models::MemoryStaleReason::SourceUnavailable)
                );
                Ok::<_, AppError>(())
            })
            .expect("verify freshness reasons");
        cleanup(database, &db_path);
    }

    fn test_evidence() -> NewMemoryEvidenceSnapshot {
        NewMemoryEvidenceSnapshot {
            record_kind: MemoryEvidenceRecordKind::Session,
            source_id: Some("codex".to_string()),
            session_id: "session-1".to_string(),
            question_id: Some("question-1".to_string()),
            turn_id: Some("turn-1".to_string()),
            part_id: Some("part-1".to_string()),
            block_id: "part-1".to_string(),
            content_hash: "sha256:one".to_string(),
            excerpt: "Use the shared AppService boundary.".to_string(),
            translated_excerpt: None,
            event_time: Some("2026-01-01T00:00:00Z".to_string()),
            source_revision: 3,
            source_unavailable: false,
        }
    }

    fn dream_persist_input(
        run_id: &str,
        note_id: &str,
        scope: &MemoryScope,
        cursor_offset: usize,
    ) -> crate::backend::models::MemoryDreamPersistInput {
        crate::backend::models::MemoryDreamPersistInput {
            run_id: run_id.to_string(),
            note_id: note_id.to_string(),
            scope: scope.clone(),
            source_revision_end: cursor_offset as i64,
            processed_count: 1,
            total_count: 1,
            markdown: format!(
                "## 近期进展\n- Finished batch {cursor_offset} [evidence: evidence-0]"
            ),
            output: serde_json::json!({"sections": []}),
            session_count: 1,
            question_count: 1,
            cursor_end: MemoryDreamCursor {
                session_sort_key: "2026-07-23T00:00:00Z\u{1f}session\u{1f}session-1".to_string(),
                question_offset: cursor_offset,
            },
            next_gate_at: "2026-07-24T00:00:00Z".to_string(),
            evidence: vec![crate::backend::models::MemoryDreamEvidenceDraft {
                reference: "evidence-0".to_string(),
                draft: test_evidence(),
            }],
        }
    }

    fn test_item() -> NewMemoryItem {
        NewMemoryItem {
            kind: MemoryItemKind::Decision,
            status: MemoryItemStatus::Active,
            title: "Keep one application workflow boundary".to_string(),
            content_markdown: "Desktop and CLI both call AppService.".to_string(),
            scope: MemoryScope {
                project_path: Some("~/assetiweave".to_string()),
                ..MemoryScope::default()
            },
            origin: MemoryItemOrigin::Manual,
            origin_run_id: None,
            origin_dream_note_id: None,
            origin_extraction_id: None,
            confidence: Some(1.0),
            supersedes_item_id: None,
            source_revision: 3,
            verified_revision: 3,
            stale_reason: None,
        }
    }

    fn test_database(label: &str) -> (crate::backend::store::Database, std::path::PathBuf) {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-memory-repo-{label}-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open(&db_path).expect("open database");
        (database, db_path)
    }

    fn cleanup(database: crate::backend::store::Database, db_path: &std::path::Path) {
        drop(database);
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    }
}
