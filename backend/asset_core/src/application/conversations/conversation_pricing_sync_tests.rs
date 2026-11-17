use super::*;
use crate::backend::application::system::{TaskCancelParams, TaskListParams, TaskView};
use crate::backend::domain::conversations::pricing::ModelPriceEntry;
use crate::backend::infrastructure::conversations::pricing_fetcher::{
    PricingSourceChannel, TieredFetchOutcome,
};
use crate::backend::infrastructure::tasks::{stages, TaskState};

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
                    downgraded: false,
                    warning: None,
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
    assert_eq!(result["catalogVersion"].as_str(), Some("litellm-test-1"));
    assert!(
        result["updatedAt"].as_str().is_some_and(|v| !v.is_empty()),
        "TaskView.result 必须携带同步完成时间"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pricing_sync_cancel_before_persist_keeps_catalog_untouched() {
    use std::sync::mpsc;
    use std::time::Duration;

    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-pricing-pipeline-cancel-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path)
        .await
        .expect("open app service");

    let pool = service.db.pool();
    let tenant_id = service.tenant_id().to_string();
    let before = load_pricing_catalog_entries_sqlx(pool, &tenant_id)
        .await
        .expect("load pricing catalog")
        .len();

    let (entered_tx, entered_rx) = mpsc::channel::<()>();
    let (release_tx, release_rx) = mpsc::channel::<()>();

    let handle = service
        .start_model_pricing_sync_with_fetcher(false, move |_cached_rate, _cached, _targets| {
            let _ = entered_tx.send(());
            let _ = release_rx.recv_timeout(Duration::from_secs(30));
            (
                7.31,
                TieredFetchOutcome {
                    channel: PricingSourceChannel::LiteLlm,
                    entries: vec![ModelPriceEntry {
                        provider: "openai".to_string(),
                        model_patterns: vec!["cancel-guard-model".to_string()],
                        currency: "CNY".to_string(),
                        input_per_million: 1.0,
                        output_per_million: 2.0,
                        source_channel: "litellm".to_string(),
                        ..Default::default()
                    }],
                    rate: 7.31,
                    catalog_version: "litellm-cancel-test".to_string(),
                    downgraded: false,
                    warning: None,
                },
            )
        })
        .expect("start pricing sync pipeline");

    // 等待抓取阶段真正进入，此时尚未写入任何价格条目。
    let entered =
        tokio::task::spawn_blocking(move || entered_rx.recv_timeout(Duration::from_secs(30)))
            .await
            .expect("join pricing fetch gate");
    entered.expect("pricing fetch must have started");

    let requested = service
        .cancel_public_task(TaskCancelParams {
            task_id: handle.task_id().to_string(),
        })
        .expect("cancel pricing sync task");
    assert_eq!(
        requested.state, "cancelling",
        "运行中的价格同步任务取消后先进入 cancelling"
    );

    release_tx.send(()).expect("release pricing fetch");

    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("terminal pricing sync snapshot");
    assert_eq!(snapshot.state, TaskState::Canceled);
    assert!(snapshot.result.is_none(), "取消的任务不得回写成功结果");

    let after = load_pricing_catalog_entries_sqlx(pool, &tenant_id)
        .await
        .expect("load pricing catalog")
        .len();
    assert_eq!(before, after, "取消后不得写入价格库");
}

