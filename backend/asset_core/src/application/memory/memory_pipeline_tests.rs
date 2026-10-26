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
}
