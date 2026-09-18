use super::{AppError, AppErrorView, AppResult};
use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::broadcast;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tokio_util::task::{task_tracker::TaskTrackerToken, TaskTracker};

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
    TeamRun,
    Other,
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
    pub(crate) agent_session_ref: Option<crate::backend::dto::AgentSessionRef>,
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
    pub(crate) agent_session_ref: Option<crate::backend::dto::AgentSessionRef>,
}

pub(crate) struct TaskContext {
    cancellation: CancellationToken,
    progress: ProgressHandle,
}

impl TaskContext {
    pub(crate) fn untracked() -> Self {
        Self {
            cancellation: CancellationToken::new(),
            progress: ProgressHandle {
                task_id: String::new(),
                runtime: TaskRuntime::new(),
            },
        }
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }
    pub(crate) fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }
    pub(crate) fn progress(&self) -> ProgressHandle {
        self.progress.clone()
    }
    pub(crate) fn task_id(&self) -> &str {
        &self.progress.task_id
    }
}

#[derive(Clone)]
pub(crate) struct ProgressHandle {
    task_id: String,
    runtime: TaskRuntime,
}

impl ProgressHandle {
    pub(crate) fn task_id(&self) -> &str {
        &self.task_id
    }

    pub(crate) fn progress(&self, current: u64, total: Option<u64>, note: Option<&str>) {
        let snapshot = if let Ok(mut tasks) = self.runtime.tasks.lock() {
            if let Some(entry) = tasks.get_mut(&self.task_id) {
                entry.snapshot.progress = Some(TaskProgress {
                    current,
                    total,
                    note: note.map(str::to_string),
                });
                entry.snapshot.revision += 1;
                entry.snapshot.updated_at = Utc::now().to_rfc3339();
                Some(entry.snapshot.clone())
            } else {
                None
            }
        } else {
            None
        };
        if let Some(snapshot) = snapshot {
            self.runtime.publish(&snapshot);
        }
    }

    pub(crate) fn set_stages(&self, stages: Vec<TaskStage>) {
        let _ = self.runtime.set_stages(&self.task_id, stages);
    }

    pub(crate) fn update_stage_status(&self, stage_id: &str, status: StageStatus) {
        let _ = self
            .runtime
            .update_stage_status(&self.task_id, stage_id, status);
    }

    pub(crate) fn record_activity(&self, activity: TaskActivity) {
        let _ = self.runtime.record_activity(&self.task_id, activity);
    }

    pub(crate) fn remove_activity(&self, stage_id: &str, worker_id: &str) {
        let _ = self
            .runtime
            .remove_activity(&self.task_id, stage_id, worker_id);
    }

    pub(crate) fn finish_stage(
        &self,
        stage_id: &str,
        status: StageStatus,
        metrics: Vec<TaskMetric>,
        failures: Vec<TaskFailure>,
        skipped: Vec<TaskSkippedGroup>,
    ) {
        let _ =
            self.runtime
                .finish_stage(&self.task_id, stage_id, status, metrics, failures, skipped);
    }

    pub(crate) fn set_outcome(
        &self,
        outcome: TaskOutcome,
        result_summary: Option<String>,
        error_summary: Option<String>,
    ) {
        let _ = self
            .runtime
            .set_outcome(&self.task_id, outcome, result_summary, error_summary);
    }

    pub(crate) fn set_stage_agent_session_ref(
        &self,
        stage_id: &str,
        session_ref: Option<crate::backend::dto::AgentSessionRef>,
    ) {
        let _ = self
            .runtime
            .set_stage_agent_session_ref(&self.task_id, stage_id, session_ref);
    }
}

pub(crate) type TaskFn = Box<dyn FnOnce(TaskContext) -> AppResult<Value> + Send + 'static>;

struct TaskEntry {
    snapshot: TaskSnapshot,
    cancellation: CancellationToken,
    conflict_keys: Vec<String>,
    started: bool,
    tracking: Option<TaskTrackerToken>,
}

#[derive(Clone)]
pub(crate) struct TaskRuntime {
    tasks: Arc<Mutex<HashMap<String, TaskEntry>>>,
    sequence: Arc<AtomicU64>,
    accepting: Arc<AtomicBool>,
    runtime_handle: Option<tokio::runtime::Handle>,
    tracker: TaskTracker,
    events: Arc<broadcast::Sender<TaskSnapshot>>,
}

impl Default for TaskRuntime {
    fn default() -> Self {
        Self::new()
    }
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

impl TaskRuntime {
    pub(crate) fn new() -> Self {
        let (events, _) = broadcast::channel(256);
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            sequence: Arc::new(AtomicU64::new(0)),
            accepting: Arc::new(AtomicBool::new(true)),
            runtime_handle: None,
            tracker: TaskTracker::new(),
            events: Arc::new(events),
        }
    }

