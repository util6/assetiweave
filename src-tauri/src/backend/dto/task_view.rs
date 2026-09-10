use crate::backend::runtime::tasks::{
    StageStatus, TaskActivity, TaskCapabilities, TaskFailure, TaskMetric, TaskOutcome,
    TaskProgress, TaskSkippedGroup, TaskSnapshot, TaskStage, TaskState,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskView {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) title: String,
    pub(crate) tenant_id: Option<String>,
    pub(crate) state: String,
    pub(crate) outcome: Option<String>,
    pub(crate) started_at: String,
    pub(crate) updated_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) progress: Option<TaskProgress>,
    pub(crate) stages: Vec<TaskStageView>,
    pub(crate) metrics: Vec<TaskMetricView>,
    pub(crate) failures: Vec<TaskFailureView>,
    pub(crate) error_summary: Option<String>,
    pub(crate) result_summary: Option<String>,
    pub(crate) capabilities: TaskCapabilitiesView,
    pub(crate) revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskStageView {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) started_at: Option<String>,
    pub(crate) finished_at: Option<String>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) progress: Option<TaskProgress>,
    pub(crate) current_activities: Vec<TaskActivityView>,
    pub(crate) metrics: Vec<TaskMetricView>,
    pub(crate) failures: Vec<TaskFailureView>,
    pub(crate) skipped: Vec<TaskSkippedReasonView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskActivityView {
    pub(crate) stage_id: String,
    pub(crate) worker_id: String,
    pub(crate) operation: String,
    pub(crate) path: Option<String>,
    pub(crate) display_path: Option<String>,
    pub(crate) started_at: String,
    pub(crate) current: Option<u64>,
    pub(crate) total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskFailureView {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) stage: String,
    pub(crate) identity: Option<String>,
    pub(crate) retryable: bool,
    pub(crate) path: Option<String>,
    pub(crate) timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskSkippedReasonView {
    pub(crate) reason_code: String,
    pub(crate) count: u64,
    pub(crate) samples: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskMetricView {
    pub(crate) code: String,
    pub(crate) value: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskCapabilitiesView {
    pub(crate) cancellable: bool,
    pub(crate) retryable: bool,
    pub(crate) clearable: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct TaskListParams {
    #[serde(default, alias = "tenantId")]
    pub(crate) tenant_id: Option<String>,
    #[serde(default, alias = "allTenants")]
    pub(crate) all_tenants: Option<bool>,
    #[serde(default, alias = "activeOnly")]
    pub(crate) active_only: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct TaskGetParams {
    #[serde(alias = "taskId")]
    pub(crate) task_id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct TaskCancelParams {
    #[serde(alias = "taskId")]
    pub(crate) task_id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct TaskRetryParams {
    #[serde(alias = "taskId")]
    pub(crate) task_id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct TaskClearParams {
    #[serde(default, alias = "tenantId")]
    pub(crate) tenant_id: Option<String>,
    #[serde(default, alias = "taskId")]
    pub(crate) task_id: Option<String>,
}

impl TaskView {
    pub(crate) fn from_snapshot(snapshot: &TaskSnapshot) -> Self {
        let title = snapshot
            .title
            .clone()
            .unwrap_or_else(|| match snapshot.kind {
                crate::backend::runtime::tasks::TaskKind::ConversationSync => {
                    "会话同步".to_string()
                }
                crate::backend::runtime::tasks::TaskKind::ConversationDataMaintenance => {
                    "会话数据维护".to_string()
                }
                crate::backend::runtime::tasks::TaskKind::SearchIndexRebuild => {
                    "搜索索引重建".to_string()
                }
                crate::backend::runtime::tasks::TaskKind::ScriptInstall => "脚本安装".to_string(),
                crate::backend::runtime::tasks::TaskKind::ExtensionLifecycle => {
                    "扩展生命周期".to_string()
                }
                crate::backend::runtime::tasks::TaskKind::AiExecution => "AI 执行".to_string(),
                crate::backend::runtime::tasks::TaskKind::AgentMarketRefresh => {
                    "市场刷新".to_string()
                }
                crate::backend::runtime::tasks::TaskKind::Memory => "记忆生成".to_string(),
                crate::backend::runtime::tasks::TaskKind::RemoteSkillAcquire => {
                    "远程技能获取".to_string()
                }
                crate::backend::runtime::tasks::TaskKind::Scan => "数据源扫描".to_string(),
                crate::backend::runtime::tasks::TaskKind::Backup => "备份维护".to_string(),
                crate::backend::runtime::tasks::TaskKind::BatchMount => "批量挂载".to_string(),
                crate::backend::runtime::tasks::TaskKind::TeamRun => "Team 执行".to_string(),
                crate::backend::runtime::tasks::TaskKind::Other => "后台任务".to_string(),
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

        let stages = snapshot
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
        }
    }
}

impl TaskStageView {
    pub(crate) fn from_model(stage: &TaskStage) -> Self {
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
                        .map(crate::backend::path_utils::display_path_or_original)
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
        }
    }
}
