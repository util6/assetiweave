//! 只读的计数与状态查询辅助。
//!
//! 从 `database` 模块抽出：那里负责连接、迁移与历史修复，纯查询逻辑混在一起会让
//! 单个文件持续膨胀。两个函数都不写库，也不参与迁移。

use sqlx::SqlitePool;

use crate::backend::store::{StoreError, StoreResult};

pub async fn latest_scan_status(pool: &SqlitePool, tenant_id: &str) -> StoreResult<String> {
    let status: Option<String> = sqlx::query_scalar(
        "SELECT last_scan_status FROM sources WHERE tenant_id = ?1 ORDER BY last_scanned_at DESC NULLS LAST LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?
    .flatten();
    Ok(status.unwrap_or_else(|| "等待首次扫描".to_string()))
}

pub async fn count_rows(pool: &SqlitePool, tenant_id: &str, table: &str) -> StoreResult<usize> {
    let count: i64 = match table {
        "sources" => sqlx::query_scalar("SELECT COUNT(*) FROM sources WHERE tenant_id = ?1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .map_err(StoreError::external)?,
        "assets" => sqlx::query_scalar("SELECT COUNT(*) FROM assets WHERE tenant_id = ?1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .map_err(StoreError::external)?,
        "profiles" => sqlx::query_scalar("SELECT COUNT(*) FROM profiles WHERE tenant_id = ?1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .map_err(StoreError::external)?,
        "navigation_state" => {
            sqlx::query_scalar("SELECT COUNT(*) FROM navigation_state WHERE tenant_id = ?1")
                .bind(tenant_id)
                .fetch_one(pool)
                .await
                .map_err(StoreError::external)?
        }
        "app_shortcut_items" => {
            sqlx::query_scalar("SELECT COUNT(*) FROM app_shortcut_items WHERE tenant_id = ?1")
                .bind(tenant_id)
                .fetch_one(pool)
                .await
                .map_err(StoreError::external)?
        }
        other => {
            return Err(StoreError::Validation(format!(
                "unsupported count table: {other}"
            )));
        }
    };
    Ok(count as usize)
}
