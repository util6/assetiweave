use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedOutlineTarget {
    Session {
        record_kind: ConversationRecordKind,
        session_id: String,
    },
    Turn {
        record_kind: ConversationRecordKind,
        session_id: String,
        turn_id: String,
    },
    Card {
        record_kind: ConversationRecordKind,
        session_id: String,
        turn_id: String,
        card_id: String,
    },
}

pub async fn resolve_outline_target_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    any_id: &str,
) -> StoreResult<ResolvedOutlineTarget> {
    let trimmed = any_id.trim();
    if trimmed.is_empty() {
        return Err(StoreError::Validation(
            "identifier cannot be empty".to_string(),
        ));
    }

    use crate::backend::domain::IdPrefixEntityKind;

    let (explicit_kind, clean_hash) =
        if let Some(stripped) = trimmed.strip_prefix("conversation-session-") {
            (Some(IdPrefixEntityKind::Session), stripped)
        } else if let Some(stripped) = trimmed.strip_prefix("web-record-session-") {
            (Some(IdPrefixEntityKind::WebSession), stripped)
        } else if let Some(stripped) = trimmed.strip_prefix("conversation-turn-") {
            (Some(IdPrefixEntityKind::Turn), stripped)
        } else if let Some(stripped) = trimmed.strip_prefix("web-record-turn-") {
            (Some(IdPrefixEntityKind::WebTurn), stripped)
        } else if let Some(stripped) = trimmed.strip_prefix("conversation-part-") {
            (Some(IdPrefixEntityKind::Part), stripped)
        } else if let Some(stripped) = trimmed.strip_prefix("web-record-part-") {
            (Some(IdPrefixEntityKind::WebPart), stripped)
        } else if let Some(stripped) = trimmed.strip_prefix("session-") {
            (Some(IdPrefixEntityKind::Session), stripped)
        } else if let Some(stripped) = trimmed.strip_prefix("turn-") {
            (Some(IdPrefixEntityKind::Turn), stripped)
        } else if let Some(stripped) = trimmed.strip_prefix("part-") {
            (Some(IdPrefixEntityKind::Part), stripped)
        } else {
            (None, trimmed)
        };

    let raw_part_id = super::projection::conversation_part_id_for_block_id(trimmed);

    let verbatim_like = format!("{trimmed}%");
    let hash_like = match explicit_kind {
        Some(IdPrefixEntityKind::Session) => format!("conversation-session-{clean_hash}%"),
        Some(IdPrefixEntityKind::WebSession) => format!("web-record-session-{clean_hash}%"),
        Some(IdPrefixEntityKind::Turn) => format!("conversation-turn-{clean_hash}%"),
        Some(IdPrefixEntityKind::WebTurn) => format!("web-record-turn-{clean_hash}%"),
        _ => format!("%-{clean_hash}%"),
    };

    let allow_cs =
        i64::from(explicit_kind.is_none() || explicit_kind == Some(IdPrefixEntityKind::Session));
    let allow_ct =
        i64::from(explicit_kind.is_none() || explicit_kind == Some(IdPrefixEntityKind::Turn));
    let allow_ws =
        i64::from(explicit_kind.is_none() || explicit_kind == Some(IdPrefixEntityKind::WebSession));
    let allow_wt =
        i64::from(explicit_kind.is_none() || explicit_kind == Some(IdPrefixEntityKind::WebTurn));

    let query = r#"
        WITH candidates AS (
            SELECT s.id AS entity_id, s.id AS session_id, '' AS turn_id, 'session' AS level, 'session' AS kind
            FROM conversation_sessions s
            WHERE s.tenant_id = ?1 AND (?5 = 1) AND (s.id = ?2 OR s.id LIKE ?3 OR s.id LIKE ?4)
            UNION
            SELECT t.id AS entity_id, t.session_id, t.id AS turn_id, 'turn' AS level, 'session' AS kind
            FROM conversation_turns t
            WHERE t.tenant_id = ?1 AND (?6 = 1) AND (t.id = ?2 OR t.id LIKE ?3 OR t.id LIKE ?4)
            UNION
            SELECT s.id AS entity_id, s.id AS session_id, '' AS turn_id, 'session' AS level, 'web' AS kind
            FROM web_record_sessions s
            WHERE s.tenant_id = ?1 AND (?7 = 1) AND (s.id = ?2 OR s.id LIKE ?3 OR s.id LIKE ?4)
            UNION
            SELECT t.id AS entity_id, t.session_id, t.id AS turn_id, 'turn' AS level, 'web' AS kind
            FROM web_record_turns t
            WHERE t.tenant_id = ?1 AND (?8 = 1) AND (t.id = ?2 OR t.id LIKE ?3 OR t.id LIKE ?4)
        )
        SELECT entity_id, session_id, turn_id, level, kind FROM candidates LIMIT 11
    "#;

    let mut rows: Vec<(String, String, String, String, String)> = sqlx::query_as(query)
        .bind(tenant_id)
        .bind(trimmed)
        .bind(&verbatim_like)
        .bind(&hash_like)
        .bind(allow_cs)
        .bind(allow_ct)
        .bind(allow_ws)
        .bind(allow_wt)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;

    if let Some(parsed) = crate::backend::domain::parse_id_key_prefix(trimmed) {
        let allow_session_parts = match explicit_kind {
            Some(IdPrefixEntityKind::Part) => true,
            None => match parsed.entity_kind {
                Some(IdPrefixEntityKind::Part) | None => true,
                _ => false,
            },
            _ => false,
        };
        let allow_web_parts = match explicit_kind {
            Some(IdPrefixEntityKind::WebPart) => true,
            None => match parsed.entity_kind {
                Some(IdPrefixEntityKind::WebPart) | None => true,
                _ => false,
            },
            _ => false,
        };

        if allow_session_parts {
            let parts_query = match parsed.lookup {
                crate::backend::domain::IdKeyLookup::Exact(key) => {
                    sqlx::query_as::<_, (String, String, i64)>(
                        "SELECT t.session_id, p.turn_id, p.part_index \
                     FROM conversation_parts p \
                     JOIN conversation_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id \
                     WHERE p.tenant_id = ?1 AND p.id_key = ?2 LIMIT 11",
                    )
                    .bind(tenant_id)
                    .bind(key.to_vec())
                }
                crate::backend::domain::IdKeyLookup::Range {
                    low,
                    high: Some(high),
                } => sqlx::query_as::<_, (String, String, i64)>(
                    "SELECT t.session_id, p.turn_id, p.part_index \
                     FROM conversation_parts p \
                     JOIN conversation_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id \
                     WHERE p.tenant_id = ?1 AND p.id_key >= ?2 AND p.id_key < ?3 LIMIT 11",
                )
                .bind(tenant_id)
                .bind(low.to_vec())
                .bind(high.to_vec()),
                crate::backend::domain::IdKeyLookup::Range { low, high: None } => {
                    sqlx::query_as::<_, (String, String, i64)>(
                        "SELECT t.session_id, p.turn_id, p.part_index \
                     FROM conversation_parts p \
                     JOIN conversation_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id \
                     WHERE p.tenant_id = ?1 AND p.id_key >= ?2 LIMIT 11",
                    )
                    .bind(tenant_id)
                    .bind(low.to_vec())
                }
            };

            let part_matches = parts_query
                .fetch_all(pool)
                .await
                .map_err(StoreError::external)?;
            for (session_id, turn_id, part_index) in part_matches {
                let full_id =
                    crate::backend::domain::derive_conversation_part_id(&turn_id, part_index);
                let hash_part = full_id
                    .strip_prefix("conversation-part-")
                    .unwrap_or(&full_id);
                if hash_part.starts_with(&parsed.normalized_hex)
                    || full_id.starts_with(trimmed)
                    || full_id == raw_part_id
                {
                    let candidate_id = if let Some(order) = parsed.node_order {
                        format!("{full_id}-node-{order}")
                    } else {
                        full_id
                    };
                    rows.push((
                        candidate_id,
                        session_id,
                        turn_id,
                        "card".to_string(),
                        "session".to_string(),
                    ));
                }
            }
        }

        if allow_web_parts {
            let web_parts_query = match parsed.lookup {
                crate::backend::domain::IdKeyLookup::Exact(key) => {
                    sqlx::query_as::<_, (String, String, i64)>(
                        "SELECT t.session_id, p.turn_id, p.part_index \
                 FROM web_record_parts p \
                 JOIN web_record_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id \
                 WHERE p.tenant_id = ?1 AND p.id_key = ?2 LIMIT 11",
                    )
                    .bind(tenant_id)
                    .bind(key.to_vec())
                }
                crate::backend::domain::IdKeyLookup::Range {
                    low,
                    high: Some(high),
                } => sqlx::query_as::<_, (String, String, i64)>(
                    "SELECT t.session_id, p.turn_id, p.part_index \
                 FROM web_record_parts p \
                 JOIN web_record_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id \
                 WHERE p.tenant_id = ?1 AND p.id_key >= ?2 AND p.id_key < ?3 LIMIT 11",
                )
                .bind(tenant_id)
                .bind(low.to_vec())
                .bind(high.to_vec()),
                crate::backend::domain::IdKeyLookup::Range { low, high: None } => {
                    sqlx::query_as::<_, (String, String, i64)>(
                        "SELECT t.session_id, p.turn_id, p.part_index \
                     FROM web_record_parts p \
                     JOIN web_record_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id \
                     WHERE p.tenant_id = ?1 AND p.id_key >= ?2 LIMIT 11",
                    )
                    .bind(tenant_id)
                    .bind(low.to_vec())
                }
            };

            let web_part_matches = web_parts_query
                .fetch_all(pool)
                .await
                .map_err(StoreError::external)?;
            for (session_id, turn_id, part_index) in web_part_matches {
                let full_id = crate::backend::domain::derive_standard_part_id(
                    "web-record-part",
                    &turn_id,
                    part_index,
                );
                let hash_part = full_id.strip_prefix("web-record-part-").unwrap_or(&full_id);
                if hash_part.starts_with(&parsed.normalized_hex)
                    || full_id.starts_with(trimmed)
                    || full_id == raw_part_id
                {
                    let candidate_id = if let Some(order) = parsed.node_order {
                        format!("{full_id}-node-{order}")
                    } else {
                        full_id
                    };
                    rows.push((
                        candidate_id,
                        session_id,
                        turn_id,
                        "card".to_string(),
                        "web".to_string(),
                    ));
                }
            }
        }
    }

    if rows.is_empty() {
        return Err(StoreError::NotFound(format!(
            "no session, turn, or card found matching identifier {:?}",
            any_id
        )));
    }

    if rows.len() > 1 {
        let max_display = std::cmp::min(rows.len(), 5);
        let examples = rows[..max_display]
            .iter()
            .map(|(id, _, _, _, _)| id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let indicator = if rows.len() > 5 { "..." } else { "" };
        return Err(StoreError::Conflict(format!(
            "ambiguous identifier {:?}: {} items match (e.g. {}{})",
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

    let (entity_id, session_id, turn_id, level, kind_str) = &rows[0];
    let record_kind = if kind_str == "web" {
        ConversationRecordKind::Web
    } else {
        ConversationRecordKind::Session
    };

    match level.as_str() {
        "session" => Ok(ResolvedOutlineTarget::Session {
            record_kind,
            session_id: entity_id.clone(),
        }),
        "turn" => Ok(ResolvedOutlineTarget::Turn {
            record_kind,
            session_id: session_id.clone(),
            turn_id: entity_id.clone(),
        }),
        _ => Ok(ResolvedOutlineTarget::Card {
            record_kind,
            session_id: session_id.clone(),
            turn_id: turn_id.clone(),
            card_id: entity_id.clone(),
        }),
    }
}
