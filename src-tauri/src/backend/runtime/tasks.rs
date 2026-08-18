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
    Succeeded,
    Failed,
    Canceled,
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
    pub(crate) detail: Value,
}

impl TaskSpec {
    pub(crate) fn new(kind: TaskKind, dedup_key: Option<String>) -> Self {
        Self {
            kind,
            task_id: None,
            dedup_key,
            detail: Value::Null,
        }
    }

    pub(crate) fn with_task_id(mut self, task_id: impl Into<String>) -> Self {
        self.task_id = Some(task_id.into());
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
}

pub(crate) struct TaskContext {
    cancellation: CancellationToken,
    progress: ProgressHandle,
}

impl TaskContext {
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
    Started(TaskSnapshot),
    Existing(TaskSnapshot),
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
        if let Some(existing) = spec.dedup_key.as_ref().and_then(|key| {
            tasks.values().find(|entry| {
                entry.snapshot.kind == spec.kind
                    && entry.snapshot.dedup_key.as_ref() == Some(key)
                    && matches!(
                        entry.snapshot.state,
                        TaskState::Pending | TaskState::Running
                    )
            })
        }) {
            return Ok(SpawnOutcome::Existing(existing.snapshot.clone()));
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
        };
        tasks.insert(
            task_id.clone(),
            TaskEntry {
                snapshot: snapshot.clone(),
                cancellation: cancellation.clone(),
            },
        );
        drop(tasks);
        if let Ok(mut active) = self.active.0.lock() {
            *active += 1;
        }

        let runtime = self.clone();
        let run_task_id = task_id.clone();
        let thread_name = format!("aiw-task-{task_id}");
        let run = move || {
            let context = TaskContext {
                cancellation,
                progress: ProgressHandle {
                    task_id: run_task_id.clone(),
                    runtime: runtime.clone(),
                },
            };
            let result = catch_unwind(AssertUnwindSafe(|| task(context)))
                .unwrap_or_else(|_| Err(AppError::Legacy("后台任务发生 panic".to_string())));
            if let Ok(mut tasks) = runtime.tasks.lock() {
                if let Some(entry) = tasks.get_mut(&run_task_id) {
                    entry.snapshot.finished_at = Some(Utc::now().to_rfc3339());
                    match result {
                        Ok(detail) => {
                            entry.snapshot.state = TaskState::Succeeded;
                            entry.snapshot.detail = detail;
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
                }
            }
            if let Ok(mut active) = runtime.active.0.lock() {
                *active = active.saturating_sub(1);
                runtime.active.1.notify_all();
            }
        };
        if let Some(handle) = self.runtime_handle.clone() {
            handle.spawn_blocking(run);
        } else {
            if let Err(error) = std::thread::Builder::new().name(thread_name).spawn(run) {
                if let Ok(mut tasks) = self.tasks.lock() {
                    if let Some(entry) = tasks.get_mut(&task_id) {
                        entry.snapshot.state = TaskState::Failed;
                        entry.snapshot.finished_at = Some(Utc::now().to_rfc3339());
                        entry.snapshot.error =
                            Some(AppError::Legacy(format!("启动后台任务失败: {error}")).into());
                    }
                }
                if let Ok(mut active) = self.active.0.lock() {
                    *active = active.saturating_sub(1);
                    self.active.1.notify_all();
                }
                return Err(AppError::Legacy(format!("启动后台任务失败: {error}")));
            }
        }
        Ok(SpawnOutcome::Started(snapshot))
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
                        TaskState::Pending | TaskState::Running
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
        if matches!(
            entry.snapshot.state,
            TaskState::Pending | TaskState::Running
        ) {
            entry.cancellation.cancel();
            return CancelOutcome::Requested(entry.snapshot.clone());
        }
        CancelOutcome::AlreadyFinished(entry.snapshot.clone())
    }

    pub(crate) fn stop_accepting(&self) {
        self.accepting.store(false, Ordering::Release);
    }

    pub(crate) fn shutdown(&self) -> ShutdownReport {
        self.shutdown_with_grace(Duration::from_secs(5))
    }

    pub(crate) fn shutdown_with_grace(&self, grace: Duration) -> ShutdownReport {
        self.stop_accepting();
        if let Ok(tasks) = self.tasks.lock() {
            for entry in tasks.values().filter(|entry| {
                matches!(
                    entry.snapshot.state,
                    TaskState::Pending | TaskState::Running
                )
            }) {
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
                    .filter(|entry| {
                        matches!(
                            entry.snapshot.state,
                            TaskState::Pending | TaskState::Running
                        )
                    })
                    .map(|entry| entry.snapshot.task_id.clone())
                    .collect()
            })
            .unwrap_or_default();
        ShutdownReport {
            unfinished_task_ids,
        }
    }
}
