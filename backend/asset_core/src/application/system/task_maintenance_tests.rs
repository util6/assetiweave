use crate::backend::application::AppService;
use crate::backend::store::{append_task_journal_entry_sqlx, TaskJournalEntry};
use chrono::{Duration, Utc};

#[tokio::test]
async fn test_prune_task_journal_pipeline_cleans_old_entries() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-prune-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));

    let service = AppService::open_with_db_path(db_path.clone())
        .await
        .expect("open test service");
    let pool = service.pool();

    let old_date = (Utc::now() - Duration::days(40)).to_rfc3339();
    let new_date = Utc::now().to_rfc3339();

    // 插入一条 40 天前的过期记录
    append_task_journal_entry_sqlx(
        pool,
        &TaskJournalEntry {
            task_id: "task_old".to_string(),
            tenant_id: Some(service.tenant_id().to_string()),
            kind: "Scan".to_string(),
            state: "Succeeded".to_string(),
            stage_id: None,
            error_summary: None,
            result_ref: None,
            created_at: old_date,
        },
    )
    .await
    .expect("insert old entry");

    // 插入一条当前的最新记录
    append_task_journal_entry_sqlx(
        pool,
        &TaskJournalEntry {
            task_id: "task_new".to_string(),
            tenant_id: Some(service.tenant_id().to_string()),
            kind: "Scan".to_string(),
            state: "Succeeded".to_string(),
            stage_id: None,
            error_summary: None,
            result_ref: None,
            created_at: new_date,
        },
    )
    .await
    .expect("insert new entry");

    // 执行修剪流水线（保留 30 天）
    let handle = service
        .prune_task_journal_pipeline(Some(30))
        .expect("start prune pipeline");

    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("task snapshot");

    assert_eq!(
        snapshot.state,
        crate::backend::infrastructure::tasks::TaskState::Succeeded
    );

    // 验证旧记录被修剪，新记录依然保留
    let remaining = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM task_journal")
        .fetch_one(pool)
        .await
        .expect("count remaining");

    // 初始有 2 条，修剪掉 1 条，剩余 1 条
    assert_eq!(remaining, 1);

    drop(service);
    let _ = std::fs::remove_file(db_path);
}