    pub(crate) fn with_runtime_handle(handle: tokio::runtime::Handle) -> Self {
        Self {
            runtime_handle: Some(handle),
            ..Self::new()
        }
    }

    pub(crate) fn runtime_handle(&self) -> Option<tokio::runtime::Handle> {
        self.runtime_handle.clone()
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<TaskSnapshot> {
        self.events.subscribe()
    }

    fn publish(&self, snapshot: &TaskSnapshot) {
        let _ = self.events.send(snapshot.clone());
    }

    fn prepare_spawn(
        &self,
        spec: TaskSpec,
    ) -> Result<Option<(String, CancellationToken, TaskTrackerToken)>, AppError> {
        if !self.accepting.load(Ordering::Acquire) {
            return Err(AppError::Cancelled(
                "应用正在关闭，不再接受新任务".to_string(),
            ));
        }
        let task_id = spec.task_id.unwrap_or_else(|| {
            format!("task-{}", self.sequence.fetch_add(1, Ordering::Relaxed) + 1)
        });
        let started_at = Utc::now().to_rfc3339();
        let cancellation = CancellationToken::new();
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
        if !self.accepting.load(Ordering::Acquire) {
            return Err(AppError::Cancelled(
                "应用正在关闭，不再接受新任务".to_string(),
            ));
        }
        Self::prune_terminal_tasks_locked(&mut tasks);
        if let Some(existing) = tasks.get(&task_id) {
            if existing.snapshot.state.is_active() {
                return Ok(None);
            }
            tasks.remove(&task_id);
        }
        if spec.dedup_key.as_ref().is_some_and(|key| {
            tasks
                .values()
                .find(|entry| {
                    entry.snapshot.kind == spec.kind
                        && entry.snapshot.tenant_id == spec.tenant_id
                        && entry.snapshot.dedup_key.as_ref() == Some(key)
                        && entry.snapshot.state.is_active()
                })
                .is_some()
        }) {
            return Ok(None);
        }
        let tracking = self.tracker.token();
        let user_visible = spec
            .user_visible
            .unwrap_or_else(|| !matches!(spec.kind, TaskKind::Other));
        let capabilities = spec.capabilities.unwrap_or_default();
        let snapshot = TaskSnapshot {
            task_id: task_id.clone(),
            kind: spec.kind,
            tenant_id: spec.tenant_id,
            title: spec.title,
            user_visible,
            dedup_key: spec.dedup_key,
            state: TaskState::Running,
            outcome: None,
            progress: None,
            error: None,
            started_at: started_at.clone(),
            updated_at: started_at,
            finished_at: None,
            stages: Vec::new(),
            metrics: Vec::new(),
            failures: Vec::new(),
            error_summary: None,
            result_summary: None,
            capabilities,
            revision: 1,
            detail: sanitize_task_detail(spec.detail),
            result: None,
            agent_session_ref: None,
        };
        tasks.insert(
            task_id.clone(),
            TaskEntry {
                snapshot: snapshot.clone(),
                cancellation: cancellation.clone(),
                conflict_keys: spec.conflict_keys,
                started: true,
                tracking: None,
            },
        );
        drop(tasks);
        self.publish(&snapshot);

        Ok(Some((task_id, cancellation, tracking)))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn spawn(&self, spec: TaskSpec, task: TaskFn) -> Result<SpawnOutcome, AppError> {
        let Some((task_id, cancellation, tracking)) = self.prepare_spawn(spec)? else {
            return Ok(SpawnOutcome::Existing);
        };
        self.launch_task(task_id, cancellation, tracking, task)?;
        Ok(SpawnOutcome::Started)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn spawn_async<F, Fut>(
        &self,
        spec: TaskSpec,
        task: F,
    ) -> Result<SpawnOutcome, AppError>
    where
        F: FnOnce(TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = AppResult<Value>> + Send + 'static,
    {
        let Some((task_id, cancellation, tracking)) = self.prepare_spawn(spec)? else {
            return Ok(SpawnOutcome::Existing);
        };
        self.launch_task_async(task_id, cancellation, tracking, task)?;
        Ok(SpawnOutcome::Started)
    }

    /// Register an externally-driven task without moving its domain work into
    /// the kernel.  This keeps task lifecycle, deduplication, cancellation and
    /// shutdown accounting in one authority while allowing adapters to retain
    /// domain-specific progress and result projections.
    pub(crate) fn register_external(
        &self,
        spec: TaskSpec,
    ) -> Result<ExternalRegistrationOutcome, AppError> {
        if !self.accepting.load(Ordering::Acquire) {
            return Err(AppError::Cancelled(
                "应用正在关闭，不再接受新任务".to_string(),
            ));
        }
        let task_id = spec.task_id.unwrap_or_else(|| {
            format!("task-{}", self.sequence.fetch_add(1, Ordering::Relaxed) + 1)
        });
        let started_at = Utc::now().to_rfc3339();
        let cancellation = CancellationToken::new();
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
        if !self.accepting.load(Ordering::Acquire) {
            return Err(AppError::Cancelled(
                "应用正在关闭，不再接受新任务".to_string(),
            ));
        }
        Self::prune_terminal_tasks_locked(&mut tasks);
        if let Some(existing) = tasks.get(&task_id) {
            return Ok(ExternalRegistrationOutcome::Existing(
                existing.snapshot.clone(),
            ));
        }
        if let Some(existing) = spec.dedup_key.as_ref().and_then(|key| {
            tasks.values().find(|entry| {
                entry.snapshot.kind == spec.kind
                    && entry.snapshot.tenant_id == spec.tenant_id
                    && entry.snapshot.dedup_key.as_ref() == Some(key)
                    && entry.snapshot.state.is_active()
            })
        }) {
            return Ok(ExternalRegistrationOutcome::Existing(
                existing.snapshot.clone(),
            ));
        }
        if let Some(existing) = tasks.values().find(|entry| {
            entry.snapshot.state.is_active()
                && entry.snapshot.tenant_id == spec.tenant_id
                && spec
                    .conflict_keys
                    .iter()
                    .any(|key| entry.conflict_keys.iter().any(|existing| existing == key))
        }) {
            return Ok(ExternalRegistrationOutcome::Conflict(
                existing.snapshot.clone(),
            ));
        }
        let tracking = self.tracker.token();
        let user_visible = spec
            .user_visible
            .unwrap_or_else(|| !matches!(spec.kind, TaskKind::Other));
        let capabilities = spec.capabilities.unwrap_or_default();
        let snapshot = TaskSnapshot {
            task_id: task_id.clone(),
            kind: spec.kind,
            tenant_id: spec.tenant_id,
            title: spec.title,
            user_visible,
            state: TaskState::Pending,
            outcome: None,
            dedup_key: spec.dedup_key,
            progress: None,
            error: None,
            started_at: started_at.clone(),
            updated_at: started_at,
            finished_at: None,
            stages: Vec::new(),
            metrics: Vec::new(),
            failures: Vec::new(),
            error_summary: None,
            result_summary: None,
            capabilities,
            revision: 1,
            detail: sanitize_task_detail(spec.detail),
            result: None,
            agent_session_ref: None,
        };
        tasks.insert(
            task_id,
            TaskEntry {
                snapshot: snapshot.clone(),
                cancellation: cancellation.clone(),
                conflict_keys: spec.conflict_keys,
                started: false,
                tracking: Some(tracking),
            },
        );
        drop(tasks);
        self.publish(&snapshot);
        Ok(ExternalRegistrationOutcome::Started(snapshot))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn start_external(&self, task_id: &str) -> AppResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
            Self::prune_terminal_tasks_locked(&mut tasks);
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
            if entry.snapshot.state == TaskState::Pending {
                entry.snapshot.state = TaskState::Running;
                entry.started = true;
            }
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    /// Mark a reserved external task as running without claiming its worker
    /// slot. The adapter can then attach the real closure through
    /// `start_external_with` while observers see the canonical running state.
    pub(crate) fn activate_external(
        &self,
        task_id: &str,
        detail: Value,
    ) -> AppResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
            Self::prune_terminal_tasks_locked(&mut tasks);
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
            if entry.snapshot.state == TaskState::Pending {
                entry.snapshot.state = TaskState::Running;
            }
            entry.snapshot.detail = detail;
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    pub(crate) fn cancellation_token(&self, task_id: &str) -> AppResult<CancellationToken> {
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
        Self::prune_terminal_tasks_locked(&mut tasks);
        tasks
            .get(task_id)
            .map(|entry| entry.cancellation.clone())
            .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))
    }

    pub(crate) fn task_context(&self, task_id: &str) -> AppResult<TaskContext> {
        Ok(TaskContext {
            cancellation: self.cancellation_token(task_id)?,
            progress: ProgressHandle {
                task_id: task_id.to_string(),
                runtime: self.clone(),
            },
        })
    }

    pub(crate) fn set_progress(
        &self,
        task_id: &str,
        current: u64,
        total: Option<u64>,
        note: Option<&str>,
    ) -> AppResult<()> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
            Self::prune_terminal_tasks_locked(&mut tasks);
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
            if entry.snapshot.state.is_active() {
                if total.is_some_and(|total| current > total) {
                    return Err(AppError::Validation("任务进度不得超过总数".to_string()));
                }
                if entry
                    .snapshot
                    .progress
                    .as_ref()
                    .is_some_and(|progress| current < progress.current)
                {
                    return Err(AppError::Validation("任务进度不得回退".to_string()));
                }
                entry.snapshot.progress = Some(TaskProgress {
                    current,
                    total,
                    note: note.map(str::to_string),
                });
                entry.snapshot.revision += 1;
                entry.snapshot.updated_at = Utc::now().to_rfc3339();
                Some(entry.snapshot.clone())
            } else {
                None
            }
        };
        if let Some(snapshot) = snapshot {
            self.publish(&snapshot);
        }
        Ok(())
    }

    /// Replace the adapter projection stored by the canonical task runtime.
    /// Tauri and Engine adapters must derive their public snapshots from this
    /// value rather than keeping a second mutable task registry.
    pub(crate) fn update_detail(&self, task_id: &str, detail: Value) -> AppResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
            Self::prune_terminal_tasks_locked(&mut tasks);
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
            entry.snapshot.detail = sanitize_task_detail(detail);
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    /// Start a task that was registered before its adapter had assembled the
    /// actual closure. This is used by lifecycle coordinators that need a
    /// pending task id for deduplication and cancellation before spawning.
    pub(crate) fn start_external_with(
        &self,
        task_id: &str,
        detail: Value,
        task: TaskFn,
    ) -> AppResult<TaskSnapshot> {
        let (snapshot, cancellation, tracking, should_launch) = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
            Self::prune_terminal_tasks_locked(&mut tasks);
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
            if entry.snapshot.state == TaskState::Pending {
                entry.snapshot.state = TaskState::Running;
            }
            entry.snapshot.detail = sanitize_task_detail(detail);
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
            let should_launch = entry.snapshot.state == TaskState::Running && !entry.started;
            let tracking = if should_launch {
                entry.started = true;
                entry
                    .tracking
                    .take()
                    .unwrap_or_else(|| self.tracker.token())
            } else {
                self.tracker.token()
            };
            (
                entry.snapshot.clone(),
                entry.cancellation.clone(),
                tracking,
                should_launch,
            )
        };
        self.publish(&snapshot);
        if should_launch {
            self.launch_task(task_id.to_string(), cancellation, tracking, task)?;
        } else if snapshot.state == TaskState::Cancelling {
            return self.complete_external(
                task_id,
                Err(AppError::Cancelled("后台任务在启动前已取消".to_string())),
            );
        }
        Ok(snapshot)
    }

