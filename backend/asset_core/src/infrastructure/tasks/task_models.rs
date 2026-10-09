use crate::backend::domain::AppErrorView;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

pub const TASK_TERMINAL_RETENTION: Duration = Duration::from_secs(10 * 60);
pub const TASK_TERMINAL_LIMIT: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum TaskKind {
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
    GitSourcePull,
    ModelPricingSync,
    #[serde(other)]
    Other,
}

/// 任务准入与托管策略 (ADR-0017 / Issue #79)
///
/// 将业务“功能适配”与运行期“准入管理”彻底解耦：
/// - `Managed`：全量托管，分配后台协程，实时广播 UI 事件，关键帧持久化到 task_journal；
/// - `Shadow`：灰度观察，后台执行并记录 P95 耗时与失败指标，但对 UI 隐形 (user_visible = false)，用于数据驱动准入决策；
/// - `Direct`：受控穿透直跑，使用 TaskContext::direct() 零开销内联执行，跳过任务中心调度、锁与持久化。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskAdmission {
    #[default]
    Managed,
    Shadow,
    Direct,
}

impl TaskKind {
    pub fn default_category_string(&self) -> &'static str {
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
            Self::GitSourcePull => "source/git_pull",
            Self::ModelPricingSync => "conversation/pricing_sync",
            Self::Other => "other",
        }
    }

    /// 根据固有行为特征（而非易漂移的耗时阈值）给出的默认准入策略
    pub fn default_admission(&self) -> TaskAdmission {
        match self {
            // 外部进程、网络 IO、递归文件系统扫描、批量软链接建立
            Self::ConversationSync
            | Self::Scan
            | Self::Backup
            | Self::BatchMount
            | Self::GitSourcePull
            | Self::SearchIndexRebuild
            | Self::AiExecution
            | Self::RemoteSkillAcquire
            | Self::ExtensionLifecycle => TaskAdmission::Managed,

            // 观察期或后台维护操作
            Self::ConversationUsageScan
            | Self::ConversationDataMaintenance
            | Self::AgentMarketRefresh
            | Self::Memory
            | Self::ModelPricingSync => TaskAdmission::Shadow,

            // 纯内联计算或其它操作
            Self::ScriptInstall | Self::Other => TaskAdmission::Direct,
        }
    }
}

impl std::fmt::Display for TaskKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum TaskState {
    Pending,
    Running,
    Cancelling,
    Succeeded,
    Failed,
    Canceled,
}

