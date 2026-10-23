use super::*;

#[derive(Debug, Clone, Default)]
pub struct ConversationSessionListFilter<'a> {
    pub adapter_id: Option<&'a str>,
    pub source_id: Option<&'a str>,
    pub query: Option<&'a str>,
    pub project_path: Option<&'a str>,
    pub since: Option<&'a str>,
    pub until: Option<&'a str>,
    pub limit: usize,
    pub offset: usize,
}

pub async fn list_conversation_sessions_filtered_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    filter: ConversationSessionListFilter<'_>,
) -> StoreResult<Vec<ConversationSessionListItem>> {
    let needle = normalize_query(filter.query);
    let id_needle = filter
        .query
        .and_then(crate::backend::domain::conversation_id_search_term);
    let project_needle = filter
        .project_path
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_ascii_lowercase);
    // 正文索引重建期间正文匹配不可用，列表搜索降级为标题级，避免给出误导性的空结果。
    let body_search_ready = conversation_part_index_ready_sqlx(pool, tenant_id).await?;

    let rows = sqlx::query_as::<_, ConversationSessionListItemRow>(
        r#"
        SELECT s.id, s.source_id, s.adapter_id, s.external_id, s.title, s.project_path,
               s.started_at, s.updated_at, s.source_locator, s.source_fingerprint,
               s.missing, s.created_at, s.imported_at,
               s.execution_origin, s.execution_purpose, s.user_visible,
               (
                   SELECT COUNT(*)
                   FROM conversation_questions q
                   WHERE q.tenant_id = s.tenant_id AND q.session_id = s.id
               ) AS question_count,
               (
                   SELECT COUNT(*)
                   FROM conversation_turns t
                   WHERE t.tenant_id = s.tenant_id AND t.session_id = s.id
               ) AS turn_count,
               (
                   SELECT t.user_text
                   FROM conversation_turns t
                   WHERE t.tenant_id = s.tenant_id AND t.session_id = s.id
                   ORDER BY t.turn_index DESC
                   LIMIT 1
               ) AS latest_user_prompt,
               CASE WHEN ?8 IS NOT NULL OR ?9 IS NOT NULL THEN (
                   SELECT COUNT(*)
                   FROM conversation_turns t
                   WHERE t.tenant_id = s.tenant_id
                     AND t.session_id = s.id
                     AND (?8 IS NULL OR COALESCE(t.started_at, t.imported_at) >= ?8)
                     AND (?9 IS NULL OR COALESCE(t.started_at, t.imported_at) <= ?9)
               ) ELSE NULL END AS window_turn_count,
               CASE WHEN ?8 IS NOT NULL OR ?9 IS NOT NULL THEN (
                   SELECT t.turn_index
                   FROM conversation_turns t
                   WHERE t.tenant_id = s.tenant_id
                     AND t.session_id = s.id
                     AND (?8 IS NULL OR COALESCE(t.started_at, t.imported_at) >= ?8)
                     AND (?9 IS NULL OR COALESCE(t.started_at, t.imported_at) <= ?9)
                   ORDER BY t.turn_index ASC
                   LIMIT 1
               ) ELSE NULL END AS window_start_turn_index,
               CASE WHEN ?8 IS NOT NULL OR ?9 IS NOT NULL THEN (
                   SELECT t.user_text
                   FROM conversation_turns t
                   WHERE t.tenant_id = s.tenant_id
                     AND t.session_id = s.id
                     AND (?8 IS NULL OR COALESCE(t.started_at, t.imported_at) >= ?8)
                     AND (?9 IS NULL OR COALESCE(t.started_at, t.imported_at) <= ?9)
                   ORDER BY t.turn_index ASC
                   LIMIT 1
               ) ELSE NULL END AS window_start_user_prompt,
               CASE WHEN ?8 IS NOT NULL OR ?9 IS NOT NULL THEN (
                   SELECT t.turn_index
                   FROM conversation_turns t
                   WHERE t.tenant_id = s.tenant_id
                     AND t.session_id = s.id
                     AND (?8 IS NULL OR COALESCE(t.started_at, t.imported_at) >= ?8)
                     AND (?9 IS NULL OR COALESCE(t.started_at, t.imported_at) <= ?9)
                   ORDER BY t.turn_index DESC
                   LIMIT 1
               ) ELSE NULL END AS window_end_turn_index,
               CASE WHEN ?8 IS NOT NULL OR ?9 IS NOT NULL THEN (
                   SELECT t.user_text
                   FROM conversation_turns t
                   WHERE t.tenant_id = s.tenant_id
                     AND t.session_id = s.id
                     AND (?8 IS NULL OR COALESCE(t.started_at, t.imported_at) >= ?8)
                     AND (?9 IS NULL OR COALESCE(t.started_at, t.imported_at) <= ?9)
                   ORDER BY t.turn_index DESC
                   LIMIT 1
               ) ELSE NULL END AS window_end_user_prompt
        FROM conversation_sessions s
        WHERE s.tenant_id = ?1
          AND s.user_visible = 1
          AND (?2 IS NULL OR s.adapter_id = ?2)
          AND (?3 IS NULL OR s.source_id = ?3)
          AND s.missing = 0
          AND (
              ?4 IS NULL
              OR instr(lower(s.title), ?4) > 0
              OR instr(lower(COALESCE(s.project_path, '')), ?4) > 0
              OR instr(lower(s.external_id), ?4) > 0
              OR (?5 IS NOT NULL AND instr(lower(s.id), ?5) > 0)
              OR (?11 = 1 AND ?4 IS NOT NULL AND EXISTS (
                SELECT 1
                  FROM conversation_part_fts f
                  WHERE f.tenant_id = s.tenant_id
                    AND f.session_id = s.id
                    AND instr(lower(f.text), ?4) > 0
              ))
              OR (?11 = 1 AND ?4 IS NOT NULL AND EXISTS (
                SELECT 1
                  FROM conversation_turns t
                  WHERE t.tenant_id = s.tenant_id
                    AND t.session_id = s.id
                    AND instr(lower(t.user_text), ?4) > 0
              ))
          )
          AND (
              ?10 IS NULL
              OR s.project_path = ?10
              OR instr(lower(COALESCE(s.project_path, '')), ?10) > 0
          )
          AND (
              (?8 IS NULL AND ?9 IS NULL)
              OR EXISTS (
                  SELECT 1
                  FROM conversation_turns t
                  WHERE t.tenant_id = s.tenant_id
                    AND t.session_id = s.id
                    AND (?8 IS NULL OR COALESCE(t.started_at, t.imported_at) >= ?8)
                    AND (?9 IS NULL OR COALESCE(t.started_at, t.imported_at) <= ?9)
              )
              OR (
                  (?8 IS NULL OR COALESCE(s.updated_at, s.imported_at) >= ?8)
                  AND (?9 IS NULL OR COALESCE(s.updated_at, s.imported_at) <= ?9)
              )
          )
        ORDER BY COALESCE(s.updated_at, s.imported_at) DESC, s.title ASC
        LIMIT ?6 OFFSET ?7
        "#,
    )
    .bind(tenant_id)
    .bind(filter.adapter_id)
    .bind(filter.source_id)
    .bind(needle.as_deref())
    .bind(id_needle.as_deref())
    .bind(i64::try_from(filter.limit).map_err(|_| {
        StoreError::external(format!("invalid conversation limit: {}", filter.limit))
    })?)
    .bind(i64::try_from(filter.offset).map_err(|_| {
        StoreError::external(format!("invalid conversation offset: {}", filter.offset))
    })?)
    .bind(filter.since)
    .bind(filter.until)
    .bind(project_needle.as_deref())
    .bind(i64::from(body_search_ready))
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    rows.into_iter()
        .map(ConversationSessionListItemRow::into_item)
        .collect()
}

