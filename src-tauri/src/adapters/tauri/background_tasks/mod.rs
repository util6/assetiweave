//! Tauri 后台异步长任务管理与状态推送模块
//!
//! 支持会话同步、扫描索引、备份导入导出以及脚本安装卸载在内的异步后台任务注册、取消控制、状态快照与事件广播。

use crate::backend::{
    application::AppResult,
    domain::AppErrorView,
    infrastructure::agent_market::AgentLifecycleTaskSnapshot,
    infrastructure::extensions::{
        LifecycleOp, LifecycleRequestKey, LifecycleReservationOutcome, LifecycleTaskCoordinator,
        PackageIdentity, PackageKind, ResourceKey,
    },
    infrastructure::tasks::{
        ExternalRegistrationOutcome, TaskFn, TaskKind, TaskRuntime, TaskSnapshot, TaskSpec,
        TaskState,
    },
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use uuid::Uuid;

macro_rules! impl_basic_projection {
    ($ty:ty) => {
        impl BackgroundTaskProjection for $ty {
            fn project_with_runtime(mut self, runtime: &TaskSnapshot) -> Self {
                self.status = background_task_status(runtime.state);
                self.finished_at = runtime.finished_at.clone();
                if runtime.state == TaskState::Canceled {
                    self.result = None;
                    self.error = runtime_error_message(runtime);
                } else if runtime.state == TaskState::Failed && self.error.is_none() {
                    self.error = runtime_error_message(runtime);
                }
                if runtime.state == TaskState::Succeeded {
                    self.result = runtime.result.clone();
                    self.error = None;
                }
                self
            }
        }
    };
}

pub(crate) use impl_basic_projection;

pub(crate) trait BackgroundTaskProjection: DeserializeOwned {
    fn project_with_runtime(self, runtime: &TaskSnapshot) -> Self;
}

pub(crate) fn runtime_error_message(snapshot: &TaskSnapshot) -> Option<AppErrorView> {
    snapshot.error.clone()
}

pub(crate) fn background_task_status(state: TaskState) -> BackgroundTaskStatus {
    match state {
        TaskState::Pending | TaskState::Running => BackgroundTaskStatus::Running,
        TaskState::Cancelling => BackgroundTaskStatus::Cancelling,
        TaskState::Succeeded => BackgroundTaskStatus::Completed,
        TaskState::Failed => BackgroundTaskStatus::Failed,
        TaskState::Canceled => BackgroundTaskStatus::Cancelled,
    }
}

pub(crate) fn extension_lifecycle_key(
    kind: PackageKind,
    package_id: &str,
    version: Option<&str>,
    action: &str,
) -> AppResult<LifecycleRequestKey> {
    let version = version.unwrap_or("0.0.0");
    let version = semver::Version::parse(version)
        .map_err(|error| crate::backend::application::AppError::Validation(error.to_string()))?;
    Ok(LifecycleRequestKey {
        resource: ResourceKey::new(PackageIdentity {
            kind,
            package_id: package_id.to_string(),
            version,
        }),
        operation: match action {
            "install" => LifecycleOp::Install,
            "update" => LifecycleOp::Upgrade,
            "reinstall" => LifecycleOp::Install,
            "uninstall" => LifecycleOp::Remove,
            "enable" => LifecycleOp::Enable,
            "disable" => LifecycleOp::Disable,
            "probe" => LifecycleOp::Probe,
            _ => {
                return Err(crate::backend::application::AppError::Validation(format!(
                    "unsupported lifecycle action: {action}"
                )))
            }
        },
    })
}

pub(crate) fn dedupe_non_empty(values: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

/// 后台异步任务的状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BackgroundTaskStatus {
    /// 任务正在后台运行中
    Running,
    /// 任务已收到取消请求，等待 worker 收敛
    Cancelling,
    /// 任务已成功完成
    Completed,
    /// 任务运行失败
    Failed,
    /// 任务已被用户取消
    Cancelled,
}

pub(crate) struct BackgroundTaskRegistry {
    /// The backend TaskRuntime is the only mutable lifecycle authority. The
    /// Tauri layer stores no parallel task maps; its DTOs are serialized in
    /// `TaskSnapshot.detail` and projected on read.
    task_runtime: TaskRuntime,
    lifecycle: LifecycleTaskCoordinator,
}

impl Default for BackgroundTaskRegistry {
    fn default() -> Self {
        Self::with_task_runtime(TaskRuntime::new())
    }
}

impl BackgroundTaskRegistry {
    pub(crate) fn with_task_runtime(task_runtime: TaskRuntime) -> Self {
        Self {
            lifecycle: LifecycleTaskCoordinator::new(task_runtime.clone()),
            task_runtime,
        }
    }

    pub(crate) fn task_runtime(&self) -> Option<TaskRuntime> {
        Some(self.task_runtime.clone())
    }

    #[allow(dead_code)]

    fn register_external_task(
        &self,
        kind: TaskKind,
        task_id: &str,
        dedup_key: Option<String>,
        conflict_keys: impl IntoIterator<Item = String>,
        detail: Value,
    ) -> AppResult<ExternalRegistrationOutcome> {
        self.register_external_task_for_tenant(
            None,
            kind,
            task_id,
            dedup_key,
            conflict_keys,
            detail,
        )
    }

    fn register_external_task_for_tenant(
        &self,
        tenant_id: Option<&str>,
        kind: TaskKind,
        task_id: &str,
        dedup_key: Option<String>,
        conflict_keys: impl IntoIterator<Item = String>,
        detail: Value,
    ) -> AppResult<ExternalRegistrationOutcome> {
        let mut spec = match tenant_id {
            Some(tenant_id) => TaskSpec::new(kind, dedup_key).with_tenant_id(tenant_id),
            None => TaskSpec::global(kind, dedup_key),
        }
        .with_task_id(task_id.to_string())
        .with_conflict_keys(conflict_keys);
        spec.detail = detail;
        match self.task_runtime.register_external(spec)? {
            ExternalRegistrationOutcome::Started(snapshot) => Ok(self
                .task_runtime
                .activate_external(task_id, snapshot.detail)
                .map(ExternalRegistrationOutcome::Started)?),
            outcome @ ExternalRegistrationOutcome::Existing(_)
            | outcome @ ExternalRegistrationOutcome::Conflict(_) => Ok(outcome),
        }
    }

    fn register_projection<T: Serialize>(
        &self,
        kind: TaskKind,
        task_id: &str,
        dedup_key: Option<String>,
        conflict_keys: impl IntoIterator<Item = String>,
        projection: &T,
    ) -> AppResult<ExternalRegistrationOutcome> {
        self.register_projection_for_tenant(
            None,
            kind,
            task_id,
            dedup_key,
            conflict_keys,
            projection,
        )
    }

    fn register_projection_for_tenant<T: Serialize>(
        &self,
        tenant_id: Option<&str>,
        kind: TaskKind,
        task_id: &str,
        dedup_key: Option<String>,
        conflict_keys: impl IntoIterator<Item = String>,
        projection: &T,
    ) -> AppResult<ExternalRegistrationOutcome> {
        self.register_projection_for_tenant_with_meta(
            tenant_id,
            kind,
            task_id,
            None,
            None,
            dedup_key,
            conflict_keys,
            projection,
        )
    }

    fn register_projection_for_tenant_with_meta<T: Serialize>(
        &self,
        tenant_id: Option<&str>,
        kind: TaskKind,
        task_id: &str,
        title: Option<String>,
        category: Option<String>,
        dedup_key: Option<String>,
        conflict_keys: impl IntoIterator<Item = String>,
        projection: &T,
    ) -> AppResult<ExternalRegistrationOutcome> {
        let detail = serde_json::to_value(projection)
            .map_err(|error| crate::backend::application::AppError::External(error.to_string()))?;
        let mut spec = match tenant_id {
            Some(tenant_id) => TaskSpec::new(kind, dedup_key).with_tenant_id(tenant_id),
            None => TaskSpec::global(kind, dedup_key),
        }
        .with_task_id(task_id.to_string())
        .with_conflict_keys(conflict_keys);
        if let Some(t) = title {
            spec = spec.with_title(t);
        }
        if let Some(c) = category {
            spec = spec.with_category(c);
        }
        spec.detail = detail;
        match self.task_runtime.register_external(spec)? {
            ExternalRegistrationOutcome::Started(snapshot) => Ok(self
                .task_runtime
                .activate_external(task_id, snapshot.detail)
                .map(ExternalRegistrationOutcome::Started)?),
            outcome @ ExternalRegistrationOutcome::Existing(_)
            | outcome @ ExternalRegistrationOutcome::Conflict(_) => Ok(outcome),
        }
    }

    fn finish_external_task(
        &self,
        task_id: &str,
        result: AppResult<Value>,
    ) -> AppResult<TaskSnapshot> {
        let result = result.map_err(AppErrorView::from);
        Ok(self.task_runtime.complete_external(task_id, result)?)
    }

    fn finish_external_result(
        &self,
        task_id: &str,
        result: crate::backend::application::AppResult<Value>,
    ) -> AppResult<TaskSnapshot> {
        let result = result.map_err(AppErrorView::from);
        Ok(self.task_runtime.complete_external(task_id, result)?)
    }

    fn external_task_snapshot(&self, task_id: &str) -> AppResult<TaskSnapshot> {
        self.task_runtime.get(task_id).ok_or_else(|| {
            crate::backend::application::AppError::NotFound(format!(
                "background task not found: {task_id}"
            ))
        })
    }

    fn external_task_snapshot_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<TaskSnapshot> {
        self.task_runtime
            .get_for_tenant(tenant_id, task_id)
            .ok_or_else(|| {
                crate::backend::application::AppError::NotFound(format!(
                    "background task not found: {task_id}"
                ))
            })
    }

    fn decode<T: DeserializeOwned>(&self, runtime: &TaskSnapshot) -> AppResult<T> {
        serde_json::from_value(runtime.detail.clone()).map_err(|error| {
            crate::backend::application::AppError::External(format!(
                "task projection {} could not be decoded: {error}",
                runtime.task_id
            ))
        })
    }

    fn projection<T: BackgroundTaskProjection>(&self, task_id: &str) -> AppResult<T> {
        let runtime = self.external_task_snapshot(task_id)?;
        Ok(self.decode::<T>(&runtime)?.project_with_runtime(&runtime))
    }

    fn projection_for_tenant<T: BackgroundTaskProjection>(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<T> {
        let runtime = self.external_task_snapshot_for_tenant(tenant_id, task_id)?;
        Ok(self.decode::<T>(&runtime)?.project_with_runtime(&runtime))
    }

    fn projection_from_runtime<T: BackgroundTaskProjection>(
        &self,
        runtime: &TaskSnapshot,
    ) -> AppResult<T> {
        Ok(self.decode::<T>(runtime)?.project_with_runtime(runtime))
    }

    fn write_projection<T: Serialize>(&self, task_id: &str, projection: &T) -> AppResult<()> {
        let detail = serde_json::to_value(projection)
            .map_err(|error| crate::backend::application::AppError::External(error.to_string()))?;
        Ok(self
            .task_runtime
            .update_detail(task_id, detail)
            .map(|_| ())?)
    }

    fn list_projections<T: BackgroundTaskProjection>(&self, kind: TaskKind) -> AppResult<Vec<T>> {
        self.task_runtime
            .list(crate::backend::infrastructure::tasks::TaskFilter {
                kind: Some(kind),
                active_only: false,
                ..Default::default()
            })
            .into_iter()
            .map(|runtime| self.projection_from_runtime(&runtime))
            .collect()
    }

    fn list_projections_for_tenant<T: BackgroundTaskProjection>(
        &self,
        tenant_id: &str,
        kind: TaskKind,
    ) -> AppResult<Vec<T>> {
        self.task_runtime
            .list_for_tenant(
                tenant_id,
                crate::backend::infrastructure::tasks::TaskFilter {
                    kind: Some(kind),
                    active_only: false,
                    ..Default::default()
                },
            )
            .into_iter()
            .map(|runtime| self.projection_from_runtime(&runtime))
            .collect()
    }

    fn cancel_external_task(&self, task_id: &str) -> AppResult<TaskSnapshot> {
        match self.task_runtime.cancel(task_id) {
            crate::backend::infrastructure::tasks::CancelOutcome::Requested(snapshot)
            | crate::backend::infrastructure::tasks::CancelOutcome::AlreadyFinished(snapshot) => {
                Ok(snapshot)
            }
            crate::backend::infrastructure::tasks::CancelOutcome::NotFound => {
                Err(crate::backend::application::AppError::NotFound(format!(
                    "background task not found: {task_id}"
                )))
            }
        }
    }

    fn cancel_external_task_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<TaskSnapshot> {
        match self.task_runtime.cancel_for_tenant(tenant_id, task_id) {
            crate::backend::infrastructure::tasks::CancelOutcome::Requested(snapshot)
            | crate::backend::infrastructure::tasks::CancelOutcome::AlreadyFinished(snapshot) => {
                Ok(snapshot)
            }
            crate::backend::infrastructure::tasks::CancelOutcome::NotFound => {
                Err(crate::backend::application::AppError::NotFound(format!(
                    "background task not found: {task_id}"
                )))
            }
        }
    }

    pub(crate) fn spawn_extension_lifecycle(
        &self,
        task_id: &str,
        task: TaskFn,
    ) -> crate::backend::application::AppResult<TaskSnapshot> {
        let detail = self
            .task_runtime
            .get(task_id)
            .map(|snapshot| snapshot.detail)
            .unwrap_or(Value::Null);
        Ok(self.lifecycle.spawn(task_id, detail, task)?)
    }

    pub(crate) fn has_running_tasks(&self) -> bool {
        self.task_runtime.has_active_tasks()
    }
}

pub(crate) mod agents;
pub(crate) mod catalog;
pub(crate) mod conversations;
pub(crate) mod mounting;

pub(crate) use agents::*;
pub(crate) use catalog::*;
pub(crate) use conversations::*;
pub(crate) use mounting::*;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
