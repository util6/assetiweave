use super::prelude::*;
use crate::backend::{
    dto::{MemoryProjectView, MemoryRebuildResult, MemoryTaskView},
    runtime::tasks::{CancelOutcome, TaskFilter, TaskKind, TaskState},
    store,
};

impl AppService {
    pub(crate) async fn get_memory_project(
        &self,
        params: MemoryProjectGetParams,
    ) -> AppResult<Option<MemoryProjectView>> {
        let project_path = self
            .resolve_context_project_path(Some(&params.project_path))
            .await?
            .ok_or_else(|| AppError::Validation("project_path is required".to_string()))?;
        let tenant_id = self.tenant_id().to_string();
        let project =
            store::load_project_memory_sqlx(self.db.pool(), &tenant_id, &project_path).await?;
        let Some(project) = project else {
            return Ok(None);
        };
        let version =
            store::load_project_memory_latest_version_sqlx(self.db.pool(), &tenant_id, &project.id)
                .await?;
        let sources = match version.as_ref() {
            Some(version) => {
                store::load_project_memory_sources_sqlx(self.db.pool(), &tenant_id, &version.id)
                    .await?
            }
            None => Vec::new(),
        };
        Ok(Some(MemoryProjectView {
            project,
            version,
            sources,
        }))
    }

    /// 获取当前项目的 L2 长期记忆视图 (M35-L2-01 ~ M35-L2-06)
    pub(crate) async fn get_project_memory_l2(
        &self,
        project_path: &str,
    ) -> AppResult<Option<crate::backend::models::L2ProjectMemoryView>> {
        let normalized_path = self
            .resolve_context_project_path(Some(project_path))
            .await?
            .unwrap_or_else(|| project_path.to_string());
        let tenant_id = self.tenant_id();
        crate::backend::application::project_consolidation_pipeline::load_l2_project_memory_view(
            self.db.pool(),
            tenant_id,
            &normalized_path,
        )
        .await
    }

    /// 协调并执行指定项目的 L2 Consolidation
    pub(crate) async fn reconcile_project_consolidation(
        &self,
        project_key: &str,
        project_path: Option<&str>,
    ) -> AppResult<Option<crate::backend::models::L2ProjectMemoryView>> {
        let lock_map = crate::backend::application::project_consolidation_pipeline::global_project_consolidation_lock_map();
        crate::backend::application::project_consolidation_pipeline::reconcile_project_consolidation_default(
            self.db.pool(),
            self.tenant_id(),
            project_key,
            project_path,
            lock_map,
        )
        .await
    }

    /// 获取当前 Tenant 的 L3 全局长期记忆视图 (M35-L3-01 ~ M35-L3-06)
    pub(crate) async fn get_global_memory_l3(
        &self,
    ) -> AppResult<Option<crate::backend::models::L3GlobalMemoryView>> {
        crate::backend::application::global_consolidation_pipeline::get_global_memory_l3_view(
            self.db.pool(),
            self.tenant_id(),
        )
        .await
    }

    /// 协调并执行当前 Tenant 的 Global Consolidation (M35-L3-01 ~ M35-L3-06)
    pub(crate) async fn reconcile_global_consolidation(
        &self,
        now: chrono::DateTime<chrono::Utc>,
        is_manual_rebuild: bool,
    ) -> AppResult<Option<crate::backend::models::L3GlobalMemoryView>> {
        crate::backend::application::global_consolidation_pipeline::reconcile_global_consolidation(
            self.db.pool(),
            self.tenant_id(),
            now,
            is_manual_rebuild,
            None,
        )
        .await
    }

    /// 协调长期记忆来源失效 (M35-L3-04)
    pub(crate) async fn reconcile_memory_source_invalidation(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> AppResult<usize> {
        let settings = self.app_settings_value();
        let memory_settings = settings
            .get("memory")
            .and_then(|v| serde_json::from_value::<crate::backend::app_settings::MemorySettings>(v.clone()).ok())
            .unwrap_or_default();

        crate::backend::application::global_consolidation_pipeline::reconcile_source_invalidation(
            self.db.pool(),
            self.tenant_id(),
            now,
            &memory_settings.excluded_source_ids,
            &memory_settings.excluded_session_ids,
        )
        .await
    }

    /// 重建 Markdown 投影文件 (M35-PROJ-01 ~ M35-PROJ-03)
    pub(crate) async fn rebuild_markdown_projections(
        &self,
    ) -> AppResult<crate::backend::application::memory_projection_v2::MemoryProjectionPaths> {
        crate::backend::application::memory_projection_v2::rebuild_markdown_projections(
            self.db.pool(),
            self.tenant_id(),
            None,
        )
        .await
    }

    /// 清理过期的 Recent Snapshot 历史 (M35-PROJ-04，默认 30 天)
    pub(crate) async fn purge_expired_memory_snapshots(
        &self,
        retention_days: i64,
    ) -> AppResult<usize> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days);
        crate::backend::application::memory_projection_v2::purge_stale_recent_memory_snapshots(
            self.db.pool(),
            self.tenant_id(),
            cutoff,
        )
        .await
    }

    pub(crate) async fn rebuild_memory_scope(
        &self,
        params: MemoryScopeRebuildParams,
    ) -> AppResult<MemoryRebuildResult> {
        if !self.backend_settings()?.is_memory_generation_enabled() {
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
        let project_path = match params.scope.project_path.as_deref() {
            Some(path) => self.resolve_context_project_path(Some(path)).await?,
            None => None,
        };
        let tenant_id = self.tenant_id().to_string();
        let now = Utc::now();
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
        let scheduled_tasks = if project_path.is_some() {
            self.reconcile_project_memory_jobs_for_tenant_at(&tenant_id, now)
                .await?
        } else {
            self.reconcile_global_memory_jobs_for_tenant_at(&tenant_id, now)
                .await?
        };
        if scheduled_tasks == 0 {
            if let Some(path) = project_path.as_deref() {
                self.rebuild_project_memory_documents_for_tenant_at(&tenant_id, Some(path))
                    .await?;
            } else {
                self.rebuild_global_memory_documents_for_tenant_at(&tenant_id)
                    .await?;
            }
        }
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
        let job_id = snapshot
            .detail
            .get("job_id")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Validation("Memory task has no durable job id".to_string()))?;
        let tenant_id = self.tenant_id().to_string();
        let changed = match domain {
            "session_memory" => {
                store::retry_session_memory_job_sqlx(self.db.pool(), &tenant_id, job_id).await?
            }
            "project_memory" => {
                store::retry_project_memory_job_sqlx(self.db.pool(), &tenant_id, job_id).await?
            }
            "global_memory" => {
                store::retry_global_memory_job_sqlx(self.db.pool(), &tenant_id, job_id).await?
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
            "session_memory" => {
                self.reconcile_session_memory_jobs_for_tenant_at(&tenant_id, now)
                    .await?;
            }
            "project_memory" => {
                self.reconcile_project_memory_jobs_for_tenant_at(&tenant_id, now)
                    .await?;
            }
            "global_memory" => {
                self.reconcile_global_memory_jobs_for_tenant_at(&tenant_id, now)
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
