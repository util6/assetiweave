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

    let raw_part_id = super::projection::conversation_part_id_for_block_id(trimmed);
    let raw_clean_hash = raw_part_id
        .strip_prefix("conversation-session-")
        .or_else(|| raw_part_id.strip_prefix("conversation-question-"))
        .or_else(|| raw_part_id.strip_prefix("conversation-turn-"))
        .or_else(|| raw_part_id.strip_prefix("conversation-part-"))
        .or_else(|| raw_part_id.strip_prefix("web-record-session-"))
        .or_else(|| raw_part_id.strip_prefix("web-record-question-"))
        .or_else(|| raw_part_id.strip_prefix("web-record-turn-"))
        .or_else(|| raw_part_id.strip_prefix("web-record-part-"))
        .unwrap_or(raw_part_id);

    let verbatim_like = format!("{trimmed}%");
    let hash_like = format!("%-{clean_hash}%");
    let raw_verbatim_like = format!("{raw_part_id}%");
    let raw_hash_like = format!("%-{raw_clean_hash}%");

    let query = r#"
        WITH candidates AS (
            SELECT s.id AS entity_id, s.id AS session_id, '' AS turn_id, 'session' AS level, 'session' AS kind
            FROM conversation_sessions s
            WHERE s.tenant_id = ?1 AND (s.id = ?2 OR s.id LIKE ?3 OR s.id LIKE ?4)
            UNION
            SELECT t.id AS entity_id, t.session_id, t.id AS turn_id, 'turn' AS level, 'session' AS kind
            FROM conversation_turns t
            WHERE t.tenant_id = ?1 AND (t.id = ?2 OR t.id LIKE ?3 OR t.id LIKE ?4)
            UNION
            SELECT p.id AS entity_id, t.session_id, p.turn_id, 'card' AS level, 'session' AS kind
            FROM conversation_parts p
            JOIN conversation_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
            WHERE p.tenant_id = ?1 AND (p.id = ?2 OR p.id LIKE ?3 OR p.id LIKE ?4 OR p.id = ?5 OR p.id LIKE ?6 OR p.id LIKE ?7)
            UNION
            SELECT s.id AS entity_id, s.id AS session_id, '' AS turn_id, 'session' AS level, 'web' AS kind
            FROM web_record_sessions s
            WHERE s.tenant_id = ?1 AND (s.id = ?2 OR s.id LIKE ?3 OR s.id LIKE ?4)
            UNION
            SELECT t.id AS entity_id, t.session_id, t.id AS turn_id, 'turn' AS level, 'web' AS kind
            FROM web_record_turns t
            WHERE t.tenant_id = ?1 AND (t.id = ?2 OR t.id LIKE ?3 OR t.id LIKE ?4)
            UNION
            SELECT p.id AS entity_id, t.session_id, p.turn_id, 'card' AS level, 'web' AS kind
            FROM web_record_parts p
            JOIN web_record_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
            WHERE p.tenant_id = ?1 AND (p.id = ?2 OR p.id LIKE ?3 OR p.id LIKE ?4 OR p.id = ?5 OR p.id LIKE ?6 OR p.id LIKE ?7)
        )
        SELECT entity_id, session_id, turn_id, level, kind FROM candidates LIMIT 11
    "#;

    let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(query)
        .bind(tenant_id)
        .bind(trimmed)
        .bind(&verbatim_like)
        .bind(&hash_like)
        .bind(raw_part_id)
        .bind(&raw_verbatim_like)
        .bind(&raw_hash_like)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;

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
            card_id: trimmed.to_string(),
        }),
    }
}