pub async fn list_conversation_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    query: Option<&str>,
    limit: usize,
    offset: usize,
) -> StoreResult<Vec<ConversationSessionListItem>> {
    list_conversation_sessions_filtered_sqlx(
        pool,
        tenant_id,
        ConversationSessionListFilter {
            adapter_id,
            source_id,
            query,
            project_path: None,
            since: None,
            until: None,
            limit,
            offset,
        },
    )
    .await
}

pub(super) const LIST_RECENT_CONVERSATION_SESSIONS_SQL: &str = r#"
        SELECT s.id, s.source_id, s.adapter_id, s.external_id, s.title, s.project_path,
               s.started_at, s.updated_at, s.source_locator, s.source_fingerprint,
               s.missing, s.created_at, s.imported_at,
               s.execution_origin, s.execution_purpose, s.user_visible,
               (
                   SELECT COUNT(*)
                   FROM conversation_questions q
                   WHERE q.tenant_id = s.tenant_id AND q.session_id = s.id
               ) AS question_count,
               (
                   SELECT COUNT(*)
                   FROM conversation_turns t
                   WHERE t.tenant_id = s.tenant_id AND t.session_id = s.id
               ) AS turn_count,
               s.updated_at AS last_activity_at,
               (
                   SELECT p.cwd
                   FROM conversation_turns t
                   CROSS JOIN conversation_parts p INDEXED BY idx_conversation_parts_tenant_turn
                   WHERE t.tenant_id = s.tenant_id
                     AND t.session_id = s.id
                     AND p.tenant_id = t.tenant_id
                     AND p.turn_id = t.id
                     AND p.cwd IS NOT NULL
                     AND trim(p.cwd) <> ''
                   ORDER BY COALESCE(t.ended_at, t.started_at) DESC,
                            t.turn_index DESC,
                            p.part_index DESC,
                            p.id DESC
                   LIMIT 1
               ) AS cwd,
               COALESCE(NULLIF(trim(a.name), ''), s.adapter_id) AS source_agent
        FROM conversation_sessions s
        JOIN conversation_sources source
          ON source.tenant_id = s.tenant_id
         AND source.id = s.source_id
         AND source.enabled = 1
        LEFT JOIN conversation_adapters a
          ON a.tenant_id = s.tenant_id AND a.id = s.adapter_id
        WHERE s.tenant_id = ?1
          AND s.user_visible = 1
          AND s.missing = 0
          AND (
                ?4 = ''
                OR s.project_path IS NULL
                OR (
                    s.project_path <> ?4
                    AND instr(s.project_path, ?4 || '/') <> 1
                )
              )
          AND s.updated_at IS NOT NULL
          AND CASE
                WHEN trim(s.updated_at) GLOB '[0-9]*'
                 AND trim(s.updated_at) NOT GLOB '*[^0-9]*'
                THEN datetime(
                    CASE WHEN length(trim(s.updated_at)) >= 12
                         THEN CAST(trim(s.updated_at) AS REAL) / 1000.0
                         ELSE CAST(trim(s.updated_at) AS REAL)
                    END,
                    'unixepoch'
                )
                ELSE datetime(s.updated_at)
              END >= datetime(?2)
          AND CASE
                WHEN trim(s.updated_at) GLOB '[0-9]*'
                 AND trim(s.updated_at) NOT GLOB '*[^0-9]*'
                THEN datetime(
                    CASE WHEN length(trim(s.updated_at)) >= 12
                         THEN CAST(trim(s.updated_at) AS REAL) / 1000.0
                         ELSE CAST(trim(s.updated_at) AS REAL)
                    END,
                    'unixepoch'
                )
                ELSE datetime(s.updated_at)
              END <= datetime(?3)
        ORDER BY CASE
                   WHEN trim(s.updated_at) GLOB '[0-9]*'
                    AND trim(s.updated_at) NOT GLOB '*[^0-9]*'
                   THEN datetime(
                       CASE WHEN length(trim(s.updated_at)) >= 12
                            THEN CAST(trim(s.updated_at) AS REAL) / 1000.0
                            ELSE CAST(trim(s.updated_at) AS REAL)
                       END,
                       'unixepoch'
                   )
                   ELSE datetime(s.updated_at)
                 END DESC,
                 s.id ASC
        "#;

