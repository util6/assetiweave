use crate::backend::domain::conversations::DomainEvent;
use crate::backend::store::{StoreError, StoreResult};
use chrono::Utc;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

pub(crate) async fn append_outbox_event_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    event: &DomainEvent,
) -> StoreResult<()> {
    let (tenant_id, event_type, source_id, revision_start, revision_end) = event.metadata();
    let payload =
        serde_json::to_string(event).map_err(|error| StoreError::External(error.to_string()))?;
    let event_id = match event {
        DomainEvent::ConversationSourceCommitted { event_id, .. } => event_id,
    };
    sqlx::query(
        r#"
        INSERT INTO domain_event_outbox (
            event_id, tenant_id, event_type, source_id,
            revision_start, revision_end, payload, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
    )
    .bind(event_id)
    .bind(tenant_id)
    .bind(event_type)
    .bind(source_id)
    .bind(revision_start)
    .bind(revision_end)
    .bind(payload)
    .bind(Utc::now().to_rfc3339())
    .execute(&mut **tx)
    .await
    .map_err(StoreError::Db)?;
    Ok(())
}

pub(crate) async fn load_max_outbox_seq_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<i64> {
    let val = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(seq) FROM domain_event_outbox WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(val.unwrap_or(0))
}

pub(crate) async fn load_pending_outbox_rows_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    consumer_id: &str,
    limit: i64,
) -> StoreResult<Vec<(i64, String)>> {
    let rows = sqlx::query(
        "SELECT seq, payload FROM domain_event_outbox WHERE tenant_id = ?1 AND seq > COALESCE((SELECT last_seq FROM domain_event_consumer_offsets WHERE consumer_id = ?2 AND tenant_id = ?1), 0) ORDER BY seq ASC LIMIT ?3",
    )
    .bind(tenant_id)
    .bind(consumer_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Db)?;

    rows.into_iter()
        .map(|row| {
            let seq: i64 = row
                .try_get(0)
                .map_err(|e| StoreError::External(e.to_string()))?;
            let payload: String = row
                .try_get(1)
                .map_err(|e| StoreError::External(e.to_string()))?;
            Ok((seq, payload))
        })
        .collect()
}

pub(crate) async fn count_pending_outbox_events_sqlx(
    pool: &SqlitePool,
    consumer_id: &str,
) -> StoreResult<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM domain_event_outbox AS events WHERE EXISTS (SELECT 1 FROM domain_event_consumer_offsets AS offsets WHERE offsets.consumer_id = ?1 AND offsets.tenant_id = events.tenant_id AND offsets.last_seq < events.seq)",
    )
    .bind(consumer_id)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Db)
}

pub(crate) async fn prune_retained_outbox_events_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    safe_seq: i64,
) -> StoreResult<u64> {
    let removed = sqlx::query(
        "DELETE FROM domain_event_outbox WHERE tenant_id = ?1 AND seq < ?2 AND created_at < datetime('now', '-30 days')",
    )
    .bind(tenant_id)
    .bind(safe_seq)
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(removed.rows_affected())
}

#[cfg(test)]
#[path = "outbox_repo_tests.rs"]
mod tests;
