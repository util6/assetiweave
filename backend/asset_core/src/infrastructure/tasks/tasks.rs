use super::{InfraError, InfraResult};
use crate::backend::domain::AppErrorView;
use chrono::Utc;
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
use tokio_util::sync::CancellationToken;
use tokio_util::task::{task_tracker::TaskTrackerToken, TaskTracker};

pub use super::task_context::*;
pub use super::task_models::*;
pub use super::task_runner::*;
pub use super::task_runtime_query::*;

pub struct TaskEntry {
    pub snapshot: TaskSnapshot,
    pub cancellation: CancellationToken,
    pub conflict_keys: Vec<String>,
    pub started: bool,
    pub tracking: Option<TaskTrackerToken>,
    pub last_cosmetic_publish: Option<std::time::Instant>,
    pub pending_cosmetic: bool,
}

#[derive(Clone)]
pub struct TaskRuntime {
    pub tasks: Arc<Mutex<HashMap<String, TaskEntry>>>,
    pub sequence: Arc<AtomicU64>,
    pub accepting: Arc<AtomicBool>,
    pub runtime_handle: Option<tokio::runtime::Handle>,
    pub tracker: TaskTracker,
    pub events: Arc<broadcast::Sender<TaskSnapshot>>,
    pub is_noop: bool,
}

