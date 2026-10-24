use sqlx::{Sqlite, Transaction};

use super::part_index::replace_turn_part_index_sqlx_tx;
use crate::backend::domain::{
    derive_conversation_part_id, derive_part_id_key, ConversationPartKind, ConversationRecordKind,
    NormalizedConversationPart,
};
use crate::backend::store::codec::encode_enum;
use crate::backend::store::{StoreError, StoreResult};

pub(super) async fn replace_conversation_parts_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    turn_id: &str,
    parts: &[NormalizedConversationPart],
) -> StoreResult<()> {
    for (index, part) in parts.iter().enumerate() {
        let part_index = index as i64;
        let part_id = derive_conversation_part_id(turn_id, part_index);
        let id_key = derive_part_id_key(&part_id).unwrap_or([0u8; 8]);
        let card_kind = part.content_card.as_ref().map(|c| c.kind.clone());

        let (content, summary) =
            if part.kind == ConversationPartKind::Command && part.command.is_some() {
                (part.command.clone(), part.text.clone())
            } else {
                (part.text.clone(), None)
            };

        let mut attrs_map = serde_json::Map::new();
        if part.kind == ConversationPartKind::Command && part.command.is_none() {
            attrs_map.insert("has_command".to_string(), serde_json::Value::Bool(false));
        }

        if let Some(ref val) = part.language {
            if !val.is_empty() {
                attrs_map.insert(
                    "language".to_string(),
                    serde_json::Value::String(val.clone()),
                );
            }
        }
        if let Some(ref val) = part.cwd {
            if !val.is_empty() {
                attrs_map.insert("cwd".to_string(), serde_json::Value::String(val.clone()));
            }
        }
        if let Some(ref val) = part.status {
            if !val.is_empty() {
                attrs_map.insert("status".to_string(), serde_json::Value::String(val.clone()));
            }
        }
        if let Some(val) = part.exit_code {
            attrs_map.insert(
                "exit_code".to_string(),
                serde_json::Value::Number(val.into()),
            );
        }
        if let Some(ref val) = part.command_label {
            if !val.is_empty() {
                attrs_map.insert(
                    "command_label".to_string(),
                    serde_json::Value::String(val.clone()),
                );
            }
        }
        if let Some(ref val) = part.source_execution_id {
            if !val.is_empty() {
                attrs_map.insert(
                    "exec_id".to_string(),
                    serde_json::Value::String(val.clone()),
                );
            }
        }
        if let Some(ref val) = summary {
            if !val.trim().is_empty() {
                attrs_map.insert(
                    "summary".to_string(),
                    serde_json::Value::String(val.clone()),
                );
            }
        }
        if let Some(ref card) = part.content_card {
            if let Some(ref renderer) = card.renderer {
                attrs_map.insert(
                    "renderer".to_string(),
                    serde_json::Value::String(renderer.clone()),
                );
            }
            if let Some(ref semantic_role) = card.semantic_role {
                attrs_map.insert(
                    "semantic_role".to_string(),
                    serde_json::Value::String(semantic_role.clone()),
                );
            }
        }

        // Extract metadata_json fields into attrs_json for unified access
        if let Some(ref meta_str) = part.metadata_json {
            if let Ok(serde_json::Value::Object(map)) =
                serde_json::from_str::<serde_json::Value>(meta_str)
            {
                if let Some(serde_json::Value::String(sig)) = map.get("signal") {
                    if !sig.is_empty() {
                        attrs_map
                            .insert("signal".to_string(), serde_json::Value::String(sig.clone()));
                    }
                }
                if let Some(serde_json::Value::String(fp)) = map.get("file_path") {
                    if !fp.is_empty() {
                        attrs_map.insert(
                            "file_path".to_string(),
                            serde_json::Value::String(fp.clone()),
                        );
                    }
                }
                if let Some(serde_json::Value::String(tn)) = map.get("tool_name") {
                    if !tn.is_empty() {
                        attrs_map.insert(
                            "tool_name".to_string(),
                            serde_json::Value::String(tn.clone()),
                        );
                    }
                }
            }
        }

        let attrs_json = if attrs_map.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&serde_json::Value::Object(attrs_map))
                    .map_err(StoreError::external)?,
            )
        };

        sqlx::query(
            r#"
            INSERT INTO conversation_parts (
                tenant_id, turn_id, part_index, id_key, role, kind, card_kind,
                content, attrs_json, metadata_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(tenant_id, turn_id, part_index) DO UPDATE SET
                id_key = excluded.id_key,
                role = excluded.role,
                kind = excluded.kind,
                card_kind = excluded.card_kind,
                content = excluded.content,
                attrs_json = excluded.attrs_json,
                metadata_json = excluded.metadata_json,
                translated_text = CASE
                    WHEN COALESCE(conversation_parts.content, '') = COALESCE(excluded.content, '')
                    THEN conversation_parts.translated_text
                    ELSE NULL
                END
            "#,
        )
        .bind(tenant_id)
        .bind(turn_id)
        .bind(part_index)
        .bind(id_key.as_slice())
        .bind(encode_enum(part.role)?)
        .bind(encode_enum(part.kind)?)
        .bind(card_kind)
        .bind(content)
        .bind(attrs_json)
        .bind(&part.metadata_json)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    }

    // 清理尾部多余零件
    sqlx::query(
        "DELETE FROM conversation_parts WHERE tenant_id = ?1 AND turn_id = ?2 AND part_index >= ?3",
    )
    .bind(tenant_id)
    .bind(turn_id)
    .bind(parts.len() as i64)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;

    // 索引随零件写入增量维护：只重建这个轮次的索引行，不触碰其它轮次。
    replace_turn_part_index_sqlx_tx(tx, tenant_id, turn_id, ConversationRecordKind::Session)
        .await?;
    Ok(())
}
