use super::{AppError, AppErrorView, AppResult};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) enum TaskKind {
    ConversationSync,
    SearchIndexRebuild,
    ScriptInstall,
    ExtensionLifecycle,
    AiExecution,
    Scan,
    Backup,
    BatchMount,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TaskProgress {
    pub(crate) current: u64,
    pub(crate) total: Option<u64>,
    pub(crate) note: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct TaskSpec {
    pub(crate) kind: TaskKind,
    pub(crate) task_id: Option<String>,
    pub(crate) dedup_key: Option<String>,
    pub(crate) conflict_keys: Vec<String>,
    pub(crate) detail: Value,
}

impl TaskSpec {
    pub(crate) fn new(kind: TaskKind, dedup_key: Option<String>) -> Self {
        Self {
            kind,
            task_id: None,
            dedup_key,
            conflict_keys: Vec::new(),
            detail: Value::Null,
        }
    }

    pub(crate) fn with_task_id(mut self, task_id: impl Into<String>) -> Self {
        self.task_id = Some(task_id.into());
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
    pub(crate) dedup_key: Option<String>,
    pub(crate) state: TaskState,
    pub(crate) progress: Option<TaskProgress>,
    pub(crate) error: Option<AppErrorView>,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) detail: Value,
    pub(crate) result: Option<Value>,
}

pub(crate) struct TaskContext {
    cancellation: CancellationToken,
    progress: ProgressHandle,
}

impl TaskContext {
    pub(crate) fn detached() -> Self {
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
}

#[derive(Clone)]
pub(crate) struct ProgressHandle {
    task_id: String,
    runtime: TaskRuntime,
}

impl ProgressHandle {
    pub(crate) fn progress(&self, current: u64, total: Option<u64>, note: Option<&str>) {
        if let Ok(mut tasks) = self.runtime.tasks.lock() {
            if let Some(entry) = tasks.get_mut(&self.task_id) {
                entry.snapshot.progress = Some(TaskProgress {
                    current,
                    total,
                    note: note.map(str::to_string),
                });
            }
        }
    }
}

pub(crate) type TaskFn = Box<dyn FnOnce(TaskContext) -> AppResult<Value> + Send + 'static>;

struct TaskEntry {
    snapshot: TaskSnapshot,
    cancellation: CancellationToken,
    conflict_keys: Vec<String>,
    started: bool,
}

#[derive(Clone, Default)]
pub(crate) struct TaskRuntime {
    tasks: Arc<Mutex<HashMap<String, TaskEntry>>>,
    sequence: Arc<AtomicU64>,
    accepting: Arc<AtomicBool>,
    runtime_handle: Option<tokio::runtime::Handle>,
    active: Arc<(Mutex<usize>, Condvar)>,
}

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

#[derive(Default)]
pub(crate) struct TaskFilter {
    pub(crate) kind: Option<TaskKind>,
    pub(crate) active_only: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ShutdownReport {
    pub(crate) unfinished_task_ids: Vec<String>,
}

impl TaskRuntime {
    pub(crate) fn new() -> Self {
        Self {
            accepting: Arc::new(AtomicBool::new(true)),
            ..Self::default()
        }
    }

    pub(crate) fn with_runtime_handle(handle: tokio::runtime::Handle) -> Self {
        Self {
            runtime_handle: Some(handle),
            ..Self::new()
        }
    }

    pub(crate) fn spawn(&self, spec: TaskSpec, task: TaskFn) -> Result<SpawnOutcome, AppError> {
        if !self.accepting.load(Ordering::Acquire) {
            return Err(AppError::Canceled(
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
        if tasks.contains_key(&task_id) {
            return Ok(SpawnOutcome::Existing);
        }
        if spec.dedup_key.as_ref().is_some_and(|key| {
            tasks
                .values()
                .find(|entry| {
                    entry.snapshot.kind == spec.kind
                        && entry.snapshot.dedup_key.as_ref() == Some(key)
                        && entry.snapshot.state.is_active()
                })
                .is_some()
        }) {
            return Ok(SpawnOutcome::Existing);
        }
        let snapshot = TaskSnapshot {
            task_id: task_id.clone(),
            kind: spec.kind,
            dedup_key: spec.dedup_key,
            state: TaskState::Running,
            progress: None,
            error: None,
            started_at,
            finished_at: None,
            detail: spec.detail,
            result: None,
        };
        tasks.insert(
            task_id.clone(),
            TaskEntry {
                snapshot: snapshot.clone(),
                cancellation: cancellation.clone(),
                conflict_keys: spec.conflict_keys,
                started: true,
            },
        );
        drop(tasks);
        if let Ok(mut active) = self.active.0.lock() {
            *active += 1;
        }

        self.launch_task(task_id.clone(), cancellation, task)?;
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
            return Err(AppError::Canceled(
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
        if let Some(existing) = tasks.get(&task_id) {
            return Ok(ExternalRegistrationOutcome::Existing(
                existing.snapshot.clone(),
            ));
        }
        if let Some(existing) = spec.dedup_key.as_ref().and_then(|key| {
            tasks.values().find(|entry| {
                entry.snapshot.kind == spec.kind
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
                && spec
                    .conflict_keys
                    .iter()
                    .any(|key| entry.conflict_keys.iter().any(|existing| existing == key))
        }) {
            return Ok(ExternalRegistrationOutcome::Conflict(
                existing.snapshot.clone(),
            ));
        }
        let snapshot = TaskSnapshot {
            task_id: task_id.clone(),
            kind: spec.kind,
            state: TaskState::Pending,
            dedup_key: spec.dedup_key,
            progress: None,
            error: None,
            started_at,
            finished_at: None,
            detail: spec.detail,
            result: None,
        };
        tasks.insert(
            task_id,
            TaskEntry {
                snapshot: snapshot.clone(),
                cancellation: cancellation.clone(),
                conflict_keys: spec.conflict_keys,
                started: false,
            },
        );
        drop(tasks);
        if let Ok(mut active) = self.active.0.lock() {
            *active += 1;
        }
        Ok(ExternalRegistrationOutcome::Started(snapshot))
    }

    pub(crate) fn start_external(&self, task_id: &str) -> AppResult<TaskSnapshot> {
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
        let entry = tasks
            .get_mut(task_id)
            .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
        if entry.snapshot.state == TaskState::Pending {
            entry.snapshot.state = TaskState::Running;
            entry.started = true;
        }
        Ok(entry.snapshot.clone())
    }

    /// Mark a reserved external task as running without claiming its worker
    /// slot. The adapter can then attach the real closure through
    /// `start_external_with` while observers see the canonical running state.
    pub(crate) fn activate_external(
        &self,
        task_id: &str,
        detail: Value,
    ) -> AppResult<TaskSnapshot> {
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
        let entry = tasks
            .get_mut(task_id)
            .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
        if entry.snapshot.state == TaskState::Pending {
            entry.snapshot.state = TaskState::Running;
            entry.snapshot.detail = detail;
        }
        Ok(entry.snapshot.clone())
    }

    pub(crate) fn cancellation_token(&self, task_id: &str) -> AppResult<CancellationToken> {
        self.tasks
            .lock()
            .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?
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
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
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
        }
        Ok(())
    }

    /// Replace the adapter projection stored by the canonical task runtime.
    /// Tauri and Engine adapters must derive their public snapshots from this
    /// value rather than keeping a second mutable task registry.
    pub(crate) fn update_detail(&self, task_id: &str, detail: Value) -> AppResult<TaskSnapshot> {
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
        let entry = tasks
            .get_mut(task_id)
            .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
        entry.snapshot.detail = detail;
        Ok(entry.snapshot.clone())
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
        let (snapshot, cancellation, should_launch) = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| AppError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
            if entry.snapshot.state == TaskState::Pending {
                entry.snapshot.state = TaskState::Running;
                entry.snapshot.detail = detail;
            }
            let should_launch = entry.snapshot.state == TaskState::Running && !entry.started;
            if should_launch {
                entry.started = true;
            }
            (
                entry.snapshot.clone(),
                entry.cancellation.clone(),
                should_launch,
            )
        };
        if should_launch {
            self.launch_task(task_id.to_string(), cancellation, task)?;
        } else if snapshot.state == TaskState::Cancelling {
            return self.complete_external(
                task_id,
                Err(AppError::Canceled("后台任务在启动前已取消".to_string())),
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
        let entry = tasks
            .get_mut(task_id)
            .ok_or_else(|| AppError::NotFound(format!("任务不存在: {task_id}")))?;
        if entry.snapshot.state.is_terminal() {
            return Ok(entry.snapshot.clone());
        }
        entry.snapshot.finished_at = Some(Utc::now().to_rfc3339());
        match result {
            Ok(detail) => {
                if entry.cancellation.is_cancelled() {
                    entry.snapshot.state = TaskState::Canceled;
                    entry.snapshot.error =
                        Some(AppError::Canceled("后台任务已取消".to_string()).into());
                } else {
                    entry.snapshot.state = TaskState::Succeeded;
                    entry.snapshot.result = Some(detail);
                }
            }
            Err(_error) if entry.cancellation.is_cancelled() => {
                entry.snapshot.state = TaskState::Canceled;
                entry.snapshot.error =
                    Some(AppError::Canceled("后台任务已取消".to_string()).into());
            }
            Err(error) if matches!(error, AppError::Canceled(_)) => {
                entry.snapshot.state = TaskState::Canceled;
                entry.snapshot.error = Some(error.into());
            }
            Err(error) => {
                entry.snapshot.state = TaskState::Failed;
                entry.snapshot.error = Some(error.into());
            }
        }
        let snapshot = entry.snapshot.clone();
        drop(tasks);
        self.release_active_slot();
        Ok(snapshot)
    }

    pub(crate) fn remove(&self, task_id: &str) -> Option<TaskSnapshot> {
        let removed = self.tasks.lock().ok()?.remove(task_id);
        if removed
            .as_ref()
            .is_some_and(|entry| entry.snapshot.state.is_active())
        {
            self.release_active_slot();
        }
        removed.map(|entry| entry.snapshot)
    }

    /// Prune terminal tasks owned by the runtime. Callers may choose a
    /// retention window and a maximum number of terminal snapshots per kind,
    /// but deletion always goes through this runtime so active-task accounting
    /// and lifecycle state remain consistent.
    pub(crate) fn prune(&self, kind: TaskKind, retention: Duration, terminal_limit: usize) {
        let now = Utc::now();
        let retention =
            chrono::Duration::from_std(retention).unwrap_or_else(|_| chrono::Duration::zero());
        let snapshots = self.list(TaskFilter {
            kind: Some(kind),
            active_only: false,
        });
        let mut terminal = snapshots
            .into_iter()
            .filter(|snapshot| snapshot.state.is_terminal())
            .map(|snapshot| {
                let finished_at = snapshot
                    .finished_at
                    .as_deref()
                    .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                    .map(|value| value.with_timezone(&Utc));
                (snapshot.task_id, finished_at)
            })
            .collect::<Vec<_>>();

        let mut remove_ids = terminal
            .iter()
            .filter_map(|(task_id, finished_at)| {
                finished_at
                    .is_some_and(|finished_at| now.signed_duration_since(finished_at) >= retention)
                    .then_some(task_id.clone())
            })
            .collect::<Vec<_>>();
        terminal.retain(|(task_id, _)| !remove_ids.iter().any(|removed| removed == task_id));

        terminal.sort_by(|(_, left), (_, right)| left.cmp(right));
        let excess = terminal.len().saturating_sub(terminal_limit);
        remove_ids.extend(
            terminal
                .into_iter()
                .take(excess)
                .map(|(task_id, _)| task_id),
        );

        for task_id in remove_ids {
            self.remove(&task_id);
        }
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

    pub(crate) fn has_active_tasks(&self) -> bool {
        self.tasks
            .lock()
            .map(|tasks| tasks.values().any(|entry| entry.snapshot.state.is_active()))
            .unwrap_or(true)
    }

    fn release_active_slot(&self) {
        if let Ok(mut active) = self.active.0.lock() {
            *active = active.saturating_sub(1);
            self.active.1.notify_all();
        }
    }

    fn launch_task(
        &self,
        task_id: String,
        cancellation: CancellationToken,
        task: TaskFn,
    ) -> AppResult<()> {
        let runtime = self.clone();
        let run_task_id = task_id.clone();
        let thread_name = format!("aiw-task-{task_id}");
        let run = move || {
            let cancellation = cancellation;
            let context = TaskContext {
                cancellation: cancellation.clone(),
                progress: ProgressHandle {
                    task_id: run_task_id.clone(),
                    runtime: runtime.clone(),
                },
            };
            let result = catch_unwind(AssertUnwindSafe(|| task(context)))
                .unwrap_or_else(|_| Err(AppError::External("后台任务发生 panic".to_string())));
            let mut release_slot = false;
            if let Ok(mut tasks) = runtime.tasks.lock() {
                if let Some(entry) = tasks.get_mut(&run_task_id) {
                    if matches!(
                        entry.snapshot.state,
                        TaskState::Pending | TaskState::Running | TaskState::Cancelling
                    ) {
                        entry.snapshot.finished_at = Some(Utc::now().to_rfc3339());
                        match result {
                            Ok(_detail) if cancellation.is_cancelled() => {
                                entry.snapshot.state = TaskState::Canceled;
                                entry.snapshot.error =
                                    Some(AppError::Canceled("后台任务已取消".to_string()).into());
                            }
                            Ok(detail) => {
                                entry.snapshot.state = TaskState::Succeeded;
                                entry.snapshot.result = Some(detail);
                            }
                            Err(error)
                                if cancellation.is_cancelled()
                                    || matches!(error, AppError::Canceled(_)) =>
                            {
                                entry.snapshot.state = TaskState::Canceled;
                                entry.snapshot.error = Some(
                                    if matches!(error, AppError::Canceled(_)) {
                                        error
                                    } else {
                                        AppError::Canceled("后台任务已取消".to_string())
                                    }
                                    .into(),
                                );
                            }
                            Err(error) => {
                                entry.snapshot.state = TaskState::Failed;
                                entry.snapshot.error = Some(error.into());
                            }
                        }
                        release_slot = true;
                    }
                }
            }
            if release_slot {
                runtime.release_active_slot();
            }
        };
        if let Some(handle) = self.runtime_handle.clone() {
            handle.spawn_blocking(run);
        } else if let Err(error) = std::thread::Builder::new().name(thread_name).spawn(run) {
            if let Ok(mut tasks) = self.tasks.lock() {
                if let Some(entry) = tasks.get_mut(&task_id) {
                    entry.snapshot.state = TaskState::Failed;
                    entry.snapshot.finished_at = Some(Utc::now().to_rfc3339());
                    entry.snapshot.error =
                        Some(AppError::External(format!("启动后台任务失败: {error}")).into());
                }
            }
            self.release_active_slot();
            return Err(AppError::External(format!("启动后台任务失败: {error}")));
        }
        Ok(())
    }

    pub(crate) fn get(&self, task_id: &str) -> Option<TaskSnapshot> {
        self.tasks
            .lock()
            .ok()?
            .get(task_id)
            .map(|e| e.snapshot.clone())
    }

    pub(crate) fn list(&self, filter: TaskFilter) -> Vec<TaskSnapshot> {
        let Ok(tasks) = self.tasks.lock() else {
            return Vec::new();
        };
        let mut snapshots = tasks
            .values()
            .filter(|entry| filter.kind.is_none_or(|kind| kind == entry.snapshot.kind))
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

    pub(crate) fn cancel(&self, task_id: &str) -> CancelOutcome {
        let Ok(mut tasks) = self.tasks.lock() else {
            return CancelOutcome::NotFound;
        };
        let Some(entry) = tasks.get_mut(task_id) else {
            return CancelOutcome::NotFound;
        };
        if entry.snapshot.state.is_active() {
            entry.cancellation.cancel();
            entry.snapshot.state = TaskState::Cancelling;
            return CancelOutcome::Requested(entry.snapshot.clone());
        }
        CancelOutcome::AlreadyFinished(entry.snapshot.clone())
    }

    pub(crate) fn stop_accepting(&self) {
        self.accepting.store(false, Ordering::Release);
    }

    pub(crate) fn shutdown_with_grace(&self, grace: Duration) -> ShutdownReport {
        self.stop_accepting();
        if let Ok(tasks) = self.tasks.lock() {
            for entry in tasks
                .values()
                .filter(|entry| entry.snapshot.state.is_active())
            {
                entry.cancellation.cancel();
            }
        }

        let deadline = Instant::now() + grace;
        if let Ok(mut active) = self.active.0.lock() {
            while *active > 0 {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                active = match self.active.1.wait_timeout(active, remaining) {
                    Ok((next, _)) => next,
                    Err(error) => error.into_inner().0,
                };
            }
        }

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
}
