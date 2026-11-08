//! 启动期路径收敛：把历史遗留的主机绝对路径回填为便携锚点。
//!
//! 契约见 ADR-0020。迁移脚本是纯 SQL，拿不到当前主机的 home 与系统数据目录，
//! 因此回填只能放在 Rust 侧、迁移执行之后。
//!
//! 收敛是**幂等**的：锚点形式的路径经 `normalize_path_for_storage` 归一后与原值相同，
//! 不产生任何写入；无法折叠的路径（`/tmp`、外接卷、系统目录）按原样保留，
//! 这是 ADR-0020 明确允许的降级，不是遗漏。
//!
//! 刻意跳过：
//! - 对话正文 / 标题 / prompt / response —— 其中出现的绝对路径属于对话内容本身；
//! - `conversation_usage_source_states.opaque_cursor` —— adapter 私有的增量游标，
//!   内部 `processed_files` 与磁盘绝对路径做集合匹配，单方面折叠会让 adapter 全量重扫。

use serde_json::{Map, Value};
use sqlx::{AssertSqlSafe, Row, SqlitePool};

use super::database::{begin_immediate, column_exists, table_exists};
use super::path_codec::normalize_path_for_storage;
use crate::backend::store::{StoreError, StoreResult};

/// 「应用状态」语义的单值路径列。
const SCALAR_PATH_COLUMNS: [(&str, &str); 5] = [
    ("assets", "stored_path"),
    ("conversation_sessions", "project_path"),
    ("conversation_sessions", "last_cwd"),
    ("conversation_sessions", "source_locator"),
    ("agent_execution_bindings", "workspace_path"),
];

/// 承载结构化路径的 JSON 列：`(表, 列, 键, 是否同时处理 items[].键)`。
const JSON_PATH_COLUMNS: [(&str, &str, &str, bool); 3] = [
    ("conversation_turns", "process", "cwd", true),
    ("web_record_turns", "process", "cwd", true),
    ("conversation_parts", "attrs_json", "file_path", false),
];

/// `capability_meta_json.source_url` 单独列出：它既可能是本地目录也可能是 URL。
const CAPABILITY_META_COLUMN: (&str, &str, &str) =
    ("app_extensions", "capability_meta_json", "source_url");

pub(crate) async fn converge_portable_paths(pool: &SqlitePool) -> StoreResult<()> {
    for (table, column) in SCALAR_PATH_COLUMNS {
        converge_scalar_path_column(pool, table, column).await?;
    }
    for (table, column, key, nested) in JSON_PATH_COLUMNS {
        converge_json_path_column(pool, table, column, key, nested).await?;
    }
    let (table, column, key) = CAPABILITY_META_COLUMN;
    converge_json_path_column(pool, table, column, key, false).await?;
    Ok(())
}

async fn converge_scalar_path_column(
    pool: &SqlitePool,
    table: &str,
    column: &str,
) -> StoreResult<()> {
    if !table_exists(pool, table).await? || !column_exists(pool, table, column).await? {
        return Ok(());
    }

    // 只捞疑似主机绝对路径的行：`/` 开头覆盖 macOS / Linux，盘符覆盖 Windows。
    // 用 GLOB 而非 LIKE，避免 SQLite 的 ASCII 大小写不敏感把 `/users/` 误判成 `/Users/`。
    let statement = format!(
        "SELECT rowid, {column} FROM {table} \
         WHERE {column} GLOB '/*' OR {column} GLOB '[A-Za-z]:*'"
    );
    let rows = sqlx::query(AssertSqlSafe(statement))
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;

    let mut pending = Vec::new();
    for row in &rows {
        let rowid: i64 = row.try_get(0).map_err(StoreError::external)?;
        let stored: String = row.try_get(1).map_err(StoreError::external)?;
        let Ok(portable) = normalize_path_for_storage(&stored) else {
            continue;
        };
        if portable != stored {
            pending.push((rowid, portable));
        }
    }
    write_pending(pool, table, column, pending).await
}

async fn converge_json_path_column(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    key: &str,
    nested_items: bool,
) -> StoreResult<()> {
    if !table_exists(pool, table).await? || !column_exists(pool, table, column).await? {
        return Ok(());
    }

    // JSON 里键值形如 `"cwd":"/Users/..."`，用 GLOB 先筛出「该键的值是绝对路径」的行，
    // 避免逐条解析整列。
    let statement = format!(
        "SELECT rowid, {column} FROM {table} \
         WHERE {column} GLOB '*\"{key}\":\"/*' OR {column} GLOB '*\"{key}\":\"[A-Za-z]:*'"
    );
    let rows = sqlx::query(AssertSqlSafe(statement))
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;

    let mut pending = Vec::new();
    for row in &rows {
        let rowid: i64 = row.try_get(0).map_err(StoreError::external)?;
        let raw: String = row.try_get(1).map_err(StoreError::external)?;
        let Ok(mut value) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        let mut changed = false;
        if let Some(map) = value.as_object_mut() {
            changed |= fold_object_key(map, key);
            if nested_items {
                if let Some(items) = map.get_mut("items").and_then(Value::as_array_mut) {
                    for item in items {
                        if let Some(obj) = item.as_object_mut() {
                            changed |= fold_object_key(obj, key);
                        }
                    }
                }
            }
        }
        if changed {
            pending.push((rowid, serde_json::to_string(&value).unwrap_or(raw)));
        }
    }
    write_pending(pool, table, column, pending).await
}

fn fold_object_key(obj: &mut Map<String, Value>, key: &str) -> bool {
    let Some(current) = obj.get(key).and_then(Value::as_str).map(str::to_string) else {
        return false;
    };
    let folded = normalize_path_for_storage(&current).unwrap_or_else(|_| current.clone());
    if folded == current {
        return false;
    }
    obj.insert(key.to_string(), Value::String(folded));
    true
}

async fn write_pending(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    pending: Vec<(i64, String)>,
) -> StoreResult<()> {
    if pending.is_empty() {
        return Ok(());
    }
    let statement = format!("UPDATE {table} SET {column} = ?1 WHERE rowid = ?2");
    let mut tx = begin_immediate(pool).await?;
    for (rowid, value) in pending {
        sqlx::query(AssertSqlSafe(statement.clone()))
            .bind(&value)
            .bind(rowid)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::external)?;
    }
    tx.commit().await.map_err(StoreError::external)?;
    Ok(())
}
