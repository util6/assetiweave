use crate::backend::runtime::{AppError, AppResult};
use sqlx::{Row, SqlitePool};

pub(crate) async fn record_memory_usage_event_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    memory_kind: &str,
    memory_id: &str,
    use_kind: &str,
    use_id: &str,
    used_at: &str,
) -> AppResult<bool> {
    let result = sqlx::query(
        "INSERT OR IGNORE INTO memory_usage_events (tenant_id, memory_kind, memory_id, use_kind, use_id, used_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(tenant_id)
    .bind(memory_kind)
    .bind(memory_id)
    .bind(use_kind)
    .bind(use_id)
    .bind(used_at)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn count_memory_usage_events_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    memory_kind: &str,
    memory_id: &str,
) -> AppResult<i64> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM memory_usage_events WHERE tenant_id = ?1 AND memory_kind = ?2 AND memory_id = ?3",
    )
    .bind(tenant_id)
    .bind(memory_kind)
    .bind(memory_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::external)
}

pub(crate) async fn list_memory_usage_events_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    memory_kind: &str,
    memory_id: &str,
) -> AppResult<Vec<(String, String, String)>> {
    let rows = sqlx::query(
        "SELECT use_kind, use_id, used_at FROM memory_usage_events WHERE tenant_id = ?1 AND memory_kind = ?2 AND memory_id = ?3 ORDER BY used_at, use_kind, use_id",
    )
    .bind(tenant_id)
    .bind(memory_kind)
    .bind(memory_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    rows.into_iter()
        .map(|row| {
            Ok((
                row.try_get("use_kind").map_err(AppError::external)?,
                row.try_get("use_id").map_err(AppError::external)?,
                row.try_get("used_at").map_err(AppError::external)?,
            ))
        })
        .collect()
}
