use sqlx::SqlitePool;

use super::prefixes::query_parts_by_id_key;
use crate::backend::domain::{parse_id_key_prefix, ConversationRecordKind};
use crate::backend::store::{StoreError, StoreResult};

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
        let part_rows = query_parts_by_id_key(
            pool,
            tenant_id,
            &parsed.lookup,
            "conversation_parts",
            "conversation_turns",
        )
        .await?;
        for (session_id, _, _, _) in part_rows {
            let item = (session_id, "session".to_string());
            if !rows.contains(&item) {
                rows.push(item);
            }
        }
        let web_part_rows = query_parts_by_id_key(
            pool,
            tenant_id,
            &parsed.lookup,
            "web_record_parts",
            "web_record_turns",
        )
        .await?;
        for (session_id, _, _, _) in web_part_rows {
            let item = (session_id, "web".to_string());
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