fn single_entry_outcome(
    channel: PricingSourceChannel,
    model_id: &str,
    input_per_million: f64,
    catalog_version: &str,
) -> TieredFetchOutcome {
    TieredFetchOutcome {
        channel,
        entries: vec![ModelPriceEntry {
            provider: "openai".to_string(),
            model_patterns: vec![model_id.to_string()],
            currency: "CNY".to_string(),
            input_per_million,
            output_per_million: input_per_million * 4.0,
            source_channel: channel.as_str().to_string(),
            ..Default::default()
        }],
        rate: 7.25,
        catalog_version: catalog_version.to_string(),
        downgraded: false,
        warning: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn force_bypasses_stuck_dedup_task_while_non_force_keeps_rejecting() {
    use std::sync::mpsc;
    use std::time::Duration;

    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-pricing-force-dedup-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path)
        .await
        .expect("open app service");

    let (entered_tx, entered_rx) = mpsc::channel::<()>();
    let (release_tx, release_rx) = mpsc::channel::<()>();

    // 模拟一次「卡死」的历史同步：抓取阶段长时间不返回。
    let stuck = service
        .start_model_pricing_sync_with_fetcher(false, move |_rate, _cached, _targets| {
            let _ = entered_tx.send(());
            let _ = release_rx.recv_timeout(Duration::from_secs(30));
            (
                7.25,
                single_entry_outcome(
                    PricingSourceChannel::Cache,
                    "stuck-model",
                    1.0,
                    "cache-stuck",
                ),
            )
        })
        .expect("start stuck sync");
    let entered =
        tokio::task::spawn_blocking(move || entered_rx.recv_timeout(Duration::from_secs(30)))
            .await
            .expect("join pricing fetch gate");
    entered.expect("stuck sync must reach fetch stage");

    // force = false：同 dedup_key 的任务仍在运行，必须保持既有「拒绝」语义。
    let rejected =
        service.start_model_pricing_sync_with_fetcher(false, |_rate, _cached, _targets| {
            (
                7.25,
                single_entry_outcome(
                    PricingSourceChannel::LiteLlm,
                    "dedup-model",
                    2.0,
                    "litellm-dedup",
                ),
            )
        });
    assert!(
        rejected.is_err(),
        "非 force 时必须拒绝与运行中任务同 dedup_key 的重复同步"
    );

    // force = true：取消卡死任务并启动新一轮同步。
    let forced = service
        .start_model_pricing_sync_with_fetcher(true, |_rate, _cached, _targets| {
            (
                7.25,
                single_entry_outcome(
                    PricingSourceChannel::LiteLlm,
                    "forced-model",
                    3.0,
                    "litellm-forced",
                ),
            )
        })
        .expect("force must start a new sync round");
    assert_ne!(
        forced.task_id(),
        stuck.task_id(),
        "force 必须产出新的任务 id"
    );

    let forced_snapshot = forced
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("forced sync snapshot");
    assert_eq!(forced_snapshot.state, TaskState::Succeeded);
    assert_eq!(
        forced_snapshot
            .result
            .as_ref()
            .and_then(|result| result["catalogVersion"].as_str()),
        Some("litellm-forced")
    );

    release_tx.send(()).expect("release stuck fetch");
    let stuck_snapshot = stuck
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("stuck sync snapshot");
    assert_eq!(
        stuck_snapshot.state,
        TaskState::Canceled,
        "force 必须取消被顶替的历史任务"
    );
    assert!(
        stuck_snapshot.result.is_none(),
        "被取消的历史任务不得回写成功结果"
    );
}

#[tokio::test]
async fn downgraded_fallback_is_surfaced_in_sync_result_and_task_view() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-pricing-downgrade-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path)
        .await
        .expect("open app service");

    let handle = service
        .start_model_pricing_sync_with_fetcher(false, |_rate, _cached, _targets| {
            let mut outcome = single_entry_outcome(
                PricingSourceChannel::Cache,
                "downgraded-model",
                5.0,
                "cache-2026-11-17",
            );
            outcome.downgraded = true;
            outcome.warning = Some("远端价格渠道均不可达，已降级使用本地缓存".to_string());
            (7.25, outcome)
        })
        .expect("start downgraded sync");

    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("downgraded sync snapshot");
    assert_eq!(snapshot.state, TaskState::Succeeded);

    let result = snapshot
        .result
        .as_ref()
        .expect("终态结果必须可由 TaskView.result 读取");
    assert_eq!(result["channelUsed"].as_str(), Some("cache"));
    assert_eq!(result["downgraded"].as_bool(), Some(true));
    assert!(
        result["warning"]
            .as_str()
            .is_some_and(|warning| warning.contains("降级")),
        "降级原因必须透出到同步结果"
    );
    assert!(
        result["modelsCount"].as_u64().unwrap_or(0) > 0,
        "降级到缓存兜底时仍必须写入可入库的参考价格"
    );

    // 任务详情同样携带降级标记（TaskCenter 投影）
    let view = list_visible(&service)
        .into_iter()
        .find(|task| task.id == handle.task_id())
        .expect("downgraded task must stay visible");
    let view_result = view.result.as_ref().expect("TaskView.result");
    assert_eq!(view_result["downgraded"].as_bool(), Some(true));
    assert!(view_result["warning"].as_str().is_some());
}
