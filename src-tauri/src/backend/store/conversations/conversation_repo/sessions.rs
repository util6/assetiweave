use super::*;

pub(crate) async fn list_conversation_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    query: Option<&str>,
    limit: usize,
    offset: usize,
) -> StoreResult<Vec<ConversationSessionListItem>> {
    let needle = normalize_query(query);
    let id_needle = query.and_then(crate::backend::domain::conversation_id_search_term);
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
               ) AS turn_count
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
              OR EXISTS (
                SELECT 1
                  FROM conversation_question_fts f
                  WHERE f.tenant_id = s.tenant_id
                    AND f.session_id = s.id
                    AND instr(lower(
                        f.question_text || char(10) || f.answer_text || char(10) ||
                        f.code_text || char(10) || f.command_text
                    ), ?4) > 0
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
            .map_err(|_| StoreError::external(format!("invalid conversation limit: {limit}")))?,
    )
    .bind(
        i64::try_from(offset)
            .map_err(|_| StoreError::external(format!("invalid conversation offset: {offset}")))?,
    )
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    rows.into_iter()
        .map(ConversationSessionListItemRow::into_item)
        .collect()
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

pub(crate) async fn list_recent_conversation_sessions_sqlx(
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

pub(crate) async fn load_conversation_session_detail_sqlx(
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
