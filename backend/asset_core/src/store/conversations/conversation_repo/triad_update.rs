use sqlx::{AssertSqlSafe, Sqlite, SqlitePool, Transaction};

use crate::backend::domain::conversations::triad::TurnProcess;
use crate::backend::domain::conversations::{
    derive_conversation_part_id, derive_standard_part_id, ConversationRecordKind,
};
use crate::backend::store::{StoreError, StoreResult};

pub async fn update_triad_part_translation_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    part_id: &str,
    translated_text: &str,
) -> StoreResult<bool> {
    let is_web = record_kind == ConversationRecordKind::Web;
    let table = if is_web {
        "web_record_turns"
    } else {
        "conversation_turns"
    };

    let rows = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
        AssertSqlSafe(format!("SELECT id, process, response FROM {table} WHERE tenant_id = ?1 AND (process IS NOT NULL OR response IS NOT NULL)"))
    )
    .bind(tenant_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;

    for (turn_id, process_json, response_text) in rows {
        let mut process: TurnProcess = process_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        let mut updated = false;

        // Check process items
        for item in &mut process.items {
            let derived = if is_web {
                derive_standard_part_id("web-record-part", &turn_id, item.seq)
            } else {
                derive_conversation_part_id(&turn_id, item.seq)
            };
            if derived == part_id {
                item.translated_text = Some(translated_text.to_string());
                updated = true;
                break;
            }
        }

        // Check response
        if !updated {
            if let Some(ref resp) = response_text {
                if !resp.trim().is_empty() {
                    let max_seq = process.items.iter().map(|i| i.seq).max().unwrap_or(-1);
                    let resp_seq = process.response_seq.unwrap_or(max_seq + 1);
                    let derived = if is_web {
                        derive_standard_part_id("web-record-part", &turn_id, resp_seq)
                    } else {
                        derive_conversation_part_id(&turn_id, resp_seq)
                    };
                    if derived == part_id {
                        process.response_translated_text = Some(translated_text.to_string());
                        updated = true;
                    }
                }
            }
        }

        if updated {
            let new_json = serde_json::to_string(&process).map_err(StoreError::external)?;
            sqlx::query(AssertSqlSafe(format!(
                "UPDATE {table} SET process = ?1 WHERE tenant_id = ?2 AND id = ?3"
            )))
            .bind(new_json)
            .bind(tenant_id)
            .bind(&turn_id)
            .execute(&mut **tx)
            .await
            .map_err(StoreError::external)?;
            return Ok(true);
        }
    }

    Ok(false)
}

pub async fn update_triad_part_translation(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    part_id: &str,
    translated_text: &str,
) -> StoreResult<bool> {
    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    let res =
        update_triad_part_translation_tx(&mut tx, tenant_id, record_kind, part_id, translated_text)
            .await?;
    if res {
        tx.commit().await.map_err(StoreError::external)?;
    }
    Ok(res)
}
