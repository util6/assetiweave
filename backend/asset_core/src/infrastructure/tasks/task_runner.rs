use super::task_context::{ProgressHandle, TaskContext};
use super::task_models::*;
use super::tasks::TaskRuntime;
use super::{InfraError, InfraResult};
use crate::backend::domain::AppErrorView;
use serde_json::Value;
use std::marker::PhantomData;
use tokio_util::sync::CancellationToken;

/// 强类型任务输出。
///
/// 包含具体业务的强类型结果 `data`，以及面向用户界面展示的友好摘要 `summary`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskOutput<T> {
    pub data: T,
    pub summary: Option<String>,
}

impl<T> TaskOutput<T> {
    pub fn new(data: T) -> Self {
        Self {
            data,
            summary: None,
        }
    }

    pub fn with_summary(data: T, summary: impl Into<String>) -> Self {
        Self {
            data,
            summary: Some(summary.into()),
        }
    }
}

impl<T> From<T> for TaskOutput<T> {
    fn from(data: T) -> Self {
        Self::new(data)
    }
}

/// 强类型任务执行句柄。
///
/// 携带对应任务的 ID 与取消令牌，便于调用方优雅取消或查询状态；
/// 在 Direct 模式下可作为 Fast-path 携带已完成的输出数据。
#[derive(Debug)]
pub struct TaskHandle<T> {
    pub task_id: String,
    pub cancellation: CancellationToken,
    pub completed_output: Option<std::sync::Arc<T>>,
    _marker: PhantomData<T>,
}

impl<T> Clone for TaskHandle<T> {
    fn clone(&self) -> Self {
        Self {
            task_id: self.task_id.clone(),
            cancellation: self.cancellation.clone(),
            completed_output: self.completed_output.clone(),
            _marker: PhantomData,
        }
    }
}

impl<T> TaskHandle<T> {
    pub fn new(task_id: String, cancellation: CancellationToken) -> Self {
        Self {
            task_id,
            cancellation,
            completed_output: None,
            _marker: PhantomData,
        }
    }

    pub fn completed(task_id: impl Into<String>, output: T) -> Self {
        Self {
            task_id: task_id.into(),
            cancellation: CancellationToken::new(),
            completed_output: Some(std::sync::Arc::new(output)),
            _marker: PhantomData,
        }
    }

    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    pub fn is_completed(&self) -> bool {
        self.completed_output.is_some()
    }

    pub fn completed_data(&self) -> Option<std::sync::Arc<T>> {
        self.completed_output.clone()
    }

    /// 等待该任务完成并获取终态快照。
    pub async fn wait_until_finished(&self, runtime: &TaskRuntime) -> Option<TaskSnapshot> {
        if self.completed_output.is_some() {
            let mut snap = TaskSnapshot::empty(&self.task_id);
            snap.state = TaskState::Succeeded;
            snap.outcome = Some(TaskOutcome::Success);
            return Some(snap);
        }
        let mut rx = runtime.subscribe();
        if let Some(snap) = runtime.get(&self.task_id) {
            if snap.state.is_terminal() {
                return Some(snap);
            }
        }
        while let Ok(snap) = rx.recv().await {
            if snap.task_id == self.task_id && snap.state.is_terminal() {
                return Some(snap);
            }
        }
        runtime.get(&self.task_id)
    }
}

impl TaskRuntime {
    pub fn run<T, F, Fut>(&self, spec: TaskSpec, task: F) -> InfraResult<TaskHandle<T>>
    where
        T: serde::Serialize + Send + 'static,
        F: FnOnce(TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = InfraResult<TaskOutput<T>>> + Send + 'static,
    {
        let dedup_key = spec.dedup_key.clone();
        let conflict_policy = spec.conflict_policy;

        let spawn_res = self.prepare_spawn(spec)?;
        let (task_id, cancellation, tracking) = match spawn_res {
            Some(tuple) => tuple,
            None => {
                if conflict_policy == ConflictPolicy::Coalesce {
                    if let Some(key) = dedup_key {
                        if let Ok(tasks) = self.tasks.lock() {
                            if let Some(existing) = tasks.values().find(|e| {
                                e.snapshot.state.is_active()
                                    && e.snapshot.dedup_key.as_deref() == Some(&key)
                            }) {
                                return Ok(TaskHandle::new(
                                    existing.snapshot.task_id.clone(),
                                    existing.cancellation.clone(),
                                ));
                            }
                        }
                    }
                }
                return Err(InfraError::Conflict("同类任务已在运行中".to_string()));
            }
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

        Ok(TaskHandle::new(ret_task_id, ret_cancel))
    }

    /// 在当前执行上下文中同步执行流水线任务（专供测试与就地同步编排）。
    pub async fn run_inline<T, F, Fut>(&self, spec: TaskSpec, task: F) -> InfraResult<TaskSnapshot>
    where
        T: serde::Serialize + Send + 'static,
        F: FnOnce(TaskContext) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = InfraResult<TaskOutput<T>>> + Send + 'static,
    {
        let Some((task_id, cancellation, _tracking)) = self.prepare_spawn(spec)? else {
            return Err(InfraError::Conflict("同类任务已在运行中".to_string()));
        };
        let context = TaskContext {
            cancellation: cancellation.clone(),
            progress: ProgressHandle {
                task_id: task_id.clone(),
                runtime: self.clone(),
            },
        };
        let result: Result<Value, AppErrorView> = match task(context).await {
            Ok(output) => {
                let value = serde_json::to_value(&output.data).unwrap_or(serde_json::Value::Null);
                if let Some(summary) = output.summary {
                    let _ = self.set_result_summary(&task_id, summary);
                }
                Ok(value)
            }
            Err(err) => Err(err.view()),
        };
        self.finish_task(&task_id, &cancellation, result);
        self.get(&task_id)
            .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))
    }
}

#[cfg(test)]
#[path = "task_runner_tests.rs"]
mod tests;
