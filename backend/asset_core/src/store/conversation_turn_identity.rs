//! 轮次位置身份的启动期原地修复。
//!
//! SQLite 内置函数算不出 SHA-256，迁移脚本只能把 `turn_index` 归一化并把列收缩到 11 列，
//! 轮次主键必须由 Rust 在迁移完成后就地重算。整个过程只看本地库，不读源文件，
//! 因此源目录已被移除的离线会话同样能完整迁移。
//!
//! 幂等：已符合 `sha256(session_id, turn_index)` 的轮次零写入跳过。

use std::collections::HashMap;

use sqlx::{AssertSqlSafe, Row, SqlitePool};

use crate::backend::domain::conversations::triad::TurnProcess;
use crate::backend::domain::{
    derive_part_id, derive_part_id_key, derive_turn_id, ConversationRecordKind,
};
use crate::backend::store::conversations::conversation_repo::part_index::replace_turn_part_index_sqlx_tx;
use crate::backend::store::conversations::snapshot::RecordTables;
use crate::backend::store::{
    database::column_exists, database::table_exists, StoreError, StoreResult,
};

/// 单事务处理的轮次数。太大容易在升级首启时长时间持有写锁。
const REPAIR_BATCH_SIZE: usize = 400;

pub(crate) async fn repair_positional_turn_identities(pool: &SqlitePool) -> StoreResult<usize> {
    let repaired = repair_family(pool, ConversationRecordKind::Session).await?;
    Ok(repaired + repair_family(pool, ConversationRecordKind::Web).await?)
}

#[derive(Debug)]
struct PendingTurn {
    tenant_id: String,
    old_id: String,
    new_id: String,
}

/// 待改写的父引用：某轮 `process.parent_turn_id` 指向一个已换轨的旧主键。
#[derive(Debug)]
struct PendingParentRef {
    tenant_id: String,
    turn_id: String,
    new_parent_id: String,
}

async fn repair_family(pool: &SqlitePool, kind: ConversationRecordKind) -> StoreResult<usize> {
    let tables = kind.record_tables();
    if !table_exists(pool, tables.turns).await? {
        return Ok(0);
    }
    // 只在 11 列终态结构上修复：还留着 external_id 说明这条库根本没跑到收缩迁移，
    // 此时动主键只会破坏调用方自己维护的旧形态数据。
    if column_exists(pool, tables.turns, "external_id").await? {
        return Ok(0);
    }

    let rows = sqlx::query(AssertSqlSafe(format!(
        "SELECT tenant_id, id, session_id, turn_index FROM {}",
        tables.turns
    )))
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    let mut pending = Vec::new();
    let mut old_to_new = HashMap::<(String, String), String>::new();
    for row in &rows {
        let tenant_id: String = row.try_get("tenant_id").map_err(StoreError::external)?;
        let old_id: String = row.try_get("id").map_err(StoreError::external)?;
        let session_id: String = row.try_get("session_id").map_err(StoreError::external)?;
        let turn_index: i64 = row.try_get("turn_index").map_err(StoreError::external)?;
        let new_id = derive_turn_id(tables.turn_id_prefix, &session_id, turn_index);
        if new_id == old_id {
            continue;
        }
        old_to_new.insert((tenant_id.clone(), old_id.clone()), new_id.clone());
        pending.push(PendingTurn {
            tenant_id,
            old_id,
            new_id,
        });
    }

    if pending.is_empty() {
        return Ok(0);
    }

    let parent_refs = load_pending_parent_refs(pool, tables, &old_to_new).await?;
    let total = pending.len();

    // 只有当前的 8 列零件结构才具备重算 part_id 与重建索引的语义；
    // 历史上被刻意改写成旧结构的库（例如回归夹具）只对齐轮次引用，不动派生索引。
    let parts_in_current_shape = column_exists(pool, tables.parts, "id_key").await?
        && column_exists(pool, tables.parts, "content").await?;

    for chunk in parent_refs.chunks(REPAIR_BATCH_SIZE) {
        let mut tx = pool.begin().await.map_err(StoreError::external)?;
        for parent_ref in chunk {
            rewrite_parent_turn_reference_sqlx_tx(&mut tx, tables, parent_ref).await?;
        }
        tx.commit().await.map_err(StoreError::external)?;
    }

    for chunk in pending.chunks(REPAIR_BATCH_SIZE) {
        let mut tx = pool.begin().await.map_err(StoreError::external)?;
        // 三段式推进：先把本批旧主键全部搬到临时值，再统一落终值，
        // 避免"某行的新主键正好是本批另一行还没让出的旧主键"这种换位冲突。
        for turn in chunk {
            repoint_parts_sqlx_tx(&mut tx, tables, turn, parts_in_current_shape).await?;
            stage_rename_turn_sqlx_tx(&mut tx, tables, turn).await?;
        }
        for turn in chunk {
            commit_turn_rename_sqlx_tx(&mut tx, tables, turn).await?;
        }
        if parts_in_current_shape {
            for turn in chunk {
                // 索引行里的 part_id 也由轮次主键派生，重建该轮次的索引是最稳的对齐方式。
                sqlx::query(
                    "DELETE FROM conversation_part_fts WHERE tenant_id = ?1 AND turn_id = ?2",
                )
                .bind(&turn.tenant_id)
                .bind(&turn.old_id)
                .execute(&mut *tx)
                .await
                .map_err(StoreError::external)?;
                replace_turn_part_index_sqlx_tx(&mut tx, &turn.tenant_id, &turn.new_id, kind)
                    .await?;
            }
        } else {
            for turn in chunk {
                sqlx::query(
                    "UPDATE conversation_part_fts SET turn_id = ?1 \
                     WHERE tenant_id = ?2 AND turn_id = ?3",
                )
                .bind(&turn.new_id)
                .bind(&turn.tenant_id)
                .bind(&turn.old_id)
                .execute(&mut *tx)
                .await
                .map_err(StoreError::external)?;
            }
        }
        tx.commit().await.map_err(StoreError::external)?;
    }
    Ok(total)
}