    pub(crate) fn start_external_with_async<F, Fut>(
        &self,
        task_id: &str,
        detail: Value,
        task: F,
    ) -> AppResult<TaskSnapshot>
    where
        F: FnOnce(TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = AppResult<Value>> + Send + 'static,
    {
        let (snapshot, cancellation, tracking, should_launch) = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
            Self::prune_terminal_tasks_locked(&mut tasks);
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
            if entry.snapshot.state == TaskState::Pending {
                entry.snapshot.state = TaskState::Running;
            }
            entry.snapshot.detail = sanitize_task_detail(detail);
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
            let should_launch = entry.snapshot.state == TaskState::Running && !entry.started;
            let tracking = if should_launch {
                entry.started = true;
                entry
                    .tracking
                    .take()
                    .unwrap_or_else(|| self.tracker.token())
            } else {
                self.tracker.token()
            };
            (
                entry.snapshot.clone(),
                entry.cancellation.clone(),
                tracking,
                should_launch,
            )
        };
        self.publish(&snapshot);
        if should_launch {
            self.launch_task_async(task_id.to_string(), cancellation, tracking, task)?;
        } else if snapshot.state == TaskState::Cancelling {
            return self.complete_external(
                task_id,
                Err(AppError::Cancelled("后台任务在启动前已取消".to_string())),
            );
        }
        Ok(snapshot)
    }

