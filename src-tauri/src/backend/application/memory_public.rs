use super::prelude::*;
use crate::backend::{
    dto::{MemoryProjectView, MemoryRebuildResult, MemoryTaskView},
    runtime::tasks::{CancelOutcome, TaskFilter, TaskKind, TaskState},
    store,
};

impl AppService {
    pub(crate) fn get_memory_project(
        &self,
        params: MemoryProjectGetParams,
    ) -> AppResult<Option<MemoryProjectView>> {
        let project_path = self
            .resolve_context_project_path(Some(&params.project_path))?
            .ok_or_else(|| AppError::Validation("project_path is required".to_string()))?;
        let tenant_id = self.tenant_id().to_string();
        let project = self.runtime.run_sync(store::load_project_memory_sqlx(
            self.db.pool(),
            &tenant_id,
            &project_path,
        ))?;
        let Some(project) = project else {
            return Ok(None);
        };
        let version = self
            .runtime
            .run_sync(store::load_project_memory_latest_version_sqlx(
                self.db.pool(),
                &tenant_id,
                &project.id,
            ))?;
        let sources = match version.as_ref() {
            Some(version) => self
                .runtime
                .run_sync(store::load_project_memory_sources_sqlx(
                    self.db.pool(),
                    &tenant_id,
                    &version.id,
                ))?,
            None => Vec::new(),
        };
        Ok(Some(MemoryProjectView {
            project,
            version,
            sources,
        }))
    }

    pub(crate) fn rebuild_memory_scope(
        &self,
        params: MemoryScopeRebuildParams,
    ) -> AppResult<MemoryRebuildResult> {
        if !crate::backend::app_settings::memory_generation_enabled_for_database(&self.db)? {
            return Ok(MemoryRebuildResult {
                scope: params.scope,
                queued: false,
                scheduled_tasks: 0,
            });
        }
        if params.scope.project_path.is_none()
            && (params.scope.app_id.is_some()
                || params.scope.source_id.is_some()
                || params.scope.session_id.is_some())
        {
            return Err(AppError::Validation(
                "scope rebuild requires project_path when a narrow scope is provided".to_string(),
            ));
        }
        let project_path = params
            .scope
            .project_path
            .as_deref()
            .map(|path| self.resolve_context_project_path(Some(path)))
            .transpose()?
            .flatten();
        let tenant_id = self.tenant_id().to_string();
        let now = Utc::now();
        let queued = self.runtime.run_sync(async {
            let mut tx = self.db.pool().begin().await.map_err(AppError::Db)?;
            let queued = if let Some(project_path) = project_path.as_deref() {
                store::enqueue_project_memory_job_tx(
                    &mut tx,
                    &tenant_id,
                    project_path,
                    &now.to_rfc3339(),
                )
                .await?
                .is_some()
            } else {
                store::enqueue_global_memory_job_tx(&mut tx, &tenant_id, &now.to_rfc3339())
                    .await?
                    .is_some()
            };
            tx.commit().await.map_err(AppError::Db)?;
            Ok::<_, AppError>(queued)
        })?;
        let scheduled_tasks = if project_path.is_some() {
            self.reconcile_project_memory_jobs_for_tenant_at(&tenant_id, now)?
        } else {
            self.reconcile_global_memory_jobs_for_tenant_at(&tenant_id, now)?
        };
        Ok(MemoryRebuildResult {
            scope: MemoryScope {
                project_path,
                ..params.scope
            },
            queued,
            scheduled_tasks,
        })
    }

    pub(crate) fn list_memory_task_views(
        &self,
        params: MemoryTaskListParams,
    ) -> AppResult<Vec<MemoryTaskView>> {
        let snapshots = self.runtime.task_runtime().list_for_tenant(
            self.tenant_id(),
            TaskFilter {
                kind: Some(TaskKind::Memory),
                active_only: params.active_only,
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

    pub(crate) fn retry_memory_task(
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
        let job_id = snapshot
            .detail
            .get("job_id")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Validation("Memory task has no durable job id".to_string()))?;
        let tenant_id = self.tenant_id().to_string();
        let changed = match domain {
            "session_memory" => self.runtime.run_sync(store::retry_session_memory_job_sqlx(
                self.db.pool(),
                &tenant_id,
                job_id,
            ))?,
            "project_memory" => self.runtime.run_sync(store::retry_project_memory_job_sqlx(
                self.db.pool(),
                &tenant_id,
                job_id,
            ))?,
            "global_memory" => self.runtime.run_sync(store::retry_global_memory_job_sqlx(
                self.db.pool(),
                &tenant_id,
                job_id,
            ))?,
            "memory_recall" => self.runtime.run_sync(store::retry_memory_recall_turn_sqlx(
                self.db.pool(),
                &tenant_id,
                job_id,
            ))?,
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
            "session_memory" => {
                self.reconcile_session_memory_jobs_for_tenant_at(&tenant_id, now)?;
            }
            "project_memory" => {
                self.reconcile_project_memory_jobs_for_tenant_at(&tenant_id, now)?;
            }
            "global_memory" => {
                self.reconcile_global_memory_jobs_for_tenant_at(&tenant_id, now)?;
            }
            "memory_recall" => {
                self.schedule_memory_recall_turn_for_tenant(&tenant_id, job_id)?;
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

fn memory_task_view(
    snapshot: crate::backend::runtime::tasks::TaskSnapshot,
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
