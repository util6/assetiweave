use crate::backend::store::{StoreError, StoreResult};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskJournalEntry {
    pub task_id: String,
    pub tenant_id: Option<String>,
    pub kind: String,
    pub state: String,
    pub stage_id: Option<String>,
    pub error_summary: Option<String>,
    pub result_ref: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskJournalRecord {
    pub id: i64,
    pub task_id: String,
    pub tenant_id: Option<String>,
    pub kind: String,
    pub state: String,
    pub stage_id: Option<String>,
    pub error_summary: Option<String>,
    pub result_ref: Option<String>,
    pub created_at: String,
}

pub async fn append_task_journal_entry_sqlx(
    pool: &SqlitePool,
    entry: &TaskJournalEntry,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        INSERT INTO task_journal (
            task_id, tenant_id, kind, state, stage_id, error_summary, result_ref, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
    )
    .bind(&entry.task_id)
    .bind(&entry.tenant_id)
    .bind(&entry.kind)
    .bind(&entry.state)
    .bind(&entry.stage_id)
    .bind(&entry.error_summary)
    .bind(&entry.result_ref)
    .bind(&entry.created_at)
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;
    Ok(())
}

pub async fn query_recent_task_journal_records_sqlx(
    pool: &SqlitePool,
    tenant_id: Option<&str>,
    limit: usize,
) -> StoreResult<Vec<TaskJournalRecord>> {
    let limit = limit.clamp(1, 500) as i64;
    let rows = if let Some(tenant_id) = tenant_id {
        sqlx::query(
            r#"
            SELECT id, task_id, tenant_id, kind, state, stage_id, error_summary, result_ref, created_at
            FROM task_journal
            WHERE tenant_id = ?1
            ORDER BY id DESC
            LIMIT ?2
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(StoreError::Db)?
    } else {
        sqlx::query(
            r#"
            SELECT id, task_id, tenant_id, kind, state, stage_id, error_summary, result_ref, created_at
            FROM task_journal
            ORDER BY id DESC
            LIMIT ?1
            "#,
        )
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(StoreError::Db)?
    };

    let records = rows
        .into_iter()
        .map(|row| TaskJournalRecord {
            id: row.get("id"),
            task_id: row.get("task_id"),
            tenant_id: row.get("tenant_id"),
            kind: row.get("kind"),
            state: row.get("state"),
            stage_id: row.get("stage_id"),
            error_summary: row.get("error_summary"),
            result_ref: row.get("result_ref"),
            created_at: row.get("created_at"),
        })
        .collect();

    Ok(records)
}

pub async fn reconcile_orphaned_tasks_sqlx(pool: &SqlitePool) -> StoreResult<Vec<String>> {
    // 查找最新状态依然处于 active (Pending, Running, Cancelling) 的任务
    let rows = sqlx::query(
        r#"
        SELECT task_id, tenant_id, kind
        FROM task_journal t1
        WHERE id = (
            SELECT MAX(id) FROM task_journal t2 WHERE t2.task_id = t1.task_id
        )
        AND state IN ('Pending', 'Running', 'Cancelling')
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(StoreError::Db)?;

    let mut orphaned_ids = Vec::new();
    let now = Utc::now().to_rfc3339();

    for row in rows {
        let task_id: String = row.get("task_id");
        let tenant_id: Option<String> = row.get("tenant_id");
        let kind: String = row.get("kind");

        append_task_journal_entry_sqlx(
            pool,
            &TaskJournalEntry {
                task_id: task_id.clone(),
                tenant_id: tenant_id.clone(),
                kind: kind.clone(),
                state: "Failed".to_string(),
                stage_id: None,
                error_summary: Some("interrupted_by_shutdown".to_string()),
                result_ref: None,
                created_at: now.clone(),
            },
        )
        .await?;

        orphaned_ids.push(task_id);
    }

    // 联动收敛 sources 表残留的 pending 扫描状态
    sqlx::query(
        r#"
        UPDATE sources
        SET last_scan_status = 'error: interrupted_by_shutdown'
        WHERE last_scan_status = 'pending' OR last_scan_status LIKE 'scanning%'
        "#,
    )
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;

    Ok(orphaned_ids)
}

pub async fn prune_task_journal_sqlx(
    pool: &SqlitePool,
    cutoff_rfc3339: &str,
) -> StoreResult<usize> {
    let result = sqlx::query(
        r#"
        DELETE FROM task_journal
        WHERE created_at < ?1
        "#,
    )
    .bind(cutoff_rfc3339)
    .execute(pool)
    .await
    .map_err(StoreError::Db)?;

    Ok(result.rows_affected() as usize)
}

#[cfg(test)]
#[path = "task_journal_repo_tests.rs"]
mod tests;
