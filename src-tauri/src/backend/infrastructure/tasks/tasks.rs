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

pub(crate) use super::task_context::*;
pub(crate) use super::task_models::*;
pub(crate) use super::task_runtime_query::*;

pub(crate) struct TaskEntry {
    pub(crate) snapshot: TaskSnapshot,
    pub(crate) cancellation: CancellationToken,
    pub(crate) conflict_keys: Vec<String>,
    pub(crate) started: bool,
    pub(crate) tracking: Option<TaskTrackerToken>,
}

#[derive(Clone)]
pub(crate) struct TaskRuntime {
    pub(crate) tasks: Arc<Mutex<HashMap<String, TaskEntry>>>,
    pub(crate) sequence: Arc<AtomicU64>,
    pub(crate) accepting: Arc<AtomicBool>,
    pub(crate) runtime_handle: Option<tokio::runtime::Handle>,
    pub(crate) tracker: TaskTracker,
    pub(crate) events: Arc<broadcast::Sender<TaskSnapshot>>,
}

impl Default for TaskRuntime {
    fn default() -> Self {
        Self::new()
    }
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

    pub(crate) fn publish(&self, snapshot: &TaskSnapshot) {
        let _ = self.events.send(snapshot.clone());
    }

    pub(crate) fn prepare_spawn(
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
    pub(crate) fn spawn(&self, spec: TaskSpec, task: TaskFn) -> Result<SpawnOutcome, InfraError> {
        let Some((task_id, cancellation, tracking)) = self.prepare_spawn(spec)? else {
            return Ok(SpawnOutcome::Existing);
        };
        self.launch_task(task_id, cancellation, tracking, task)?;
        Ok(SpawnOutcome::Started)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn spawn_async<F, Fut, E>(
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

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn run<T, F, Fut>(
        &self,
        spec: TaskSpec,
        task: F,
    ) -> InfraResult<super::task_runner::TaskHandle<T>>
    where
        T: serde::Serialize + Send + 'static,
        F: FnOnce(TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = InfraResult<super::task_runner::TaskOutput<T>>>
            + Send
            + 'static,
    {
        let Some((task_id, cancellation, tracking)) = self.prepare_spawn(spec)? else {
            return Err(InfraError::Conflict("同类任务已在运行中".to_string()));
        };
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
        let ret_task_id = task_id.clone();
        let ret_cancel = cancellation.clone();

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
                    Ok(Ok(output)) => {
                        let value =
                            serde_json::to_value(&output.data).unwrap_or(serde_json::Value::Null);
                        if let Some(summary) = output.summary {
                            let _ = runtime.set_result_summary(&run_task_id, summary);
                        }
                        Ok(value)
                    }
                    Ok(Err(err)) => Err(err.view()),
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

        Ok(super::task_runner::TaskHandle::new(ret_task_id, ret_cancel))
    }

    pub(crate) fn has_active_tasks(&self) -> bool {
        self.tasks
            .lock()
            .map(|tasks| tasks.values().any(|entry| entry.snapshot.state.is_active()))
            .unwrap_or(true)
    }

    pub(crate) fn finish_task(
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

    pub(crate) fn launch_task(
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

    pub(crate) fn launch_task_async<F, Fut, E>(
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
