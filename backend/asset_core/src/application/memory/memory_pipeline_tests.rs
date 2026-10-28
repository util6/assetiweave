use super::*;
use crate::backend::application::AppService;
use crate::backend::infrastructure::tasks::stages;
use chrono::Utc;

#[tokio::test]
async fn test_start_memory_pipeline_creates_single_task_with_three_stages() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-1-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();

    let handle = service
        .start_memory_pipeline(MemoryPipelineParams::background())
        .await
        .unwrap();

    assert_eq!(
        handle.task_id(),
        format!("memory-pipeline:{}", service.tenant_id())
    );

    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("pipeline snapshot");

    assert_eq!(snapshot.stages.len(), 3);
    assert_eq!(snapshot.stages[0].id, stages::SESSION_FACTS);
    assert_eq!(snapshot.stages[1].id, stages::RECENT_SNAPSHOT);
    assert_eq!(snapshot.stages[2].id, stages::GLOBAL_CONSOLIDATION);
    assert_eq!(
        snapshot.state,
        crate::backend::infrastructure::tasks::TaskState::Succeeded
    );
}

#[tokio::test]
async fn test_concurrency_rejection_returns_conflict() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-2-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();

    let task_id = format!("memory-pipeline:{}", service.tenant_id());
    // 手动注册一个活跃任务模拟并发
    let spec = crate::backend::infrastructure::tasks::TaskSpec::new(
        crate::backend::infrastructure::tasks::TaskKind::Memory,
        Some(task_id.clone()),
    )
    .with_task_id(task_id.clone())
    .with_tenant_id(service.tenant_id().to_string())
    .with_conflict_key(format!("memory:{}", service.tenant_id()))
    .with_conflict_policy(crate::backend::infrastructure::tasks::ConflictPolicy::Reject);

    let _h = service
        .runtime
        .task_runtime()
        .run(spec, move |_cx| async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            Ok(crate::backend::infrastructure::tasks::TaskOutput::new(
                serde_json::json!({}),
            ))
        })
        .unwrap();

    assert!(service.is_memory_pipeline_active());

    // 第二次调用必须返回 Conflict
    let conflict_res = service
        .start_memory_pipeline(MemoryPipelineParams::background())
        .await;

    assert!(conflict_res.is_err());
    assert!(matches!(conflict_res.unwrap_err(), AppError::Conflict(_)));
}

#[tokio::test]
async fn test_has_pending_memory_work_returns_false_when_clean() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-5-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();

    let now = Utc::now();
    let has_work = service.has_pending_memory_work(now).await.unwrap();
    assert!(
        !has_work,
        "clean database must NOT have pending memory work"
    );
}

#[tokio::test]
async fn test_pipeline_records_steps_for_all_executed_stages() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-6-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();

    let handle = service
        .start_memory_pipeline(MemoryPipelineParams::background())
        .await
        .unwrap();

    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("pipeline snapshot");

    // 阶段 1: SESSION_FACTS
    let stage1 = snapshot
        .stages
        .iter()
        .find(|s| s.id == stages::SESSION_FACTS)
        .expect("stage 1");
    assert_eq!(stage1.status, StageStatus::Succeeded);
    assert!(!stage1.steps.is_empty(), "stage 1 must record steps");

    // 阶段 2: RECENT_SNAPSHOT
    let stage2 = snapshot
        .stages
        .iter()
        .find(|s| s.id == stages::RECENT_SNAPSHOT)
        .expect("stage 2");
    assert_eq!(stage2.status, StageStatus::Succeeded);
    assert!(!stage2.steps.is_empty(), "stage 2 must record steps");

    // 阶段 3: GLOBAL_CONSOLIDATION
    let stage3 = snapshot
        .stages
        .iter()
        .find(|s| s.id == stages::GLOBAL_CONSOLIDATION)
        .expect("stage 3");
    assert_eq!(stage3.status, StageStatus::Succeeded);
    assert!(!stage3.steps.is_empty(), "stage 3 must record steps");
}

