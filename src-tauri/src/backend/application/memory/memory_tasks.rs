use super::types::MemoryTaskView;
use crate::backend::application::prelude::*;
use crate::backend::{
    infrastructure::tasks::{CancelOutcome, TaskFilter, TaskKind, TaskState},
    store,
};
use chrono::Utc;

impl AppService {
    pub(crate) fn list_memory_task_views(
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

    pub(crate) fn get_memory_task_view(
        &self,
        params: MemoryTaskGetParams,
    ) -> AppResult<Option<MemoryTaskView>> {
        let snapshot = self
            .runtime
            .task_runtime()
            .get_for_tenant(self.tenant_id(), &params.task_id);
        snapshot.map(memory_task_view).transpose()
    }

    pub(crate) fn cancel_memory_task_view(
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

    pub(crate) async fn retry_memory_task(
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
        let job_id = snapshot.detail.get("job_id").and_then(Value::as_str);
        let maintenance_job_id = snapshot
            .detail
            .get("maintenance_job_id")
            .and_then(Value::as_str);
        if matches!(domain, "project_memory" | "global_memory") && maintenance_job_id.is_some() {
            let maintenance_job_id = maintenance_job_id.expect("checked above");
            let changed = store::retry_memory_maintenance_job_sqlx(
                self.db.pool(),
                &tenant_id,
                maintenance_job_id,
                &Utc::now().to_rfc3339(),
            )
            .await?;
            if !changed {
                return Err(AppError::Conflict(
                    "Memory maintenance task is not in a retryable durable state".to_string(),
                ));
            }
            let _ = self.runtime.task_runtime().remove_terminal(&params.task_id);
            self.reconcile_memory_maintenance_jobs_for_tenant_at(&tenant_id, Utc::now())
                .await?;
            return self
                .get_memory_task_view(MemoryTaskGetParams {
                    task_id: params.task_id,
                })?
                .ok_or_else(|| AppError::NotFound("Retried Memory task was pruned".to_string()));
        }
        if matches!(
            domain,
            "project_memory" | "global_memory" | "memory_projection"
        ) {
            let _ = self.runtime.task_runtime().remove_terminal(&params.task_id);
            let now = Utc::now();
            let scheduled_task_ids = match domain {
                "project_memory" => {
                    let project_path = snapshot
                        .detail
                        .get("project_path")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            AppError::Validation(
                                "Project Memory task has no project path".to_string(),
                            )
                        })?;
                    self.schedule_project_memory_rebuild(project_path, now)
                        .await?
                }
                "global_memory" => self.schedule_global_memory_rebuild(now).await?,
                "memory_projection" => self.schedule_memory_projection_rebuild().await?,
                _ => Vec::new(),
            };
            if scheduled_task_ids.is_empty() {
                return Err(AppError::Conflict(
                    "Memory task could not be scheduled again".to_string(),
                ));
            }
            return self
                .get_memory_task_view(MemoryTaskGetParams {
                    task_id: params.task_id,
                })?
                .ok_or_else(|| AppError::NotFound("Retried Memory task was pruned".to_string()));
        }
        let job_id = job_id
            .ok_or_else(|| AppError::Validation("Memory task has no durable job id".to_string()))?;
        let changed = match domain {
            "recent_snapshot" => {
                store::retry_recent_memory_job_sqlx(
                    self.db.pool(),
                    &tenant_id,
                    job_id,
                    &Utc::now().to_rfc3339(),
                )
                .await?
            }
            "session_memory" => {
                store::retry_session_memory_job_sqlx(self.db.pool(), &tenant_id, job_id).await?
            }
            "memory_recall" => {
                store::retry_memory_recall_turn_sqlx(self.db.pool(), &tenant_id, job_id).await?
            }
            _ => false,
        };
        if !changed {
            return Err(AppError::Conflict(
                "Memory task is not in a retryable durable state".to_string(),
            ));
        }
        let _ = self.runtime.task_runtime().remove_terminal(&params.task_id);
        let now = Utc::now();
        match domain {
            "recent_snapshot" => {
                self.reconcile_recent_memory_jobs_for_tenant_at(&tenant_id, now)
                    .await?;
            }
            "session_memory" => {
                self.reconcile_session_memory_jobs_for_tenant_at(&tenant_id, now)
                    .await?;
            }
            "memory_recall" => {
                self.schedule_memory_recall_turn_for_tenant(&tenant_id, job_id)
                    .await?;
            }
            _ => {}
        }
        self.find_memory_task_by_durable_job(&tenant_id, domain, job_id)
            .ok_or_else(|| AppError::NotFound("Retried Memory task was pruned".to_string()))
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

pub(crate) fn memory_task_view(
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
