use super::*;
use crate::backend::infrastructure::tasks::{self, TaskKind, TaskRuntime, TaskSpec, TaskState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SyncTestResult {
    synced_items: usize,
    target_path: String,
}

#[test]
fn task_output_construction_and_summary() {
    let raw = SyncTestResult {
        synced_items: 10,
        target_path: "/tmp/test".to_string(),
    };

    let output = TaskOutput::with_summary(raw.clone(), "同步完成 10 项");
    assert_eq!(output.data, raw);
    assert_eq!(output.summary.as_deref(), Some("同步完成 10 项"));

    let plain_output: TaskOutput<SyncTestResult> = raw.clone().into();
    assert_eq!(plain_output.data, raw);
    assert!(plain_output.summary.is_none());
}

#[tokio::test]
async fn task_runner_executes_typed_future_and_persists_summary() {
    let runtime = TaskRuntime::with_runtime_handle(tokio::runtime::Handle::current());

    let spec = TaskSpec::new(TaskKind::Other, None).with_task_id("typed-task-1");

    let handle = runtime
        .run(spec, |context| async move {
            let mut guard = context.enter_stage("process");
            guard.record_metric("processed", 5);
            drop(guard);

            Ok(TaskOutput::with_summary(
                SyncTestResult {
                    synced_items: 5,
                    target_path: "/data/vault".to_string(),
                },
                "已成功同步 5 个会话文件",
            ))
        })
        .expect("run typed task");

    assert_eq!(handle.task_id, "typed-task-1");
    assert!(!handle.is_cancelled());

    // 等待任务异步完成
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let snapshot = runtime.get("typed-task-1").expect("snapshot exists");
    assert_eq!(snapshot.state, TaskState::Succeeded);
    assert_eq!(
        snapshot.result_summary.as_deref(),
        Some("已成功同步 5 个会话文件")
    );

    let result_json = snapshot.result.expect("result json exists");
    let decoded: SyncTestResult = serde_json::from_value(result_json).expect("decode result");
    assert_eq!(decoded.synced_items, 5);
    assert_eq!(decoded.target_path, "/data/vault");
}

#[tokio::test]
async fn task_runner_intercepts_cancellation_at_boundary() {
    let runtime = TaskRuntime::with_runtime_handle(tokio::runtime::Handle::current());

    let spec = TaskSpec::new(TaskKind::Other, None).with_task_id("typed-task-cancel");
    let (step1_done_tx, step1_done_rx) = tokio::sync::oneshot::channel();

    let handle = runtime
        .run::<(), _, _>(spec, |context| async move {
            // 第一个阶段正常完成
            {
                let _guard = context.enter_stage("step1");
            }
            let _ = step1_done_tx.send(());

            // 稍微等待主线程发起取消
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;

            // 在步骤边界检查取消信号
            context.check_cancellation()?;

            // 如果没有被取消，执行第二阶段（本测试中不应该到达此处）
            {
                let _guard = context.enter_stage("step2");
            }

            Ok(TaskOutput::new(()))
        })
        .expect("run typed task");

    // 等待 step1 明确完成
    let _ = step1_done_rx.await;

    // 在 stage 1 完成后发起取消
    handle.cancel();

    // 等待任务响应取消
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let snapshot = runtime.get("typed-task-cancel").expect("snapshot exists");
    assert_eq!(snapshot.state, TaskState::Canceled);

    // 验证 step1 成功闭环，而 step2 根本没有被创建/执行
    assert_eq!(snapshot.stages.len(), 1);
    assert_eq!(snapshot.stages[0].id, "step1");
    assert_eq!(snapshot.stages[0].status, tasks::StageStatus::Succeeded);
}

#[tokio::test]
async fn resource_pool_enforces_fifo_queueing_and_pending_cancellation() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use tasks::{CancelOutcome, ResourcePool, TaskOutcome};

    let runtime = TaskRuntime::with_runtime_handle(tokio::runtime::Handle::current());
    let (tx1, rx1) = tokio::sync::oneshot::channel::<()>();
    let (started1_tx, started1_rx) = tokio::sync::oneshot::channel::<()>();
    let executed_order = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let task2_ran = Arc::new(AtomicBool::new(false));

    let order1 = executed_order.clone();
    let h1 = runtime
        .run(
            TaskSpec::new(TaskKind::Scan, None)
                .with_task_id("task-pool-1")
                .with_resource_pool(ResourcePool::SqliteWrite),
            move |_cx| async move {
                order1.lock().unwrap().push("task-1");
                let _ = started1_tx.send(());
                let _ = rx1.await;
                Ok(TaskOutput::new("done-1"))
            },
        )
        .expect("task 1 should spawn");

    started1_rx.await.expect("task 1 should start immediately");
    assert_eq!(
        runtime.get("task-pool-1").unwrap().resource_pool,
        ResourcePool::SqliteWrite
    );
    assert_eq!(
        runtime.get("task-pool-1").unwrap().state,
        TaskState::Running
    );

    let t2_ran_flag = task2_ran.clone();
    let order2 = executed_order.clone();
    let h2 = runtime
        .run(
            TaskSpec::new(TaskKind::Scan, Some("pool-dedup-2".to_string()))
                .with_task_id("task-pool-2")
                .with_conflict_policy(ConflictPolicy::Coalesce)
                .with_resource_pool(ResourcePool::SqliteWrite),
            move |_cx| async move {
                t2_ran_flag.store(true, Ordering::SeqCst);
                order2.lock().unwrap().push("task-2");
                Ok(TaskOutput::new("done-2"))
            },
        )
        .expect("task 2 should queue");

    let order3 = executed_order.clone();
    let h3 = runtime
        .run(
            TaskSpec::new(TaskKind::Scan, None)
                .with_task_id("task-pool-3")
                .with_resource_pool(ResourcePool::SqliteWrite),
            move |_cx| async move {
                order3.lock().unwrap().push("task-3");
                Ok(TaskOutput::new("done-3"))
            },
        )
        .expect("task 3 should queue");

    // 确认排队中状态与 Pending 去重收敛
    let snap2 = runtime.get("task-pool-2").unwrap();
    let snap3 = runtime.get("task-pool-3").unwrap();
    assert_eq!(snap2.state, TaskState::Pending);
    assert_eq!(snap2.resource_pool, ResourcePool::SqliteWrite);
    assert_eq!(snap3.state, TaskState::Pending);

    let coalesced = runtime
        .run(
            TaskSpec::new(TaskKind::Scan, Some("pool-dedup-2".to_string()))
                .with_task_id("task-pool-2-dup")
                .with_conflict_policy(ConflictPolicy::Coalesce)
                .with_resource_pool(ResourcePool::SqliteWrite),
            |_cx| async move { Ok(TaskOutput::new("dup")) },
        )
        .expect("coalesce should return queued task");
    assert_eq!(coalesced.task_id(), h2.task_id());

    // 在 Pending 状态取消任务 2，应当瞬时终态 Canceled 且不阻塞也不运行任务体
    let cancel_outcome = runtime.cancel("task-pool-2");
    match cancel_outcome {
        CancelOutcome::Requested(snap) => {
            assert_eq!(snap.state, TaskState::Canceled);
            assert_eq!(snap.outcome, Some(TaskOutcome::Canceled));
        }
        _ => panic!("expected Requested(Canceled) for Pending task"),
    }

    // 释放任务 1，任务 3 应按 FIFO 顺序获准执行
    let _ = tx1.send(());
    h1.wait_until_finished(&runtime).await;
    h3.wait_until_finished(&runtime).await;

    assert!(!task2_ran.load(Ordering::SeqCst));
    assert_eq!(
        runtime.get("task-pool-3").unwrap().state,
        TaskState::Succeeded
    );
    assert_eq!(*executed_order.lock().unwrap(), vec!["task-1", "task-3"]);
}

