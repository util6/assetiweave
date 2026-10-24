use sqlx::{Sqlite, Transaction};

use super::part_index::replace_turn_part_index_sqlx_tx;
use crate::backend::domain::conversations::triad::TriadPart;
use crate::backend::domain::{
    derive_conversation_part_id, derive_part_id_key, ConversationRecordKind,
};
use crate::backend::store::{StoreError, StoreResult};

pub(super) async fn replace_conversation_parts_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    turn_id: &str,
    parts: &[TriadPart],
) -> StoreResult<()> {
    for part in parts {
        let part_id = derive_conversation_part_id(turn_id, part.part_index);
        let id_key = derive_part_id_key(&part_id).unwrap_or([0u8; 8]);

        sqlx::query(
            r#"
            INSERT INTO conversation_parts (
                tenant_id, turn_id, part_index, id_key, kind, content, attrs_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(tenant_id, turn_id, part_index) DO UPDATE SET
                id_key = excluded.id_key,
                kind = excluded.kind,
                content = excluded.content,
                attrs_json = excluded.attrs_json,
                translated_text = CASE
                    WHEN COALESCE(conversation_parts.content, '') = COALESCE(excluded.content, '')
                    THEN conversation_parts.translated_text
                    ELSE NULL
                END
            "#,
        )
        .bind(tenant_id)
        .bind(turn_id)
        .bind(part.part_index)
        .bind(id_key.as_slice())
        .bind(&part.kind)
        .bind(&part.content)
        .bind(&part.attrs_json)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    }

    if parts.is_empty() {
        sqlx::query("DELETE FROM conversation_parts WHERE tenant_id = ?1 AND turn_id = ?2")
            .bind(tenant_id)
            .bind(turn_id)
            .execute(&mut **tx)
            .await
            .map_err(StoreError::external)?;
    } else {
        let retained_indices: Vec<i64> = parts.iter().map(|p| p.part_index).collect();
        let indices_json =
            serde_json::to_string(&retained_indices).map_err(StoreError::external)?;
        sqlx::query(
            "DELETE FROM conversation_parts WHERE tenant_id = ?1 AND turn_id = ?2 AND part_index NOT IN (SELECT value FROM json_each(?3))",
        )
        .bind(tenant_id)
        .bind(turn_id)
        .bind(indices_json)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    }

    replace_turn_part_index_sqlx_tx(tx, tenant_id, turn_id, ConversationRecordKind::Session)
        .await?;
    Ok(())
}
