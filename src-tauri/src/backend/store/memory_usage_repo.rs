use crate::backend::runtime::AppResult;
use sqlx::SqlitePool;

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
    .await?;
    Ok(result.rows_affected() == 1)
}

#[cfg(test)]
#[cfg(test)]
#[path = "memory_usage_repo_tests.rs"]
mod tests;
