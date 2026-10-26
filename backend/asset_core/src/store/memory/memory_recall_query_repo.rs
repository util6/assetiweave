use crate::backend::domain::{MemoryRecallQuestionRef, MemoryRecordKind, MemoryScope};
use crate::backend::store::{StoreError, StoreResult};
use sqlx::{sqlite::SqliteRow, AssertSqlSafe, Row, SqlitePool};

/// 召回单元（等价于轮次分组）的定位时间与顺序统一由轮次列派生。
///
/// 定位时间取组内首轮的 `started_at`；缺失（NULL / 空白）或不可解析时回退该轮的
/// `imported_at`。窗口边界与定位时间都先归一到 UTC 秒级再比较，因此：
/// `since` 与 `until` 都是**双端闭区间**，且跨时区偏移不会因字符串字典序而错位。
/// 计数与分页共用同一段 CTE，时间口径不可能漂移。
const RECALL_UNITS_SQL: &str = r#"
  WITH recall_turns AS (
    SELECT 'session' AS record_kind, s.source_id, s.id AS session_id,
           s.title AS session_title, s.project_path, t.id AS question_id,
           t.turn_index, t.id AS turn_id,
           COALESCE(
               strftime('%Y-%m-%dT%H:%M:%SZ', NULLIF(trim(t.started_at), '')),
               strftime('%Y-%m-%dT%H:%M:%SZ', t.imported_at)
           ) AS turn_time
    FROM conversation_turns t
    JOIN conversation_sessions s ON s.tenant_id=t.tenant_id AND s.id=t.session_id
    JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id
    WHERE t.tenant_id=?1 AND source.adapter_id <> 'assetiweave-memory-recall'
      AND t.id <> ''
      AND s.user_visible = 1 AND s.execution_origin != 'internal_memory'
      AND (?2 IS NULL OR s.adapter_id=?2)
      AND (?3 IS NULL OR s.source_id=?3) AND (?4 IS NULL OR s.project_path=?4)
      AND (?5 IS NULL OR s.id=?5) AND (?6=1 OR (s.missing=0 AND source.enabled=1))
    UNION ALL
    SELECT 'web', s.source_id, s.id, s.title, NULL, t.id AS question_id,
           t.turn_index, t.id,
           COALESCE(
               strftime('%Y-%m-%dT%H:%M:%SZ', NULLIF(trim(t.started_at), '')),
               strftime('%Y-%m-%dT%H:%M:%SZ', t.imported_at)
           )
    FROM web_record_turns t
    JOIN web_record_sessions s ON s.tenant_id=t.tenant_id AND s.id=t.session_id
    JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id
    WHERE t.tenant_id=?1 AND source.adapter_id <> 'assetiweave-memory-recall'
      AND t.id <> ''
      AND (?2 IS NULL OR s.adapter_id=?2)
      AND (?3 IS NULL OR s.source_id=?3) AND ?4 IS NULL
      AND (?5 IS NULL OR s.id=?5) AND (?6=1 OR (s.missing=0 AND source.enabled=1))
  ),
  ranked_turns AS (
    SELECT *,
           ROW_NUMBER() OVER (
               PARTITION BY record_kind, session_id, question_id
               ORDER BY turn_index ASC, turn_id ASC
           ) AS group_rank
    FROM recall_turns
  ),
  recall_units AS (
    SELECT record_kind, source_id, session_id, session_title, project_path,
           question_id, turn_time AS located_at,
           ROW_NUMBER() OVER (
               PARTITION BY record_kind, session_id
               ORDER BY turn_index ASC, turn_id ASC
           ) - 1 AS question_index
    FROM ranked_turns
    WHERE group_rank = 1
  )
"#;

const RECALL_UNITS_TIME_FILTER: &str = r#"
    WHERE (?7 IS NULL OR located_at >= COALESCE(strftime('%Y-%m-%dT%H:%M:%SZ', ?7), ?7))
      AND (?8 IS NULL OR located_at <= COALESCE(strftime('%Y-%m-%dT%H:%M:%SZ', ?8), ?8))
"#;

pub async fn list_memory_recall_question_refs_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    scope: &MemoryScope,
    since: Option<&str>,
    until: Option<&str>,
    include_unavailable: bool,
    limit: usize,
    offset: usize,
) -> StoreResult<(usize, Vec<MemoryRecallQuestionRef>)> {
    let count_sql =
        format!("{RECALL_UNITS_SQL} SELECT COUNT(*) FROM recall_units{RECALL_UNITS_TIME_FILTER}");
    let total_count: i64 = sqlx::query(AssertSqlSafe(count_sql))
        .bind(tenant_id)
        .bind(&scope.app_id)
        .bind(&scope.source_id)
        .bind(&scope.project_path)
        .bind(&scope.session_id)
        .bind(if include_unavailable { 1_i64 } else { 0_i64 })
        .bind(since)
        .bind(until)
        .fetch_one(pool)
        .await?
        .try_get(0)?;
    let page_sql = format!(
        "{RECALL_UNITS_SQL}
         SELECT record_kind,source_id,session_id,session_title,project_path,question_id,question_index
         FROM recall_units{RECALL_UNITS_TIME_FILTER}
         ORDER BY located_at DESC,record_kind,session_id,question_index,question_id
         LIMIT ?9 OFFSET ?10"
    );
    let rows = sqlx::query(AssertSqlSafe(page_sql))
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
                .map_err(|_| StoreError::Validation("invalid Recall limit".to_string()))?,
        )
        .bind(
            i64::try_from(offset)
                .map_err(|_| StoreError::Validation("invalid Recall offset".to_string()))?,
        )
        .fetch_all(pool)
        .await?;
    let total = usize::try_from(total_count)
        .map_err(|_| StoreError::Validation("invalid Recall question count".to_string()))?;
    let selected = rows
        .iter()
        .map(map_memory_recall_question_ref)
        .collect::<StoreResult<Vec<_>>>()?;
    Ok((total, selected))
}

