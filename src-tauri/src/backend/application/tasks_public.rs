use super::prelude::*;
use crate::backend::{
    dto::{
        TaskCancelParams, TaskClearParams, TaskGetParams, TaskListParams, TaskRetryParams, TaskView,
    },
    runtime::tasks::{CancelOutcome, TaskFilter},
};

impl AppService {
    pub(crate) fn list_public_tasks(&self, params: TaskListParams) -> AppResult<Vec<TaskView>> {
        let task_runtime = self.runtime.task_runtime();
        let current_tenant = self.tenant_id();
        let all_tenants = params.all_tenants.unwrap_or(false);
        let active_only = params.active_only.unwrap_or(false);

        let filter = TaskFilter {
            kind: None,
            active_only,
            user_visible_only: true,
        };

        let mut snapshots = if all_tenants {
            task_runtime.list(filter)
        } else {
            let target_tenant = params.tenant_id.as_deref().unwrap_or(current_tenant);
            task_runtime.list_for_tenant(target_tenant, filter)
        };

        // Sort: current tenant first, then active tasks first, then by started_at descending (most recent first)
        snapshots.sort_by(|a, b| {
            let a_is_current = a.tenant_id.as_deref() == Some(current_tenant);
            let b_is_current = b.tenant_id.as_deref() == Some(current_tenant);
            b_is_current
                .cmp(&a_is_current)
                .then_with(|| b.state.is_active().cmp(&a.state.is_active()))
                .then_with(|| b.started_at.cmp(&a.started_at))
        });

        Ok(snapshots.iter().map(TaskView::from_snapshot).collect())
    }

    pub(crate) fn get_public_task(&self, params: TaskGetParams) -> AppResult<Option<TaskView>> {
        let task_runtime = self.runtime.task_runtime();
        let snapshot = task_runtime.get(&params.task_id);
        Ok(snapshot
            .filter(|s| s.user_visible)
            .as_ref()
            .map(TaskView::from_snapshot))
    }

    pub(crate) fn cancel_public_task(&self, params: TaskCancelParams) -> AppResult<TaskView> {
        let task_runtime = self.runtime.task_runtime();
        let snapshot = task_runtime
            .get(&params.task_id)
            .ok_or_else(|| AppError::NotFound(format!("任务不存在: {}", params.task_id)))?;

        if !snapshot.capabilities.cancellable {
            return Err(AppError::Validation("该任务不支持取消".to_string()));
        }

        match task_runtime.cancel(&params.task_id) {
            CancelOutcome::Requested(snapshot) | CancelOutcome::AlreadyFinished(snapshot) => {
                Ok(TaskView::from_snapshot(&snapshot))
            }
            CancelOutcome::NotFound => Err(AppError::NotFound(format!(
                "任务不存在: {}",
                params.task_id
            ))),
        }
    }

    pub(crate) async fn retry_public_task(&self, params: TaskRetryParams) -> AppResult<TaskView> {
        let task_runtime = self.runtime.task_runtime();
        let snapshot = task_runtime
            .get(&params.task_id)
            .ok_or_else(|| AppError::NotFound(format!("任务不存在: {}", params.task_id)))?;

        if !snapshot.capabilities.retryable {
            return Err(AppError::Validation("该任务不支持重试".to_string()));
        }

        if snapshot.kind == crate::backend::runtime::tasks::TaskKind::Memory {
            self.retry_memory_task(crate::backend::application::MemoryTaskRetryParams {
                task_id: params.task_id.clone(),
            })
            .await?;

            if let Some(new_snapshot) = task_runtime.get(&params.task_id) {
                return Ok(TaskView::from_snapshot(&new_snapshot));
            }
            let mut retried_snapshot = snapshot;
            retried_snapshot.state = crate::backend::runtime::tasks::TaskState::Pending;
            retried_snapshot.error = None;
            retried_snapshot.outcome = None;
            return Ok(TaskView::from_snapshot(&retried_snapshot));
        }

        Err(AppError::Validation("该任务当前不支持重试".to_string()))
    }

    pub(crate) fn clear_terminal_tasks(&self, params: TaskClearParams) -> AppResult<usize> {
        let task_runtime = self.runtime.task_runtime();
        if let Some(task_id) = &params.task_id {
            if let Some(snapshot) = task_runtime.get(task_id) {
                if !snapshot.state.is_terminal() {
                    return Err(AppError::Validation("不能清除尚未结束的任务".to_string()));
                }
                let removed = task_runtime.remove_terminal(task_id);
                return Ok(if removed.is_some() { 1 } else { 0 });
            }
            return Ok(0);
        }

        let tenant_id = if params.tenant_id.as_deref() == Some("*") {
            None
        } else {
            params.tenant_id.as_deref().or(Some(self.tenant_id()))
        };

        Ok(task_runtime.clear_terminal(tenant_id))
    }
}
