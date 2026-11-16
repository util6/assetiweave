use super::*;
use crate::backend::application::system::TaskView;
use crate::backend::infrastructure::tasks::TaskRuntime;

#[tokio::test]
async fn test_model_pricing_sync_registers_visible_metadata_and_projects_result() {
    let task_runtime = TaskRuntime::new();
    let registry = BackgroundTaskRegistry::with_task_runtime(task_runtime.clone());

    let (snapshot, should_start) = registry
        .begin_model_pricing_sync_for_tenant("tenant-test", true)
        .expect("begin model pricing sync");

    assert!(should_start);
    assert_eq!(snapshot.status, BackgroundTaskStatus::Running);
    assert_eq!(
        snapshot.progress.phase,
        ModelPricingSyncProgressPhase::Preparing
    );
    // 任务起步前汇率未知，不得用硬编码占位值冒充真实汇率。
    assert_eq!(snapshot.progress.exchange_rate, 0.0);

    let running = task_runtime.get(&snapshot.id).expect("running snapshot");
    assert_eq!(running.title.as_deref(), Some("模型价格同步与重算"));
    assert_eq!(running.category, "conversation/pricing_sync");
    assert!(
        running.capabilities.cancellable,
        "价格同步任务必须声明可取消"
    );
    assert!(running.user_visible, "价格同步任务必须在任务中心可见");

    let result = serde_json::json!({
        "channelUsed": "litellm",
        "modelsCount": 42,
        "exchangeRate": 7.31,
        "recalculatedEventsCount": 9,
        "catalogVersion": "litellm-2026-11-16",
        "updatedAt": "2026-11-16T10:00:00+00:00",
    });
    let finished = registry
        .finish_model_pricing_sync(&snapshot.id, Ok(result.clone()))
        .expect("finish task");

    assert_eq!(finished.status, BackgroundTaskStatus::Completed);
    assert_eq!(
        finished.progress.phase,
        ModelPricingSyncProgressPhase::Completed
    );
    assert_eq!(finished.progress.channel.as_deref(), Some("litellm"));
    assert_eq!(finished.progress.models_count, 42);
    assert_eq!(finished.progress.recalculated_events_count, 9);
    assert_eq!(finished.progress.exchange_rate, 7.31);
    assert_eq!(finished.result.as_ref(), Some(&result));

    // 终态结果经 TaskView 投影后可直接被任务中心消费。
    let terminal = task_runtime.get(&snapshot.id).expect("terminal snapshot");
    let view = TaskView::from_snapshot(&terminal);
    assert_eq!(view.title, "模型价格同步与重算");
    assert_eq!(view.category, "conversation/pricing_sync");
    assert_eq!(view.state, "succeeded");
    let projected = view.result.expect("TaskView.result 必须携带同步结果");
    assert_eq!(projected["channelUsed"].as_str(), Some("litellm"));
    assert_eq!(projected["modelsCount"].as_u64(), Some(42));
    assert_eq!(projected["recalculatedEventsCount"].as_u64(), Some(9));
    assert_eq!(projected["exchangeRate"].as_f64(), Some(7.31));
    assert_eq!(
        projected["catalogVersion"].as_str(),
        Some("litellm-2026-11-16")
    );
}

#[tokio::test]
async fn test_model_pricing_sync_deduplicates_active_task() {
    let task_runtime = TaskRuntime::new();
    let registry = BackgroundTaskRegistry::with_task_runtime(task_runtime.clone());

    let (first, started) = registry
        .begin_model_pricing_sync_for_tenant("tenant-test", false)
        .expect("begin first sync");
    assert!(started);

    let (second, started_again) = registry
        .begin_model_pricing_sync_for_tenant("tenant-test", false)
        .expect("begin second sync");
    assert!(!started_again, "同一租户价格同步任务必须去重");
    assert_eq!(second.id, first.id);
}