fn map_memory_recall_question_ref(row: &SqliteRow) -> StoreResult<MemoryRecallQuestionRef> {
    let kind: String = row.try_get("record_kind")?;
    let raw_question_id: String = row.try_get("question_id")?;
    let question_id =
        crate::backend::domain::conversations::derive_question_id_from_turn(&raw_question_id);
    Ok(MemoryRecallQuestionRef {
        record_kind: if kind == "web" {
            MemoryRecordKind::Web
        } else {
            MemoryRecordKind::Session
        },
        source_id: row.try_get("source_id")?,
        session_id: row.try_get("session_id")?,
        session_title: row.try_get("session_title")?,
        project_path: row.try_get("project_path")?,
        question_id,
        question_index: row.try_get("question_index")?,
    })
}

pub async fn filter_valid_recall_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: MemoryRecordKind,
    session_ids: &[String],
) -> StoreResult<std::collections::BTreeSet<String>> {
    if session_ids.is_empty() {
        return Ok(std::collections::BTreeSet::new());
    }
    let session_ids_json = serde_json::to_string(session_ids).map_err(StoreError::external)?;
    let (sessions_table, extra_filter) = match record_kind {
        MemoryRecordKind::Session => (
            "conversation_sessions",
            "AND s.user_visible = 1 AND s.execution_origin != 'internal_memory'",
        ),
        MemoryRecordKind::Web => ("web_record_sessions", ""),
    };
    let sql = format!(
        r#"
        SELECT s.id
        FROM {sessions_table} s
        JOIN conversation_sources source ON source.tenant_id = s.tenant_id AND source.id = s.source_id
        WHERE s.tenant_id = ?1
          AND s.missing = 0
          AND source.enabled = 1
          AND source.adapter_id <> 'assetiweave-memory-recall'
          {extra_filter}
          AND s.id IN (SELECT value FROM json_each(?2))
        "#
    );
    let rows: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(session_ids_json)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;
    Ok(rows.into_iter().collect())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RecallPartFacts {
    pub question_id: String,
    pub kind: String,
    pub text: Option<String>,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub command_label: Option<String>,
    pub metadata_json: Option<String>,
    pub status: Option<String>,
    pub exit_code: Option<i64>,
    #[sqlx(default)]
    pub signal: Option<String>,
    #[sqlx(default)]
    pub tool_name: Option<String>,
    #[sqlx(default)]
    pub file_path: Option<String>,
}

pub async fn load_recall_parts_for_questions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: MemoryRecordKind,
    question_ids: &[String],
) -> StoreResult<Vec<RecallPartFacts>> {
    if question_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut turn_ids = std::collections::BTreeSet::new();
    for qid in question_ids {
        turn_ids.insert(qid.clone());
        if let Some(turn_id) =
            crate::backend::domain::conversations::derive_turn_id_from_question_id(qid)
        {
            turn_ids.insert(turn_id);
        }
    }
    let turn_ids_vec: Vec<String> = turn_ids.into_iter().collect();
    let turn_ids_json = serde_json::to_string(&turn_ids_vec).map_err(StoreError::external)?;
    let sql = match record_kind {
        MemoryRecordKind::Session => {
            r#"
            SELECT t.id AS question_id, p.kind,
                   (CASE WHEN p.kind = 'command' THEN
                       CASE WHEN json_extract(p.attrs_json, '$.has_command') = 0 THEN COALESCE(json_extract(p.attrs_json, '$.summary'), p.content)
                            ELSE json_extract(p.attrs_json, '$.summary')
                       END
                    ELSE p.content END) AS text,
                   (CASE WHEN p.kind = 'command' THEN
                       CASE WHEN json_extract(p.attrs_json, '$.has_command') = 0 THEN NULL
                            ELSE p.content
                       END
                    ELSE NULL END) AS command,
                   json_extract(p.attrs_json, '$.cwd') AS cwd,
                   json_extract(p.attrs_json, '$.command_label') AS command_label,
                   NULL AS metadata_json,
                   json_extract(p.attrs_json, '$.status') AS status,
                   CAST(json_extract(p.attrs_json, '$.exit_code') AS INTEGER) AS exit_code,
                   json_extract(p.attrs_json, '$.signal') AS signal,
                   json_extract(p.attrs_json, '$.tool_name') AS tool_name,
                   json_extract(p.attrs_json, '$.file_path') AS file_path
            FROM conversation_parts p
            JOIN conversation_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
            WHERE p.tenant_id = ?1
              AND t.id IN (SELECT value FROM json_each(?2))
            "#
        }
        MemoryRecordKind::Web => {
            r#"
            SELECT t.id AS question_id,
                   (CASE WHEN p.kind = 'answer' THEN 'text' ELSE 'tool' END) AS kind,
                   p.content AS text,
                   NULL AS command,
                   NULL AS cwd,
                   NULL AS command_label,
                   NULL AS metadata_json,
                   NULL AS status,
                   NULL AS exit_code,
                   NULL AS signal,
                   NULL AS tool_name,
                   NULL AS file_path
            FROM web_record_parts p
            JOIN web_record_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
            WHERE p.tenant_id = ?1
              AND t.id IN (SELECT value FROM json_each(?2))
            "#
        }
    };
    let mut rows: Vec<RecallPartFacts> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(turn_ids_json)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;
    for row in &mut rows {
        row.question_id =
            crate::backend::domain::conversations::derive_question_id_from_turn(&row.question_id);
    }
    Ok(rows)
}

#[cfg(test)]
#[path = "memory_recall_query_repo_tests.rs"]
mod tests;
