use super::*;
use crate::backend::infrastructure::tasks::TaskOutput;
use crate::backend::infrastructure::tasks::{
    StageStatus, TaskKind, TaskRuntime, TaskSpec, TaskState,
};
use std::time::Duration;

#[tokio::test]
async fn worker_tracker_reports_and_auto_cleans_up_on_drop() {
    let runtime = TaskRuntime::with_runtime_handle(tokio::runtime::Handle::current());
    let spec = TaskSpec::new(TaskKind::Other, None).with_task_id("activity-task-1");
    let _ = runtime.register_external(spec).expect("register task");
    let context = runtime.task_context("activity-task-1").expect("context");

    let _stage = context.enter_stage("harvest");

    // 作用域内启动 Worker
    {
        let tracker = WorkerTracker::new("activity-task-1", "harvest", "worker-0", runtime.clone());

        tracker.report(
            "reading_file",
            Some(1),
            Some(10),
            Some("file_1.json".to_string()),
        );

        let snap = runtime.get("activity-task-1").expect("snapshot exists");
        let stage = snap.stages.iter().find(|s| s.id == "harvest").unwrap();
        assert_eq!(stage.current_activities.len(), 1);
        let act = &stage.current_activities[0];
        assert_eq!(act.worker_id, "worker-0");
        assert_eq!(act.operation, "reading_file");
        assert_eq!(act.display_path.as_deref(), Some("file_1.json"));
        assert_eq!(act.current, Some(1));
        assert_eq!(act.total, Some(10));
    } // tracker dropped here

    // drop 后 worker 应该已被自动清理
    let snap = runtime.get("activity-task-1").expect("snapshot exists");
    let stage = snap.stages.iter().find(|s| s.id == "harvest").unwrap();
    assert!(stage.current_activities.is_empty());
}

#[tokio::test]
async fn worker_tracker_throttled_reporting_keeps_latest_in_memory() {
    let runtime = TaskRuntime::with_runtime_handle(tokio::runtime::Handle::current());
    let spec = TaskSpec::new(TaskKind::Other, None).with_task_id("throttle-task-1");
    let _ = runtime.register_external(spec).expect("register task");
    let context = runtime.task_context("throttle-task-1").expect("context");

    let _stage = context.enter_stage("process");

    let tracker = WorkerTracker::new("throttle-task-1", "process", "worker-fast", runtime.clone())
        .with_throttle_interval(Duration::from_millis(50));

    // 高频连续上报 20 次
    for i in 1..=20 {
        tracker.report("syncing", Some(i), Some(20), Some(format!("chunk_{i}.dat")));
    }

    // 内存中的状态始终是最新的第 20 次
    let snap = runtime.get("throttle-task-1").expect("snapshot exists");
    let stage = snap.stages.iter().find(|s| s.id == "process").unwrap();
    assert_eq!(stage.current_activities.len(), 1);
    let act = &stage.current_activities[0];
    assert_eq!(act.current, Some(20));
    assert_eq!(act.display_path.as_deref(), Some("chunk_20.dat"));

    tracker.complete();

    let snap_after = runtime.get("throttle-task-1").expect("snapshot exists");
    let stage_after = snap_after
        .stages
        .iter()
        .find(|s| s.id == "process")
        .unwrap();
    assert!(stage_after.current_activities.is_empty());
}

#[tokio::test]
async fn stage_guard_activity_facade_and_auto_clear_on_stage_finish() {
    let runtime = TaskRuntime::with_runtime_handle(tokio::runtime::Handle::current());
    let spec = TaskSpec::new(TaskKind::Other, None).with_task_id("stage-facade-task");

    let handle = runtime
        .run(spec, |context| async move {
            let mut guard = context.enter_stage("bulk_index");
            guard.progress(5, Some(10), Some("半数完成".to_string()));
            guard.activity("worker-x", "parsing", Some("document.pdf".to_string()));
            guard.record_skipped_group("too_large", vec!["huge.zip", "giant.tar"]);

            // 检查活动与进度存在
            let snap = context.runtime().get(context.task_id()).unwrap();
            let stage = snap.stages.iter().find(|s| s.id == "bulk_index").unwrap();
            assert_eq!(stage.current_activities.len(), 1);
            assert_eq!(stage.current_activities[0].worker_id, "worker-x");

            guard.record_metric("indexed_docs", 5);
            drop(guard); // stage finishes

            // 阶段结束后，活跃 worker 自动移出，但进度、指标和跳过保留
            let snap_after = context.runtime().get(context.task_id()).unwrap();
            let stage_after = snap_after
                .stages
                .iter()
                .find(|s| s.id == "bulk_index")
                .unwrap();
            assert!(stage_after.current_activities.is_empty());
            assert_eq!(stage_after.status, StageStatus::Succeeded);
            assert_eq!(stage_after.metrics.len(), 1);
            assert_eq!(stage_after.skipped.len(), 1);
            assert_eq!(stage_after.skipped[0].reason_code, "too_large");
            assert_eq!(stage_after.skipped[0].count, 2);

            Ok(TaskOutput::from("done".to_string()))
        })
        .expect("run");

    tokio::time::sleep(Duration::from_millis(50)).await;
    let final_snap = runtime.get(&handle.task_id).unwrap();
    assert_eq!(final_snap.state, TaskState::Succeeded);
}

#[tokio::test]
async fn worker_tracker_concurrent_multiple_workers() {
    let runtime = TaskRuntime::with_runtime_handle(tokio::runtime::Handle::current());
    let spec = TaskSpec::new(TaskKind::Other, None).with_task_id("concurrent-task");
    let _ = runtime.register_external(spec).expect("register");
    let context = runtime.task_context("concurrent-task").expect("context");

    let guard = context.enter_stage("multi_worker");

    let tracker_a = guard.worker("worker-A");
    let tracker_b = guard.worker("worker-B");

    tracker_a.report(
        "downloading",
        Some(10),
        Some(100),
        Some("part1.bin".to_string()),
    );
    tracker_b.report(
        "processing",
        Some(50),
        Some(100),
        Some("part2.bin".to_string()),
    );

    let snap = runtime.get("concurrent-task").unwrap();
    let stage = snap.stages.iter().find(|s| s.id == "multi_worker").unwrap();
    assert_eq!(stage.current_activities.len(), 2);

    // 完成 A，B 依然活跃
    tracker_a.complete();

    let snap_mid = runtime.get("concurrent-task").unwrap();
    let stage_mid = snap_mid
        .stages
        .iter()
        .find(|s| s.id == "multi_worker")
        .unwrap();
    assert_eq!(stage_mid.current_activities.len(), 1);
    assert_eq!(stage_mid.current_activities[0].worker_id, "worker-B");

    drop(tracker_b);
    drop(guard);

    let snap_end = runtime.get("concurrent-task").unwrap();
    let stage_end = snap_end
        .stages
        .iter()
        .find(|s| s.id == "multi_worker")
        .unwrap();
    assert!(stage_end.current_activities.is_empty());
}
