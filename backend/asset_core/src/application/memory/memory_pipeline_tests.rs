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

    // 阶段 1: 必须有步骤记录（即使会话为0，也记录了 check_session_facts）
    let stage1 = snapshot
        .stages
        .iter()
        .find(|s| s.id == stages::SESSION_FACTS)
        .expect("stage 1");
    assert!(
        !stage1.steps.is_empty(),
        "stage 1 must record steps even when 0 sessions"
    );
    assert_eq!(stage1.steps[0].operation, "check_session_facts");

    // 阶段 3: 跳过时记录 skipped
    let stage3 = snapshot
        .stages
        .iter()
        .find(|s| s.id == stages::GLOBAL_CONSOLIDATION)
        .expect("stage 3");
    assert_eq!(stage3.status, StageStatus::Skipped);

    // 阶段 4: 投影记录步骤
    let stage4 = snapshot
        .stages
        .iter()
        .find(|s| s.id == stages::MARKDOWN_PROJECTION)
        .expect("stage 4");
    assert_eq!(stage4.status, StageStatus::Succeeded);
    assert!(
        !stage4.steps.is_empty(),
        "stage 4 must record markdown_projected step"
    );
}

#[tokio::test]
async fn test_orphaned_queued_jobs_marked_stale() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-7-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let pool = service.db.pool();

    // 插入一个 >48h 的孤儿 queued job
    let old_date = "2026-10-10T10:00:00Z";
    sqlx::query(
        "INSERT INTO recent_memory_jobs (
            tenant_id, id, target_watermark_utc, window_hours,
            target_fingerprint, content_fingerprint, status, retry_count,
            work_order_json, created_at, updated_at
        ) VALUES (
            'default', 'job-stale-1', ?1, 24,
            'fp-2', 'cp-2', 'queued', 0,
            '{}', ?2, ?2
        )",
    )
    .bind(old_date)
    .bind(old_date)
    .execute(pool)
    .await
    .unwrap();

    let has_work = service.has_pending_memory_work(Utc::now()).await.unwrap();
    assert!(
        !has_work,
        "stale orphaned jobs must not trigger pending memory work"
    );

    let stale_status: String =
        sqlx::query_scalar("SELECT status FROM recent_memory_jobs WHERE id = 'job-stale-1'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(stale_status, "stale");
}

#[tokio::test]
async fn test_restart_recent_memory_job_resets_running_status() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-8-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let pool = service.db.pool();

    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO recent_memory_jobs (
            tenant_id, id, target_watermark_utc, window_hours,
            target_fingerprint, content_fingerprint, status, retry_count,
            ownership_token, lease_expires_at, work_order_json, created_at, updated_at
        ) VALUES (
            'default', 'job-running-1', '2026-10-14T02:00:00Z', 24,
            'fp-run', 'cp-run', 'running', 2,
            'owner-ghost', '2026-10-14T10:00:00Z', '{}', ?1, ?1
        )",
    )
    .bind(&now)
    .execute(pool)
    .await
    .unwrap();

    let restarted = store::restart_recent_memory_job_for_rebuild_sqlx(
        pool,
        service.tenant_id(),
        "job-running-1",
        &now,
    )
    .await
    .unwrap();
    assert!(restarted, "must successfully restart running orphaned job");

    let row: (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT status, ownership_token, lease_expires_at FROM recent_memory_jobs WHERE id = 'job-running-1'",
    )
    .fetch_one(pool)
    .await
    .unwrap();

    assert_eq!(row.0, "queued");
    assert!(row.1.is_none(), "ownership_token must be cleared");
    assert!(row.2.is_none(), "lease_expires_at must be cleared");
}