impl Default for TaskRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskRuntime {
    pub fn new() -> Self {
        let (events, _) = broadcast::channel(256);
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            sequence: Arc::new(AtomicU64::new(0)),
            accepting: Arc::new(AtomicBool::new(true)),
            runtime_handle: None,
            tracker: TaskTracker::new(),
            events: Arc::new(events),
            is_noop: false,
        }
    }

    pub fn noop() -> Self {
        let (events, _) = broadcast::channel(1);
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            sequence: Arc::new(AtomicU64::new(0)),
            accepting: Arc::new(AtomicBool::new(false)),
            runtime_handle: None,
            tracker: TaskTracker::new(),
            events: Arc::new(events),
            is_noop: true,
        }
    }

    pub fn with_runtime_handle(handle: tokio::runtime::Handle) -> Self {
        Self {
            runtime_handle: Some(handle),
            ..Self::new()
        }
    }

    pub fn runtime_handle(&self) -> Option<tokio::runtime::Handle> {
        self.runtime_handle.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TaskSnapshot> {
        self.events.subscribe()
    }

    pub fn publish(&self, snapshot: &TaskSnapshot) {
        if self.is_noop {
            return;
        }
        let _ = self.events.send(snapshot.clone());
    }

    pub fn prepare_spawn(
        &self,
        spec: TaskSpec,
    ) -> Result<Option<(String, CancellationToken, TaskTrackerToken)>, InfraError> {
        if !self.accepting.load(Ordering::Acquire) {
            return Err(InfraError::Cancelled(
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
            .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
        if !self.accepting.load(Ordering::Acquire) {
            return Err(InfraError::Cancelled(
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
        let conflicting_ids: Vec<String> = tasks
            .values()
            .filter(|entry| {
                entry.snapshot.state.is_active()
                    && (spec.dedup_key.as_ref().is_some_and(|key| {
                        entry.snapshot.kind == spec.kind
                            && entry.snapshot.tenant_id == spec.tenant_id
                            && entry.snapshot.dedup_key.as_deref() == Some(key)
                    }) || (!spec.conflict_keys.is_empty()
                        && entry
                            .conflict_keys
                            .iter()
                            .any(|k| spec.conflict_keys.contains(k))))
            })
            .map(|entry| entry.snapshot.task_id.clone())
            .collect();

        if !conflicting_ids.is_empty() {
            match spec.conflict_policy {
                ConflictPolicy::Reject | ConflictPolicy::Coalesce => {
                    return Ok(None);
                }
                ConflictPolicy::CancelExisting => {
                    for id in &conflicting_ids {
                        if let Some(entry) = tasks.get_mut(id) {
                            entry.cancellation.cancel();
                            entry.snapshot.state = TaskState::Cancelling;
                            entry.snapshot.revision += 1;
                        }
                    }
                }
            }
        }
        let tracking = self.tracker.token();
        let user_visible = spec
            .user_visible
            .unwrap_or_else(|| !matches!(spec.kind, TaskKind::Other));
        let capabilities = spec.capabilities.unwrap_or_default();
        let category = spec
            .category
            .clone()
            .unwrap_or_else(|| super::task_pipeline::TaskCategory::from(spec.kind))
            .0;
        let stages = if let Some(pipeline) = &spec.pipeline {
            pipeline.to_initial_stages()
        } else {
            Vec::new()
        };
        let snapshot = TaskSnapshot {
            task_id: task_id.clone(),
            kind: spec.kind,
            category,
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
            stages,
            metrics: Vec::new(),
            failures: Vec::new(),
            error_summary: None,
            result_summary: None,
            capabilities,
            revision: 1,
            detail: sanitize_task_detail(spec.detail),
            result: None,
            change_class: TaskChangeClass::Structural,
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
                last_cosmetic_publish: None,
                pending_cosmetic: false,
            },
        );
        drop(tasks);
        self.publish(&snapshot);

        Ok(Some((task_id, cancellation, tracking)))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn spawn(&self, spec: TaskSpec, task: TaskFn) -> Result<SpawnOutcome, InfraError> {
        let Some((task_id, cancellation, tracking)) = self.prepare_spawn(spec)? else {
            return Ok(SpawnOutcome::Existing);
        };
        self.launch_task(task_id, cancellation, tracking, task)?;
        Ok(SpawnOutcome::Started)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn spawn_async<F, Fut, E>(
        &self,
        spec: TaskSpec,
        task: F,
    ) -> Result<SpawnOutcome, InfraError>
    where
        E: Into<AppErrorView> + Send + 'static,
        F: FnOnce(TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<Value, E>> + Send + 'static,
    {
        let Some((task_id, cancellation, tracking)) = self.prepare_spawn(spec)? else {
            return Ok(SpawnOutcome::Existing);
        };
        self.launch_task_async(task_id, cancellation, tracking, task)?;
        Ok(SpawnOutcome::Started)
    }

    pub(crate) fn flush_cosmetic_locked(
        entry: &mut TaskEntry,
        events: &broadcast::Sender<TaskSnapshot>,
    ) {
        if entry.pending_cosmetic {
            entry.pending_cosmetic = false;
            entry.last_cosmetic_publish = Some(std::time::Instant::now());
            let mut snapshot = entry.snapshot.clone();
            snapshot.change_class = TaskChangeClass::Cosmetic;
            let _ = events.send(snapshot);
        }
    }

    pub fn has_active_tasks(&self) -> bool {
        self.tasks
            .lock()
            .map(|tasks| tasks.values().any(|entry| entry.snapshot.state.is_active()))
            .unwrap_or(true)
    }

    pub fn finish_task(
        &self,
        task_id: &str,
        cancellation: &CancellationToken,
        result: Result<Value, AppErrorView>,
    ) {
        let mut terminal_snapshot = None;
        if let Ok(mut tasks) = self.tasks.lock() {
            if let Some(entry) = tasks.get_mut(task_id) {
                if matches!(
                    entry.snapshot.state,
                    TaskState::Pending | TaskState::Running | TaskState::Cancelling
                ) {
                    Self::flush_cosmetic_locked(entry, &self.events);
                    entry.snapshot.change_class = TaskChangeClass::Structural;
                    let now = Utc::now().to_rfc3339();
                    entry.snapshot.finished_at = Some(now.clone());
                    entry.snapshot.updated_at = now;
                    entry.snapshot.revision += 1;
                    match result {
                        Ok(_detail) if cancellation.is_cancelled() => {
                            entry.snapshot.state = TaskState::Canceled;
                            entry.snapshot.outcome = Some(TaskOutcome::Canceled);
                            entry.snapshot.error =
                                Some(InfraError::Cancelled("后台任务已取消".to_string()).view());
                        }
                        Ok(detail) => {
                            entry.snapshot.state = TaskState::Succeeded;
                            if entry.snapshot.outcome.is_none() {
                                entry.snapshot.outcome = Some(TaskOutcome::Success);
                            }
                            entry.snapshot.result = Some(detail);
                        }
                        Err(error) if cancellation.is_cancelled() || error.code == "cancelled" => {
                            entry.snapshot.state = TaskState::Canceled;
                            entry.snapshot.outcome = Some(TaskOutcome::Canceled);
                            entry.snapshot.error = Some(if error.code == "cancelled" {
                                error
                            } else {
                                InfraError::Cancelled("后台任务已取消".to_string()).view()
                            });
                        }
                        Err(error) => {
                            entry.snapshot.state = TaskState::Failed;
                            if entry.snapshot.outcome.is_none() {
                                entry.snapshot.outcome = Some(TaskOutcome::Failure);
                            }
                            entry.snapshot.error = Some(error);
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

    pub fn launch_task(
        &self,
        task_id: String,
        cancellation: CancellationToken,
        tracking: TaskTrackerToken,
        task: TaskFn,
    ) -> InfraResult<()> {
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
            let result = catch_unwind(AssertUnwindSafe(|| task(context))).unwrap_or_else(|_| {
                Err(InfraError::External("后台任务发生 panic".to_string()).view())
            });
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
                        Some(InfraError::External(format!("启动后台任务失败: {error}")).view());
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
            return Err(InfraError::External(format!("启动后台任务失败: {error}")));
        }
        Ok(())
    }

    pub fn launch_task_async<F, Fut, E>(
        &self,
        task_id: String,
        cancellation: CancellationToken,
        tracking: TaskTrackerToken,
        task: F,
    ) -> InfraResult<()>
    where
        E: Into<AppErrorView> + Send + 'static,
        F: FnOnce(TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<Value, E>> + Send + 'static,
    {
        let handle = self
            .runtime_handle
            .clone()
            .or_else(|| tokio::runtime::Handle::try_current().ok())
            .ok_or_else(|| {
                InfraError::external(
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
            let result: Result<Value, AppErrorView> =
                match tokio::spawn(task(context).instrument(span)).await {
                    Ok(Ok(val)) => Ok(val),
                    Ok(Err(err)) => Err(err.into()),
                    Err(join_err) => {
                        if join_err.is_cancelled() {
                            Err(InfraError::Cancelled("后台任务已取消".to_string()).view())
                        } else {
                            Err(InfraError::External("后台任务发生 panic".to_string()).view())
                        }
                    }
                };
            runtime.finish_task(&run_task_id, &cancellation_for_finish, result);
        });
        Ok(())
    }
}
