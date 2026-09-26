use crate::backend::application::{AppError, AppResult, AppService};
use crate::backend::{
    domain::{
        compute_global_consolidation_fingerprint, compute_project_consolidation_fingerprint,
        MemoryJobPurpose, MemoryMaintenanceWorkOrderPayload, MemoryWindow, MemoryWorkOrder,
        MemoryWorkOrderScope,
    },
    infrastructure::tasks::tasks::{TaskCapabilities, TaskKind, TaskSpec},
    store,
};
use chrono::{DateTime, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

const MAX_MEMORY_MAINTENANCE_CONCURRENCY: usize = 1;

impl AppService {
    pub(crate) async fn load_memory_source_revision_set_hash(
        &self,
        purpose: MemoryJobPurpose,
        project_key: Option<&str>,
        project_path: Option<&str>,
    ) -> AppResult<String> {
        match purpose {
            MemoryJobPurpose::ProjectConsolidation => {
                let project_key = project_key.ok_or_else(|| {
                    AppError::Validation(
                        "project maintenance job is missing project_key".to_string(),
                    )
                })?;
                let input = crate::backend::application::memory::project_consolidation_pipeline::
                    load_project_consolidation_input(
                        self.db.pool(),
                        self.tenant_id(),
                        project_key,
                        project_path,
                    )
                    .await?;
                Ok(compute_project_consolidation_fingerprint(&input))
            }
            MemoryJobPurpose::GlobalConsolidation => {
                let input = crate::backend::application::memory::global_consolidation_pipeline::
                    load_global_consolidation_input(self.db.pool(), self.tenant_id())
                    .await?;
                Ok(compute_global_consolidation_fingerprint(&input))
            }
            MemoryJobPurpose::RecentSnapshot => Err(AppError::Validation(
                "recent snapshot uses its dedicated evidence fingerprint".to_string(),
            )),
        }
    }

    pub(crate) async fn enqueue_memory_maintenance_job(
        &self,
        purpose: MemoryJobPurpose,
        project_key: Option<&str>,
        project_path: Option<&str>,
        now: DateTime<Utc>,
    ) -> AppResult<String> {
        let settings = self.backend_settings()?.memory.clone();
        settings.validate_schedule()?;
        let skill = self.get_active_generation_skill_binding().await?;
        let skill_text = self.load_active_generation_skill_text().await?;
        let target_watermark_utc = now.to_rfc3339();
        let hours = settings.recent_window_hours;
        let (project_input, global_input) = match purpose {
            MemoryJobPurpose::ProjectConsolidation => {
                let project_key = project_key.ok_or_else(|| {
                    AppError::Validation(
                        "project maintenance job is missing project_key".to_string(),
                    )
                })?;
                let input = crate::backend::application::memory::project_consolidation_pipeline::
                    load_project_consolidation_input(
                        self.db.pool(),
                        self.tenant_id(),
                        project_key,
                        project_path,
                    )
                    .await?;
                (Some(input), None)
            }
            MemoryJobPurpose::GlobalConsolidation => {
                let input = crate::backend::application::memory::global_consolidation_pipeline::
                    load_global_consolidation_input(self.db.pool(), self.tenant_id())
                    .await?;
                (None, Some(input))
            }
            MemoryJobPurpose::RecentSnapshot => {
                return Err(AppError::Validation(
                    "recent snapshot must use its dedicated durable queue".to_string(),
                ));
            }
        };
        let source_revision_set_hash = match (&project_input, &global_input) {
            (Some(input), None) => compute_project_consolidation_fingerprint(input),
            (None, Some(input)) => compute_global_consolidation_fingerprint(input),
            _ => {
                return Err(AppError::Validation(
                    "memory maintenance Work Order must contain exactly one frozen input"
                        .to_string(),
                ));
            }
        };
        let work_order = MemoryWorkOrder::new(
            format!("memory-maintenance-{}", uuid::Uuid::new_v4()),
            self.tenant_id().to_string(),
            purpose,
            target_watermark_utc.clone(),
            MemoryWindow {
                start_utc: (now - chrono::Duration::hours(hours as i64)).to_rfc3339(),
                end_utc: target_watermark_utc,
                hours,
            },
            MemoryWorkOrderScope {
                project_key: project_key.map(str::to_string),
            },
            source_revision_set_hash,
            skill,
            now.to_rfc3339(),
        );
        let payload = MemoryMaintenanceWorkOrderPayload {
            work_order: work_order.clone(),
            project_path: project_path.map(str::to_string),
            skill_text,
            project_input,
            global_input,
        };
        let work_order_json = serde_json::to_string(&payload).map_err(AppError::external)?;
        let purpose_text = match purpose {
            MemoryJobPurpose::ProjectConsolidation => "project_consolidation",
            MemoryJobPurpose::GlobalConsolidation => "global_consolidation",
            MemoryJobPurpose::RecentSnapshot => {
                return Err(AppError::Validation(
                    "recent snapshot must use its dedicated durable queue".to_string(),
                ));
            }
        };
        let job_id = format!(
            "memory-maint-{}",
            short_digest(&format!(
                "{}:{}:{}",
                self.tenant_id(),
                purpose_text,
                project_key.unwrap_or("")
            ))
        );
        Ok(store::enqueue_memory_maintenance_job_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &job_id,
            purpose_text,
            project_key,
            project_path,
            &work_order.input_fingerprint,
            &work_order_json,
            &now.to_rfc3339(),
        )
        .await?)
    }

    pub(crate) async fn schedule_project_memory_rebuild(
        &self,
        project_path: &str,
        now: DateTime<Utc>,
    ) -> AppResult<Vec<String>> {
        let normalized_path = self
            .resolve_context_project_path(Some(project_path))
            .await?
            .ok_or_else(|| AppError::Validation("project_path is required".to_string()))?;
        let job_id = self
            .enqueue_memory_maintenance_job(
                MemoryJobPurpose::ProjectConsolidation,
                Some(&normalized_path),
                Some(&normalized_path),
                now,
            )
            .await?;
        let _ = self
            .reconcile_memory_maintenance_jobs_for_tenant_at(self.tenant_id(), now)
            .await?;
        Ok(vec![format!("memory-maint-{job_id}")])
    }

    pub(crate) async fn schedule_global_memory_rebuild(
        &self,
        now: DateTime<Utc>,
    ) -> AppResult<Vec<String>> {
        let job_id = self
            .enqueue_memory_maintenance_job(MemoryJobPurpose::GlobalConsolidation, None, None, now)
            .await?;
        let _ = self
            .reconcile_memory_maintenance_jobs_for_tenant_at(self.tenant_id(), now)
            .await?;
        Ok(vec![format!("memory-maint-{job_id}")])
    }

    /// Project/Global consolidation 的 durable worker。SQLite lease 是唯一的
    /// ownership authority；TaskRuntime 只负责展示、取消和进程内执行槽位。
    pub(crate) async fn reconcile_memory_maintenance_jobs_for_tenant_at(
        &self,
        tenant_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<usize> {
        if tenant_id != self.tenant_id() {
            return Err(AppError::Conflict(
                "Memory maintenance coordinator requires a tenant-bound AppService".to_string(),
            ));
        }
        if !self.backend_settings()?.is_memory_generation_enabled() {
            return Ok(0);
        }

        let pool = self.db.pool().clone();
        let now_text = now.to_rfc3339();
        store::recover_expired_memory_maintenance_leases_sqlx(&pool, tenant_id, &now_text).await?;

        let active_count = self
            .runtime
            .task_runtime()
            .list_for_tenant(
                tenant_id,
                crate::backend::infrastructure::tasks::TaskFilter {
                    kind: Some(TaskKind::Memory),
                    active_only: true,
                    ..Default::default()
                },
            )
            .into_iter()
            .filter(|task| {
                matches!(
                    task.detail.get("domain").and_then(|value| value.as_str()),
                    Some("project_memory") | Some("global_memory")
                )
            })
            .count();
        if active_count >= MAX_MEMORY_MAINTENANCE_CONCURRENCY {
            return Ok(0);
        }

        let job_ids = store::list_memory_maintenance_job_ids_for_scheduler_sqlx(
            &pool, tenant_id, &now_text, 8,
        )
        .await?;
        for job_id in job_ids {
            let ownership_token = format!("memory-maint-owner-{}", uuid::Uuid::new_v4());
            if !store::claim_memory_maintenance_job_with_lease_sqlx(
                &pool,
                tenant_id,
                &job_id,
                &ownership_token,
                &now_text,
            )
            .await?
            {
                continue;
            }

            let Some(job) =
                store::load_memory_maintenance_job_sqlx(&pool, tenant_id, &job_id).await?
            else {
                continue;
            };
            let task_id = format!("memory-maint-{job_id}");
            let _ = self.runtime.task_runtime().remove_terminal(&task_id);
            let domain = if job.purpose == "project_consolidation" {
                "project_memory"
            } else {
                "global_memory"
            };
            let mut spec = TaskSpec::new(
                TaskKind::Memory,
                Some(format!("memory-maintenance:{tenant_id}:{job_id}")),
            )
            .with_task_id(task_id.clone())
            .with_tenant_id(tenant_id.to_string())
            .with_title(if domain == "project_memory" {
                "项目长期记忆维护".to_string()
            } else {
                "全局长期记忆维护".to_string()
            })
            .with_capabilities(TaskCapabilities {
                cancellable: true,
                retryable: true,
                clearable: true,
            })
            .with_conflict_key(format!("memory-maintenance:{tenant_id}:{job_id}"));
            spec.detail = json!({
                "domain": domain,
                "target": if domain == "project_memory" { "project" } else { "global" },
                "maintenance_job_id": job_id,
                "project_key": job.project_key,
                "project_path": job.project_path,
                "reason": "manual",
            });

            let runtime = self.runtime.clone();
            let tenant_id_for_task = tenant_id.to_string();
            let job_id_for_task = job_id.clone();
            let owner_for_task = ownership_token.clone();
            let purpose_for_task = job.purpose.clone();
            let spawn_result =
                self.runtime
                    .task_runtime()
                    .spawn_async(spec, move |context| async move {
                        super::memory_maintenance_runner::run_memory_maintenance_task(
                            &runtime,
                            &tenant_id_for_task,
                            &job_id_for_task,
                            &owner_for_task,
                            &purpose_for_task,
                            &context,
                        )
                        .await
                    });

            match spawn_result {
                Ok(crate::backend::infrastructure::tasks::SpawnOutcome::Started) => return Ok(1),
                Ok(crate::backend::infrastructure::tasks::SpawnOutcome::Existing) => {
                    let _ = store::finish_memory_maintenance_job_sqlx(
                        &pool,
                        tenant_id,
                        &job_id,
                        &ownership_token,
                        "failed",
                        Some("TASK_ALREADY_EXISTS"),
                        Some("memory maintenance task already exists"),
                        true,
                        &Utc::now().to_rfc3339(),
                    )
                    .await?;
                }
                Err(error) => {
                    let _ = store::finish_memory_maintenance_job_sqlx(
                        &pool,
                        tenant_id,
                        &job_id,
                        &ownership_token,
                        "failed",
                        Some(&error.code()),
                        Some(&error.to_string()),
                        error.retryable(),
                        &Utc::now().to_rfc3339(),
                    )
                    .await;
                    return Err(error.into());
                }
            }
        }
        Ok(0)
    }
}

fn short_digest(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    format!("{digest:x}")[..16].to_string()
}
