use super::*;
use crate::backend::application::system::{TaskListParams, TaskView};
use crate::backend::domain::conversations::pricing::ModelPriceEntry;
use crate::backend::infrastructure::conversations::pricing_fetcher::{
    PricingSourceChannel, TieredFetchOutcome,
};
use crate::backend::infrastructure::tasks::stages;

fn list_visible(service: &AppService) -> Vec<TaskView> {
    service
        .list_public_tasks(TaskListParams {
            tenant_id: None,
            all_tenants: None,
            active_only: None,
        })
        .expect("list public tasks")
}

#[tokio::test]
async fn pricing_sync_pipeline_is_managed_visible_and_projects_result() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-pricing-pipeline-managed-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path)
        .await
        .expect("open app service");

    // 注入假 fetcher，彻底禁网。
    let handle = service
        .start_model_pricing_sync_with_fetcher(false, |_cached_rate, _cached, _targets| {
            (
                7.31,
                TieredFetchOutcome {
                    channel: PricingSourceChannel::LiteLlm,
                    entries: vec![ModelPriceEntry {
                        provider: "openai".to_string(),
                        model_patterns: vec!["gpt-4o".to_string()],
                        currency: "CNY".to_string(),
                        input_per_million: 18.5,
                        output_per_million: 74.0,
                        source_channel: "litellm".to_string(),
                        ..Default::default()
                    }],
                    rate: 7.31,
                    catalog_version: "litellm-test-1".to_string(),
                },
            )
        })
        .expect("start pricing sync pipeline");

    // 运行期：任务中心可见，元数据完整且可取消
    let visible = list_visible(&service);
    let running = visible
        .iter()
        .find(|task| task.id == handle.task_id())
        .expect("pricing sync task must be visible in task center");
    assert_eq!(running.title, "模型价格同步与重算");
    assert_eq!(running.category, "conversation/pricing_sync");
    assert!(
        running.capabilities.cancellable,
        "运行中的价格同步任务必须声明可取消"
    );

    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("pricing sync snapshot");
    assert_eq!(
        snapshot.state,
        crate::backend::infrastructure::tasks::TaskState::Succeeded
    );
    let stage_ids: Vec<&str> = snapshot.stages.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        stage_ids,
        vec![
            stages::FETCH_PRICING,
            stages::PERSIST_CATALOG,
            stages::RECALCULATE_HISTORY
        ]
    );

    // 终态：结果由 TaskRuntime 投影到公开 TaskView
    let finished = list_visible(&service);
    let view = finished
        .iter()
        .find(|task| task.id == handle.task_id())
        .expect("finished pricing sync task must stay visible");
    assert_eq!(view.state, "succeeded");
    let result = view
        .result
        .as_ref()
        .expect("TaskView.result 必须包含同步结果");
    assert_eq!(result["channelUsed"].as_str(), Some("litellm"));
    assert_eq!(result["modelsCount"].as_u64(), Some(1));
    assert_eq!(result["recalculatedEventsCount"].as_u64(), Some(0));
    assert_eq!(result["exchangeRate"].as_f64(), Some(7.31));
}
