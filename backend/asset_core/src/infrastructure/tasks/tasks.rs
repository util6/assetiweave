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
    pub stage_timeouts: HashMap<String, Duration>,
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
    pub pool_limiter: Arc<super::task_pool::ResourcePoolLimiter>,
    pub runtime_metrics: Arc<super::task_metrics::TaskRuntimeMetrics>,
    pub is_noop: bool,
}

pub(crate) struct PreparedSpawn {
    pub task_id: String,
    pub cancellation: CancellationToken,
    pub tracking: TaskTrackerToken,
    pub slot: super::task_pool::PoolSlot,
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
            pool_limiter: Arc::new(super::task_pool::ResourcePoolLimiter::new()),
            runtime_metrics: Arc::new(super::task_metrics::TaskRuntimeMetrics::new()),
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
            pool_limiter: Arc::new(super::task_pool::ResourcePoolLimiter::new()),
            runtime_metrics: Arc::new(super::task_metrics::TaskRuntimeMetrics::new()),
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

    pub fn metrics_snapshot(&self) -> Vec<super::task_metrics::TaskKindMetricsSnapshot> {
        self.runtime_metrics.snapshot()
    }

    pub(crate) fn stage_timeout(&self, task_id: &str, stage_id: &str) -> Option<Duration> {
        let tasks = self.tasks.lock().ok()?;
        tasks
            .get(task_id)
            .and_then(|entry| entry.stage_timeouts.get(stage_id).copied())
    }

    pub(crate) fn publish_terminal_and_record(&self, snapshot: &TaskSnapshot) {
        if let Some(duration_ms) = self.runtime_metrics.record(snapshot) {
            let error_code = snapshot
                .error
                .as_ref()
                .map(|e| e.code.as_str())
                .unwrap_or("none");
            if snapshot.state == TaskState::Failed {
                tracing::warn!(
                    target: "task_center",
                    task_id = %snapshot.task_id,
                    kind = %snapshot.kind,
                    pool = %snapshot.resource_pool.as_str(),
                    state = ?snapshot.state,
                    duration_ms,
                    error_code = %error_code,
                    user_visible = snapshot.user_visible,
                    "background task finished with error"
                );
            } else {
                tracing::info!(
                    target: "task_center",
                    task_id = %snapshot.task_id,
                    kind = %snapshot.kind,
                    pool = %snapshot.resource_pool.as_str(),
                    state = ?snapshot.state,
                    duration_ms,
                    error_code = %error_code,
                    user_visible = snapshot.user_visible,
                    "background task finished"
                );
            }
        }
        self.publish(snapshot);
    }

    pub fn prepare_spawn(
        &self,
        spec: TaskSpec,
    ) -> Result<Option<(String, CancellationToken, TaskTrackerToken)>, InfraError> {
        Ok(self
            .prepare_spawn_with_pool(spec, false)?
            .map(|prepared| (prepared.task_id, prepared.cancellation, prepared.tracking)))
    }

