use crate::backend::domain::AppErrorView;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

pub(crate) const TASK_TERMINAL_RETENTION: Duration = Duration::from_secs(10 * 60);
pub(crate) const TASK_TERMINAL_LIMIT: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) enum TaskKind {
    ConversationSync,
    ConversationUsageScan,
    ConversationDataMaintenance,
    SearchIndexRebuild,
    ScriptInstall,
    ExtensionLifecycle,
    AiExecution,
    AgentMarketRefresh,
    Memory,
    RemoteSkillAcquire,
    Scan,
    Backup,
    BatchMount,
    Other,
}

impl TaskKind {
    pub(crate) fn default_category_string(&self) -> &'static str {
        match self {
            Self::ConversationSync => "conversation/sync",
            Self::ConversationUsageScan => "conversation/usage_scan",
            Self::ConversationDataMaintenance => "conversation/maintenance",
            Self::SearchIndexRebuild => "search/rebuild",
            Self::ScriptInstall => "script/install",
            Self::ExtensionLifecycle => "extension/lifecycle",
            Self::AiExecution => "ai/execution",
            Self::AgentMarketRefresh => "agent_market/refresh",
            Self::Memory => "memory/general",
            Self::RemoteSkillAcquire => "skill/remote_acquire",
            Self::Scan => "source/scan",
            Self::Backup => "system/backup",
            Self::BatchMount => "mount/batch",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) enum TaskState {
    Pending,
    Running,
    Cancelling,
    Succeeded,
    Failed,
    Canceled,
}

impl TaskState {
    pub(crate) fn is_active(self) -> bool {
        matches!(self, Self::Pending | Self::Running | Self::Cancelling)
    }