#[tokio::test]
async fn task_runner_truncates_oversized_result_while_run_inline_preserves_full_result() {
    let runtime = TaskRuntime::with_runtime_handle(tokio::runtime::Handle::current());
    let huge_payload = vec!["a".repeat(256); 512]; // > 128 KiB
    let huge_for_bg = huge_payload.clone();

    let handle = runtime
        .run(
            TaskSpec::new(TaskKind::Scan, None).with_task_id("task-huge-bg"),
            move |_cx| async move {
                Ok(TaskOutput::with_summary(
                    huge_for_bg,
                    "扫描完成，共 512 项资产",
                ))
            },
        )
        .expect("bg task should spawn");

    let bg_snap = handle
        .wait_until_finished(&runtime)
        .await
        .expect("bg terminal snapshot");
    assert_eq!(bg_snap.state, TaskState::Succeeded);
    assert_eq!(
        bg_snap.result_summary.as_deref(),
        Some("扫描完成，共 512 项资产")
    );
    let result_val = bg_snap.result.expect("truncated marker present");
    assert_eq!(
        result_val.get("truncated").and_then(|v| v.as_bool()),
        Some(true),
        "background TaskSnapshot.result must be slimmed when exceeding TASK_RESULT_MAX_BYTES"
    );

    let inline_snap = runtime
        .run_inline(
            TaskSpec::new(TaskKind::Scan, None).with_task_id("task-huge-inline"),
            move |_cx| async move { Ok(TaskOutput::new(huge_payload)) },
        )
        .await
        .expect("inline run succeeds");
    assert_eq!(inline_snap.state, TaskState::Succeeded);
    assert_eq!(
        inline_snap
            .result
            .as_ref()
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(512)
    );
}