    pub(crate) fn complete_external(
        &self,
        task_id: &str,
        result: AppResult<Value>,
    ) -> AppResult<TaskSnapshot> {
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
        Self::prune_terminal_tasks_locked(&mut tasks);
        let entry = tasks
            .get_mut(task_id)
            .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
        if entry.snapshot.state.is_terminal() {
            return Ok(entry.snapshot.clone());
        }
        let now = Utc::now().to_rfc3339();
        entry.snapshot.finished_at = Some(now.clone());
        entry.snapshot.updated_at = now;
        entry.snapshot.revision += 1;
        let _tracking = entry.tracking.take();
        match result {
            Ok(detail) => {
                if entry.cancellation.is_cancelled() {
                    entry.snapshot.state = TaskState::Canceled;
                    entry.snapshot.outcome = Some(TaskOutcome::Canceled);
                    entry.snapshot.error =
                        Some(AppError::Cancelled("后台任务已取消".to_string()).view());
                } else {
                    entry.snapshot.state = TaskState::Succeeded;
                    if entry.snapshot.outcome.is_none() {
                        entry.snapshot.outcome = Some(TaskOutcome::Success);
                    }
                    entry.snapshot.result = Some(detail);
                }
            }
            Err(_error) if entry.cancellation.is_cancelled() => {
                entry.snapshot.state = TaskState::Canceled;
                entry.snapshot.outcome = Some(TaskOutcome::Canceled);
                entry.snapshot.error =
                    Some(AppError::Cancelled("后台任务已取消".to_string()).view());
            }
            Err(error) if matches!(error, AppError::Cancelled(_)) => {
                entry.snapshot.state = TaskState::Canceled;
                entry.snapshot.outcome = Some(TaskOutcome::Canceled);
                entry.snapshot.error = Some(error.view());
            }
            Err(error) => {
                entry.snapshot.state = TaskState::Failed;
                if entry.snapshot.outcome.is_none() {
                    entry.snapshot.outcome = Some(TaskOutcome::Failure);
                }
                entry.snapshot.error = Some(error.view());
            }
        }
        let snapshot = entry.snapshot.clone();
        drop(tasks);
        self.publish(&snapshot);
        Ok(snapshot)
    }

