use super::*;
use crate::backend::application::system::TaskListParams;
use crate::backend::infrastructure::tasks::stages;

fn temp_db_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "assetiweave-usage-pipeline-{label}-{}.sqlite",
        uuid::Uuid::new_v4()
    ))
}

fn list_visible(service: &AppService) -> Vec<crate::backend::application::system::TaskView> {
    service
        .list_public_tasks(TaskListParams {
            tenant_id: None,
            all_tenants: None,
            active_only: None,
        })
        .expect("list public tasks")
}

#[tokio::test]
async fn usage_scan_pipeline_is_managed_visible_and_projects_result() {
    let service = AppService::open_with_db_path(temp_db_path("managed"))
        .await
        .expect("open app service");

    let handle = service
        .start_conversation_usage_scan(UsageScanOptions {
            // 隔离外部适配器子进程：只匹配不存在的来源，流水线全阶段仍真实走完。
            source_id: Some("test-unmatched-source".to_string()),
            ..Default::default()
        })
        .expect("start usage scan pipeline");

    // 运行期：任务中心可见，元数据完整且可取消
    let visible = list_visible(&service);
    let running = visible
        .iter()
        .find(|task| task.id == handle.task_id())
        .expect("usage scan task must be visible in task center");
    assert_eq!(running.title, "用量统计扫描");
    assert_eq!(running.category, "conversation/usage_scan");
    assert!(
        running.capabilities.cancellable,
        "运行中的用量扫描任务必须声明可取消"
    );

    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("usage scan snapshot");
    assert_eq!(
        snapshot.state,
        crate::backend::infrastructure::tasks::TaskState::Succeeded
    );

    // 终态：阶段骨架来自编译期常量
    let stage_ids: Vec<&str> = snapshot.stages.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        stage_ids,
        vec![
            stages::PREPARE_SOURCES,
            stages::COLLECT_USAGE,
            stages::PERSIST_EVENTS,
            stages::RESOLVE_PRICING,
            stages::CALCULATE_COSTS,
        ]
    );

    // 终态：结果由 TaskRuntime 投影到公开 TaskView
    let finished = list_visible(&service);
    let view = finished
        .iter()
        .find(|task| task.id == handle.task_id())
        .expect("finished usage scan task must stay visible");
    assert_eq!(view.state, "succeeded");
    let result = view
        .result
        .as_ref()
        .expect("TaskView.result 必须包含扫描统计");
    assert_eq!(result["total_sources"].as_u64(), Some(0));
    assert_eq!(result["total_events_ingested"].as_u64(), Some(0));
    assert_eq!(result["mode"].as_str(), Some("incremental"));
}
