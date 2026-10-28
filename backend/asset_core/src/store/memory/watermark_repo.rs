use sqlx::{Row, SqlitePool};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemorySessionWatermarkRecord {
    pub tenant_id: String,
    pub session_id: String,
    pub project_path: Option<String>,
    pub source_updated_at: String,
    pub summary_rel_path: String,
    pub has_signal: bool,
    pub updated_at: String,
}

pub async fn load_watermarks_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> sqlx::Result<HashMap<String, MemorySessionWatermarkRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT tenant_id, session_id, project_path, source_updated_at, summary_rel_path, has_signal, updated_at
        FROM memory_session_watermarks
        WHERE tenant_id = ?1
        "#,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;

    let mut map = HashMap::new();
    for row in rows {
        let has_signal_int: i64 = row.get("has_signal");
        let record = MemorySessionWatermarkRecord {
            tenant_id: row.get("tenant_id"),
            session_id: row.get("session_id"),
            project_path: row.get("project_path"),
            source_updated_at: row.get("source_updated_at"),
            summary_rel_path: row.get("summary_rel_path"),
            has_signal: has_signal_int != 0,
            updated_at: row.get("updated_at"),
        };
        map.insert(record.session_id.clone(), record);
    }
    Ok(map)
}

pub async fn upsert_watermark_sqlx(
    pool: &SqlitePool,
    record: &MemorySessionWatermarkRecord,
) -> sqlx::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO memory_session_watermarks (
            tenant_id, session_id, project_path, source_updated_at, summary_rel_path, has_signal, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(tenant_id, session_id) DO UPDATE SET
            project_path = excluded.project_path,
            source_updated_at = excluded.source_updated_at,
            summary_rel_path = excluded.summary_rel_path,
            has_signal = excluded.has_signal,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&record.tenant_id)
    .bind(&record.session_id)
    .bind(&record.project_path)
    .bind(&record.source_updated_at)
    .bind(&record.summary_rel_path)
    .bind(if record.has_signal { 1i64 } else { 0i64 })
    .bind(&record.updated_at)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn clear_watermarks_sqlx(pool: &SqlitePool, tenant_id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM memory_session_watermarks WHERE tenant_id = ?1")
        .bind(tenant_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
#[path = "watermark_repo_tests.rs"]
mod tests;
