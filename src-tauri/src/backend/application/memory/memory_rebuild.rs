use super::types::MemoryRebuildResult;
use crate::backend::application::prelude::*;
use crate::backend::store;
use chrono::Utc;

impl AppService {
    pub(crate) async fn rebuild_memory_scope(
        &self,
        params: MemoryScopeRebuildParams,
    ) -> AppResult<MemoryRebuildResult> {
        if !self.backend_settings()?.is_memory_generation_enabled() {
            return Ok(MemoryRebuildResult {
                accepted: false,
                scheduled_task_ids: Vec::new(),
                target_watermark: None,
                reused: false,
            });
        }
        let now = Utc::now();
        let project_path = params
            .project_path
            .as_deref()
            .or(params.scope.project_path.as_deref());
        let target = params.target.unwrap_or_else(|| {
            if project_path.is_some() {
                MemoryRebuildTarget::Project
            } else {
                MemoryRebuildTarget::Recent
            }
        });
        let narrow_scope = params.scope.app_id.is_some()
            || params.scope.source_id.is_some()
            || params.scope.session_id.is_some();
        if narrow_scope {
            return Err(AppError::Validation(
                "Memory rebuild does not support app/source/session scopes".to_string(),
            ));
        }
        if matches!(params.reason, Some(MemoryRebuildReason::ProjectionRepair)) {
            let scheduled_task_ids = self.schedule_memory_projection_rebuild().await?;
            return Ok(MemoryRebuildResult {
                accepted: true,
                scheduled_task_ids,
                target_watermark: None,
                reused: false,
            });
        }

        let (scheduled_task_ids, target_watermark) = match target {
            MemoryRebuildTarget::Recent => {
                if project_path.is_some() {
                    return Err(AppError::Validation(
                        "Recent rebuild does not accept project_path; use target=project"
                            .to_string(),
                    ));
                }
                let phase1_target = self
                    .ensure_recent_snapshot_phase1_jobs_at(now, true)
                    .await?
                    .map(|(target, _)| target);
                self.reconcile_session_memory_jobs_for_tenant_at(self.tenant_id(), now)
                    .await?;

                let preparation = self
                    .prepare_recent_snapshot_generation_with_options(Some(now), true)
                    .await?;
                let target_watermark = phase1_target
                    .map(|target| target.target_watermark_utc.to_rfc3339())
                    .or_else(|| {
                        preparation
                            .as_ref()
                            .map(|value| value.target.target_watermark_utc.to_rfc3339())
                    });
                let mut scheduled_task_ids = Vec::new();
                if let Some(preparation) = preparation {
                    let job_id = self
                        .enqueue_recent_snapshot_generation(&preparation, now)
                        .await?;
                    let _ = store::restart_recent_memory_job_for_rebuild_sqlx(
                        self.db.pool(),
                        self.tenant_id(),
                        &job_id,
                        &now.to_rfc3339(),
                    )
                    .await?;
                    scheduled_task_ids.push(format!("memory-recent-{job_id}"));
                }
                self.reconcile_recent_memory_jobs_for_tenant_at(self.tenant_id(), now)
                    .await?;
                if scheduled_task_ids.is_empty() {
                    if let Ok(active_memory_tasks) =
                        self.list_memory_task_views(MemoryTaskListParams { active_only: true })
                    {
                        for task in active_memory_tasks {
                            if task.status == "running" || task.status == "pending" {
                                scheduled_task_ids.push(task.id);
                            }
                        }
                    }
                }
                (scheduled_task_ids, target_watermark)
            }
            MemoryRebuildTarget::Project => {
                let path = project_path.ok_or_else(|| {
                    AppError::Validation("Project rebuild requires project_path".to_string())
                })?;
                (self.schedule_project_memory_rebuild(path, now).await?, None)
            }
            MemoryRebuildTarget::Global => {
                if project_path.is_some() {
                    return Err(AppError::Validation(
                        "Global rebuild does not accept project_path".to_string(),
                    ));
                }
                (self.schedule_global_memory_rebuild(now).await?, None)
            }
            MemoryRebuildTarget::All => {
                if project_path.is_some() {
                    return Err(AppError::Validation(
                        "All rebuild does not accept project_path".to_string(),
                    ));
                }
                let phase1_target = self
                    .ensure_recent_snapshot_phase1_jobs_at(now, true)
                    .await?
                    .map(|(target, _)| target);
                self.reconcile_session_memory_jobs_for_tenant_at(self.tenant_id(), now)
                    .await?;

                let preparation = self
                    .prepare_recent_snapshot_generation_with_options(Some(now), true)
                    .await?;
                let target_watermark = phase1_target
                    .map(|target| target.target_watermark_utc.to_rfc3339())
                    .or_else(|| {
                        preparation
                            .as_ref()
                            .map(|value| value.target.target_watermark_utc.to_rfc3339())
                    });
                let mut scheduled_task_ids = Vec::new();
                if let Some(preparation) = preparation {
                    let job_id = self
                        .enqueue_recent_snapshot_generation(&preparation, now)
                        .await?;
                    let _ = store::restart_recent_memory_job_for_rebuild_sqlx(
                        self.db.pool(),
                        self.tenant_id(),
                        &job_id,
                        &now.to_rfc3339(),
                    )
                    .await?;
                    scheduled_task_ids.push(format!("memory-recent-{job_id}"));
                }
                self.reconcile_recent_memory_jobs_for_tenant_at(self.tenant_id(), now)
                    .await?;
                for project_path in
                    store::list_memory_project_paths_sqlx(self.db.pool(), self.tenant_id()).await?
                {
                    scheduled_task_ids.extend(
                        self.schedule_project_memory_rebuild(&project_path, now)
                            .await?,
                    );
                }
                scheduled_task_ids.extend(self.schedule_global_memory_rebuild(now).await?);
                (scheduled_task_ids, target_watermark)
            }
        };
        let mut scheduled_task_ids = scheduled_task_ids;
        scheduled_task_ids.sort();
        scheduled_task_ids.dedup();
        Ok(MemoryRebuildResult {
            accepted: true,
            scheduled_task_ids,
            target_watermark,
            reused: false,
        })
    }
}