#[tokio::test]
async fn test_pipeline_recovers_orphaned_running_job_without_claim_failure() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-9-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();

    // 预先运行一次提取
    let now = Utc::now();
    let prep = service
        .prepare_recent_snapshot_generation_for_pipeline(Some(now), false)
        .await
        .unwrap();

    if let Some(prep) = prep {
        let pool = service.db.pool();
        let jid = service
            .enqueue_recent_snapshot_generation(&prep, now)
            .await
            .unwrap();

        // 模拟上一次流水线中断，将状态人工置为带有有效租约的 running
        let future_lease = (now + chrono::Duration::minutes(5)).to_rfc3339();
        let now_text = now.to_rfc3339();
        sqlx::query(
            "UPDATE recent_memory_jobs SET status = 'running', ownership_token = 'ghost-owner', \
             lease_expires_at = ?1, updated_at = ?2 WHERE id = ?3",
        )
        .bind(&future_lease)
        .bind(&now_text)
        .bind(&jid)
        .execute(pool)
        .await
        .unwrap();

        // 启动流水线，阶段 2 必须自动接管该 running job，记录 recover_orphaned_job 步骤，绝不报错 'Job status running cannot be claimed'
        let handle = service
            .start_memory_pipeline(MemoryPipelineParams::background())
            .await
            .unwrap();

        let snapshot = handle
            .wait_until_finished(service.runtime.task_runtime())
            .await
            .expect("pipeline snapshot");

        let stage2 = snapshot
            .stages
            .iter()
            .find(|s| s.id == stages::RECENT_SNAPSHOT)
            .expect("stage 2");

        // 验证步骤中记录了 recover_orphaned_job
        let recovered_step = stage2
            .steps
            .iter()
            .find(|step| step.operation == "recover_orphaned_job");
        assert!(
            recovered_step.is_some(),
            "stage 2 must record recover_orphaned_job step when encountering running job"
        );

        // 绝不可出现 recent_job_claim_failed 报错
        let claim_failed = stage2
            .failures
            .iter()
            .any(|f| f.code == "recent_job_claim_failed");
        assert!(
            !claim_failed,
            "stage 2 must NOT fail with recent_job_claim_failed"
        );
    }
}

#[tokio::test]
async fn test_memory_pipeline_stage1_attaches_agent_session_ref() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-session-ref-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let pool = service.db.pool();

    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO session_memory_jobs (
                tenant_id, id, session_id, source_id, source_revision,
                source_fingerprint, contract_version, prompt_version,
                source_event_id, source_sync_run_id, status, not_before,
                attempt_count, created_at, updated_at
            ) VALUES (
                'default', 'job-sample-1', 'session-1', 'source-1', 1,
                'fp-1', 'cv-1', 'pv-1',
                'ev-1', 'sync-1', 'queued', ?1,
                0, ?1, ?1
            )",
    )
    .bind(&now)
    .execute(pool)
    .await
    .unwrap();

    let handle = service
        .start_memory_pipeline(MemoryPipelineParams::background())
        .await
        .unwrap();

    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("pipeline snapshot");

    let stage1 = snapshot
        .stages
        .iter()
        .find(|s| s.id == stages::SESSION_FACTS)
        .expect("stage 1");

    assert_eq!(
        stage1.agent_session_ref.as_ref().map(|r| r.value.as_str()),
        Some("sm-job-sample-1")
    );
}

#[tokio::test]
async fn test_memory_pipeline_watchdog_activity_tracking() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-watchdog-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();

    // Verify session_streams elapsed_since_active_by_ref returns None for nonexistent session
    assert!(service
        .runtime
        .session_streams()
        .elapsed_since_active_by_ref("sm-nonexistent")
        .is_none());

    // Register a session stream and verify elapsed_since_active_by_ref works
    let key = crate::backend::infrastructure::runtime::session_streams::SessionStreamKey {
        tenant_id: "default".to_string(),
        scope_id: "test".to_string(),
        execution_id: "exec-1".to_string(),
    };
    let meta = crate::backend::infrastructure::runtime::session_streams::AgentSessionMetadata {
        session_ref: crate::backend::domain::agents::AgentSessionRef::new("sm-test-job-1"),
        execution_id: "exec-1".to_string(),
        purpose: "test".to_string(),
        mode: "oneshot".to_string(),
        tenant_id: Some("default".to_string()),
        agent: crate::backend::domain::agents::AgentInfoView {
            id: "test".to_string(),
            display_name: None,
            model: None,
            protocol: "builtin".to_string(),
        },
        context: crate::backend::domain::agents::AgentSessionContextView {
            memory_scope: None,
            memory_job_id: None,
            task_id: None,
        },
        allow_stop: true,
    };
    let _projection = service
        .runtime
        .session_streams()
        .register_with_metadata(key, meta);
    assert!(service
        .runtime
        .session_streams()
        .elapsed_since_active_by_ref("sm-test-job-1")
        .is_some());
    assert!(
        service
            .runtime
            .session_streams()
            .elapsed_since_active_by_ref("sm-test-job-1")
            .unwrap()
            .as_secs()
            < 5
    );
}

