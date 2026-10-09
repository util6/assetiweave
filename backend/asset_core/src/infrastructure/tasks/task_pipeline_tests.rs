use super::*;
use crate::backend::infrastructure::tasks::{self, ExternalRegistrationOutcome, TaskSpec};

#[test]
fn task_category_from_str_and_display() {
    let cat = TaskCategory::from("custom/sync");
    assert_eq!(cat.as_str(), "custom/sync");
    assert_eq!(format!("{cat}"), "custom/sync");

    let serialized = serde_json::to_string(&cat).expect("serialize");
    assert_eq!(serialized, "\"custom/sync\"");

    let deserialized: TaskCategory = serde_json::from_str(&serialized).expect("deserialize");
    assert_eq!(deserialized, cat);
}

#[test]
fn task_category_from_task_kind() {
    assert_eq!(
        TaskCategory::from(TaskKind::ConversationSync).as_str(),
        "conversation/sync"
    );
    assert_eq!(
        TaskCategory::from(TaskKind::Backup).as_str(),
        "system/backup"
    );
    assert_eq!(
        TaskCategory::from(TaskKind::BatchMount).as_str(),
        "mount/batch"
    );
}

#[test]
fn pipeline_descriptor_builder_and_to_initial_stages() {
    let pipeline = PipelineDescriptor::builder()
        .stage("stage_prepare", "准备环境")
        .stage_with_visibility("stage_internal", "内部索引", false)
        .stage("stage_execute", "执行操作")
        .build();

    assert_eq!(pipeline.stages.len(), 3);
    assert_eq!(pipeline.stages[0].id, "stage_prepare");
    assert_eq!(pipeline.stages[0].name, "准备环境");
    assert!(pipeline.stages[0].user_visible);

    assert_eq!(pipeline.stages[1].id, "stage_internal");
    assert!(!pipeline.stages[1].user_visible);

    let initial_stages = pipeline.to_initial_stages();
    assert_eq!(initial_stages.len(), 3);

    for stage in &initial_stages {
        assert_eq!(stage.status, StageStatus::Pending);
        assert!(stage.started_at.is_none());
        assert!(stage.finished_at.is_none());
        assert!(stage.duration_ms.is_none());
    }

    assert_eq!(initial_stages[0].id, "stage_prepare");
    assert_eq!(initial_stages[1].id, "stage_internal");
    assert_eq!(initial_stages[2].id, "stage_execute");
}

#[test]
fn stage_guard_auto_succeeds_on_normal_drop() {
    let tasks = TaskRuntime::new();
    let pipeline = PipelineDescriptor::builder()
        .stage("prepare", "环境准备")
        .stage("execute", "开始执行")
        .build();

    let spec = tasks::TaskSpec::new(TaskKind::Other, None)
        .with_task_id("task-guard-1")
        .with_pipeline(pipeline);

    let outcome = tasks.register_external(spec).expect("register");
    let snapshot = match outcome {
        tasks::ExternalRegistrationOutcome::Started(s) => s,
        _ => panic!("expected started"),
    };
    assert_eq!(snapshot.stages.len(), 2);
    assert_eq!(snapshot.stages[0].status, StageStatus::Pending);

    // 进入作用域
    {
        let mut guard = StageGuard::enter(
            "task-guard-1",
            "prepare",
            tasks.clone(),
            CancellationToken::new(),
        );
        guard.record_metric("items_processed", 100);
        guard.set_progress(10, Some(20), Some("处理中".to_string()));

        // 在 scope 内部检查状态为 Running
        let running_snap = tasks.get("task-guard-1").expect("get task");
        let stage = running_snap
            .stages
            .iter()
            .find(|s| s.id == "prepare")
            .expect("stage");
        assert_eq!(stage.status, StageStatus::Running);
        assert!(stage.started_at.is_some());
    } // guard 在此 drop，应当自动以 Succeeded 闭环并自动计算耗时

    let finished_snap = tasks.get("task-guard-1").expect("get task");
    let stage = finished_snap
        .stages
        .iter()
        .find(|s| s.id == "prepare")
        .expect("stage");
    assert_eq!(stage.status, StageStatus::Succeeded);
    assert!(stage.finished_at.is_some());
    assert!(stage.duration_ms.is_some());
    assert_eq!(stage.metrics.len(), 1);
    assert_eq!(stage.metrics[0].code, "items_processed");
    assert_eq!(stage.metrics[0].value, 100);

    // 第二个 stage 仍保持 Pending，未被影响
    let next_stage = finished_snap
        .stages
        .iter()
        .find(|s| s.id == "execute")
        .expect("stage");
    assert_eq!(next_stage.status, StageStatus::Pending);
}

#[test]
fn stage_guard_explicit_skip_marks_skipped() {
    let tasks = TaskRuntime::new();
    let spec = tasks::TaskSpec::new(TaskKind::Other, None).with_task_id("task-guard-skip");
    let _ = tasks.register_external(spec).expect("register");

    {
        let mut guard = StageGuard::enter(
            "task-guard-skip",
            "download",
            tasks.clone(),
            CancellationToken::new(),
        );
        guard.skip("cached", "已存在本地缓存");
    }

    let snap = tasks.get("task-guard-skip").expect("get task");
    let stage = snap
        .stages
        .iter()
        .find(|s| s.id == "download")
        .expect("stage");
    assert_eq!(stage.status, StageStatus::Skipped);
    assert_eq!(stage.skipped.len(), 1);
    assert_eq!(stage.skipped[0].reason_code, "cached");
}

#[test]
fn stage_guard_explicit_fail_marks_failed() {
    let tasks = TaskRuntime::new();
    let spec = tasks::TaskSpec::new(TaskKind::Other, None).with_task_id("task-guard-fail");
    let _ = tasks.register_external(spec).expect("register");

    {
        let mut guard = StageGuard::enter(
            "task-guard-fail",
            "upload",
            tasks.clone(),
            CancellationToken::new(),
        );
        guard.fail("network_err", "连接超时", true);
    }

    let snap = tasks.get("task-guard-fail").expect("get task");
    let stage = snap
        .stages
        .iter()
        .find(|s| s.id == "upload")
        .expect("stage");
    assert_eq!(stage.status, StageStatus::Failed);
    assert_eq!(stage.failures.len(), 1);
    assert_eq!(stage.failures[0].code, "network_err");
}

#[test]
fn stage_guard_cancelled_token_marks_canceled() {
    let tasks = TaskRuntime::new();
    let spec = tasks::TaskSpec::new(TaskKind::Other, None).with_task_id("task-guard-cancel");
    let _ = tasks.register_external(spec).expect("register");
    let cancel = CancellationToken::new();

    {
        let _guard = StageGuard::enter(
            "task-guard-cancel",
            "compute",
            tasks.clone(),
            cancel.clone(),
        );
        cancel.cancel();
    }

    let snap = tasks.get("task-guard-cancel").expect("get task");
    let stage = snap
        .stages
        .iter()
        .find(|s| s.id == "compute")
        .expect("stage");
    assert_eq!(stage.status, StageStatus::Canceled);
}
