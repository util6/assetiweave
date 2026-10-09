use crate::backend::{
    application::{AppError, AppService, SourceScanParams},
    infrastructure::tasks::{CancelOutcome, TaskKind, TaskRuntime, TaskSpec},
};
use std::fs;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread")]
async fn app_service_task_aware_source_scan_preserves_cancellation() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-source-scan-boundary-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service = AppService::open_with_db_path(root.join("app.db"))
        .await
        .expect("open application service");

    let task_id = "source-scan-cancellation-boundary";
    let task_runtime = TaskRuntime::new();
    task_runtime
        .register_external(TaskSpec::new(TaskKind::Scan, None).with_task_id(task_id))
        .expect("register scan task");
    task_runtime
        .start_external(task_id)
        .expect("start scan task");
    let context = task_runtime
        .task_context(task_id)
        .expect("create scan task context");
    assert!(matches!(
        task_runtime.cancel(task_id),
        CancelOutcome::Requested(_)
    ));

    let error = service
        .scan_sources_with_task_context(
            SourceScanParams {
                kind: None,
                dry_run: false,
            },
            &context,
            false,
        )
        .await
        .expect_err("cancelled scan should stop through AppService");

    assert!(matches!(error, AppError::Cancelled(_)));
    drop(service);
    fs::remove_dir_all(root).ok();
}