#[tokio::test]
async fn test_stage1_failure_sampling_caps_error_cards_and_adds_summary() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-sampling-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let pool = service.db.pool();

    // 插入 5 个必然失败的 session_memory_jobs
    let now = Utc::now();
    let now_text = now.to_rfc3339();
    for i in 1..=5 {
        sqlx::query(
            "INSERT INTO session_memory_jobs (
                tenant_id, id, session_id, source_id, source_revision,
                source_fingerprint, contract_version, prompt_version,
                source_event_id, source_sync_run_id,
                status, not_before, attempt_count, retry_count,
                created_at, updated_at
            ) VALUES (
                $1, $2, $3, 'test-source', 1,
                'fp', '1.0', '1.0',
                'event-1', 'sync-1',
                'queued', $4, 0, 0,
                $4, $4
            )",
        )
        .bind(service.tenant_id())
        .bind(format!("sample-job-{}", i))
        .bind(format!("sample-session-{}", i))
        .bind(&now_text)
        .execute(pool)
        .await
        .unwrap();
    }

    let handle = service
        .start_memory_pipeline(MemoryPipelineParams::background())
        .await
        .unwrap();

    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("pipeline snapshot");

    let stage1 = snapshot
        .stages
        .iter()
        .find(|s| s.id == stages::SESSION_FACTS)
        .expect("stage 1");

    // 验证失败卡片数量：最多 3 条采样 + 1 条汇总 = 4 条，绝不膨胀至 5 条或以上
    assert!(
        stage1.failures.len() <= 4,
        "failures count should be capped, got {}",
        stage1.failures.len()
    );
    assert!(
        stage1
            .failures
            .iter()
            .any(|f| f.code == "session_facts_partial_failed"),
        "must contain aggregate summary failure"
    );
}

#[tokio::test]
async fn test_queue_heartbeat_keeps_memory_agent_session_active_during_permit_wait() {
    use crate::backend::infrastructure::agent_execution::AiExecutionProgressSink;

    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-queue-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();

    let session =
        crate::backend::application::memory::memory_agent_session::ActiveMemoryAgentSession::start(
            &service.runtime,
            crate::backend::application::memory::memory_agent_session::MemoryAgentSessionParams {
                tenant_id: "default",
                scope: "session",
                job_id: "job-queue-test-1",
                task_id: Some("task-1"),
                agent_id: "builtin:assistant",
                display_name: Some("Test Agent".to_string()),
                model: None,
                prompt_summary: "测试排队心跳保活",
                custom_session_ref: Some(crate::backend::domain::agents::AgentSessionRef::new(
                    "sm-job-queue-test-1",
                )),
                persistent: false,
            },
        );

    // 初始活跃时间差应在极小范围内 (< 1s)
    let initial_elapsed = session.elapsed_since_active();
    assert!(initial_elapsed.as_millis() < 1000);

    // 模拟在队列中排队，阶段心跳 Queued 触发
    session
        .sink
        .set_phase(crate::backend::infrastructure::agent_execution::AiExecutionPhase::Queued);
    let after_queued = session.elapsed_since_active();
    assert!(after_queued.as_millis() < 500);

    // 模拟拿到许可证，进入 Resolving 阶段触发 touch
    session
        .sink
        .set_phase(crate::backend::infrastructure::agent_execution::AiExecutionPhase::Resolving);
    let after_resolving = session.elapsed_since_active();
    assert!(after_resolving.as_millis() < 500);

    // 验证 session_streams 的 touch_by_ref
    service
        .runtime
        .session_streams()
        .touch_by_ref("sm-job-queue-test-1");
    let after_touch = service
        .runtime
        .session_streams()
        .elapsed_since_active_by_ref("sm-job-queue-test-1")
        .expect("session stream exists");
    assert!(after_touch.as_millis() < 500);
}