impl TaskState {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Pending | Self::Running | Self::Cancelling)
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Canceled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskOutcome {
    Success,
    PartialSuccess,
    Failure,
    Canceled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskChangeClass {
    #[default]
    Structural,
    Cosmetic,
}

fn default_structural() -> TaskChangeClass {
    TaskChangeClass::Structural
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    #[default]
    Reject,
    Coalesce,
    CancelExisting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    Pending,
    Running,
    Succeeded,
    PartialSuccess,
    Failed,
    Canceled,
    Skipped,
}

impl StageStatus {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Pending | Self::Running)
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::PartialSuccess | Self::Failed | Self::Canceled | Self::Skipped
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskActivity {
    pub stage_id: String,
    pub worker_id: String,
    pub operation: String,
    pub path: Option<String>,
    pub display_path: Option<String>,
    pub started_at: String,
    pub current: Option<u64>,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskFailure {
    pub code: String,
    pub message: String,
    pub stage: String,
    pub identity: Option<String>,
    pub retryable: bool,
    pub path: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSkippedGroup {
    pub reason_code: String,
    pub count: u64,
    pub samples: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMetric {
    pub code: String,
    pub value: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskStageStep {
    pub timestamp: String,
    pub operation: String,
    pub detail: Option<String>,
    pub current: Option<u64>,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStage {
    pub id: String,
    pub name: String,
    pub status: StageStatus,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub duration_ms: Option<u64>,
    pub progress: Option<TaskProgress>,
    pub current_activities: Vec<TaskActivity>,
    pub metrics: Vec<TaskMetric>,
    pub failures: Vec<TaskFailure>,
    pub skipped: Vec<TaskSkippedGroup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_ref: Option<crate::backend::domain::agents::AgentSessionRef>,
    #[serde(default)]
    pub steps: Vec<TaskStageStep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskCapabilities {
    pub cancellable: bool,
    pub retryable: bool,
    pub clearable: bool,
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
pub struct TaskProgress {
    pub current: u64,
    pub total: Option<u64>,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TaskSpec {
    pub kind: TaskKind,
    pub admission: TaskAdmission,
    pub task_id: Option<String>,
    pub tenant_id: Option<String>,
    pub title: Option<String>,
    pub user_visible: Option<bool>,
    pub dedup_key: Option<String>,
    pub conflict_keys: Vec<String>,
    pub conflict_policy: ConflictPolicy,
    pub capabilities: Option<TaskCapabilities>,
    pub detail: Value,
    pub category: Option<super::task_pipeline::TaskCategory>,
    pub pipeline: Option<super::task_pipeline::PipelineDescriptor>,
}

impl TaskSpec {
    pub fn global(kind: TaskKind, dedup_key: Option<String>) -> Self {
        Self {
            kind,
            admission: kind.default_admission(),
            task_id: None,
            tenant_id: None,
            title: None,
            user_visible: None,
            dedup_key,
            conflict_keys: Vec::new(),
            conflict_policy: ConflictPolicy::Reject,
            capabilities: None,
            detail: Value::Null,
            category: None,
            pipeline: None,
        }
    }

    pub fn new(kind: TaskKind, dedup_key: Option<String>) -> Self {
        Self::global(kind, dedup_key)
    }

    pub fn with_admission(mut self, admission: TaskAdmission) -> Self {
        self.admission = admission;
        self
    }

    pub fn with_task_id(mut self, task_id: impl Into<String>) -> Self {
        self.task_id = Some(task_id.into());
        self
    }

    pub fn with_tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_user_visible(mut self, user_visible: bool) -> Self {
        self.user_visible = Some(user_visible);
        self
    }

    pub fn with_capabilities(mut self, capabilities: TaskCapabilities) -> Self {
        self.capabilities = Some(capabilities);
        self
    }

    pub fn with_category(
        mut self,
        category: impl Into<super::task_pipeline::TaskCategory>,
    ) -> Self {
        self.category = Some(category.into());
        self
    }

    pub fn with_pipeline(mut self, pipeline: super::task_pipeline::PipelineDescriptor) -> Self {
        self.pipeline = Some(pipeline);
        self
    }

    pub fn with_conflict_key(mut self, conflict_key: impl Into<String>) -> Self {
        self.conflict_keys.push(conflict_key.into());
        self
    }

    pub fn with_conflict_keys(
        mut self,
        conflict_keys: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.conflict_keys
            .extend(conflict_keys.into_iter().map(Into::into));
        self
    }

    pub fn with_conflict_policy(mut self, conflict_policy: ConflictPolicy) -> Self {
        self.conflict_policy = conflict_policy;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSnapshot {
    pub task_id: String,
    pub kind: TaskKind,
    #[serde(default)]
    pub category: String,
    #[serde(skip)]
    pub tenant_id: Option<String>,
    pub title: Option<String>,
    pub user_visible: bool,
    pub dedup_key: Option<String>,
    pub state: TaskState,
    pub outcome: Option<TaskOutcome>,
    pub progress: Option<TaskProgress>,
    pub error: Option<AppErrorView>,
    pub started_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
    pub stages: Vec<TaskStage>,
    pub metrics: Vec<TaskMetric>,
    pub failures: Vec<TaskFailure>,
    pub error_summary: Option<String>,
    pub result_summary: Option<String>,
    pub capabilities: TaskCapabilities,
    pub revision: u64,
    pub detail: Value,
    pub result: Option<Value>,
    #[serde(default = "default_structural")]
    pub change_class: TaskChangeClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_ref: Option<crate::backend::domain::agents::AgentSessionRef>,
}

impl TaskSnapshot {
    pub fn empty(task_id: &str) -> Self {
        Self {
            task_id: task_id.to_string(),
            kind: TaskKind::Other,
            category: "noop".to_string(),
            tenant_id: None,
            title: None,
            user_visible: false,
            dedup_key: None,
            state: TaskState::Succeeded,
            outcome: Some(TaskOutcome::Success),
            progress: None,
            error: None,
            started_at: String::new(),
            updated_at: String::new(),
            finished_at: None,
            stages: Vec::new(),
            metrics: Vec::new(),
            failures: Vec::new(),
            error_summary: None,
            result_summary: None,
            capabilities: TaskCapabilities::default(),
            revision: 0,
            detail: Value::Null,
            result: None,
            change_class: TaskChangeClass::Structural,
            agent_session_ref: None,
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub enum SpawnOutcome {
    Started,
    Existing,
}

pub enum ExternalRegistrationOutcome {
    Started(TaskSnapshot),
    Existing(TaskSnapshot),
    Conflict(TaskSnapshot),
}

pub enum CancelOutcome {
    Requested(TaskSnapshot),
    AlreadyFinished(TaskSnapshot),
    NotFound,
}

#[derive(Default, Clone)]
pub struct TaskFilter {
    pub kind: Option<TaskKind>,
    pub active_only: bool,
    pub user_visible_only: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ShutdownReport {
    pub unfinished_task_ids: Vec<String>,
}
