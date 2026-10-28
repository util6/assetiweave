use super::types::MemoryTaskView;
use crate::backend::application::prelude::*;
use crate::backend::{
    infrastructure::tasks::{CancelOutcome, TaskFilter, TaskKind, TaskState},
    store,
};

impl AppService {
    pub fn list_memory_task_views(
        &self,
        params: MemoryTaskListParams,
    ) -> AppResult<Vec<MemoryTaskView>> {
        let snapshots = self.runtime.task_runtime().list_for_tenant(
            self.tenant_id(),
            TaskFilter {
                kind: Some(TaskKind::Memory),
                active_only: params.active_only,
                ..Default::default()
            },
        );
        snapshots.into_iter().map(memory_task_view).collect()
    }

    pub fn get_memory_task_view(
        &self,
        params: MemoryTaskGetParams,
    ) -> AppResult<Option<MemoryTaskView>> {
        let snapshot = self
            .runtime
            .task_runtime()
            .get_for_tenant(self.tenant_id(), &params.task_id);
        snapshot.map(memory_task_view).transpose()
    }

    pub fn cancel_memory_task_view(
        &self,
        params: MemoryTaskGetParams,
    ) -> AppResult<MemoryTaskView> {
        match self
            .runtime
            .task_runtime()
            .cancel_for_tenant(self.tenant_id(), &params.task_id)
        {
            CancelOutcome::Requested(snapshot) | CancelOutcome::AlreadyFinished(snapshot) => {
                memory_task_view(snapshot)
            }
            CancelOutcome::NotFound => Err(AppError::NotFound(format!(
                "Memory task not found: {}",
                params.task_id
            ))),
        }
    }

    pub async fn retry_memory_task(
        &self,
        params: MemoryTaskRetryParams,
    ) -> AppResult<MemoryTaskView> {
        let snapshot = self
            .runtime
            .task_runtime()
            .get_for_tenant(self.tenant_id(), &params.task_id)
            .ok_or_else(|| {
                AppError::NotFound(format!("Memory task not found: {}", params.task_id))
            })?;
        if snapshot.state.is_active() {
            return Err(AppError::Conflict(
                "Memory task is still active".to_string(),
            ));
        }
        let domain = snapshot
            .detail
            .get("domain")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                AppError::Validation("Memory task has no retryable domain".to_string())
            })?;
        let tenant_id = self.tenant_id().to_string();
        if domain == "memory_pipeline" || params.task_id.starts_with("memory-pipeline:") {
            let _ = self.runtime.task_runtime().remove_terminal(&params.task_id);
            let _ = self
                .start_memory_pipeline(super::memory_pipeline::MemoryPipelineParams::rebuild(
                    MemoryRebuildTarget::All,
                    None,
                    Some(MemoryRebuildReason::Manual),
                ))
                .await?;
            return self
                .get_memory_task_view(MemoryTaskGetParams {
                    task_id: params.task_id,
                })?
                .ok_or_else(|| AppError::NotFound("Retried Memory task was pruned".to_string()));
        }
        if domain == "memory_recall" {
            let job_id = snapshot
                .detail
                .get("job_id")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    AppError::Validation("Memory recall task has no durable job id".to_string())
                })?;
            let changed =
                store::retry_memory_recall_turn_sqlx(self.db.pool(), &tenant_id, job_id).await?;
            if !changed {
                return Err(AppError::Conflict(
                    "Memory recall task is not in a retryable durable state".to_string(),
                ));
            }
            let _ = self.runtime.task_runtime().remove_terminal(&params.task_id);
            self.schedule_memory_recall_turn_for_tenant(&tenant_id, job_id)
                .await?;
            return self
                .find_memory_task_by_durable_job(&tenant_id, domain, job_id)
                .ok_or_else(|| AppError::NotFound("Retried Memory task was pruned".to_string()));
        }
        Err(AppError::Validation(format!(
            "Memory domain '{}' does not support retry",
            domain
        )))
    }

    pub async fn wait_for_memory_task(&self, task_id: &str) -> AppResult<Option<MemoryTaskView>> {
        let task_runtime = self.runtime.task_runtime();
        let mut rx = task_runtime.subscribe();
        if let Some(snap) = task_runtime.get_for_tenant(self.tenant_id(), task_id) {
            if snap.state.is_terminal() {
                return Ok(Some(memory_task_view(snap)?));
            }
        }
        while let Ok(snap) = rx.recv().await {
            if snap.task_id == task_id && snap.state.is_terminal() {
                return Ok(Some(memory_task_view(snap)?));
            }
        }
        self.get_memory_task_view(MemoryTaskGetParams {
            task_id: task_id.to_string(),
        })
    }

    fn find_memory_task_by_durable_job(
        &self,
        tenant_id: &str,
        domain: &str,
        job_id: &str,
    ) -> Option<MemoryTaskView> {
        self.runtime
            .task_runtime()
            .list_for_tenant(
                tenant_id,
                TaskFilter {
                    kind: Some(TaskKind::Memory),
                    active_only: false,
                    ..Default::default()
                },
            )
            .into_iter()
            .filter(|snapshot| {
                snapshot.detail.get("domain").and_then(Value::as_str) == Some(domain)
                    && snapshot.detail.get("job_id").and_then(Value::as_str) == Some(job_id)
            })
            .max_by(|left, right| {
                left.started_at
                    .cmp(&right.started_at)
                    .then_with(|| left.task_id.cmp(&right.task_id))
            })
            .and_then(|snapshot| memory_task_view(snapshot).ok())
    }
}

pub fn memory_task_view(
    snapshot: crate::backend::infrastructure::tasks::TaskSnapshot,
) -> AppResult<MemoryTaskView> {
    let status = match snapshot.state {
        TaskState::Pending => "pending",
        TaskState::Running => "running",
        TaskState::Cancelling => "cancelling",
        TaskState::Succeeded => "succeeded",
        TaskState::Failed => "failed",
        TaskState::Canceled => "cancelled",
    };
    let kind = match snapshot.kind {
        TaskKind::Memory => "memory",
        _ => return Err(AppError::Validation("not a Memory task".to_string())),
    };
    Ok(MemoryTaskView {
        id: snapshot.task_id,
        status: status.to_string(),
        kind: kind.to_string(),
        progress: snapshot.progress,
        started_at: snapshot.started_at,
        finished_at: snapshot.finished_at,
        result: snapshot.result,
        error: snapshot.error,
        detail: snapshot.detail,
    })
}
