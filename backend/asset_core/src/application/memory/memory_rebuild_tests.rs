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
async fn rebuild_memory_scope_with_force_is_non_destructive_and_stays_isolated() {
    let tmp = std::env::temp_dir().join(format!("rebuild-mem-force-{}", uuid::Uuid::new_v4()));
    let db_path = tmp.join("db").join("app.db");
    std::fs::create_dir_all(tmp.join("db")).unwrap();

    // 哨兵：旧版默认记忆根目录布局（<tmp>/memories/<tenant>）里的资产必须毫发无损
    let foreign_root = tmp.join("memories");
    let foreign_tenant = foreign_root.join("default");
    std::fs::create_dir_all(foreign_tenant.join(".state/projects")).unwrap();
    let sentinel = foreign_tenant.join(".state/projects/sentinel-proj.json");
    std::fs::write(&sentinel, "{\"sentinel\":true}").unwrap();
    let foreign_md = foreign_tenant.join("MEMORY.md");
    std::fs::write(&foreign_md, "# 全局长期记忆哨兵").unwrap();

    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let storage = service.memory_storage();

    // 记忆根目录必须锚定在数据库同级的隔离目录，绝不回落到用户主目录
    assert!(storage.root_dir().starts_with(tmp.join("db")));
    assert!(storage
        .root_dir()
        .starts_with(tmp.join("db").join("memories")));

    // 在隔离目录里放派生文件，force 重建后派生视图应被重投影而非整体清空
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

    // 旧默认根目录布局里的哨兵资产不得被触碰（证明没有回落到旧路径）
    assert!(sentinel.exists());
    assert!(foreign_md.exists());
    // 服务的状态写入只发生在隔离根目录
    assert!(storage
        .root_dir()
        .starts_with(tmp.join("db").join("memories")));

    // 隔离目录结构保持不变（重建不再整体清空目录）
    assert!(storage.state_dir().exists());
    assert!(storage.state_projects_dir().exists());
    assert!(storage.recent_dir().exists());
    assert!(storage.projects_dir().exists());

    std::fs::remove_dir_all(tmp).ok();
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