pub async fn list_recent_conversation_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    cutoff: &str,
    now: &str,
    excluded_project_root: &str,
) -> StoreResult<Vec<RecentConversationSessionRecord>> {
    let rows = sqlx::query_as::<_, RecentConversationSessionRecordRow>(
        LIST_RECENT_CONVERSATION_SESSIONS_SQL,
    )
    .bind(tenant_id)
    .bind(cutoff)
    .bind(now)
    .bind(excluded_project_root)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    rows.into_iter()
        .map(RecentConversationSessionRecordRow::into_record)
        .collect()
}

pub async fn load_conversation_session_detail_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<ConversationSessionDetail> {
    let session_row = sqlx::query(
        r#"
        SELECT id, source_id, adapter_id, external_id, title, project_path,
               started_at, updated_at, source_locator, source_fingerprint,
               missing, created_at, imported_at,
               execution_origin, execution_purpose, user_visible
        FROM conversation_sessions
        WHERE tenant_id = ?1 AND id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?
    .ok_or_else(|| StoreError::external(format!("conversation session not found: {session_id}")))?;
    let session = map_sqlx_conversation_session(&session_row)?;
    let questions =
        load_conversation_question_details_for_session_sqlx(pool, tenant_id, session_id).await?;
    Ok(ConversationSessionDetail { session, questions })
}