#[tokio::test]
async fn cancel_existing_conflict_immediately_cancels_pending_queued_task() {
    use std::sync::Arc;
    use tokio::sync::Notify;

    let runtime = TaskRuntime::with_runtime_handle(tokio::runtime::Handle::current());
    let release_holder = Arc::new(Notify::new());
    let holder_started = Arc::new(Notify::new());

    // 占用唯一的 SqliteWrite(1) 槽位，使后续同池任务处于 Pending 排队态
    let started_tx = holder_started.clone();
    let release_rx = release_holder.clone();
    let holder_handle = runtime
        .run(
            TaskSpec::new(TaskKind::SearchIndexRebuild, None)
                .with_task_id("sqlite-holder")
                .with_resource_pool(tasks::ResourcePool::SqliteWrite),
            move |_cx| async move {
                started_tx.notify_one();
                release_rx.notified().await;
                Ok::<_, InfraError>(TaskOutput::new(serde_json::json!({"ok": true})))
            },
        )
        .expect("holder should spawn");

    holder_started.notified().await;

    // 第二个任务进入 Pending 排队态，带冲突键 "k-pending"
    let pending_spec = TaskSpec::new(TaskKind::SearchIndexRebuild, Some("k-pending".to_string()))
        .with_task_id("pending-sync-1")
        .with_resource_pool(tasks::ResourcePool::SqliteWrite);
    let _pending_handle = runtime
        .run(pending_spec, |_cx| async move {
            Ok::<_, InfraError>(TaskOutput::new(serde_json::json!({"ok": true})))
        })
        .expect("pending task should queue");

    tokio::task::yield_now().await;
    assert_eq!(
        runtime.get("pending-sync-1").map(|s| s.state),
        Some(TaskState::Pending)
    );

    // 触发 ConflictPolicy::CancelExisting，被取消的 Pending 任务必须立刻成为 Canceled（而非 Cancelling）
    let mut rx = runtime.subscribe();
    let replacement_spec =
        TaskSpec::new(TaskKind::SearchIndexRebuild, Some("k-pending".to_string()))
            .with_task_id("replacement-sync-2")
            .with_resource_pool(tasks::ResourcePool::SqliteWrite)
            .with_conflict_policy(tasks::ConflictPolicy::CancelExisting);
    let _replacement_handle = runtime
        .run(replacement_spec, |_cx| async move {
            Ok::<_, InfraError>(TaskOutput::new(serde_json::json!({"ok": true})))
        })
        .expect("replacement should spawn");

    let canceled_snap = runtime
        .get("pending-sync-1")
        .expect("canceled snapshot should stay observable");
    assert_eq!(canceled_snap.state, TaskState::Canceled);
    assert_eq!(canceled_snap.outcome, Some(tasks::TaskOutcome::Canceled));

    let mut saw_terminal_canceled = false;
    while let Ok(ev) = rx.try_recv() {
        if ev.task_id == "pending-sync-1" && ev.state == TaskState::Canceled {
            saw_terminal_canceled = true;
            break;
        }
    }
    assert!(
        saw_terminal_canceled,
        "ConflictPolicy::CancelExisting must publish terminal Canceled event for Pending task"
    );

    release_holder.notify_waiters();
    let _ = holder_handle.wait_until_finished(&runtime).await;
    let _ = runtime
        .shutdown_until(tokio::time::Instant::now() + std::time::Duration::from_secs(2))
        .await;
}