#[tokio::test]
async fn test_force_rebuild_resets_watermarks() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-rebuild-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let pool = service.db.pool();

    // 插入假水位记录
    let fake_rec = store::MemorySessionWatermarkRecord {
        tenant_id: service.tenant_id().to_string(),
        session_id: "fake-sess-1".to_string(),
        project_path: None,
        source_updated_at: "2026-10-26T12:00:00Z".to_string(),
        summary_rel_path: "sessions/fake-sess-1.md".to_string(),
        has_signal: true,
        updated_at: "2026-10-26T12:00:00Z".to_string(),
    };
    store::upsert_watermark_sqlx(pool, &fake_rec).await.unwrap();

    let watermarks = store::load_watermarks_sqlx(pool, service.tenant_id())
        .await
        .unwrap();
    assert_eq!(watermarks.len(), 1);

    let temp_mem_root =
        std::env::temp_dir().join(format!("test-mem-root-fb-{}", uuid::Uuid::new_v4()));
    std::env::set_var("ASSETIWEAVE_MEMORY_ROOT", &temp_mem_root);

    let storage =
        crate::backend::infrastructure::memory::storage::MemoryStorage::new(service.tenant_id());
    storage
        .write_markdown_atomic("sessions/fake-sess-1.md", "# Fake session")
        .unwrap();
    storage
        .write_markdown_atomic("recent/latest.md", "# Fake recent")
        .unwrap();
    assert!(storage.root_dir().join("sessions/fake-sess-1.md").exists());

    // 启动带 force_rebuild 的流水线
    let handle = service
        .start_memory_pipeline(MemoryPipelineParams::manual(true))
        .await
        .unwrap();

    let _ = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("pipeline snapshot");

    // 水位表必须已被清空
    let watermarks_after = store::load_watermarks_sqlx(pool, service.tenant_id())
        .await
        .unwrap();
    assert_eq!(watermarks_after.len(), 0);

    // 磁盘存储目录必须已被清理并重新初始化干净分层结构
    assert!(!storage.root_dir().join("sessions/fake-sess-1.md").exists());
    assert!(!storage.root_dir().join("recent/latest.md").exists());
    assert!(storage.sessions_dir().exists());
    assert!(storage.recent_dir().exists());
    assert!(storage.projects_dir().exists());
    assert_eq!(
        std::fs::read_dir(storage.sessions_dir()).unwrap().count(),
        0
    );
    assert_eq!(std::fs::read_dir(storage.recent_dir()).unwrap().count(), 0);
    assert_eq!(
        std::fs::read_dir(storage.projects_dir()).unwrap().count(),
        0
    );

    std::fs::remove_dir_all(temp_mem_root).ok();
}

