use crate::backend::store::{StoreError, StoreResult};
use chrono::Utc;
use sqlx::SqlitePool;

pub(crate) async fn is_consumer_offset_initialized_sqlx(
    pool: &SqlitePool,
    consumer_id: &str,
    tenant_id: &str,
) -> StoreResult<bool> {
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS (SELECT 1 FROM domain_event_consumer_offsets WHERE consumer_id = ?1 AND tenant_id = ?2)",
    )
    .bind(consumer_id)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map(|v| v != 0)
    .map_err(StoreError::Db)?;
    Ok(exists)
}

pub(crate) async fn init_consumer_offset_if_missing_sqlx(
    pool: &SqlitePool,
    consumer_id: &str,
    tenant_id: &str,
    initial_seq: i64,
) -> StoreResult<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO domain_event_consumer_offsets (consumer_id, tenant_id, last_seq, updated_at) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(consumer_id)
    .bind(tenant_id)
    .bind(initial_seq)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(())
}

pub(crate) async fn load_consumer_last_seq_sqlx(
    pool: &SqlitePool,
    consumer_id: &str,
    tenant_id: &str,
) -> StoreResult<Option<i64>> {
    sqlx::query_scalar::<_, i64>(
        "SELECT last_seq FROM domain_event_consumer_offsets WHERE consumer_id = ?1 AND tenant_id = ?2",
    )
    .bind(consumer_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::Db)
}

pub(crate) async fn upsert_consumer_offset_sqlx(
    pool: &SqlitePool,
    consumer_id: &str,
    tenant_id: &str,
    last_seq: i64,
) -> StoreResult<()> {
    sqlx::query(
        "INSERT INTO domain_event_consumer_offsets (consumer_id, tenant_id, last_seq, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (consumer_id, tenant_id) DO UPDATE SET last_seq = excluded.last_seq, updated_at = excluded.updated_at",
    )
    .bind(consumer_id)
    .bind(tenant_id)
    .bind(last_seq)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(())
}

pub(crate) async fn load_all_tenant_ids_sqlx(pool: &SqlitePool) -> StoreResult<Vec<String>> {
    sqlx::query_scalar::<_, String>("SELECT id FROM tenants ORDER BY id")
        .fetch_all(pool)
        .await
        .map_err(StoreError::Db)
}

#[cfg(test)]
#[path = "consumer_offset_repo_tests.rs"]
mod tests;
