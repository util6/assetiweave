use super::recent_snapshot_task_progress::RecentSnapshotTaskProgress;
use super::service::AppService;
use crate::backend::{
    models::{
        MemoryJobPurpose, MemoryMaintenanceWorkOrderPayload, MemoryWindow, MemoryWorkOrder,
        MemoryWorkOrderScope,
    },
    runtime::{
        tasks::{TaskCapabilities, TaskKind, TaskSpec},
        AppError, AppResult,
    },
    store,
};
use chrono::{DateTime, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

const MAX_RECENT_MEMORY_CONCURRENCY: usize = 1;

impl AppService {
    pub(crate) async fn schedule_memory_projection_rebuild(&self) -> AppResult<Vec<String>> {
        let tenant_id = self.tenant_id().to_string();
        let task_id = format!("memory-projection-{}", short_digest(&tenant_id));
        if self
            .runtime
            .task_runtime()
            .get_for_tenant(&tenant_id, &task_id)
            .is_some_and(|task| task.state.is_active())
        {
            return Ok(Vec::new());
        }
        let _ = self.runtime.task_runtime().remove_terminal(&task_id);
        let task_id_for_result = task_id.clone();
        let runtime = self.runtime.clone();
        let mut spec = TaskSpec::new(
            TaskKind::Memory,
            Some(format!("memory-projection:{tenant_id}")),
        )
        .with_task_id(task_id)
        .with_tenant_id(tenant_id.clone())
        .with_title("Memory Markdown 投影修复".to_string())
        .with_capabilities(TaskCapabilities {
            cancellable: true,
            retryable: true,
            clearable: true,
        })
        .with_conflict_key(format!("memory-projection:{tenant_id}"));
        spec.detail = json!({
            "domain": "memory_projection",
            "target": "projection_repair",
            "reason": "projection_repair",
        });
        match self
            .runtime
            .task_runtime()
            .spawn_async(spec, move |_context| async move {
                let service = AppService::from_runtime(&runtime)
                    .for_tenant(&tenant_id)
                    .await?;
                let paths = service.rebuild_markdown_projections().await?;
                Ok(json!({
                    "domain": "memory_projection",
                    "summary_path": paths.summary_path,
                    "memory_path": paths.memory_path,
                }))
            }) {
            Ok(crate::backend::runtime::tasks::SpawnOutcome::Started) => {
                Ok(vec![task_id_for_result])
            }
            Ok(crate::backend::runtime::tasks::SpawnOutcome::Existing) => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    /// 调度并运行 Recent Snapshot durable jobs。
    ///
    /// Session Phase 1 仍由 session coordinator 负责写入 SQLite 来源事实；这里不再
    /// 触碰旧 project_memory_jobs/global_memory_jobs，避免两个生成管线同时写入。
    pub(crate) async fn reconcile_recent_memory_jobs_for_tenant_at(
        &self,
        tenant_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<usize> {
        if tenant_id != self.tenant_id() {
            return Err(AppError::Conflict(
                "Recent memory coordinator requires a tenant-bound AppService".to_string(),
            ));
        }
        if !self.backend_settings()?.is_memory_generation_enabled() {
            return Ok(0);
        }

        let pool = self.db.pool().clone();
        let now_text = now.to_rfc3339();
        store::recover_expired_recent_memory_leases_sqlx(&pool, tenant_id, &now_text).await?;

        // Phase 2 and Phase 1 must share one frozen watermark window. This
        // pass is idempotent: active projections with the same source
        // fingerprint are reused instead of dispatching another Agent call.
        let _ = self
            .ensure_recent_snapshot_phase1_jobs_at(now, false)
            .await?;

        if self
            .runtime
            .task_runtime()
            .list_for_tenant(
                self.tenant_id(),
                crate::backend::runtime::tasks::TaskFilter {
                    kind: Some(TaskKind::Memory),
                    active_only: true,
                    ..Default::default()
                },
            )
            .into_iter()
            .filter(|task| {
                task.detail.get("domain").and_then(|value| value.as_str())
                    == Some("recent_snapshot")
            })
            .count()
            >= MAX_RECENT_MEMORY_CONCURRENCY
        {
            return Ok(0);
        }

        if let Some(preparation) = self.prepare_recent_snapshot_generation(Some(now)).await? {
            let _ = self
                .enqueue_recent_snapshot_generation(&preparation, now)
                .await?;
        }

        let job_ids =
            store::list_recent_memory_job_ids_for_scheduler_sqlx(&pool, tenant_id, &now_text, 8)
                .await?;
        let mut scheduled = 0;
        for job_id in job_ids {
            if scheduled >= 1 {
                break;
            }
            let ownership_token = format!("recent-memory-owner-{}", uuid::Uuid::new_v4());
            if !store::claim_recent_memory_job_with_lease_sqlx(
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
            let task_id = format!("memory-recent-{job_id}");
            // Durable retry 可能发生在 TaskRuntime 的 terminal 保留窗口内；清理旧
            // terminal 记录后才能用同一个稳定 task id 重新投递本次 job。
            let _ = self.runtime.task_runtime().remove_terminal(&task_id);
            let mut spec = TaskSpec::new(
                TaskKind::Memory,
                Some(format!("memory-recent-job:{tenant_id}:{job_id}")),
            )
            .with_task_id(task_id.clone())
            .with_tenant_id(tenant_id.to_string())
            .with_title("近期记忆快照生成".to_string())
            .with_capabilities(TaskCapabilities {
                cancellable: true,
                retryable: true,
                clearable: true,
            })
            .with_conflict_key(format!("memory-recent:{tenant_id}"));
            spec.detail = json!({
                "domain": "recent_snapshot",
                "job_id": job_id,
                "ownership_token": ownership_token,
                "target_watermark": now_text,
            });

            let runtime = self.runtime.clone();
            let tenant_id_for_task = tenant_id.to_string();
            let job_id_for_task = job_id.clone();
            let owner_for_task = ownership_token.clone();
            match self
                .runtime
                .task_runtime()
                .spawn_async(spec, move |context| async move {
                    let mut task_progress = RecentSnapshotTaskProgress::start(context.progress());
                    let service = AppService::from_runtime(&runtime)
                        .for_tenant(&tenant_id_for_task)
                        .await?;
                    let task_id = context.task_id().to_string();
                    let _ = store::record_recent_memory_attempt_task_sqlx(
                        service.db.pool(),
                        &tenant_id_for_task,
                        &task_id,
                        &Utc::now().to_rfc3339(),
                    )
                    .await;
                    let job = store::load_recent_memory_job_sqlx(
                        service.db.pool(),
                        &tenant_id_for_task,
                        &job_id_for_task,
                    )
                    .await?
                    .ok_or_else(|| AppError::NotFound("recent memory job not found".to_string()))?;
                    task_progress.transition("load_facts");
                    let heartbeat_cancel = CancellationToken::new();
                    let heartbeat_task = {
                        let heartbeat_cancel = heartbeat_cancel.clone();
                        let heartbeat_pool = service.db.pool().clone();
                        let heartbeat_tenant = tenant_id_for_task.clone();
                        let heartbeat_job = job_id_for_task.clone();
                        let heartbeat_owner = owner_for_task.clone();
                        tokio::spawn(async move {
                            let mut interval =
                                tokio::time::interval(std::time::Duration::from_secs(30));
                            loop {
                                tokio::select! {
                                    _ = heartbeat_cancel.cancelled() => break,
                                    _ = interval.tick() => {
                                        let now = Utc::now().to_rfc3339();
                                        match store::heartbeat_recent_memory_job_sqlx(
                                            &heartbeat_pool,
                                            &heartbeat_tenant,
                                            &heartbeat_job,
                                            &heartbeat_owner,
                                            &now,
                                        ).await {
                                            Ok(true) => {}
                                            Ok(false) | Err(_) => break,
                                        }
                                    }
                                }
                            }
                        })
                    };
                    let result = service
                        .run_recent_snapshot_generation_job(
                            &job,
                            context.cancellation(),
                            None,
                            Some(&mut task_progress),
                        )
                        .await;
                    heartbeat_cancel.cancel();
                    let _ = heartbeat_task.await;
                    match result {
                        Ok(snapshot) => {
                            let finished_at = Utc::now().to_rfc3339();
                            let _ = store::record_recent_memory_attempt_task_sqlx(
                                service.db.pool(),
                                &tenant_id_for_task,
                                &task_id,
                                &finished_at,
                            )
                            .await;
                            if let Err(error) = service
                                .reconcile_memory_source_invalidation(Utc::now())
                                .await
                            {
                                tracing::warn!(
                                    action = "memory.source_invalidation",
                                    tenant_id = %tenant_id_for_task,
                                    job_id = %job_id_for_task,
                                    error = %error,
                                    "Memory source availability reconciliation failed"
                                );
                            }
                            if let Err(error) = service
                                .reconcile_project_consolidations_after_snapshot(&snapshot)
                                .await
                            {
                                tracing::warn!(
                                    action = "memory.promotion",
                                    tenant_id = %tenant_id_for_task,
                                    job_id = %job_id_for_task,
                                    error = %error,
                                    "Memory promotion was not published; Recent Snapshot remains successful"
                                );
                            }
                            if let Err(error) = service.rebuild_markdown_projections().await {
                                tracing::warn!(
                                    action = "memory.projection",
                                    tenant_id = %tenant_id_for_task,
                                    job_id = %job_id_for_task,
                                    error = %error,
                                    "Memory Markdown projection was not published"
                                );
                            }
                            Ok(json!({
                                "domain": "recent_snapshot",
                                "job_id": job_id_for_task,
                                "snapshot_id": snapshot.snapshot_id,
                            }))
                        }
                        Err(error) => {
                            task_progress.fail(&error);
                            let failed_at = Utc::now().to_rfc3339();
                            let retryable = error.retryable()
                                && !matches!(&error, AppError::Cancelled(_));
                            let durable_status = if matches!(&error, AppError::Cancelled(_)) {
                                "canceled"
                            } else if error.code() == "MEMORY_RESULT_STALE" {
                                "stale"
                            } else {
                                "failed"
                            };
                            let _ = service
                                .record_recent_memory_failure(&error.code(), &error.to_string())
                                .await;
                            let committed = store::finish_recent_memory_job_sqlx(
                                service.db.pool(),
                                &tenant_id_for_task,
                                &job_id_for_task,
                                &owner_for_task,
                                durable_status,
                                Some(&error.code()),
                                Some(&error.to_string()),
                                retryable,
                                &failed_at,
                            )
                            .await?;
                            if !committed {
                                return Err(AppError::Conflict(
                                    "recent memory job lease is no longer owned".to_string(),
                                ));
                            }
                            Err(error)
                        }
                    }
                }) {
                Ok(crate::backend::runtime::tasks::SpawnOutcome::Started) => scheduled += 1,
                Ok(crate::backend::runtime::tasks::SpawnOutcome::Existing) => {
                    let _ = store::finish_recent_memory_job_sqlx(
                        &pool,
                        tenant_id,
                        &job_id,
                        &ownership_token,
                        "failed",
                        Some("TASK_ALREADY_EXISTS"),
                        Some("recent memory task already exists"),
                        true,
                        &Utc::now().to_rfc3339(),
                    )
                    .await?;
                }
                Err(error) => {
                    let _ = store::finish_recent_memory_job_sqlx(
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
                    return Err(error);
                }
            }
        }
        Ok(scheduled)
    }

    async fn reconcile_project_consolidations_after_snapshot(
        &self,
        snapshot: &crate::backend::dto::RecentMemorySnapshotView,
    ) -> AppResult<()> {
        for project in &snapshot.projects {
            if project.project_key == "unassigned" {
                continue;
            }
            if let Some(project_path) = project.project_path.as_deref() {
                let normalized_path = self
                    .resolve_context_project_path(Some(project_path))
                    .await?
                    .ok_or_else(|| AppError::Validation("project_path is required".to_string()))?;
                if crate::backend::application::project_consolidation_pipeline::should_schedule_project_consolidation(
                        self.db.pool(),
                        self.tenant_id(),
                        &normalized_path,
                    )
                    .await?
                {
                    self.schedule_project_memory_rebuild(&normalized_path, Utc::now())
                        .await?;
                }
            }
        }
        let now = Utc::now();
        if crate::backend::application::global_consolidation_pipeline::should_schedule_global_consolidation(
            self.db.pool(),
            self.tenant_id(),
            now,
        )
            .await?
        {
            self.schedule_global_memory_rebuild(now).await?;
        }
        Ok(())
    }
}

fn short_digest(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    format!("{digest:x}")[..16].to_string()
}