/// 收集所有需要换轨的父引用。
///
/// 父引用可能挂在任何一轮上（包括主键本来就合法的轮次），因此必须独立扫一遍，
/// 不能只在“自身主键要改”的轮次里顺带处理。
async fn load_pending_parent_refs(
    pool: &SqlitePool,
    tables: RecordTables,
    old_to_new: &HashMap<(String, String), String>,
) -> StoreResult<Vec<PendingParentRef>> {
    let rows = sqlx::query(AssertSqlSafe(format!(
        "SELECT tenant_id, id, json_extract(process, '$.parent_turn_id') AS parent_turn_id \
         FROM {turns} WHERE json_extract(process, '$.parent_turn_id') IS NOT NULL",
        turns = tables.turns
    )))
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    let mut refs = Vec::new();
    for row in &rows {
        let tenant_id: String = row.try_get("tenant_id").map_err(StoreError::external)?;
        let turn_id: String = row.try_get("id").map_err(StoreError::external)?;
        let parent_turn_id: Option<String> = row
            .try_get("parent_turn_id")
            .map_err(StoreError::external)?;
        let Some(parent_turn_id) = parent_turn_id else {
            continue;
        };
        let Some(new_parent_id) = old_to_new.get(&(tenant_id.clone(), parent_turn_id)) else {
            continue;
        };
        refs.push(PendingParentRef {
            tenant_id,
            turn_id,
            new_parent_id: new_parent_id.clone(),
        });
    }
    Ok(refs)
}

async fn rewrite_parent_turn_reference_sqlx_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    tables: RecordTables,
    parent_ref: &PendingParentRef,
) -> StoreResult<()> {
    // process 可能很大，只在确实需要改写的轮次上单独读取，避免每次启动都全表搬运 JSON。
    let raw: Option<String> = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT process FROM {} WHERE tenant_id = ?1 AND id = ?2",
        tables.turns
    )))
    .bind(&parent_ref.tenant_id)
    .bind(&parent_ref.turn_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::external)?
    .flatten();
    let Some(raw) = raw else {
        return Ok(());
    };
    let Ok(mut process) = serde_json::from_str::<TurnProcess>(&raw) else {
        return Ok(());
    };
    process.parent_turn_id = Some(parent_ref.new_parent_id.clone());
    let rewritten = serde_json::to_string(&process).map_err(StoreError::external)?;
    sqlx::query(AssertSqlSafe(format!(
        "UPDATE {} SET process = ?1 WHERE tenant_id = ?2 AND id = ?3",
        tables.turns
    )))
    .bind(rewritten)
    .bind(&parent_ref.tenant_id)
    .bind(&parent_ref.turn_id)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

async fn repoint_parts_sqlx_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    tables: RecordTables,
    turn: &PendingTurn,
    parts_have_id_key: bool,
) -> StoreResult<()> {
    let part_indices: Vec<i64> = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT part_index FROM {} WHERE tenant_id = ?1 AND turn_id = ?2",
        tables.parts
    )))
    .bind(&turn.tenant_id)
    .bind(&turn.old_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;

    for part_index in part_indices {
        if parts_have_id_key {
            let new_part_id = derive_part_id(tables.part_id_prefix, &turn.new_id, part_index);
            let id_key = derive_part_id_key(&new_part_id).unwrap_or([0u8; 8]);
            sqlx::query(AssertSqlSafe(format!(
                "UPDATE {parts} SET turn_id = ?1, id_key = ?2 \
                 WHERE tenant_id = ?3 AND turn_id = ?4 AND part_index = ?5",
                parts = tables.parts
            )))
            .bind(&turn.new_id)
            .bind(id_key.as_slice())
            .bind(&turn.tenant_id)
            .bind(&turn.old_id)
            .bind(part_index)
            .execute(&mut **tx)
            .await
            .map_err(StoreError::external)?;
        } else {
            sqlx::query(AssertSqlSafe(format!(
                "UPDATE {parts} SET turn_id = ?1 \
                 WHERE tenant_id = ?2 AND turn_id = ?3 AND part_index = ?4",
                parts = tables.parts
            )))
            .bind(&turn.new_id)
            .bind(&turn.tenant_id)
            .bind(&turn.old_id)
            .bind(part_index)
            .execute(&mut **tx)
            .await
            .map_err(StoreError::external)?;
        }
    }
    Ok(())
}

/// 第一段改名：旧主键先让位到临时值。
async fn stage_rename_turn_sqlx_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    tables: RecordTables,
    turn: &PendingTurn,
) -> StoreResult<()> {
    sqlx::query(AssertSqlSafe(format!(
        "UPDATE {} SET id = ?1 WHERE tenant_id = ?2 AND id = ?3",
        tables.turns
    )))
    .bind(staging_id(&turn.new_id))
    .bind(&turn.tenant_id)
    .bind(&turn.old_id)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

/// 第二段改名：临时值落到位置主键终值。
async fn commit_turn_rename_sqlx_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    tables: RecordTables,
    turn: &PendingTurn,
) -> StoreResult<()> {
    sqlx::query(AssertSqlSafe(format!(
        "UPDATE {} SET id = ?1 WHERE tenant_id = ?2 AND id = ?3",
        tables.turns
    )))
    .bind(&turn.new_id)
    .bind(&turn.tenant_id)
    .bind(staging_id(&turn.new_id))
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

fn staging_id(new_id: &str) -> String {
    format!("__pending_identity__{new_id}")
}
