use super::task_context::{ProgressHandle, TaskContext, TaskFn};
use super::task_models::*;
use super::tasks::{sanitize_task_detail, TaskEntry, TaskRuntime};
use super::{InfraError, InfraResult};
use crate::backend::domain::AppErrorView;
use chrono::Utc;
use serde_json::Value;
use std::sync::atomic::Ordering;
use tokio_util::sync::CancellationToken;

impl TaskRuntime {
    pub(crate) fn register_external(
        &self,
        spec: TaskSpec,
    ) -> Result<ExternalRegistrationOutcome, InfraError> {
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
            state: TaskState::Pending,
            outcome: None,
            dedup_key: spec.dedup_key,
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
    pub(crate) fn start_external(&self, task_id: &str) -> InfraResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
            Self::prune_terminal_tasks_locked(&mut tasks);
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
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
    ) -> InfraResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
            Self::prune_terminal_tasks_locked(&mut tasks);
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
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

    pub(crate) fn cancellation_token(&self, task_id: &str) -> InfraResult<CancellationToken> {
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
        Self::prune_terminal_tasks_locked(&mut tasks);
        tasks
            .get(task_id)
            .map(|entry| entry.cancellation.clone())
            .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))
    }

    pub(crate) fn task_context(&self, task_id: &str) -> InfraResult<TaskContext> {
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
    ) -> InfraResult<()> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
            Self::prune_terminal_tasks_locked(&mut tasks);
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
            if entry.snapshot.state.is_active() {
                if total.is_some_and(|total| current > total) {
                    return Err(InfraError::Validation("任务进度不得超过总数".to_string()));
                }
                if entry
                    .snapshot
                    .progress
                    .as_ref()
                    .is_some_and(|progress| current < progress.current)
                {
                    return Err(InfraError::Validation("任务进度不得回退".to_string()));
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
    pub(crate) fn update_detail(&self, task_id: &str, detail: Value) -> InfraResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
            Self::prune_terminal_tasks_locked(&mut tasks);
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
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
    ) -> InfraResult<TaskSnapshot> {
        let (snapshot, cancellation, tracking, should_launch) = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
            Self::prune_terminal_tasks_locked(&mut tasks);
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
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
                Err(InfraError::Cancelled("后台任务在启动前已取消".to_string()).view()),
            );
        }
        Ok(snapshot)
    }

    pub(crate) fn start_external_with_async<F, Fut, E>(
        &self,
        task_id: &str,
        detail: Value,
        task: F,
    ) -> InfraResult<TaskSnapshot>
    where
        E: Into<AppErrorView> + Send + 'static,
        F: FnOnce(TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<Value, E>> + Send + 'static,
    {
        let (snapshot, cancellation, tracking, should_launch) = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
            Self::prune_terminal_tasks_locked(&mut tasks);
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
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
                Err(InfraError::Cancelled("后台任务在启动前已取消".to_string()).view()),
            );
        }
        Ok(snapshot)
    }

    pub(crate) fn complete_external(
        &self,
        task_id: &str,
        result: Result<Value, AppErrorView>,
    ) -> InfraResult<TaskSnapshot> {
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
        Self::prune_terminal_tasks_locked(&mut tasks);
        let entry = tasks
            .get_mut(task_id)
            .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
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
                        Some(InfraError::Cancelled("后台任务已取消".to_string()).view());
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
                    Some(InfraError::Cancelled("后台任务已取消".to_string()).view());
            }
            Err(error) if error.code == "cancelled" => {
                entry.snapshot.state = TaskState::Canceled;
                entry.snapshot.outcome = Some(TaskOutcome::Canceled);
                entry.snapshot.error = Some(error);
            }
            Err(error) => {
                entry.snapshot.state = TaskState::Failed;
                if entry.snapshot.outcome.is_none() {
                    entry.snapshot.outcome = Some(TaskOutcome::Failure);
                }
                entry.snapshot.error = Some(error);
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
    ) -> InfraResult<()> {
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
        let entry = tasks
            .get_mut(task_id)
            .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
        entry.snapshot.finished_at = Some(finished_at);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn set_user_visible_for_test(
        &self,
        task_id: &str,
        user_visible: bool,
    ) -> InfraResult<()> {
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
        let entry = tasks
            .get_mut(task_id)
            .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
        entry.snapshot.user_visible = user_visible;
        Ok(())
    }
}
