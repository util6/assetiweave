use super::*;
use crate::domain::conversations::{derive_conversation_part_id, parse_id_key_prefix, IdKeyLookup};

pub(super) fn conversation_id_fragment_range(prefix: &str, fragment: &str) -> (String, String) {
    let lower = format!("{prefix}{}", fragment.trim().to_ascii_lowercase());
    // Stable IDs continue with lowercase hex, so `g` is the exclusive prefix-range sentinel.
    let upper = format!("{lower}g");
    (lower, upper)
}

pub async fn list_conversation_sessions_by_id_fragment_sqlx(
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

pub async fn resolve_conversation_session_id_prefix_sqlx(
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

/// 分组标识 → 完整标识。候选集来自轮次列，并额外并入跳转表里的墓碑标识，
/// 使合并前的旧标识继续解析到幸存分组。
pub async fn resolve_conversation_question_id_prefix_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    prefix_or_id: &str,
) -> StoreResult<String> {
    if prefix_or_id.len() >= 36 {
        return resolve_conversation_question_redirect_pool_sqlx(pool, tenant_id, prefix_or_id)
            .await;
    }
    let clean_prefix = prefix_or_id
        .strip_prefix("conversation-question-")
        .unwrap_or(prefix_or_id);
    let like_pattern_verbatim = format!("{}%", prefix_or_id);
    let like_pattern_domain = format!("conversation-question-{}%", clean_prefix);

    let rows: Vec<String> = sqlx::query_scalar(
        r#"
        WITH candidates AS (
            SELECT question_id AS id
            FROM conversation_turns
            WHERE tenant_id = ?1 AND (question_id = ?2 OR question_id LIKE ?3 OR question_id LIKE ?4)
            UNION
            SELECT source_question_id AS id
            FROM conversation_question_redirects
            WHERE tenant_id = ?1
              AND (source_question_id = ?2 OR source_question_id LIKE ?3 OR source_question_id LIKE ?4)
        )
        SELECT id FROM candidates LIMIT 11
        "#,
    )
    .bind(tenant_id)
    .bind(prefix_or_id)
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
    let mut matched = BTreeSet::new();
    for row in &rows {
        matched
            .insert(resolve_conversation_question_redirect_pool_sqlx(pool, tenant_id, row).await?);
    }
    if matched.len() > 1 {
        let examples = matched
            .iter()
            .take(5)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        let indicator = if matched.len() > 5 { "..." } else { "" };
        return Err(StoreError::Conflict(format!(
            "ambiguous prefix {:?}: {} questions match (e.g. {}{})",
            prefix_or_id,
            if matched.len() > 10 {
                "10+".to_string()
            } else {
                matched.len().to_string()
            },
            examples,
            indicator
        )));
    }
    matched.into_iter().next().ok_or_else(|| {
        StoreError::NotFound(format!("no question matches prefix {:?}", prefix_or_id))
    })
}

pub async fn resolve_conversation_turn_id_prefix_sqlx(
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

async fn query_parts_by_id_key(
    pool: &SqlitePool,
    tenant_id: &str,
    lookup: &IdKeyLookup,
) -> StoreResult<Vec<(String, String, i64)>> {
    match lookup {
        IdKeyLookup::Exact(key) => sqlx::query_as(
            "SELECT t.session_id, p.turn_id, p.part_index FROM conversation_parts p \
                 JOIN conversation_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id \
                 WHERE p.tenant_id = ?1 AND p.id_key = ?2 LIMIT 11",
        )
        .bind(tenant_id)
        .bind(key.to_vec())
        .fetch_all(pool)
        .await
        .map_err(StoreError::external),
        IdKeyLookup::Range {
            low,
            high: Some(high),
        } => sqlx::query_as(
            "SELECT t.session_id, p.turn_id, p.part_index FROM conversation_parts p \
                 JOIN conversation_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id \
                 WHERE p.tenant_id = ?1 AND p.id_key >= ?2 AND p.id_key < ?3 LIMIT 11",
        )
        .bind(tenant_id)
        .bind(low.to_vec())
        .bind(high.to_vec())
        .fetch_all(pool)
        .await
        .map_err(StoreError::external),
        IdKeyLookup::Range { low, high: None } => sqlx::query_as(
            "SELECT t.session_id, p.turn_id, p.part_index FROM conversation_parts p \
                 JOIN conversation_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id \
                 WHERE p.tenant_id = ?1 AND p.id_key >= ?2 LIMIT 11",
        )
        .bind(tenant_id)
        .bind(low.to_vec())
        .fetch_all(pool)
        .await
        .map_err(StoreError::external),
    }
}

pub async fn resolve_conversation_part_id_prefix_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    prefix_or_id: &str,
) -> StoreResult<String> {
    let trimmed = prefix_or_id.trim();
    if trimmed.is_empty() {
        return Err(StoreError::Validation(
            "part identifier cannot be empty".to_string(),
        ));
    }

    let clean_hash = trimmed
        .strip_prefix("conversation-part-")
        .or_else(|| trimmed.strip_prefix("web-record-part-"))
        .unwrap_or(trimmed);

    let verbatim_like = format!("{trimmed}%");
    let web_like = format!("web-record-part-{clean_hash}%");
    let hash_like = format!("%-{clean_hash}%");

    let mut rows: Vec<String> = Vec::new();

    if let Some(parsed) = parse_id_key_prefix(clean_hash) {
        let part_rows = query_parts_by_id_key(pool, tenant_id, &parsed.lookup).await?;
        for (_, turn_id, part_index) in part_rows {
            let full_id = derive_conversation_part_id(&turn_id, part_index);
            let hash_part = full_id
                .strip_prefix("conversation-part-")
                .unwrap_or(&full_id);
            if hash_part.starts_with(&parsed.normalized_hex)
                || full_id.starts_with(trimmed)
                || full_id == trimmed
            {
                if !rows.contains(&full_id) {
                    rows.push(full_id);
                }
            }
        }
    }

    let web_rows: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM web_record_parts \
         WHERE tenant_id = ?1 AND (id = ?2 OR id LIKE ?3 OR id LIKE ?4 OR id LIKE ?5) LIMIT 11",
    )
    .bind(tenant_id)
    .bind(trimmed)
    .bind(&verbatim_like)
    .bind(&web_like)
    .bind(&hash_like)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    for id in web_rows {
        if !rows.contains(&id) {
            rows.push(id);
        }
    }

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

pub async fn resolve_any_id_to_session_id_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    any_id: &str,
) -> StoreResult<(ConversationRecordKind, String)> {
    let trimmed = any_id.trim();
    if trimmed.is_empty() {
        return Err(StoreError::Validation(
            "identifier cannot be empty".to_string(),
        ));
    }

    let clean_hash = trimmed
        .strip_prefix("conversation-session-")
        .or_else(|| trimmed.strip_prefix("conversation-question-"))
        .or_else(|| trimmed.strip_prefix("conversation-turn-"))
        .or_else(|| trimmed.strip_prefix("conversation-part-"))
        .or_else(|| trimmed.strip_prefix("web-record-session-"))
        .or_else(|| trimmed.strip_prefix("web-record-question-"))
        .or_else(|| trimmed.strip_prefix("web-record-turn-"))
        .or_else(|| trimmed.strip_prefix("web-record-part-"))
        .unwrap_or(trimmed);

    let verbatim_like = format!("{trimmed}%");
    let hash_like = format!("%-{clean_hash}%");

    let query = r#"
        WITH candidates AS (
            SELECT s.id AS session_id, 'session' AS kind
            FROM conversation_sessions s
            WHERE s.tenant_id = ?1 AND (s.id = ?2 OR s.id LIKE ?3 OR s.id LIKE ?4)
            UNION
            SELECT t.session_id, 'session' AS kind
            FROM conversation_turns t
            WHERE t.tenant_id = ?1
              AND (t.question_id = ?2 OR t.question_id LIKE ?3 OR t.question_id LIKE ?4)
            UNION
            SELECT t.session_id, 'session' AS kind
            FROM conversation_turns t
            WHERE t.tenant_id = ?1 AND (t.id = ?2 OR t.id LIKE ?3 OR t.id LIKE ?4)
            UNION
            SELECT s.id AS session_id, 'web' AS kind
            FROM web_record_sessions s
            WHERE s.tenant_id = ?1 AND (s.id = ?2 OR s.id LIKE ?3 OR s.id LIKE ?4)
            UNION
            SELECT t.session_id, 'web' AS kind
            FROM web_record_turns t
            WHERE t.tenant_id = ?1
              AND (t.question_id = ?2 OR t.question_id LIKE ?3 OR t.question_id LIKE ?4)
            UNION
            SELECT t.session_id, 'web' AS kind
            FROM web_record_turns t
            WHERE t.tenant_id = ?1 AND (t.id = ?2 OR t.id LIKE ?3 OR t.id LIKE ?4)
            UNION
            SELECT t.session_id, 'web' AS kind
            FROM web_record_parts p
            JOIN web_record_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
            WHERE p.tenant_id = ?1 AND (p.id = ?2 OR p.id LIKE ?3 OR p.id LIKE ?4)
        )
        SELECT session_id, kind FROM candidates LIMIT 11
    "#;

    let mut rows: Vec<(String, String)> = sqlx::query_as(query)
        .bind(tenant_id)
        .bind(trimmed)
        .bind(&verbatim_like)
        .bind(&hash_like)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;

    if let Some(parsed) = parse_id_key_prefix(clean_hash) {
        let part_rows = query_parts_by_id_key(pool, tenant_id, &parsed.lookup).await?;
        for (session_id, _, _) in part_rows {
            let item = (session_id, "session".to_string());
            if !rows.contains(&item) {
                rows.push(item);
            }
        }
    }

    if rows.is_empty() {
        return Err(StoreError::NotFound(format!(
            "no session or card found matching identifier {:?}",
            any_id
        )));
    }

    if rows.len() > 1 {
        let max_display = std::cmp::min(rows.len(), 5);
        let examples = rows[..max_display]
            .iter()
            .map(|(id, _)| id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let indicator = if rows.len() > 5 { "..." } else { "" };
        return Err(StoreError::Conflict(format!(
            "ambiguous identifier {:?}: {} sessions match (e.g. {}{})",
            any_id,
            if rows.len() > 10 {
                "10+".to_string()
            } else {
                rows.len().to_string()
            },
            examples,
            indicator
        )));
    }

    let (session_id, kind_str) = &rows[0];
    let kind = if kind_str == "web" {
        ConversationRecordKind::Web
    } else {
        ConversationRecordKind::Session
    };

    Ok((kind, session_id.clone()))
}