#[tokio::test]
async fn test_unattended_pipeline_does_not_write_junk_to_disk() {
    let temp_mem_root =
        std::env::temp_dir().join(format!("test-mem-root-{}", uuid::Uuid::new_v4()));
    std::env::set_var("ASSETIWEAVE_MEMORY_ROOT", &temp_mem_root);

    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-nojunk-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let tenant_id = service.tenant_id();

    // 运行无人值守流水线
    let handle = service
        .start_memory_pipeline(MemoryPipelineParams::background())
        .await
        .unwrap();

    let _ = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("pipeline snapshot");

    let storage = crate::backend::infrastructure::memory::storage::MemoryStorage::new(tenant_id);
    let sessions_dir = storage.sessions_dir();
    if sessions_dir.exists() {
        let entries: Vec<_> = std::fs::read_dir(&sessions_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(
            entries.len(),
            0,
            "Unattended pipeline must NOT write sessions/*.md"
        );
    }

    assert!(
        !storage.recent_dir().join("latest.md").exists(),
        "Unattended pipeline must NOT write recent/latest.md"
    );
    assert!(
        !storage.global_file().exists(),
        "Unattended pipeline must NOT write MEMORY.md"
    );

    std::env::remove_var("ASSETIWEAVE_MEMORY_ROOT");
    std::fs::remove_dir_all(&temp_mem_root).ok();
}

#[tokio::test]
async fn test_has_pending_memory_work_respects_window_and_internal_filter() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-pending-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let pool = service.db.pool();
    let tenant_id = service.tenant_id();

    let adapter = crate::backend::domain::ConversationAdapter {
        id: "adapter-pending".to_string(),
        name: "Test Adapter".to_string(),
        kind: crate::backend::domain::ConversationAdapterKind::External,
        version: "1.0.0".to_string(),
        enabled: true,
        manifest_path: None,
        executable_path: None,
        content_hash: None,
        trusted_hash: None,
        trust_state: crate::backend::domain::ConversationAdapterTrustState::Trusted,
        protocol_version: Some(1),
        capabilities: vec!["read_session".to_string()],
        input_kinds: vec![crate::backend::domain::ConversationSourceKind::Directory],
        card_contract_version: None,
        card_kinds: Vec::new(),
        projection_version: Some(1),
        created_at: "2026-10-21T10:00:00Z".to_string(),
        updated_at: "2026-10-21T10:00:00Z".to_string(),
    };
    let source = crate::backend::domain::ConversationSource {
        id: "source-pending".to_string(),
        adapter_id: "adapter-pending".to_string(),
        name: "Test Source".to_string(),
        kind: crate::backend::domain::ConversationSourceKind::Directory,
        location: "/fixture/path".to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: "2026-10-21T10:00:00Z".to_string(),
        updated_at: "2026-10-21T10:00:00Z".to_string(),
    };

    crate::backend::store::upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter)
        .await
        .unwrap();
    crate::backend::store::upsert_conversation_source_sqlx(pool, tenant_id, &source)
        .await
        .unwrap();

    let now = chrono::Utc::now();
    // 1. Expired session (100h ago, outside 48h/72h window)
    let ts_expired = (now - chrono::Duration::hours(100)).to_rfc3339();
    let expired_session = crate::backend::domain::NormalizedConversationSession {
        external_id: "s-pending-expired".to_string(),
        title: Some("Expired".to_string()),
        project_path: Some("/code/expired".to_string()),
        started_at: Some(ts_expired.clone()),
        updated_at: Some(ts_expired.clone()),
        source_locator: None,
        source_fingerprint: None,
        turns: vec![crate::backend::domain::NormalizedConversationTurn {
            external_id: "t1".to_string(),
            turn_index: 0,
            user_text: "old work".to_string(),
            title: None,
            started_at: Some(ts_expired.clone()),
            ended_at: Some(ts_expired.clone()),
            model: None,
            parts: vec![],
            ..Default::default()
        }],
        execution_origin: None,
        execution_purpose: None,
        user_visible: Some(true),
    };

    // 2. Internal agent session
    let internal_root =
        crate::backend::infrastructure::agent_execution::agent_execution_workspace_root(
            &service.db_path,
        );
    let internal_session = crate::backend::domain::NormalizedConversationSession {
        external_id: "s-pending-internal".to_string(),
        title: Some("Internal".to_string()),
        project_path: Some(format!("{internal_root}/agent-run-1")),
        started_at: Some((now - chrono::Duration::hours(1)).to_rfc3339()),
        updated_at: Some((now - chrono::Duration::hours(1)).to_rfc3339()),
        source_locator: None,
        source_fingerprint: None,
        turns: vec![crate::backend::domain::NormalizedConversationTurn {
            external_id: "t2".to_string(),
            turn_index: 0,
            user_text: "internal run".to_string(),
            title: None,
            started_at: Some((now - chrono::Duration::hours(1)).to_rfc3339()),
            ended_at: Some((now - chrono::Duration::hours(1)).to_rfc3339()),
            model: None,
            parts: vec![],
            ..Default::default()
        }],
        execution_origin: Some("internal_agent".to_string()),
        execution_purpose: None,
        user_visible: Some(false),
    };

    crate::backend::store::import_conversation_sessions_sqlx(
        pool,
        tenant_id,
        &source,
        &[expired_session, internal_session],
        false,
    )
    .await
    .unwrap();

    // Expired and internal sessions must be ignored -> has_pending_memory_work is FALSE
    assert!(
        !service.has_pending_memory_work(now).await.unwrap(),
        "Expired and internal sessions must not trigger pending memory work"
    );

    // 3. Add an active session within recent window
    let ts_active = (now - chrono::Duration::hours(2)).to_rfc3339();
    let active_session = crate::backend::domain::NormalizedConversationSession {
        external_id: "s-pending-active".to_string(),
        title: Some("Active".to_string()),
        project_path: Some("/code/active".to_string()),
        started_at: Some(ts_active.clone()),
        updated_at: Some(ts_active.clone()),
        source_locator: None,
        source_fingerprint: None,
        turns: vec![crate::backend::domain::NormalizedConversationTurn {
            external_id: "t3".to_string(),
            turn_index: 0,
            user_text: "active recent work".to_string(),
            title: None,
            started_at: Some(ts_active.clone()),
            ended_at: Some(ts_active.clone()),
            model: None,
            parts: vec![],
            ..Default::default()
        }],
        execution_origin: None,
        execution_purpose: None,
        user_visible: Some(true),
    };

    crate::backend::store::import_conversation_sessions_sqlx(
        pool,
        tenant_id,
        &source,
        &[active_session],
        false,
    )
    .await
    .unwrap();

    // Now has_pending_memory_work should be TRUE
    assert!(
        service.has_pending_memory_work(now).await.unwrap(),
        "Active session within recent window must trigger pending memory work"
    );
}
