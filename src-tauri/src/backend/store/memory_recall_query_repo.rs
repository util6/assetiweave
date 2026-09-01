use crate::backend::models::{MemoryRecallQuestionRef, MemoryRecordKind, MemoryScope};
use crate::backend::runtime::{AppError, AppResult};
use sqlx::{sqlite::SqliteRow, QueryBuilder, Row, Sqlite, SqlitePool};

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
        WHERE q.tenant_id=?1 AND source.adapter_id <> 'assetiweave-memory-recall'
          AND (?2 IS NULL OR s.adapter_id=?2)
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
        WHERE q.tenant_id=?1 AND source.adapter_id <> 'assetiweave-memory-recall'
          AND (?2 IS NULL OR s.adapter_id=?2)
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
        WHERE q.tenant_id=?1 AND source.adapter_id <> 'assetiweave-memory-recall'
          AND (?2 IS NULL OR s.adapter_id=?2)
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
        WHERE q.tenant_id=?1 AND source.adapter_id <> 'assetiweave-memory-recall'
          AND (?2 IS NULL OR s.adapter_id=?2)
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
    query.push(" AND source.adapter_id <> 'assetiweave-memory-recall'");
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
            MemoryRecordKind::Web
        } else {
            MemoryRecordKind::Session
        },
        source_id: row.try_get("source_id").map_err(AppError::external)?,
        session_id: row.try_get("session_id").map_err(AppError::external)?,
        session_title: row.try_get("session_title").map_err(AppError::external)?,
        project_path: row.try_get("project_path").map_err(AppError::external)?,
        question_id: row.try_get("question_id").map_err(AppError::external)?,
        question_index: row.try_get("question_index").map_err(AppError::external)?,
    })
}