    pub(crate) fn prepare_spawn_with_pool(
        &self,
        spec: TaskSpec,
        acquire_slot: bool,
    ) -> Result<Option<PreparedSpawn>, InfraError> {
        if !self.accepting.load(Ordering::Acquire) {
            return Err(InfraError::Cancelled(
                "应用正在关闭，不再接受新任务".to_string(),
            ));
        }
        let resource_pool = spec.resolved_resource_pool();
        let user_visible = spec.resolved_user_visible();
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
                        && entry.snapshot.tenant_id == spec.tenant_id
                        && entry
                            .conflict_keys
                            .iter()
                            .any(|k| spec.conflict_keys.contains(k))))
            })
            .map(|entry| entry.snapshot.task_id.clone())
            .collect();

        let mut cancelled_notifications = Vec::new();
        if !conflicting_ids.is_empty() {
            match spec.conflict_policy {
                ConflictPolicy::Reject | ConflictPolicy::Coalesce => {
                    return Ok(None);
                }
                ConflictPolicy::CancelExisting => {
                    for id in &conflicting_ids {
                        if let Some(entry) = tasks.get_mut(id) {
                            cancelled_notifications.push(Self::cancel_entry_locked(
                                entry,
                                &self.events,
                                "被后续冲突任务取消",
                            ));
                        }
                    }
                }
            }
        }
        let tracking = self.tracker.token();
        let slot = if acquire_slot && spec.admission != TaskAdmission::Direct {
            self.pool_limiter.acquire_or_enqueue(resource_pool)
        } else {
            super::task_pool::PoolSlot::Unbounded
        };
        let initial_state = if slot.is_queued() {
            TaskState::Pending
        } else {
            TaskState::Running
        };
        let started = !slot.is_queued();
        let capabilities = spec.capabilities.unwrap_or_default();
        let category = spec
            .category
            .clone()
            .unwrap_or_else(|| super::task_pipeline::TaskCategory::from(spec.kind))
            .0;
        let (stages, stage_timeouts) = if let Some(pipeline) = &spec.pipeline {
            (pipeline.to_initial_stages(), pipeline.to_stage_timeouts())
        } else {
            (Vec::new(), HashMap::new())
        };
        let snapshot = TaskSnapshot {
            task_id: task_id.clone(),
            kind: spec.kind,
            resource_pool,
            category,
            tenant_id: spec.tenant_id,
            title: spec.title,
            user_visible,
            dedup_key: spec.dedup_key,
            state: initial_state,
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
                stage_timeouts,
                started,
                tracking: None,
                last_cosmetic_publish: None,
                pending_cosmetic: false,
            },
        );
        drop(tasks);
        for (canceled_snapshot, was_pending) in cancelled_notifications {
            if was_pending {
                self.publish_terminal_and_record(&canceled_snapshot);
            } else {
                self.publish(&canceled_snapshot);
            }
        }
        self.publish(&snapshot);

        Ok(Some(PreparedSpawn {
            task_id,
            cancellation,
            tracking,
            slot,
        }))
    }

    pub(crate) fn mark_queued_task_running(
        &self,
        task_id: &str,
        cancellation: &CancellationToken,
    ) -> bool {
        if cancellation.is_cancelled() {
            return false;
        }
        let mut updated_snapshot = None;
        if let Ok(mut tasks) = self.tasks.lock() {
            if let Some(entry) = tasks.get_mut(task_id) {
                if entry.snapshot.state == TaskState::Pending && !cancellation.is_cancelled() {
                    entry.snapshot.state = TaskState::Running;
                    entry.snapshot.change_class = TaskChangeClass::Structural;
                    entry.snapshot.revision += 1;
                    entry.snapshot.updated_at = Utc::now().to_rfc3339();
                    entry.started = true;
                    updated_snapshot = Some(entry.snapshot.clone());
                }
            }
        }
        if let Some(snapshot) = updated_snapshot {
            self.publish(&snapshot);
            true
        } else {
            false
        }
    }

    pub(crate) async fn acquire_pool_permit_for_task(
        &self,
        task_id: &str,
        cancellation: &CancellationToken,
        slot: super::task_pool::PoolSlot,
    ) -> Result<Option<super::task_pool::PoolPermit>, ()> {
        let is_queued = slot.is_queued();
        let permit = match slot.await_permit(cancellation).await {
            super::task_pool::PoolAcquireOutcome::Granted(permit) => permit,
            super::task_pool::PoolAcquireOutcome::Cancelled => {
                self.finish_task(
                    task_id,
                    cancellation,
                    Err(InfraError::Cancelled("排队任务已取消".to_string()).view()),
                );
                return Err(());
            }
        };
        if is_queued && !self.mark_queued_task_running(task_id, cancellation) {
            self.finish_task(
                task_id,
                cancellation,
                Err(InfraError::Cancelled("排队任务已取消".to_string()).view()),
            );
            return Err(());
        }
        Ok(permit)
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
        let Some(prepared) = self.prepare_spawn_with_pool(spec, true)? else {
            return Ok(SpawnOutcome::Existing);
        };
        self.launch_task_async_with_slot(
            prepared.task_id,
            prepared.cancellation,
            prepared.tracking,
            prepared.slot,
            task,
        )?;
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

    pub(crate) fn begin_structural_transition_locked(
        entry: &mut TaskEntry,
        events: &broadcast::Sender<TaskSnapshot>,
    ) {
        Self::flush_cosmetic_locked(entry, events);
        entry.snapshot.change_class = TaskChangeClass::Structural;
        entry.snapshot.revision += 1;
        entry.snapshot.updated_at = Utc::now().to_rfc3339();
    }

    pub(crate) fn cancel_entry_locked(
        entry: &mut TaskEntry,
        events: &broadcast::Sender<TaskSnapshot>,
        reason: &str,
    ) -> (TaskSnapshot, bool) {
        entry.cancellation.cancel();
        // 仅由资源池队列托管且尚未开始运行的内部任务（tracking == None）才在取消时立刻直达 Canceled；
        // 由外部适配器注册的任务（tracking == Some）在外部协调器完成收尾前保持 Cancelling。
        let immediate_cancel =
            entry.snapshot.state == TaskState::Pending && entry.tracking.is_none();
        Self::begin_structural_transition_locked(entry, events);
        if immediate_cancel {
            entry.snapshot.state = TaskState::Canceled;
            entry.snapshot.outcome = Some(TaskOutcome::Canceled);
            entry.snapshot.finished_at = Some(entry.snapshot.updated_at.clone());
            entry.snapshot.error = Some(InfraError::Cancelled(reason.to_string()).view());
        } else {
            entry.snapshot.state = TaskState::Cancelling;
        }
        (entry.snapshot.clone(), immediate_cancel)
    }

    pub(crate) fn record_cosmetic_change_locked(entry: &mut TaskEntry) -> Option<TaskSnapshot> {
        const COSMETIC_THROTTLE: Duration = Duration::from_millis(150);
        entry.snapshot.change_class = TaskChangeClass::Cosmetic;
        entry.snapshot.revision += 1;
        entry.snapshot.updated_at = Utc::now().to_rfc3339();

        let now = std::time::Instant::now();
        let should_throttle = entry
            .last_cosmetic_publish
            .is_some_and(|last| now.duration_since(last) < COSMETIC_THROTTLE);
        if should_throttle {
            entry.pending_cosmetic = true;
            None
        } else {
            entry.last_cosmetic_publish = Some(now);
            entry.pending_cosmetic = false;
            Some(entry.snapshot.clone())
        }
    }

    pub(crate) fn update_and_publish_cosmetic<F>(
        &self,
        task_id: &str,
        update: F,
    ) -> InfraResult<TaskSnapshot>
    where
        F: FnOnce(&mut TaskSnapshot),
    {
        if self.is_noop {
            return Ok(TaskSnapshot::empty(task_id));
        }
        let (snapshot, should_publish) = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
            update(&mut entry.snapshot);
            let maybe_publish = Self::record_cosmetic_change_locked(entry);
            let should_publish = maybe_publish.is_some();
            (
                maybe_publish.unwrap_or_else(|| entry.snapshot.clone()),
                should_publish,
            )
        };
        if should_publish {
            self.publish(&snapshot);
        }
        Ok(snapshot)
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
                    let timed_out_failure = entry
                        .snapshot
                        .stages
                        .iter()
                        .flat_map(|stage| stage.failures.iter())
                        .find(|failure| {
                            failure.code == super::task_pipeline::STAGE_TIMEOUT_ERROR_CODE
                        })
                        .cloned();
                    match result {
                        Ok(_detail) if cancellation.is_cancelled() => {
                            entry.snapshot.state = TaskState::Canceled;
                            entry.snapshot.outcome = Some(TaskOutcome::Canceled);
                            entry.snapshot.error =
                                Some(InfraError::Cancelled("后台任务已取消".to_string()).view());
                        }
                        Ok(_detail) if timed_out_failure.is_some() => {
                            let failure = timed_out_failure.expect("checked above");
                            entry.snapshot.state = TaskState::Failed;
                            entry.snapshot.outcome = Some(TaskOutcome::Failure);
                            if entry.snapshot.error_summary.is_none() {
                                entry.snapshot.error_summary = Some(failure.message.clone());
                            }
                            entry.snapshot.error = Some(AppErrorView {
                                code: super::task_pipeline::STAGE_TIMEOUT_ERROR_CODE.to_string(),
                                message: failure.message,
                                retryable: failure.retryable,
                                details: None,
                            });
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
                        Err(mut error) => {
                            if let Some(failure) = timed_out_failure.as_ref() {
                                if error.code == "timeout" {
                                    error.code =
                                        super::task_pipeline::STAGE_TIMEOUT_ERROR_CODE.to_string();
                                    error.message = failure.message.clone();
                                    error.retryable = failure.retryable;
                                }
                            }
                            entry.snapshot.state = TaskState::Failed;
                            if entry.snapshot.outcome.is_none() {
                                entry.snapshot.outcome = Some(TaskOutcome::Failure);
                            }
                            if entry.snapshot.error_summary.is_none() {
                                entry.snapshot.error_summary = Some(error.message.clone());
                            }
                            entry.snapshot.error = Some(error);
                        }
                    }
                    terminal_snapshot = Some(entry.snapshot.clone());
                }
            }
        }
        if let Some(snapshot) = terminal_snapshot {
            self.publish_terminal_and_record(&snapshot);
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
        self.launch_task_async_with_slot(
            task_id,
            cancellation,
            tracking,
            super::task_pool::PoolSlot::Unbounded,
            task,
        )
    }

    pub(crate) fn launch_task_async_with_slot<F, Fut, E>(
        &self,
        task_id: String,
        cancellation: CancellationToken,
        tracking: TaskTrackerToken,
        slot: super::task_pool::PoolSlot,
        task: F,
    ) -> InfraResult<()>
    where
        E: Into<AppErrorView> + Send + 'static,
        F: FnOnce(TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<Value, E>> + Send + 'static,
    {
        self.spawn_tracked_future(
            task_id,
            cancellation,
            tracking,
            slot,
            move |context, _rt, _id| async move { task(context).await.map_err(Into::into) },
        )
    }

    pub(crate) fn spawn_tracked_future<F, Fut>(
        &self,
        task_id: String,
        cancellation: CancellationToken,
        tracking: TaskTrackerToken,
        slot: super::task_pool::PoolSlot,
        run_future: F,
    ) -> InfraResult<()>
    where
        F: FnOnce(TaskContext, TaskRuntime, String) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<Value, AppErrorView>> + Send + 'static,
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
        let run_task_id = task_id;
        handle.spawn(async move {
            let _tracking = tracking;
            let cancellation_for_finish = cancellation.clone();
            let Ok(_pool_permit) = runtime
                .acquire_pool_permit_for_task(&run_task_id, &cancellation, slot)
                .await
            else {
                return;
            };
            let context = TaskContext {
                cancellation: cancellation.clone(),
                progress: ProgressHandle {
                    task_id: run_task_id.clone(),
                    runtime: runtime.clone(),
                },
            };
            let span = tracing::info_span!("task_execution", task_id = %run_task_id);
            use tracing::Instrument;
            let future = run_future(context, runtime.clone(), run_task_id.clone());
            let result: Result<Value, AppErrorView> =
                match tokio::spawn(future.instrument(span)).await {
                    Ok(inner) => inner,
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

#[cfg(test)]
#[path = "tasks_tests.rs"]
mod tests;