    pub(crate) fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Canceled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TaskOutcome {
    Success,
    PartialSuccess,
    Failure,
    Canceled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StageStatus {
    Pending,
    Running,
    Succeeded,
    PartialSuccess,
    Failed,
    Canceled,
    Skipped,
}

impl StageStatus {
    pub(crate) fn is_active(self) -> bool {
        matches!(self, Self::Pending | Self::Running)
    }

    pub(crate) fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::PartialSuccess | Self::Failed | Self::Canceled | Self::Skipped
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TaskActivity {
    pub(crate) stage_id: String,
    pub(crate) worker_id: String,
    pub(crate) operation: String,
    pub(crate) path: Option<String>,
    pub(crate) display_path: Option<String>,
    pub(crate) started_at: String,
    pub(crate) current: Option<u64>,
    pub(crate) total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TaskFailure {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) stage: String,
    pub(crate) identity: Option<String>,
    pub(crate) retryable: bool,
    pub(crate) path: Option<String>,
    pub(crate) timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TaskSkippedGroup {
    pub(crate) reason_code: String,
    pub(crate) count: u64,
    pub(crate) samples: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TaskMetric {
    pub(crate) code: String,
    pub(crate) value: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskStageStep {
    pub(crate) timestamp: String,
    pub(crate) operation: String,
    pub(crate) detail: Option<String>,
    pub(crate) current: Option<u64>,
    pub(crate) total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TaskStage {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) status: StageStatus,
    pub(crate) started_at: Option<String>,
    pub(crate) finished_at: Option<String>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) progress: Option<TaskProgress>,
    pub(crate) current_activities: Vec<TaskActivity>,
    pub(crate) metrics: Vec<TaskMetric>,
    pub(crate) failures: Vec<TaskFailure>,
    pub(crate) skipped: Vec<TaskSkippedGroup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) agent_session_ref: Option<crate::backend::domain::agents::AgentSessionRef>,
    #[serde(default)]
    pub(crate) steps: Vec<TaskStageStep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TaskCapabilities {
    pub(crate) cancellable: bool,
    pub(crate) retryable: bool,
    pub(crate) clearable: bool,
}

impl Default for TaskCapabilities {
    fn default() -> Self {
        Self {
            cancellable: true,
            retryable: false,
            clearable: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub(crate) struct TaskProgress {
    pub(crate) current: u64,
    pub(crate) total: Option<u64>,
    pub(crate) note: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct TaskSpec {
    pub(crate) kind: TaskKind,
    pub(crate) task_id: Option<String>,
    pub(crate) tenant_id: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) user_visible: Option<bool>,
    pub(crate) dedup_key: Option<String>,
    pub(crate) conflict_keys: Vec<String>,
    pub(crate) capabilities: Option<TaskCapabilities>,
    pub(crate) detail: Value,
    pub(crate) category: Option<super::task_pipeline::TaskCategory>,
    pub(crate) pipeline: Option<super::task_pipeline::PipelineDescriptor>,
}

impl TaskSpec {
    pub(crate) fn global(kind: TaskKind, dedup_key: Option<String>) -> Self {
        Self {
            kind,
            task_id: None,
            tenant_id: None,
            title: None,
            user_visible: None,
            dedup_key,
            conflict_keys: Vec::new(),
            capabilities: None,
            detail: Value::Null,
            category: None,
            pipeline: None,
        }
    }

    pub(crate) fn new(kind: TaskKind, dedup_key: Option<String>) -> Self {
        Self::global(kind, dedup_key)
    }

    pub(crate) fn with_task_id(mut self, task_id: impl Into<String>) -> Self {
        self.task_id = Some(task_id.into());
        self
    }

    pub(crate) fn with_tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    pub(crate) fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub(crate) fn with_user_visible(mut self, user_visible: bool) -> Self {
        self.user_visible = Some(user_visible);
        self
    }

    pub(crate) fn with_capabilities(mut self, capabilities: TaskCapabilities) -> Self {
        self.capabilities = Some(capabilities);
        self
    }

    pub(crate) fn with_category(
        mut self,
        category: impl Into<super::task_pipeline::TaskCategory>,
    ) -> Self {
        self.category = Some(category.into());
        self
    }

    pub(crate) fn with_pipeline(
        mut self,
        pipeline: super::task_pipeline::PipelineDescriptor,
    ) -> Self {
        self.pipeline = Some(pipeline);
        self
    }

    pub(crate) fn with_conflict_key(mut self, conflict_key: impl Into<String>) -> Self {
        self.conflict_keys.push(conflict_key.into());
        self
    }

    pub(crate) fn with_conflict_keys(
        mut self,
        conflict_keys: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.conflict_keys
            .extend(conflict_keys.into_iter().map(Into::into));
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TaskSnapshot {
    pub(crate) task_id: String,
    pub(crate) kind: TaskKind,
    #[serde(default)]
    pub(crate) category: String,
    #[serde(skip)]
    pub(crate) tenant_id: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) user_visible: bool,
    pub(crate) dedup_key: Option<String>,
    pub(crate) state: TaskState,
    pub(crate) outcome: Option<TaskOutcome>,
    pub(crate) progress: Option<TaskProgress>,
    pub(crate) error: Option<AppErrorView>,
    pub(crate) started_at: String,
    pub(crate) updated_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) stages: Vec<TaskStage>,
    pub(crate) metrics: Vec<TaskMetric>,
    pub(crate) failures: Vec<TaskFailure>,
    pub(crate) error_summary: Option<String>,
    pub(crate) result_summary: Option<String>,
    pub(crate) capabilities: TaskCapabilities,
    pub(crate) revision: u64,
    pub(crate) detail: Value,
    pub(crate) result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) agent_session_ref: Option<crate::backend::domain::agents::AgentSessionRef>,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum SpawnOutcome {
    Started,
    Existing,
}

pub(crate) enum ExternalRegistrationOutcome {
    Started(TaskSnapshot),
    Existing(TaskSnapshot),
    Conflict(TaskSnapshot),
}

pub(crate) enum CancelOutcome {
    Requested(TaskSnapshot),
    AlreadyFinished(TaskSnapshot),
    NotFound,
}

#[derive(Default, Clone)]
pub(crate) struct TaskFilter {
    pub(crate) kind: Option<TaskKind>,
    pub(crate) active_only: bool,
    pub(crate) user_visible_only: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ShutdownReport {
    pub(crate) unfinished_task_ids: Vec<String>,
}
