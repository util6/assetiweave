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
            force: false,
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
            force: false,
        })
        .await
        .unwrap();

    assert!(result.accepted);
    assert_eq!(result.scheduled_task_ids.len(), 1);
    assert!(result.scheduled_task_ids[0].starts_with("memory-pipeline:"));
}

#[tokio::test]
async fn rebuild_memory_scope_with_force_cleans_storage_and_watermarks() {
    let temp_mem_root =
        std::env::temp_dir().join(format!("test-mem-rebuild-{}", uuid::Uuid::new_v4()));
    std::env::set_var("ASSETIWEAVE_MEMORY_ROOT", &temp_mem_root);

    let db_path = std::env::temp_dir().join(format!(
        "rebuild-mem-test-force-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let storage =
        crate::backend::infrastructure::memory::storage::MemoryStorage::new(service.tenant_id());
    storage
        .write_markdown_atomic("sessions/old-sess.md", "# Old session")
        .unwrap();
    assert!(storage.root_dir().join("sessions/old-sess.md").exists());

    let result = service
        .rebuild_memory_scope(MemoryScopeRebuildParams {
            scope: MemoryScope::default(),
            target: Some(MemoryRebuildTarget::All),
            project_path: None,
            reason: Some(MemoryRebuildReason::Manual),
            force: true,
        })
        .await
        .unwrap();

    assert!(result.accepted);
    let task_id = &result.scheduled_task_ids[0];
    let _ = service.wait_for_memory_task(task_id).await;

    assert!(!storage.root_dir().join("sessions/old-sess.md").exists());
    assert!(storage.sessions_dir().exists());
    assert!(storage.recent_dir().exists());
    assert!(storage.projects_dir().exists());

    std::fs::remove_dir_all(temp_mem_root).ok();
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
            force: false,
        })
        .await;

    assert!(result.is_err());
}
