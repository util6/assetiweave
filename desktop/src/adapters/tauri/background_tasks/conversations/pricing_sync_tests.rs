use super::*;
use crate::backend::infrastructure::tasks::TaskRuntime;
use std::sync::Arc;

#[tokio::test]
async fn test_model_pricing_sync_background_registration_and_progress() {
    let task_runtime = TaskRuntime::new();
    let registry = BackgroundTaskRegistry::with_task_runtime(task_runtime);

    let (snapshot, should_start) = registry
        .begin_model_pricing_sync_for_tenant("tenant-test", false)
        .expect("begin model pricing sync");

    assert!(should_start);
    assert_eq!(snapshot.status, BackgroundTaskStatus::Running);
    assert_eq!(
        snapshot.progress.phase,
        ModelPricingSyncProgressPhase::Preparing
    );

    // Update progress
    let updated = registry
        .update_model_pricing_sync_progress(
            &snapshot.id,
            ModelPricingSyncProgressPhase::Fetching,
            Some("正在拉取最新模型价格".to_string()),
            Some("litellm".to_string()),
            120,
            0,
            7.25,
        )
        .expect("update progress");

    assert_eq!(
        updated.progress.phase,
        ModelPricingSyncProgressPhase::Fetching
    );
    assert_eq!(updated.progress.models_count, 120);

    // Finish
    let finished = registry
        .finish_model_pricing_sync(&snapshot.id, Ok(serde_json::json!({ "models_count": 120 })))
        .expect("finish task");

    assert_eq!(finished.status, BackgroundTaskStatus::Completed);
    assert_eq!(
        finished.progress.phase,
        ModelPricingSyncProgressPhase::Completed
    );
}