    pub(crate) fn remove_terminal(&self, task_id: &str) -> Option<TaskSnapshot> {
        let mut tasks = self.tasks.lock().ok()?;
        let should_remove = tasks
            .get(task_id)
            .is_some_and(|entry| entry.snapshot.state.is_terminal());
        if !should_remove {
            return None;
        }
        tasks.remove(task_id).map(|mut entry| {
            let _ = entry.tracking.take();
            entry.snapshot
        })
    }

    #[cfg(test)]
    pub(crate) fn remove(&self, task_id: &str) -> Option<TaskSnapshot> {
        let mut removed = self.tasks.lock().ok()?.remove(task_id);
        let _ = removed.as_mut().and_then(|entry| entry.tracking.take());
        removed.map(|entry| entry.snapshot)
    }

    #[cfg(test)]
    pub(crate) fn set_finished_at_for_test(
        &self,
        task_id: &str,
        finished_at: String,
    ) -> AppResult<()> {
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
        let entry = tasks
            .get_mut(task_id)
            .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
        entry.snapshot.finished_at = Some(finished_at);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn set_user_visible_for_test(
        &self,
        task_id: &str,
        user_visible: bool,
    ) -> AppResult<()> {
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
        let entry = tasks
            .get_mut(task_id)
            .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
        entry.snapshot.user_visible = user_visible;
        Ok(())
    }

    pub(crate) fn has_active_tasks(&self) -> bool {
        self.tasks
            .lock()
            .map(|tasks| tasks.values().any(|entry| entry.snapshot.state.is_active()))
            .unwrap_or(true)
    }

    fn finish_task(
        &self,
        task_id: &str,
        cancellation: &CancellationToken,
        result: AppResult<Value>,
    ) {
        let mut terminal_snapshot = None;
        if let Ok(mut tasks) = self.tasks.lock() {
            if let Some(entry) = tasks.get_mut(task_id) {
                if matches!(
                    entry.snapshot.state,
                    TaskState::Pending | TaskState::Running | TaskState::Cancelling
                ) {
                    let now = Utc::now().to_rfc3339();
                    entry.snapshot.finished_at = Some(now.clone());
                    entry.snapshot.updated_at = now;
                    entry.snapshot.revision += 1;
                    match result {
                        Ok(_detail) if cancellation.is_cancelled() => {
                            entry.snapshot.state = TaskState::Canceled;
                            entry.snapshot.outcome = Some(TaskOutcome::Canceled);
                            entry.snapshot.error =
                                Some(AppError::Cancelled("后台任务已取消".to_string()).view());
                        }
                        Ok(detail) => {
                            entry.snapshot.state = TaskState::Succeeded;
                            if entry.snapshot.outcome.is_none() {
                                entry.snapshot.outcome = Some(TaskOutcome::Success);
                            }
                            entry.snapshot.result = Some(detail);
                        }
                        Err(error)
                            if cancellation.is_cancelled()
                                || matches!(error, AppError::Cancelled(_)) =>
                        {
                            entry.snapshot.state = TaskState::Canceled;
                            entry.snapshot.outcome = Some(TaskOutcome::Canceled);
                            entry.snapshot.error = Some(
                                if matches!(error, AppError::Cancelled(_)) {
                                    error
                                } else {
                                    AppError::Cancelled("后台任务已取消".to_string())
                                }
                                .view(),
                            );
                        }
                        Err(error) => {
                            entry.snapshot.state = TaskState::Failed;
                            if entry.snapshot.outcome.is_none() {
                                entry.snapshot.outcome = Some(TaskOutcome::Failure);
                            }
                            entry.snapshot.error = Some(error.view());
                        }
                    }
                    terminal_snapshot = Some(entry.snapshot.clone());
                }
            }
        }
        if let Some(snapshot) = terminal_snapshot {
            self.publish(&snapshot);
        }
    }

    fn launch_task(
        &self,
        task_id: String,
        cancellation: CancellationToken,
        tracking: TaskTrackerToken,
        task: TaskFn,
    ) -> AppResult<()> {
        let runtime = self.clone();
        let run_task_id = task_id.clone();
        let thread_name = format!("aiw-task-{task_id}");
        let run = move || {
            let _tracking = tracking;
            let cancellation = cancellation;
            let context = TaskContext {
                cancellation: cancellation.clone(),
                progress: ProgressHandle {
                    task_id: run_task_id.clone(),
                    runtime: runtime.clone(),
                },
            };
            let span = tracing::info_span!("task_execution", task_id = %run_task_id);
            let _span_guard = span.enter();
            let result = catch_unwind(AssertUnwindSafe(|| task(context)))
                .unwrap_or_else(|_| Err(AppError::External("后台任务发生 panic".to_string())));
            runtime.finish_task(&run_task_id, &cancellation, result);
        };
        if let Some(handle) = self.runtime_handle.clone() {
            handle.spawn_blocking(run);
        } else if let Err(error) = std::thread::Builder::new().name(thread_name).spawn(run) {
            let failed_snapshot = if let Ok(mut tasks) = self.tasks.lock() {
                if let Some(entry) = tasks.get_mut(&task_id) {
                    entry.snapshot.state = TaskState::Failed;
                    entry.snapshot.finished_at = Some(Utc::now().to_rfc3339());
                    entry.snapshot.error =
                        Some(AppError::External(format!("启动后台任务失败: {error}")).view());
                    Some(entry.snapshot.clone())
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(snapshot) = failed_snapshot {
                self.publish(&snapshot);
            }
            return Err(AppError::External(format!("启动后台任务失败: {error}")));
        }
        Ok(())
    }

    fn launch_task_async<F, Fut>(
        &self,
        task_id: String,
        cancellation: CancellationToken,
        tracking: TaskTrackerToken,
        task: F,
    ) -> AppResult<()>
    where
        F: FnOnce(TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = AppResult<Value>> + Send + 'static,
    {
        let handle = self
            .runtime_handle
            .clone()
            .or_else(|| tokio::runtime::Handle::try_current().ok())
            .ok_or_else(|| {
                AppError::external(
                    "TaskRuntime requires runtime_handle for async tasks".to_string(),
                )
            })?;
        let runtime = self.clone();
        let run_task_id = task_id.clone();
        handle.spawn(async move {
            let _tracking = tracking;
            let cancellation_for_finish = cancellation.clone();
            let context = TaskContext {
                cancellation: cancellation.clone(),
                progress: ProgressHandle {
                    task_id: run_task_id.clone(),
                    runtime: runtime.clone(),
                },
            };
            let span = tracing::info_span!("task_execution", task_id = %run_task_id);
            use tracing::Instrument;
            let result = match tokio::spawn(task(context).instrument(span)).await {
                Ok(task_res) => task_res,
                Err(join_err) => {
                    if join_err.is_cancelled() {
                        Err(AppError::Cancelled("后台任务已取消".to_string()))
                    } else {
                        Err(AppError::External("后台任务发生 panic".to_string()))
                    }
                }
            };
            runtime.finish_task(&run_task_id, &cancellation_for_finish, result);
        });
        Ok(())
    }

    pub(crate) fn get(&self, task_id: &str) -> Option<TaskSnapshot> {
        let mut tasks = self.tasks.lock().ok()?;
        Self::prune_terminal_tasks_locked(&mut tasks);
        tasks.get(task_id).map(|e| e.snapshot.clone())
    }

    pub(crate) fn get_for_tenant(&self, tenant_id: &str, task_id: &str) -> Option<TaskSnapshot> {
        self.get(task_id).filter(|snapshot| {
            snapshot.tenant_id.is_none() || snapshot.tenant_id.as_deref() == Some(tenant_id)
        })
    }

    pub(crate) fn list(&self, filter: TaskFilter) -> Vec<TaskSnapshot> {
        let Ok(mut tasks) = self.tasks.lock() else {
            return Vec::new();
        };
        Self::prune_terminal_tasks_locked(&mut tasks);
        let mut snapshots = tasks
            .values()
            .filter(|entry| filter.kind.is_none_or(|kind| kind == entry.snapshot.kind))
            .filter(|entry| !filter.user_visible_only || entry.snapshot.user_visible)
            .filter(|entry| {
                !filter.active_only
                    || matches!(
                        entry.snapshot.state,
                        TaskState::Pending | TaskState::Running | TaskState::Cancelling
                    )
            })
            .map(|entry| entry.snapshot.clone())
            .collect::<Vec<_>>();
        snapshots.sort_by(|left, right| {
            left.started_at
                .cmp(&right.started_at)
                .then_with(|| left.task_id.cmp(&right.task_id))
        });
        snapshots
    }

    pub(crate) fn list_for_tenant(&self, tenant_id: &str, filter: TaskFilter) -> Vec<TaskSnapshot> {
        self.list(filter)
            .into_iter()
            .filter(|snapshot| {
                snapshot.tenant_id.is_none() || snapshot.tenant_id.as_deref() == Some(tenant_id)
            })
            .collect()
    }

    pub(crate) fn cancel(&self, task_id: &str) -> CancelOutcome {
        let Ok(mut tasks) = self.tasks.lock() else {
            return CancelOutcome::NotFound;
        };
        Self::prune_terminal_tasks_locked(&mut tasks);
        let Some(entry) = tasks.get_mut(task_id) else {
            return CancelOutcome::NotFound;
        };
        if entry.snapshot.state.is_active() {
            entry.cancellation.cancel();
            entry.snapshot.state = TaskState::Cancelling;
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
            let snapshot = entry.snapshot.clone();
            drop(tasks);
            self.publish(&snapshot);
            return CancelOutcome::Requested(snapshot);
        }
        CancelOutcome::AlreadyFinished(entry.snapshot.clone())
    }

    pub(crate) fn cancel_for_tenant(&self, tenant_id: &str, task_id: &str) -> CancelOutcome {
        if self.get_for_tenant(tenant_id, task_id).is_none() {
            return CancelOutcome::NotFound;
        }
        self.cancel(task_id)
    }

    pub(crate) fn stop_accepting(&self) {
        self.accepting.store(false, Ordering::Release);
        self.tracker.close();
    }

    pub(crate) async fn shutdown_until(&self, deadline: Instant) -> ShutdownReport {
        self.stop_accepting();
        {
            let tasks = self
                .tasks
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            for entry in tasks
                .values()
                .filter(|entry| entry.snapshot.state.is_active())
            {
                entry.cancellation.cancel();
            }
        }
        self.tracker.close();
        let remaining = deadline.saturating_duration_since(Instant::now());
        let _ = tokio::time::timeout(remaining, self.tracker.wait()).await;

        let unfinished_task_ids = self
            .tasks
            .lock()
            .map(|tasks| {
                tasks
                    .values()
                    .filter(|entry| entry.snapshot.state.is_active())
                    .map(|entry| entry.snapshot.task_id.clone())
                    .collect()
            })
            .unwrap_or_default();
        ShutdownReport {
            unfinished_task_ids,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn shutdown_with_grace(&self, grace: Duration) -> ShutdownReport {
        self.shutdown_until(Instant::now() + grace).await
    }

    pub(crate) fn set_stages(
        &self,
        task_id: &str,
        stages: Vec<TaskStage>,
    ) -> AppResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
            entry.snapshot.stages = stages;
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    pub(crate) fn set_stage_agent_session_ref(
        &self,
        task_id: &str,
        stage_id: &str,
        session_ref: Option<crate::backend::dto::AgentSessionRef>,
    ) -> AppResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
            if let Some(stage) = entry.snapshot.stages.iter_mut().find(|s| s.id == stage_id) {
                stage.agent_session_ref = session_ref.clone();
            }
            if entry.snapshot.agent_session_ref.is_none() {
                entry.snapshot.agent_session_ref = session_ref;
            }
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    pub(crate) fn update_stage_status(
        &self,
        task_id: &str,
        stage_id: &str,
        status: StageStatus,
    ) -> AppResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
            let now = Utc::now().to_rfc3339();
            if let Some(stage) = entry.snapshot.stages.iter_mut().find(|s| s.id == stage_id) {
                stage.status = status;
                if status == StageStatus::Running && stage.started_at.is_none() {
                    stage.started_at = Some(now.clone());
                } else if status.is_terminal() && stage.finished_at.is_none() {
                    stage.finished_at = Some(now.clone());
                    if let Some(started_at) = &stage.started_at {
                        if let (Ok(s), Ok(f)) = (
                            chrono::DateTime::parse_from_rfc3339(started_at),
                            chrono::DateTime::parse_from_rfc3339(&now),
                        ) {
                            if let Ok(duration) = (f - s).to_std() {
                                stage.duration_ms = Some(duration.as_millis() as u64);
                            }
                        }
                    }
                }
            }
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = now;
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    pub(crate) fn record_activity(
        &self,
        task_id: &str,
        activity: TaskActivity,
    ) -> AppResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
            let now = Utc::now().to_rfc3339();
            if let Some(stage) = entry
                .snapshot
                .stages
                .iter_mut()
                .find(|s| s.id == activity.stage_id)
            {
                if let Some(existing) = stage
                    .current_activities
                    .iter_mut()
                    .find(|a| a.worker_id == activity.worker_id)
                {
                    *existing = activity;
                } else {
                    stage.current_activities.push(activity);
                }
            }
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = now;
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    pub(crate) fn remove_activity(
        &self,
        task_id: &str,
        stage_id: &str,
        worker_id: &str,
    ) -> AppResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
            let now = Utc::now().to_rfc3339();
            if let Some(stage) = entry.snapshot.stages.iter_mut().find(|s| s.id == stage_id) {
                stage
                    .current_activities
                    .retain(|a| a.worker_id != worker_id);
            }
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = now;
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    pub(crate) fn finish_stage(
        &self,
        task_id: &str,
        stage_id: &str,
        status: StageStatus,
        metrics: Vec<TaskMetric>,
        failures: Vec<TaskFailure>,
        skipped: Vec<TaskSkippedGroup>,
    ) -> AppResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
            let now = Utc::now().to_rfc3339();
            if let Some(stage) = entry.snapshot.stages.iter_mut().find(|s| s.id == stage_id) {
                stage.status = status;
                stage.finished_at = Some(now.clone());
                if stage.started_at.is_none() {
                    stage.started_at = Some(now.clone());
                }
                if let (Some(started_at), Some(finished_at)) =
                    (&stage.started_at, &stage.finished_at)
                {
                    if let (Ok(s), Ok(f)) = (
                        chrono::DateTime::parse_from_rfc3339(started_at),
                        chrono::DateTime::parse_from_rfc3339(finished_at),
                    ) {
                        if let Ok(duration) = (f - s).to_std() {
                            stage.duration_ms = Some(duration.as_millis() as u64);
                        }
                    }
                }
                stage.current_activities.clear();
                stage.metrics.extend(metrics.clone());
                stage.failures.extend(failures.clone());
                stage.skipped.extend(skipped.clone());
            }
            for metric in metrics {
                if let Some(existing) = entry
                    .snapshot
                    .metrics
                    .iter_mut()
                    .find(|m| m.code == metric.code)
                {
                    existing.value += metric.value;
                } else {
                    entry.snapshot.metrics.push(metric);
                }
            }
            entry.snapshot.failures.extend(failures);
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = now;
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    pub(crate) fn set_outcome(
        &self,
        task_id: &str,
        outcome: TaskOutcome,
        result_summary: Option<String>,
        error_summary: Option<String>,
    ) -> AppResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
            entry.snapshot.outcome = Some(outcome);
            if let Some(r) = result_summary {
                entry.snapshot.result_summary = Some(r);
            }
            if let Some(e) = error_summary {
                entry.snapshot.error_summary = Some(e);
            }
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    pub(crate) fn clear_terminal(&self, tenant_id: Option<&str>) -> usize {
        let Ok(mut tasks) = self.tasks.lock() else {
            return 0;
        };
        let mut to_remove = Vec::new();
        for (task_id, entry) in tasks.iter() {
            if entry.snapshot.state.is_terminal() {
                if tenant_id.is_none()
                    || entry.snapshot.tenant_id.as_deref() == tenant_id
                    || entry.snapshot.tenant_id.is_none()
                {
                    to_remove.push(task_id.clone());
                }
            }
        }
        let count = to_remove.len();
        for task_id in to_remove {
            if let Some(mut entry) = tasks.remove(&task_id) {
                let _ = entry.tracking.take();
            }
        }
        count
    }

    fn prune_terminal_tasks_locked(tasks: &mut HashMap<String, TaskEntry>) {
        let now = Utc::now();
        let retention = chrono::Duration::from_std(TASK_TERMINAL_RETENTION)
            .unwrap_or_else(|_| chrono::Duration::zero());
        let mut terminal = tasks
            .values()
            .filter(|entry| entry.snapshot.state.is_terminal())
            .map(|entry| {
                let finished_at = entry
                    .snapshot
                    .finished_at
                    .as_deref()
                    .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                    .map(|value| value.with_timezone(&Utc));
                (
                    entry.snapshot.task_id.clone(),
                    finished_at,
                    entry.snapshot.user_visible,
                )
            })
            .collect::<Vec<_>>();

        let mut remove_ids = terminal
            .iter()
            .filter_map(|(task_id, finished_at, user_visible)| {
                (!user_visible
                    && finished_at.is_some_and(|f| now.signed_duration_since(f) >= retention))
                .then_some(task_id.clone())
            })
            .collect::<Vec<_>>();
        terminal.retain(|(task_id, _, _)| !remove_ids.iter().any(|removed| removed == task_id));

        terminal.sort_by(|(_, left, _), (_, right, _)| left.cmp(right));
        let excess = terminal.len().saturating_sub(TASK_TERMINAL_LIMIT);
        remove_ids.extend(
            terminal
                .into_iter()
                .take(excess)
                .map(|(task_id, _, _)| task_id),
        );
        for task_id in remove_ids {
            tasks.remove(&task_id);
        }
    }
}

fn sanitize_task_detail(mut detail: Value) -> Value {
    if let Some(object) = detail.as_object_mut() {
        object.remove("result");
        object.remove("assets");
    }
    detail
}
