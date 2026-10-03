use super::*;
use crate::backend::domain::conversations::ConversationPartLink;
use crate::backend::domain::{ConversationPartKind, NormalizedConversationPart};
use serde_json::Value;

#[derive(Debug, FromRow)]
pub(super) struct ConversationPartLinkRow {
    pub(super) part_id: String,
    pub(super) relation: String,
    pub(super) target_kind: String,
    pub(super) target_id: String,
    pub(super) metadata_json: Option<String>,
}

impl ConversationPartLinkRow {
    pub(super) fn into_domain(self) -> ConversationPartLink {
        ConversationPartLink {
            part_id: self.part_id,
            relation: self.relation,
            target_kind: self.target_kind,
            target_id: self.target_id,
            metadata_json: self.metadata_json,
        }
    }
}

pub(crate) fn extract_part_links_for_turn(
    turn_id: &str,
    parts: &[NormalizedConversationPart],
) -> Vec<ConversationPartLink> {
    let mut links = Vec::new();
    let mut execution_commands: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut execution_results: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    let part_infos: Vec<(String, &NormalizedConversationPart)> = parts
        .iter()
        .enumerate()
        .map(|(index, part)| {
            let part_id = stable_id("conversation-part", &[turn_id, &index.to_string()]);
            (part_id, part)
        })
        .collect();

    for (part_id, part) in &part_infos {
        if let Some(exec_id) = part
            .source_execution_id
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            links.push(ConversationPartLink {
                part_id: part_id.clone(),
                relation: "execution".to_string(),
                target_kind: "execution".to_string(),
                target_id: exec_id.to_string(),
                metadata_json: None,
            });

            let is_command = part.kind == ConversationPartKind::Command
                || part.command.is_some()
                || part
                    .content_card
                    .as_ref()
                    .and_then(|c| c.semantic_role.as_deref())
                    == Some("command");

            let is_result = part.kind == ConversationPartKind::Tool
                || part.kind == ConversationPartKind::Text
                || part
                    .content_card
                    .as_ref()
                    .and_then(|c| c.semantic_role.as_deref())
                    == Some("result");

            if is_command {
                execution_commands.insert(exec_id.to_string(), part_id.clone());
            } else if is_result {
                execution_results
                    .entry(exec_id.to_string())
                    .or_default()
                    .push(part_id.clone());
            }
        }

        if let Some(target_session_id) = extract_child_session_id(part) {
            let meta = part.metadata_json.clone();
            links.push(ConversationPartLink {
                part_id: part_id.clone(),
                relation: "spawned_session".to_string(),
                target_kind: "session".to_string(),
                target_id: target_session_id,
                metadata_json: meta,
            });
        }
    }

    for (exec_id, cmd_part_id) in execution_commands {
        if let Some(res_part_ids) = execution_results.get(&exec_id) {
            let meta = serde_json::json!({ "execution_id": exec_id }).to_string();
            for res_part_id in res_part_ids {
                links.push(ConversationPartLink {
                    part_id: cmd_part_id.clone(),
                    relation: "execution_result".to_string(),
                    target_kind: "part".to_string(),
                    target_id: res_part_id.clone(),
                    metadata_json: Some(meta.clone()),
                });
                links.push(ConversationPartLink {
                    part_id: res_part_id.clone(),
                    relation: "execution_command".to_string(),
                    target_kind: "part".to_string(),
                    target_id: cmd_part_id.clone(),
                    metadata_json: Some(meta.clone()),
                });
            }
        }
    }

    links
}

fn extract_child_session_id(part: &NormalizedConversationPart) -> Option<String> {
    fn find_id(v: &Value) -> Option<String> {
        let keys = [
            "child_session_id",
            "childSessionId",
            "subagent_session_id",
            "subagentSessionId",
            "subagent_id",
            "spawned_session_id",
            "sidechain_id",
            "sidechainId",
        ];
        for k in keys {
            if let Some(id) = v.get(k).and_then(|val| val.as_str()) {
                if !id.trim().is_empty() {
                    return Some(id.trim().to_string());
                }
            }
        }
        None
    }

    if let Some(meta_str) = &part.metadata_json {
        if let Ok(v) = serde_json::from_str::<Value>(meta_str) {
            if let Some(id) = find_id(&v) {
                return Some(id);
            }
            if let Some(payload) = v.get("payload") {
                if let Some(id) = find_id(payload) {
                    return Some(id);
                }
            }
        }
    }

    if let Some(text) = &part.text {
        if let Ok(v) = serde_json::from_str::<Value>(text) {
            if let Some(id) = find_id(&v) {
                return Some(id);
            }
            if let Some(payload) = v.get("payload") {
                if let Some(id) = find_id(payload) {
                    return Some(id);
                }
            }
        }
    }

    None
}

pub(crate) async fn load_conversation_part_links_for_turn_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    turn_id: &str,
) -> StoreResult<Vec<ConversationPartLink>> {
    let rows = sqlx::query_as::<_, ConversationPartLinkRow>(
        r#"
        SELECT part_id, relation, target_kind, target_id, metadata_json
        FROM conversation_part_links
        WHERE tenant_id = ?1 AND part_id IN (
            SELECT id FROM conversation_parts WHERE tenant_id = ?1 AND turn_id = ?2
        )
        ORDER BY part_id ASC, relation ASC, target_id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(turn_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    Ok(rows
        .into_iter()
        .map(ConversationPartLinkRow::into_domain)
        .collect())
}

pub(crate) async fn find_parts_linking_to_target_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    target_kind: &str,
    target_id: &str,
) -> StoreResult<Vec<ConversationPartLink>> {
    let rows = sqlx::query_as::<_, ConversationPartLinkRow>(
        r#"
        SELECT part_id, relation, target_kind, target_id, metadata_json
        FROM conversation_part_links
        WHERE tenant_id = ?1 AND target_kind = ?2 AND target_id = ?3
        ORDER BY part_id ASC, relation ASC
        "#,
    )
    .bind(tenant_id)
    .bind(target_kind)
    .bind(target_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    Ok(rows
        .into_iter()
        .map(ConversationPartLinkRow::into_domain)
        .collect())
}

#[cfg(test)]
#[path = "part_links_tests.rs"]
mod tests;
