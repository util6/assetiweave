use super::*;
use crate::backend::store::test_support::create_test_pool;

#[tokio::test]
async fn test_task_journal_append_and_reconcile() {
    let pool = create_test_pool().await;
    let task_id = "test-task-1";
    let now = Utc::now().to_rfc3339();

    append_task_journal_entry_sqlx(
        &pool,
        &TaskJournalEntry {
            task_id: task_id.to_string(),
            tenant_id: Some("default".to_string()),
            kind: "Scan".to_string(),
            state: "Running".to_string(),
            stage_id: Some("scan_files".to_string()),
            error_summary: None,
            result_ref: None,
            created_at: now.clone(),
        },
    )
    .await
    .expect("append should succeed");

    let records = query_recent_task_journal_records_sqlx(&pool, Some("default"), 10)
        .await
        .expect("query should succeed");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].task_id, task_id);
    assert_eq!(records[0].state, "Running");

    // 测试冷启动自愈
    let orphaned = reconcile_orphaned_tasks_sqlx(&pool)
        .await
        .expect("reconcile should succeed");
    assert_eq!(orphaned, vec![task_id.to_string()]);

    let updated_records = query_recent_task_journal_records_sqlx(&pool, Some("default"), 10)
        .await
        .expect("query should succeed");
    assert_eq!(updated_records.len(), 2);
    assert_eq!(updated_records[0].state, "Failed");
    assert_eq!(
        updated_records[0].error_summary.as_deref(),
        Some("interrupted_by_shutdown")
    );

    // 再次自愈，不再有 orphaned
    let empty = reconcile_orphaned_tasks_sqlx(&pool)
        .await
        .expect("second reconcile should find none");
    assert!(empty.is_empty());
}
