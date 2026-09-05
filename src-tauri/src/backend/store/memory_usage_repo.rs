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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_memory_usage_events_record_count_and_list() {
        let path = std::env::temp_dir().join(format!(
            "assetiweave-memory-usage-test-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open_initialized_async(&path)
            .await
            .expect("open fixture");
        let pool = database.pool();

        // 1. Initial count is 0
        let count = count_memory_usage_events_sqlx(pool, "default", "recall_session", "session-1")
            .await
            .expect("count");
        assert_eq!(count, 0);

        // 2. Record first event
        let recorded = record_memory_usage_event_sqlx(
            pool,
            "default",
            "recall_session",
            "session-1",
            "recall_turn",
            "turn-1",
            "2026-09-01T10:00:00Z",
        )
        .await
        .expect("record event");
        assert!(recorded);

        // 3. Record duplicate event with same primary key
        let duplicate = record_memory_usage_event_sqlx(
            pool,
            "default",
            "recall_session",
            "session-1",
            "recall_turn",
            "turn-1",
            "2026-09-01T10:00:00Z",
        )
        .await
        .expect("record duplicate event");
        assert!(!duplicate);

        // 4. Record second event for different turn
        let second = record_memory_usage_event_sqlx(
            pool,
            "default",
            "recall_session",
            "session-1",
            "recall_turn",
            "turn-2",
            "2026-09-01T10:05:00Z",
        )
        .await
        .expect("record second event");
        assert!(second);

        // 5. Check count for default tenant
        let count = count_memory_usage_events_sqlx(pool, "default", "recall_session", "session-1")
            .await
            .expect("count");
        assert_eq!(count, 2);

        // 6. Check list for default tenant
        let list = list_memory_usage_events_sqlx(pool, "default", "recall_session", "session-1")
            .await
            .expect("list");
        assert_eq!(
            list,
            vec![
                (
                    "recall_turn".to_string(),
                    "turn-1".to_string(),
                    "2026-09-01T10:00:00Z".to_string()
                ),
                (
                    "recall_turn".to_string(),
                    "turn-2".to_string(),
                    "2026-09-01T10:05:00Z".to_string()
                ),
            ]
        );

        // 7. Insert second tenant to test tenant isolation
        sqlx::query(
            "INSERT INTO tenants (id, slug, name, kind, status, created_at, updated_at) VALUES ('tenant-b', 'tenant-b', 'Tenant B', 'local_workspace', 'active', '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z')",
        )
        .execute(pool)
        .await
        .expect("insert tenant-b");

        let count_b =
            count_memory_usage_events_sqlx(pool, "tenant-b", "recall_session", "session-1")
                .await
                .expect("count tenant b");
        assert_eq!(count_b, 0);

        let list_b = list_memory_usage_events_sqlx(pool, "tenant-b", "recall_session", "session-1")
            .await
            .expect("list tenant b");
        assert!(list_b.is_empty());

        drop(database);
        std::fs::remove_file(path).ok();
    }
}
