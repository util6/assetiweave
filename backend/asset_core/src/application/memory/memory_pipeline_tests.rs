use super::*;
use crate::backend::application::AppService;
use crate::backend::infrastructure::tasks::stages;
use chrono::Utc;

#[tokio::test]
async fn test_start_memory_pipeline_creates_single_task_with_four_stages() {
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

    assert_eq!(snapshot.stages.len(), 4);
    assert_eq!(snapshot.stages[0].id, stages::SESSION_FACTS);
    assert_eq!(snapshot.stages[1].id, stages::RECENT_SNAPSHOT);
    assert_eq!(snapshot.stages[2].id, stages::GLOBAL_CONSOLIDATION);
    assert_eq!(snapshot.stages[3].id, stages::MARKDOWN_PROJECTION);
}

#[tokio::test]
async fn test_concurrency_rejection_returns_conflict() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-2-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();

    let task_id = format!("memory-pipeline:{}", service.tenant_id());
    // Manually register an active task to simulate running pipeline
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

    // Second spawn must return Conflict
    let conflict_res = service
        .start_memory_pipeline(MemoryPipelineParams::background())
        .await;

    assert!(conflict_res.is_err());
    assert!(matches!(conflict_res.unwrap_err(), AppError::Conflict(_)));

    // Rebuild scope must also return Conflict
    let rebuild_conflict = service
        .rebuild_memory_scope(MemoryScopeRebuildParams {
            scope: MemoryScope::default(),
            target: Some(MemoryRebuildTarget::Recent),
            project_path: None,
            reason: None,
        })
        .await;

    assert!(rebuild_conflict.is_err());
    assert!(matches!(
        rebuild_conflict.unwrap_err(),
        AppError::Conflict(_)
    ));
}

#[tokio::test]
async fn test_stage3_skipped_when_not_requested() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-3-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();

    let handle = service
        .start_memory_pipeline(MemoryPipelineParams::rebuild(
            MemoryRebuildTarget::Recent,
            None,
            None,
        ))
        .await
        .unwrap();

    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("pipeline snapshot");

    let stage3 = snapshot
        .stages
        .iter()
        .find(|s| s.id == stages::GLOBAL_CONSOLIDATION)
        .expect("stage 3");

    assert_eq!(stage3.status, StageStatus::Skipped);
}

#[tokio::test]
async fn test_cold_start_reset_orphaned_running_jobs() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-4-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let pool = service.db.pool();

    // Insert orphaned running jobs
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO session_memory_jobs (
            tenant_id, id, session_id, source_id, source_revision,
            source_fingerprint, contract_version, prompt_version,
            source_event_id, source_sync_run_id, status, not_before,
            attempt_count, created_at, updated_at
        ) VALUES (
            'default', 'sm-orphan', 'session-1', 'source-1', 1,
            'fp-1', 'cv-1', 'pv-1',
            'ev-1', 'sync-1', 'running', ?1,
            1, ?1, ?1
        )",
    )
    .bind(&now)
    .execute(pool)
    .await
    .unwrap();

    let reset_count = store::reset_orphaned_running_memory_jobs_sqlx(pool)
        .await
        .unwrap();

    assert!(reset_count >= 1);

    let status: String =
        sqlx::query_scalar("SELECT status FROM session_memory_jobs WHERE id = 'sm-orphan'")
            .fetch_one(pool)
            .await
            .unwrap();

    assert_eq!(status, "queued");
}
