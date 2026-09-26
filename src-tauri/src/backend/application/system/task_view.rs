use crate::backend::application::agents::AgentSessionRef;
use crate::backend::infrastructure::tasks::{
    StageStatus, TaskOutcome, TaskProgress, TaskSnapshot, TaskStage, TaskState,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskView {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub tenant_id: Option<String>,
    pub state: String,
    pub outcome: Option<String>,
    pub started_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
    pub progress: Option<TaskProgress>,
    pub stages: Vec<TaskStageView>,
    pub metrics: Vec<TaskMetricView>,
    pub failures: Vec<TaskFailureView>,
    pub error_summary: Option<String>,
    pub result_summary: Option<String>,
    pub capabilities: TaskCapabilitiesView,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_ref: Option<AgentSessionRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskStageView {
    pub id: String,
    pub name: String,
    pub status: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub duration_ms: Option<u64>,
    pub progress: Option<TaskProgress>,
    pub current_activities: Vec<TaskActivityView>,
    pub metrics: Vec<TaskMetricView>,
    pub failures: Vec<TaskFailureView>,
    pub skipped: Vec<TaskSkippedReasonView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_ref: Option<AgentSessionRef>,
    #[serde(default)]
    pub steps: Vec<TaskStageStepView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskStageStepView {
    pub timestamp: String,
    pub operation: String,
    pub detail: Option<String>,
    pub current: Option<u64>,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskActivityView {
    pub stage_id: String,
    pub worker_id: String,
    pub operation: String,
    pub path: Option<String>,
    pub display_path: Option<String>,
    pub started_at: String,
    pub current: Option<u64>,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskFailureView {
    pub code: String,
    pub message: String,
    pub stage: String,
    pub identity: Option<String>,
    pub retryable: bool,
    pub path: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskSkippedReasonView {
    pub reason_code: String,
    pub count: u64,
    pub samples: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskMetricView {
    pub code: String,
    pub value: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskCapabilitiesView {
    pub cancellable: bool,
    pub retryable: bool,
    pub clearable: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TaskListParams {
    #[serde(default, alias = "tenantId")]
    pub tenant_id: Option<String>,
    #[serde(default, alias = "allTenants")]
    pub all_tenants: Option<bool>,
    #[serde(default, alias = "activeOnly")]
    pub active_only: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TaskGetParams {
    #[serde(alias = "taskId")]
    pub task_id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TaskCancelParams {
    #[serde(alias = "taskId")]
    pub task_id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TaskRetryParams {
    #[serde(alias = "taskId")]
    pub task_id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TaskClearParams {
    #[serde(default, alias = "tenantId")]
    pub tenant_id: Option<String>,
    #[serde(default, alias = "taskId")]
    pub task_id: Option<String>,
}

impl TaskView {
    pub fn from_snapshot(snapshot: &TaskSnapshot) -> Self {
        let title = snapshot
            .title
            .clone()
            .unwrap_or_else(|| match snapshot.kind {
                crate::backend::infrastructure::tasks::TaskKind::ConversationSync => {
                    match snapshot.category.as_str() {
                        "conversation/web_sync" => "网页记录同步".to_string(),
                        "conversation/session_sync" => "会话记录同步".to_string(),
                        _ => "会话同步".to_string(),
                    }
                }
                crate::backend::infrastructure::tasks::TaskKind::ConversationUsageScan => {
                    "用量统计扫描".to_string()
                }
                crate::backend::infrastructure::tasks::TaskKind::ConversationDataMaintenance => {
                    "会话数据维护".to_string()
                }
                crate::backend::infrastructure::tasks::TaskKind::SearchIndexRebuild => {
                    "搜索索引重建".to_string()
                }
                crate::backend::infrastructure::tasks::TaskKind::ScriptInstall => {
                    "脚本安装".to_string()
                }
                crate::backend::infrastructure::tasks::TaskKind::ExtensionLifecycle => {
                    "扩展生命周期".to_string()
                }
                crate::backend::infrastructure::tasks::TaskKind::AiExecution => {
                    "AI 执行".to_string()
                }
                crate::backend::infrastructure::tasks::TaskKind::AgentMarketRefresh => {
                    "市场刷新".to_string()
                }
                crate::backend::infrastructure::tasks::TaskKind::Memory => "记忆生成".to_string(),
                crate::backend::infrastructure::tasks::TaskKind::RemoteSkillAcquire => {
                    "远程技能获取".to_string()
                }
                crate::backend::infrastructure::tasks::TaskKind::Scan => "数据源扫描".to_string(),
                crate::backend::infrastructure::tasks::TaskKind::Backup => "备份维护".to_string(),
                crate::backend::infrastructure::tasks::TaskKind::BatchMount => {
                    "批量挂载".to_string()
                }
                crate::backend::infrastructure::tasks::TaskKind::Other => "后台任务".to_string(),
            });

        let state_str = match snapshot.state {
            TaskState::Pending => "pending",
            TaskState::Running => "running",
            TaskState::Cancelling => "cancelling",
            TaskState::Succeeded => "succeeded",
            TaskState::Failed => "failed",
            TaskState::Canceled => "canceled",
        };

        let outcome_str = snapshot.outcome.map(|outcome| match outcome {
            TaskOutcome::Success => "success",
            TaskOutcome::PartialSuccess => "partial_success",
            TaskOutcome::Failure => "failure",
            TaskOutcome::Canceled => "canceled",
        });

        let stages: Vec<TaskStageView> = snapshot
            .stages
            .iter()
            .map(TaskStageView::from_model)
            .collect();

        let metrics = snapshot
            .metrics
            .iter()
            .map(|m| TaskMetricView {
                code: m.code.clone(),
                value: m.value,
            })
            .collect();

        let failures = snapshot
            .failures
            .iter()
            .map(|f| TaskFailureView {
                code: f.code.clone(),
                message: f.message.clone(),
                stage: f.stage.clone(),
                identity: f.identity.clone(),
                retryable: f.retryable,
                path: f.path.clone(),
                timestamp: f.timestamp.clone(),
            })
            .collect();

        let agent_session_ref = snapshot.agent_session_ref.clone().or_else(|| {
            stages
                .iter()
                .find_map(|stage| stage.agent_session_ref.clone())
        });

        Self {
            id: snapshot.task_id.clone(),
            kind: format!("{:?}", snapshot.kind).to_ascii_lowercase(),
            title,
            tenant_id: snapshot.tenant_id.clone(),
            state: state_str.to_string(),
            outcome: outcome_str.map(str::to_string),
            started_at: snapshot.started_at.clone(),
            updated_at: snapshot.updated_at.clone(),
            finished_at: snapshot.finished_at.clone(),
            progress: snapshot.progress.clone(),
            stages,
            metrics,
            failures,
            error_summary: snapshot.error_summary.clone(),
            result_summary: snapshot.result_summary.clone(),
            capabilities: TaskCapabilitiesView {
                cancellable: snapshot.capabilities.cancellable && snapshot.state.is_active(),
                retryable: snapshot.capabilities.retryable && snapshot.state.is_terminal(),
                clearable: snapshot.capabilities.clearable && snapshot.state.is_terminal(),
            },
            revision: snapshot.revision,
            agent_session_ref,
        }
    }
}

impl TaskStageView {
    pub fn from_model(stage: &TaskStage) -> Self {
        let status_str = match stage.status {
            StageStatus::Pending => "pending",
            StageStatus::Running => "running",
            StageStatus::Succeeded => "succeeded",
            StageStatus::PartialSuccess => "partial_success",
            StageStatus::Failed => "failed",
            StageStatus::Canceled => "canceled",
            StageStatus::Skipped => "skipped",
        };

        let current_activities = stage
            .current_activities
            .iter()
            .map(|a| TaskActivityView {
                stage_id: a.stage_id.clone(),
                worker_id: a.worker_id.clone(),
                operation: a.operation.clone(),
                path: a.path.clone(),
                display_path: a.display_path.clone().or_else(|| {
                    a.path
                        .as_deref()
                        .map(crate::backend::infrastructure::path_utils::display_path_or_original)
                }),
                started_at: a.started_at.clone(),
                current: a.current,
                total: a.total,
            })
            .collect();

        let metrics = stage
            .metrics
            .iter()
            .map(|m| TaskMetricView {
                code: m.code.clone(),
                value: m.value,
            })
            .collect();

        let failures = stage
            .failures
            .iter()
            .map(|f| TaskFailureView {
                code: f.code.clone(),
                message: f.message.clone(),
                stage: f.stage.clone(),
                identity: f.identity.clone(),
                retryable: f.retryable,
                path: f.path.clone(),
                timestamp: f.timestamp.clone(),
            })
            .collect();

        let skipped = stage
            .skipped
            .iter()
            .map(|s| TaskSkippedReasonView {
                reason_code: s.reason_code.clone(),
                count: s.count,
                samples: s.samples.clone(),
            })
            .collect();

        let steps = stage
            .steps
            .iter()
            .map(|s| TaskStageStepView {
                timestamp: s.timestamp.clone(),
                operation: s.operation.clone(),
                detail: s.detail.clone(),
                current: s.current,
                total: s.total,
            })
            .collect();

        Self {
            id: stage.id.clone(),
            name: stage.name.clone(),
            status: status_str.to_string(),
            started_at: stage.started_at.clone(),
            finished_at: stage.finished_at.clone(),
            duration_ms: stage.duration_ms,
            progress: stage.progress.clone(),
            current_activities,
            metrics,
            failures,
            skipped,
            agent_session_ref: stage.agent_session_ref.clone(),
            steps,
        }
    }
}
