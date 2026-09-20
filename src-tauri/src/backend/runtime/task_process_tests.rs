use super::*;
use crate::backend::runtime::tasks::{StageStatus, TaskKind, TaskRuntime, TaskSpec, TaskState};

#[tokio::test]
async fn process_runner_streams_ndjson_protocol_and_updates_stage() {
    let runtime = TaskRuntime::with_runtime_handle(tokio::runtime::Handle::current());
    let spec = TaskSpec::new(TaskKind::Other, None).with_task_id("proc-task-1");

    let script = r#"
echo '{"type":"progress","current":5,"total":10,"note":"正在转换"}'
echo '{"type":"metric","code":"converted_files","value":5}'
echo '{"type":"activity","worker_id":"worker-1","operation":"transforming","display_path":"a.md"}'
echo '{"type":"skipped","reason_code":"ignore_hidden","sample":".git"}'
echo '{"type":"result_summary","summary":"完成 5 个文件转换"}'
"#;

    let proc_spec = ProcessCommandSpec::new("/bin/sh", "transform")
        .arg("-c")
        .arg(script);

    let _handle = runtime.run_process(spec, proc_spec).expect("run process");

    // 等待子进程执行完毕
    tokio::time::sleep(Duration::from_millis(100)).await;

    let snap = runtime.get("proc-task-1").expect("snapshot exists");
    assert_eq!(snap.state, TaskState::Succeeded);
    assert_eq!(snap.result_summary.as_deref(), Some("完成 5 个文件转换"));

    let stage = snap
        .stages
        .iter()
        .find(|s| s.id == "transform")
        .expect("stage exists");
    assert_eq!(stage.status, StageStatus::Succeeded);
    assert_eq!(stage.progress.as_ref().map(|p| p.current), Some(5));
    assert_eq!(stage.progress.as_ref().map(|p| p.total), Some(Some(10)));
    assert_eq!(
        stage
            .metrics
            .iter()
            .find(|m| m.code == "converted_files")
            .map(|m| m.value),
        Some(5)
    );
    assert_eq!(
        stage
            .skipped
            .iter()
            .find(|s| s.reason_code == "ignore_hidden")
            .map(|s| s.count),
        Some(1)
    );
    // 阶段完成后活跃活动已被清空
    assert!(stage.current_activities.is_empty());
}

#[tokio::test]
async fn process_runner_handles_non_zero_exit_code() {
    let runtime = TaskRuntime::with_runtime_handle(tokio::runtime::Handle::current());
    let spec = TaskSpec::new(TaskKind::Other, None).with_task_id("proc-task-fail");

    let proc_spec = ProcessCommandSpec::new("/bin/sh", "build")
        .arg("-c")
        .arg("echo 'something broke' >&2; exit 42");

    let _handle = runtime.run_process(spec, proc_spec).expect("run process");

    tokio::time::sleep(Duration::from_millis(100)).await;

    let snap = runtime.get("proc-task-fail").expect("snapshot exists");
    assert_eq!(snap.state, TaskState::Failed);

    let stage = snap
        .stages
        .iter()
        .find(|s| s.id == "build")
        .expect("stage exists");
    assert_eq!(stage.status, StageStatus::Failed);
    let failure = stage
        .failures
        .iter()
        .find(|f| f.code == "process_exit_error")
        .expect("failure exists");
    assert!(failure.message.contains("42"));
}

#[tokio::test]
async fn process_runner_kills_child_process_on_cancellation() {
    let runtime = TaskRuntime::with_runtime_handle(tokio::runtime::Handle::current());
    let spec = TaskSpec::new(TaskKind::Other, None).with_task_id("proc-task-cancel");

    // 运行长达 10 秒的子进程
    let proc_spec = ProcessCommandSpec::new("/bin/sh", "long_job")
        .arg("-c")
        .arg("sleep 10");

    let handle = runtime.run_process(spec, proc_spec).expect("run process");

    // 等待子进程真正拉起
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 发起取消
    handle.cancel();

    // 等待 kill 与退出
    tokio::time::sleep(Duration::from_millis(100)).await;

    let snap = runtime.get("proc-task-cancel").expect("snapshot exists");
    assert_eq!(snap.state, TaskState::Canceled);

    let stage = snap
        .stages
        .iter()
        .find(|s| s.id == "long_job")
        .expect("stage exists");
    assert_eq!(stage.status, StageStatus::Canceled);
}
