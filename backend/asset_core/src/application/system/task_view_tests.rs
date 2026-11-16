use super::*;
use crate::backend::infrastructure::tasks::{
    TaskCapabilities, TaskKind, TaskOutcome, TaskSnapshot, TaskState,
};

fn snapshot_of(kind: TaskKind, category: &str) -> TaskSnapshot {
    let mut snapshot = TaskSnapshot::empty("task-view-test");
    snapshot.kind = kind;
    snapshot.category = category.to_string();
    snapshot.title = Some("规范化标题".to_string());
    snapshot
}

#[test]
fn task_view_projects_category_and_terminal_result() {
    let mut snapshot = snapshot_of(TaskKind::ModelPricingSync, "conversation/pricing_sync");
    snapshot.state = TaskState::Succeeded;
    snapshot.outcome = Some(TaskOutcome::Success);
    snapshot.result = Some(serde_json::json!({
        "channelUsed": "litellm",
        "modelsCount": 3,
        "exchangeRate": 7.25,
    }));

    let view = TaskView::from_snapshot(&snapshot);

    assert_eq!(view.category, "conversation/pricing_sync");
    assert_eq!(view.title, "规范化标题");
    let result = view.result.expect("终态结果必须投影到 TaskView.result");
    assert_eq!(
        result.get("channelUsed").and_then(|value| value.as_str()),
        Some("litellm")
    );
    assert_eq!(
        result.get("modelsCount").and_then(|value| value.as_u64()),
        Some(3)
    );
}

#[test]
fn running_task_view_exposes_cancellable_capability() {
    let mut snapshot = snapshot_of(TaskKind::ConversationUsageScan, "conversation/usage_scan");
    snapshot.state = TaskState::Running;
    snapshot.capabilities = TaskCapabilities {
        cancellable: true,
        retryable: false,
        clearable: true,
    };

    let view = TaskView::from_snapshot(&snapshot);

    assert_eq!(view.category, "conversation/usage_scan");
    assert!(view.capabilities.cancellable);
    assert!(view.result.is_none(), "运行期不应出现终态结果");
}
