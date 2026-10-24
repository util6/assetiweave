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
            SELECT s.id AS session_id, 'session' AS kind
            FROM conversation_sessions s
            WHERE s.tenant_id = ?1 AND (?5 = 1) AND (s.id = ?2 OR s.id LIKE ?3 OR s.id LIKE ?4)
            UNION
            SELECT t.session_id, 'session' AS kind
            FROM conversation_turns t
            WHERE t.tenant_id = ?1 AND (?6 = 1)
              AND (t.question_id = ?2 OR t.question_id LIKE ?3 OR t.question_id LIKE ?4)
            UNION
            SELECT t.session_id, 'session' AS kind
            FROM conversation_turns t
            WHERE t.tenant_id = ?1 AND (?6 = 1) AND (t.id = ?2 OR t.id LIKE ?3 OR t.id LIKE ?4)
            UNION
            SELECT s.id AS session_id, 'web' AS kind
            FROM web_record_sessions s
            WHERE s.tenant_id = ?1 AND (?7 = 1) AND (s.id = ?2 OR s.id LIKE ?3 OR s.id LIKE ?4)
            UNION
            SELECT t.session_id, 'web' AS kind
            FROM web_record_turns t
            WHERE t.tenant_id = ?1 AND (?8 = 1)
              AND (t.question_id = ?2 OR t.question_id LIKE ?3 OR t.question_id LIKE ?4)
            UNION
            SELECT t.session_id, 'web' AS kind
            FROM web_record_turns t
            WHERE t.tenant_id = ?1 AND (?8 = 1) AND (t.id = ?2 OR t.id LIKE ?3 OR t.id LIKE ?4)
        )
        SELECT session_id, kind FROM candidates LIMIT 11
    "#;

    let mut rows: Vec<(String, String)> = sqlx::query_as(query)
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

    let allow_session_parts =
        explicit_kind.is_none() || explicit_kind == Some(IdPrefixEntityKind::Part);
    let allow_web_parts =
        explicit_kind.is_none() || explicit_kind == Some(IdPrefixEntityKind::WebPart);

    if allow_session_parts || allow_web_parts {
        if let Some(parsed) = parse_id_key_prefix(clean_hash) {
            if allow_session_parts {
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
            }
            if allow_web_parts {
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
