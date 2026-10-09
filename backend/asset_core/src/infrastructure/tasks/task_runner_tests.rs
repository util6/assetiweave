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
