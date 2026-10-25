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
    if prefix_or_id.len() >= 85 && prefix_or_id.starts_with("conversation-session-") {
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

/// 分组标识 → 完整标识。通过轮次 ID 确定性解析。
pub async fn resolve_conversation_question_id_prefix_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    prefix_or_id: &str,
) -> StoreResult<String> {
    let trimmed = prefix_or_id.trim();
    if trimmed.is_empty() {
        return Err(StoreError::Validation(
            "question identifier cannot be empty".to_string(),
        ));
    }

    if trimmed.starts_with("conversation-turn-")
        || trimmed.starts_with("web-record-turn-")
        || trimmed.starts_with("turn-")
        || trimmed.starts_with("conversation-session-")
        || trimmed.starts_with("web-record-session-")
        || trimmed.starts_with("session-")
        || trimmed.starts_with("conversation-part-")
        || trimmed.starts_with("web-record-part-")
        || trimmed.starts_with("part-")
    {
        return Err(StoreError::NotFound(format!(
            "no question matches prefix {:?}",
            prefix_or_id
        )));
    }

    let (clean_prefix, allow_conversation, allow_web) =
        if let Some(stripped) = trimmed.strip_prefix("conversation-question-") {
            (stripped, true, false)
        } else if let Some(stripped) = trimmed.strip_prefix("question-") {
            (stripped, true, false)
        } else if let Some(stripped) = trimmed.strip_prefix("web-record-question-") {
            (stripped, false, true)
        } else if let Some(stripped) = trimmed.strip_prefix("web-question-") {
            (stripped, false, true)
        } else {
            (trimmed, true, true)
        };

    let mut matched = BTreeSet::new();

    if allow_conversation {
        let like_verbatim = format!("{}%", trimmed);
        let like_turn = format!("conversation-turn-{}%", clean_prefix);
        let rows: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT id
            FROM conversation_turns
            WHERE tenant_id = ?1
              AND (id = ?2 OR id LIKE ?3 OR id LIKE ?4)
            LIMIT 11
            "#,
        )
        .bind(tenant_id)
        .bind(format!("conversation-turn-{}", clean_prefix))
        .bind(&like_verbatim)
        .bind(&like_turn)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;

        for row in rows {
            let qid = crate::backend::domain::conversations::derive_question_id_from_turn_id(
                "conversation-question",
                &row,
            );
            matched.insert(qid);
        }
    }

    if allow_web {
        let like_verbatim = format!("{}%", trimmed);
        let like_turn = format!("web-record-turn-{}%", clean_prefix);
        let rows: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT id
            FROM web_record_turns
            WHERE tenant_id = ?1
              AND (id = ?2 OR id LIKE ?3 OR id LIKE ?4)
            LIMIT 11
            "#,
        )
        .bind(tenant_id)
        .bind(format!("web-record-turn-{}", clean_prefix))
        .bind(&like_verbatim)
        .bind(&like_turn)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;

        for row in rows {
            let qid = crate::backend::domain::conversations::derive_question_id_from_turn_id(
                "web-record-question",
                &row,
            );
            matched.insert(qid);
        }
    }

    if matched.is_empty() {
        return Err(StoreError::NotFound(format!(
            "no question matches prefix {:?}",
            prefix_or_id
        )));
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
    if prefix_or_id.len() >= 82 && prefix_or_id.starts_with("conversation-turn-") {
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

pub(super) async fn query_parts_by_id_key(
    pool: &SqlitePool,
    tenant_id: &str,
    lookup: &IdKeyLookup,
    parts_table: &str,
    turns_table: &str,
) -> StoreResult<Vec<(String, String, i64, Vec<u8>)>> {
    let (clause, low_vec, high_vec) = match lookup {
        IdKeyLookup::Exact(key) => ("p.id_key = ?2", key.to_vec(), None),
        IdKeyLookup::Range {
            low,
            high: Some(high),
        } => (
            "p.id_key >= ?2 AND p.id_key < ?3",
            low.to_vec(),
            Some(high.to_vec()),
        ),
        IdKeyLookup::Range { low, high: None } => ("p.id_key >= ?2", low.to_vec(), None),
    };
    let sql = format!(
        "SELECT t.session_id, p.turn_id, p.part_index, p.id_key FROM {parts_table} p \
         JOIN {turns_table} t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id \
         WHERE p.tenant_id = ?1 AND {clause} LIMIT 11"
    );
    let mut q = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(low_vec);
    if let Some(high) = high_vec {
        q = q.bind(high);
    }
    q.fetch_all(pool).await.map_err(StoreError::external)
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

    use crate::backend::domain::IdPrefixEntityKind;
    let (clean_hash, explicit_entity_kind) =
        if let Some(stripped) = trimmed.strip_prefix("conversation-part-") {
            (stripped, Some(IdPrefixEntityKind::Part))
        } else if let Some(stripped) = trimmed.strip_prefix("web-record-part-") {
            (stripped, Some(IdPrefixEntityKind::WebPart))
        } else if let Some(stripped) = trimmed.strip_prefix("part-") {
            (stripped, Some(IdPrefixEntityKind::Part))
        } else if trimmed.starts_with("conversation-session-")
            || trimmed.starts_with("web-record-session-")
            || trimmed.starts_with("session-")
            || trimmed.starts_with("conversation-turn-")
            || trimmed.starts_with("web-record-turn-")
            || trimmed.starts_with("turn-")
        {
            return Err(StoreError::NotFound(format!(
                "no part matches prefix {:?}",
                prefix_or_id
            )));
        } else {
            (trimmed, None)
        };

    let mut rows: Vec<String> = Vec::new();

    if let Some(parsed) = parse_id_key_prefix(clean_hash) {
        let allow_conversation = explicit_entity_kind.is_none()
            || explicit_entity_kind == Some(IdPrefixEntityKind::Part);
        let allow_web = explicit_entity_kind.is_none()
            || explicit_entity_kind == Some(IdPrefixEntityKind::WebPart);

        if allow_conversation {
            let part_rows = query_parts_by_id_key(
                pool,
                tenant_id,
                &parsed.lookup,
                "conversation_parts",
                "conversation_turns",
            )
            .await?;
            for (_, turn_id, part_index, id_key) in part_rows {
                let full_id = derive_conversation_part_id(&turn_id, part_index);
                let hash_part = full_id
                    .strip_prefix("conversation-part-")
                    .unwrap_or(&full_id);
                let id_key_hex = id_key
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<String>();
                if hash_part.starts_with(&parsed.normalized_hex)
                    || (parsed.normalized_hex.len() <= 16
                        && id_key_hex.starts_with(&parsed.normalized_hex))
                    || full_id.starts_with(trimmed)
                    || full_id == trimmed
                {
                    if !rows.contains(&full_id) {
                        rows.push(full_id);
                    }
                }
            }
        }
        if allow_web {
            let web_rows = query_parts_by_id_key(
                pool,
                tenant_id,
                &parsed.lookup,
                "web_record_parts",
                "web_record_turns",
            )
            .await?;
            for (_, turn_id, part_index, id_key) in web_rows {
                let full_id = crate::backend::domain::derive_standard_part_id(
                    "web-record-part",
                    &turn_id,
                    part_index,
                );
                let hash_part = full_id.strip_prefix("web-record-part-").unwrap_or(&full_id);
                let id_key_hex = id_key
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<String>();
                if hash_part.starts_with(&parsed.normalized_hex)
                    || (parsed.normalized_hex.len() <= 16
                        && id_key_hex.starts_with(&parsed.normalized_hex))
                    || full_id.starts_with(trimmed)
                    || full_id == trimmed
                {
                    if !rows.contains(&full_id) {
                        rows.push(full_id);
                    }
                }
            }
        }

        let triad_candidates = triad_locator::query_triad_parts_by_prefix(
            pool,
            tenant_id,
            &parsed.normalized_hex,
            allow_conversation,
            allow_web,
        )
        .await?;
        for candidate in triad_candidates {
            if !rows.contains(&candidate.full_id) {
                rows.push(candidate.full_id);
            }
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

pub use super::prefixes_any::resolve_any_id_to_session_id_sqlx;
