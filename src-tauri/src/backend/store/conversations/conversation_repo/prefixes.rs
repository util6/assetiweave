use super::*;

pub(super) fn conversation_id_fragment_range(prefix: &str, fragment: &str) -> (String, String) {
    let lower = format!("{prefix}{}", fragment.trim().to_ascii_lowercase());
    // Stable IDs continue with lowercase hex, so `g` is the exclusive prefix-range sentinel.
    let upper = format!("{lower}g");
    (lower, upper)
}

pub(crate) async fn list_conversation_sessions_by_id_fragment_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    query: &str,
    limit: usize,
    offset: usize,
) -> StoreResult<Vec<ConversationSessionListItem>> {
    if query.trim().len() != 8 {
        return Err(StoreError::Validation(
            "conversation short ID must be exactly 8 hexadecimal characters".to_string(),
        ));
    }
    let search_term =
        crate::backend::domain::conversation_id_search_term(query).ok_or_else(|| {
            StoreError::external("conversation short ID must be exactly 8 hexadecimal characters")
        })?;
    let fragment = crate::backend::domain::conversation_id_fragment(&search_term);
    let tables = record_kind.tables();
    let session_ids =
        load_search_session_ids_by_id_fragment_sqlx(pool, tenant_id, tables, &fragment).await?;
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }
    let session_ids_json = serde_json::to_string(&session_ids).map_err(StoreError::external)?;
    let sessions = load_search_sessions_sqlx(
        pool,
        tenant_id,
        tables,
        adapter_id,
        source_id,
        Some(&session_ids_json),
    )
    .await?;
    Ok(sessions.into_iter().skip(offset).take(limit).collect())
}

pub(crate) async fn resolve_conversation_session_id_prefix_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    prefix_or_id: &str,
) -> StoreResult<String> {
    if prefix_or_id.len() >= 36 {
        return Ok(prefix_or_id.to_string());
    }
    let clean_prefix = prefix_or_id
        .strip_prefix("conversation-session-")
        .unwrap_or(prefix_or_id);
    let like_pattern_verbatim = format!("{}%", prefix_or_id);
    let like_pattern_domain = format!("conversation-session-{}%", clean_prefix);

    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND (id LIKE ?2 OR id LIKE ?3) LIMIT 11",
    )
    .bind(tenant_id)
    .bind(&like_pattern_verbatim)
    .bind(&like_pattern_domain)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    if rows.is_empty() {
        return Err(StoreError::NotFound(format!(
            "no session matches prefix {:?}",
            prefix_or_id
        )));
    }
    if rows.len() > 1 {
        let max_display = std::cmp::min(rows.len(), 5);
        let examples = rows[..max_display].join(", ");
        let indicator = if rows.len() > 5 { "..." } else { "" };
        return Err(StoreError::Conflict(format!(
            "ambiguous prefix {:?}: {} sessions match (e.g. {}{})",
            prefix_or_id,
            if rows.len() > 10 {
                "10+".to_string()
            } else {
                rows.len().to_string()
            },
            examples,
            indicator
        )));
    }
    Ok(rows[0].clone())
}

pub(crate) async fn resolve_conversation_question_id_prefix_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    prefix_or_id: &str,
) -> StoreResult<String> {
    if prefix_or_id.len() >= 36 {
        return Ok(prefix_or_id.to_string());
    }
    let clean_prefix = prefix_or_id
        .strip_prefix("conversation-question-")
        .unwrap_or(prefix_or_id);
    let like_pattern_verbatim = format!("{}%", prefix_or_id);
    let like_pattern_domain = format!("conversation-question-{}%", clean_prefix);

    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM conversation_questions WHERE tenant_id = ?1 AND (id LIKE ?2 OR id LIKE ?3) LIMIT 11",
    )
    .bind(tenant_id)
    .bind(&like_pattern_verbatim)
    .bind(&like_pattern_domain)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    if rows.is_empty() {
        return Err(StoreError::NotFound(format!(
            "no question matches prefix {:?}",
            prefix_or_id
        )));
    }
    if rows.len() > 1 {
        let max_display = std::cmp::min(rows.len(), 5);
        let examples = rows[..max_display].join(", ");
        let indicator = if rows.len() > 5 { "..." } else { "" };
        return Err(StoreError::Conflict(format!(
            "ambiguous prefix {:?}: {} questions match (e.g. {}{})",
            prefix_or_id,
            if rows.len() > 10 {
                "10+".to_string()
            } else {
                rows.len().to_string()
            },
            examples,
            indicator
        )));
    }
    Ok(rows[0].clone())
}

pub(crate) async fn resolve_conversation_turn_id_prefix_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    prefix_or_id: &str,
) -> StoreResult<String> {
    if prefix_or_id.len() >= 36 {
        return Ok(prefix_or_id.to_string());
    }
    let clean_prefix = prefix_or_id
        .strip_prefix("conversation-turn-")
        .unwrap_or(prefix_or_id);
    let like_pattern_verbatim = format!("{}%", prefix_or_id);
    let like_pattern_domain = format!("conversation-turn-{}%", clean_prefix);

    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM conversation_turns WHERE tenant_id = ?1 AND (id LIKE ?2 OR id LIKE ?3) LIMIT 11",
    )
    .bind(tenant_id)
    .bind(&like_pattern_verbatim)
    .bind(&like_pattern_domain)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    if rows.is_empty() {
        return Err(StoreError::NotFound(format!(
            "no turn matches prefix {:?}",
            prefix_or_id
        )));
    }
    if rows.len() > 1 {
        let max_display = std::cmp::min(rows.len(), 5);
        let examples = rows[..max_display].join(", ");
        let indicator = if rows.len() > 5 { "..." } else { "" };
        return Err(StoreError::Conflict(format!(
            "ambiguous prefix {:?}: {} turns match (e.g. {}{})",
            prefix_or_id,
            if rows.len() > 10 {
                "10+".to_string()
            } else {
                rows.len().to_string()
            },
            examples,
            indicator
        )));
    }
    Ok(rows[0].clone())
}

pub(crate) async fn resolve_conversation_part_id_prefix_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    prefix_or_id: &str,
) -> StoreResult<String> {
    if prefix_or_id.len() >= 36 {
        return Ok(prefix_or_id.to_string());
    }
    let clean_prefix = prefix_or_id
        .strip_prefix("conversation-part-")
        .unwrap_or(prefix_or_id);
    let like_pattern_verbatim = format!("{}%", prefix_or_id);
    let like_pattern_domain = format!("conversation-part-{}%", clean_prefix);

    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM conversation_parts WHERE tenant_id = ?1 AND (id LIKE ?2 OR id LIKE ?3) LIMIT 11",
    )
    .bind(tenant_id)
    .bind(&like_pattern_verbatim)
    .bind(&like_pattern_domain)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    if rows.is_empty() {
        return Err(StoreError::NotFound(format!(
            "no part matches prefix {:?}",
            prefix_or_id
        )));
    }
    if rows.len() > 1 {
        let max_display = std::cmp::min(rows.len(), 5);
        let examples = rows[..max_display].join(", ");
        let indicator = if rows.len() > 5 { "..." } else { "" };
        return Err(StoreError::Conflict(format!(
            "ambiguous prefix {:?}: {} parts match (e.g. {}{})",
            prefix_or_id,
            if rows.len() > 10 {
                "10+".to_string()
            } else {
                rows.len().to_string()
            },
            examples,
            indicator
        )));
    }
    Ok(rows[0].clone())
}
