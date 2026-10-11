use super::*;

#[tokio::test]
async fn rebuild_memory_scope_returns_not_accepted_when_generation_disabled() {
    let db_path = std::env::temp_dir().join(format!(
        "rebuild-mem-test-1-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();

    service
        .save_app_settings(serde_json::json!({
            "memory": {
                "generationEnabled": false
            }
        }))
        .await
        .unwrap();

    let result = service
        .rebuild_memory_scope(MemoryScopeRebuildParams {
            scope: MemoryScope::default(),
            target: Some(MemoryRebuildTarget::Recent),
            project_path: None,
            reason: None,
        })
        .await
        .unwrap();

    assert!(!result.accepted);
    assert!(result.scheduled_task_ids.is_empty());
}

#[tokio::test]
async fn rebuild_memory_scope_accepts_valid_scope() {
    let db_path = std::env::temp_dir().join(format!(
        "rebuild-mem-test-2-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();

    let result = service
        .rebuild_memory_scope(MemoryScopeRebuildParams {
            scope: MemoryScope::default(),
            target: Some(MemoryRebuildTarget::Recent),
            project_path: None,
            reason: None,
        })
        .await
        .unwrap();

    assert!(result.accepted);
}

#[tokio::test]
async fn rebuild_memory_scope_rejects_narrow_scopes() {
    let db_path = std::env::temp_dir().join(format!(
        "rebuild-mem-test-3-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();

    let result = service
        .rebuild_memory_scope(MemoryScopeRebuildParams {
            scope: MemoryScope {
                session_id: Some("session-1".to_string()),
                ..Default::default()
            },
            target: Some(MemoryRebuildTarget::Recent),
            project_path: None,
            reason: None,
        })
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn rebuild_memory_scope_project_returns_empty_and_no_maintenance_tasks() {
    let db_path = std::env::temp_dir().join(format!(
        "rebuild-mem-test-project-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();

    let result = service
        .rebuild_memory_scope(MemoryScopeRebuildParams {
            scope: MemoryScope::default(),
            target: Some(MemoryRebuildTarget::Project),
            project_path: Some("/test/project".to_string()),
            reason: None,
        })
        .await
        .unwrap();

    assert!(result.accepted);
    assert!(
        result.scheduled_task_ids.is_empty(),
        "L2 已剥离，不产生维护任务"
    );

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memory_maintenance_jobs WHERE purpose = 'project_consolidation'",
    )
    .fetch_one(service.db.pool())
    .await
    .unwrap();

    assert_eq!(count, 0, "维护队列中不能有 project_consolidation 任务");
}

#[tokio::test]
async fn schedule_project_memory_rebuild_is_noop() {
    let db_path = std::env::temp_dir().join(format!(
        "rebuild-mem-test-noop-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();

    let task_ids = service
        .schedule_project_memory_rebuild("/test/project", chrono::Utc::now())
        .await
        .unwrap();

    assert!(
        task_ids.is_empty(),
        "schedule_project_memory_rebuild 必须返回空任务列表"
    );
}
