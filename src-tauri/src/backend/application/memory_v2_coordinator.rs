use super::recent_snapshot_task_progress::RecentSnapshotTaskProgress;
use super::service::AppService;
use crate::backend::{
    models::{
        compute_global_consolidation_fingerprint, compute_project_consolidation_fingerprint,
        MemoryJobPurpose, MemoryMaintenanceWorkOrderPayload, MemoryScopeV2, MemoryWindowV2,
        MemoryWorkOrderV2,
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
const MAX_MEMORY_V2_MAINTENANCE_CONCURRENCY: usize = 1;

impl AppService {
    async fn load_memory_v2_source_revision_set_hash(
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
                let input = crate::backend::application::project_consolidation_pipeline::
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
                let input = crate::backend::application::global_consolidation_pipeline::
                    load_global_consolidation_input(self.db.pool(), self.tenant_id())
                    .await?;
                Ok(compute_global_consolidation_fingerprint(&input))
            }
            MemoryJobPurpose::RecentSnapshot => Err(AppError::Validation(
                "recent snapshot uses its dedicated evidence fingerprint".to_string(),
            )),
        }
    }

    async fn enqueue_memory_v2_maintenance_job(
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
                let input = crate::backend::application::project_consolidation_pipeline::
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
                let input = crate::backend::application::global_consolidation_pipeline::
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
        let work_order = MemoryWorkOrderV2::new(
            format!("memory-maintenance-{}", uuid::Uuid::new_v4()),
            self.tenant_id().to_string(),
            purpose,
            target_watermark_utc.clone(),
            MemoryWindowV2 {
                start_utc: (now - chrono::Duration::hours(hours as i64)).to_rfc3339(),
                end_utc: target_watermark_utc,
                hours,
            },
            MemoryScopeV2 {
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
            "memory-v2-maint-{}",
            short_digest(&format!(
                "{}:{}:{}",
                self.tenant_id(),
                purpose_text,
                project_key.unwrap_or("")
            ))
        );
        store::enqueue_memory_v2_maintenance_job_sqlx(
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
        .await
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
            .enqueue_memory_v2_maintenance_job(
                MemoryJobPurpose::ProjectConsolidation,
                Some(&normalized_path),
                Some(&normalized_path),
                now,
            )
            .await?;
        let _ = self
            .reconcile_memory_v2_maintenance_jobs_for_tenant_at(self.tenant_id(), now)
            .await?;
        Ok(vec![format!("memory-v2-maint-{job_id}")])
    }

    pub(crate) async fn schedule_global_memory_rebuild(
        &self,
        now: DateTime<Utc>,
    ) -> AppResult<Vec<String>> {
        let job_id = self
            .enqueue_memory_v2_maintenance_job(
                MemoryJobPurpose::GlobalConsolidation,
                None,
                None,
                now,
            )
            .await?;
        let _ = self
            .reconcile_memory_v2_maintenance_jobs_for_tenant_at(self.tenant_id(), now)
            .await?;
        Ok(vec![format!("memory-v2-maint-{job_id}")])
    }

    pub(crate) async fn schedule_memory_projection_rebuild(&self) -> AppResult<Vec<String>> {
        let tenant_id = self.tenant_id().to_string();
        let task_id = format!("memory-v2-projection-{}", short_digest(&tenant_id));
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
            Some(format!("memory-v2-projection:{tenant_id}")),
        )
        .with_task_id(task_id)
        .with_tenant_id(tenant_id.clone())
        .with_title("Memory Markdown 投影修复".to_string())
        .with_capabilities(TaskCapabilities {
            cancellable: true,
            retryable: true,
            clearable: true,
        })
        .with_conflict_key(format!("memory-v2-projection:{tenant_id}"));
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

    /// 调度并运行 v2 Recent Snapshot durable jobs。
    ///
    /// Session Phase 1 仍由旧 coordinator 负责写入 SQLite 来源事实；这里不再
    /// 触碰旧 project_memory_jobs/global_memory_jobs，避免两个生成管线同时写入。
    pub(crate) async fn reconcile_recent_memory_jobs_for_tenant_at(
        &self,
        tenant_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<usize> {
        if tenant_id != self.tenant_id() {
            return Err(AppError::Conflict(
                "Memory v2 coordinator requires a tenant-bound AppService".to_string(),
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
            let task_id = format!("memory-v2-recent-{job_id}");
            // Durable retry 可能发生在 TaskRuntime 的 terminal 保留窗口内；清理旧
            // terminal 记录后才能用同一个稳定 task id 重新投递本次 job。
            let _ = self.runtime.task_runtime().remove_terminal(&task_id);
            let mut spec = TaskSpec::new(
                TaskKind::Memory,
                Some(format!("memory-v2-recent-job:{tenant_id}:{job_id}")),
            )
            .with_task_id(task_id.clone())
            .with_tenant_id(tenant_id.to_string())
            .with_title("近期记忆快照生成".to_string())
            .with_capabilities(TaskCapabilities {
                cancellable: true,
                retryable: true,
                clearable: true,
            })
            .with_conflict_key(format!("memory-v2-recent:{tenant_id}"));
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
                            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
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
                                    action = "memory_v2.source_invalidation",
                                    tenant_id = %tenant_id_for_task,
                                    job_id = %job_id_for_task,
                                    error = %error,
                                    "Memory v2 source availability reconciliation failed"
                                );
                            }
                            if let Err(error) = service
                                .reconcile_project_consolidations_after_snapshot(&snapshot)
                                .await
                            {
                                tracing::warn!(
                                    action = "memory_v2.promotion",
                                    tenant_id = %tenant_id_for_task,
                                    job_id = %job_id_for_task,
                                    error = %error,
                                    "Memory v2 promotion was not published; Recent Snapshot remains successful"
                                );
                            }
                            if let Err(error) = service.rebuild_markdown_projections().await {
                                tracing::warn!(
                                    action = "memory_v2.projection",
                                    tenant_id = %tenant_id_for_task,
                                    job_id = %job_id_for_task,
                                    error = %error,
                                    "Memory v2 Markdown projection was not published"
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

    /// Project/Global consolidation 的 durable worker。SQLite lease 是唯一的
    /// ownership authority；TaskRuntime 只负责展示、取消和进程内执行槽位。
    pub(crate) async fn reconcile_memory_v2_maintenance_jobs_for_tenant_at(
        &self,
        tenant_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<usize> {
        if tenant_id != self.tenant_id() {
            return Err(AppError::Conflict(
                "Memory v2 maintenance coordinator requires a tenant-bound AppService".to_string(),
            ));
        }
        if !self.backend_settings()?.is_memory_generation_enabled() {
            return Ok(0);
        }

        let pool = self.db.pool().clone();
        let now_text = now.to_rfc3339();
        // Durable lease recovery must run before the in-process slot check. A
        // stale TaskRuntime snapshot must not hide an expired SQLite lease
        // forever after restart or process interruption.
        store::recover_expired_memory_v2_maintenance_leases_sqlx(&pool, tenant_id, &now_text)
            .await?;

        let active_count = self
            .runtime
            .task_runtime()
            .list_for_tenant(
                tenant_id,
                crate::backend::runtime::tasks::TaskFilter {
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
        if active_count >= MAX_MEMORY_V2_MAINTENANCE_CONCURRENCY {
            return Ok(0);
        }

        let job_ids = store::list_memory_v2_maintenance_job_ids_for_scheduler_sqlx(
            &pool, tenant_id, &now_text, 8,
        )
        .await?;
        for job_id in job_ids {
            let ownership_token = format!("memory-v2-maint-owner-{}", uuid::Uuid::new_v4());
            if !store::claim_memory_v2_maintenance_job_with_lease_sqlx(
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
                store::load_memory_v2_maintenance_job_sqlx(&pool, tenant_id, &job_id).await?
            else {
                continue;
            };
            let task_id = format!("memory-v2-maint-{job_id}");
            let _ = self.runtime.task_runtime().remove_terminal(&task_id);
            let domain = if job.purpose == "project_consolidation" {
                "project_memory"
            } else {
                "global_memory"
            };
            let mut spec = TaskSpec::new(
                TaskKind::Memory,
                Some(format!("memory-v2-maintenance:{tenant_id}:{job_id}")),
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
            .with_conflict_key(format!("memory-v2-maintenance:{tenant_id}:{job_id}"));
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
                        let service = AppService::from_runtime(&runtime)
                            .for_tenant(&tenant_id_for_task)
                            .await?;
                        let job = store::load_memory_v2_maintenance_job_sqlx(
                            service.db.pool(),
                            &tenant_id_for_task,
                            &job_id_for_task,
                        )
                        .await?
                        .ok_or_else(|| {
                            AppError::NotFound("memory v2 maintenance job not found".to_string())
                        })?;
                        let payload: MemoryMaintenanceWorkOrderPayload =
                            serde_json::from_str(&job.work_order_json).map_err(|_| {
                                AppError::Validation(
                                    "MEMORY_WORK_ORDER_INVALID: invalid maintenance work order"
                                        .to_string(),
                                )
                            })?;
                        if payload.work_order.tenant_id != tenant_id_for_task
                            || payload.work_order.purpose
                                != if purpose_for_task == "project_consolidation" {
                                    MemoryJobPurpose::ProjectConsolidation
                                } else {
                                    MemoryJobPurpose::GlobalConsolidation
                                }
                        {
                            return Err(AppError::Validation(
                                "MEMORY_WORK_ORDER_INVALID: maintenance scope mismatch".to_string(),
                            ));
                        }
                        if payload.work_order.input_fingerprint != job.input_fingerprint
                            || payload.work_order.scope.project_key.as_deref()
                                != job.project_key.as_deref()
                            || payload.project_path.as_deref() != job.project_path.as_deref()
                        {
                            return Err(AppError::Validation(
                                "MEMORY_WORK_ORDER_INVALID: durable row binding mismatch"
                                    .to_string(),
                            ));
                        }
                        match payload.work_order.purpose {
                            MemoryJobPurpose::ProjectConsolidation => {
                                let project_input = payload.project_input.as_ref().ok_or_else(|| {
                                    AppError::Validation(
                                        "MEMORY_WORK_ORDER_INVALID: missing frozen project input"
                                            .to_string(),
                                    )
                                })?;
                                if payload.global_input.is_some()
                                    || project_input.project_key
                                        != payload.work_order.scope.project_key.as_deref().unwrap_or_default()
                                    || project_input.project_path.as_deref()
                                        != payload.project_path.as_deref()
                                    || compute_project_consolidation_fingerprint(project_input)
                                        != payload.work_order.source_revision_set_hash
                                {
                                    return Err(AppError::Validation(
                                        "MEMORY_WORK_ORDER_INVALID: frozen project input binding mismatch"
                                            .to_string(),
                                    ));
                                }
                            }
                            MemoryJobPurpose::GlobalConsolidation => {
                                let global_input = payload.global_input.as_ref().ok_or_else(|| {
                                    AppError::Validation(
                                        "MEMORY_WORK_ORDER_INVALID: missing frozen global input"
                                            .to_string(),
                                    )
                                })?;
                                if payload.project_input.is_some()
                                    || global_input.tenant_id != tenant_id_for_task
                                    || compute_global_consolidation_fingerprint(global_input)
                                        != payload.work_order.source_revision_set_hash
                                {
                                    return Err(AppError::Validation(
                                        "MEMORY_WORK_ORDER_INVALID: frozen global input binding mismatch"
                                            .to_string(),
                                    ));
                                }
                            }
                            MemoryJobPurpose::RecentSnapshot => {
                                return Err(AppError::Validation(
                                    "MEMORY_WORK_ORDER_INVALID: recent snapshot in maintenance queue"
                                        .to_string(),
                                ));
                            }
                        }
                        let current_source_revision_set_hash = service
                            .load_memory_v2_source_revision_set_hash(
                                payload.work_order.purpose,
                                payload.work_order.scope.project_key.as_deref(),
                                payload.project_path.as_deref(),
                            )
                            .await?;
                        if current_source_revision_set_hash
                            != payload.work_order.source_revision_set_hash
                        {
                            return Err(AppError::Domain {
                                code: "MEMORY_WORK_ORDER_STALE".to_string(),
                                message: "memory maintenance evidence changed after enqueue"
                                    .to_string(),
                                retryable: false,
                                details: None,
                            });
                        }
                        let current_skill = service.get_active_generation_skill_binding().await?;
                        if current_skill != payload.work_order.skill {
                            return Err(AppError::Domain {
                                code: "MEMORY_WORK_ORDER_STALE".to_string(),
                                message: "memory generation skill changed after enqueue"
                                    .to_string(),
                                retryable: false,
                                details: None,
                            });
                        }
                        if context.is_cancelled() {
                            return Err(AppError::Cancelled(
                                "memory v2 maintenance task cancelled".to_string(),
                            ));
                        }

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
                                            let heartbeat_now = Utc::now().to_rfc3339();
                                            match store::heartbeat_memory_v2_maintenance_job_sqlx(
                                                &heartbeat_pool,
                                                &heartbeat_tenant,
                                                &heartbeat_job,
                                                &heartbeat_owner,
                                                &heartbeat_now,
                                            ).await {
                                                Ok(true) => {}
                                                Ok(false) | Err(_) => break,
                                            }
                                        }
                                    }
                                }
                            })
                        };

                        let result = if purpose_for_task == "project_consolidation" {
                            let project_key = job.project_key.as_deref().ok_or_else(|| {
                                AppError::Validation(
                                    "project maintenance job is missing project_key".to_string(),
                                )
                            })?;
                            service
                                .reconcile_project_consolidation_with_agent(
                                    project_key,
                                    job.project_path.as_deref(),
                                    payload.work_order.clone(),
                                    payload.skill_text.clone(),
                                    context.cancellation(),
                                    Some((&job_id_for_task, &owner_for_task)),
                                    payload.project_input.clone(),
                                )
                                .await
                                .map(|view| json!({ "updated": view.is_some() }))
                        } else {
                            service
                                .reconcile_global_consolidation_with_agent(
                                    Utc::now(),
                                    payload.work_order.clone(),
                                    payload.skill_text.clone(),
                                    context.cancellation(),
                                    Some((&job_id_for_task, &owner_for_task)),
                                    payload.global_input.clone(),
                                )
                                .await
                                .map(|view| json!({ "updated": view.is_some() }))
                        };
                        heartbeat_cancel.cancel();
                        let _ = heartbeat_task.await;

                        match result {
                            Ok(summary) => {
                                let job_after = store::load_memory_v2_maintenance_job_sqlx(
                                    service.db.pool(),
                                    &tenant_id_for_task,
                                    &job_id_for_task,
                                )
                                .await?;
                                let durably_succeeded =
                                    job_after.as_ref().map(|job| job.status.as_str())
                                        == Some("succeeded");
                                if context.is_cancelled() {
                                    if durably_succeeded {
                                        // Cancellation arrived after the atomic
                                        // publication; the durable job result wins.
                                    } else {
                                        let cancelled_at = Utc::now().to_rfc3339();
                                        let _ = store::finish_memory_v2_maintenance_job_sqlx(
                                            service.db.pool(),
                                            &tenant_id_for_task,
                                            &job_id_for_task,
                                            &owner_for_task,
                                            "canceled",
                                            Some("CANCELED"),
                                            Some("memory v2 maintenance task cancelled"),
                                            false,
                                            &cancelled_at,
                                        )
                                        .await?;
                                        return Err(AppError::Cancelled(
                                            "memory v2 maintenance task cancelled".to_string(),
                                        ));
                                    }
                                }
                                if !durably_succeeded {
                                    return Err(AppError::Conflict(
                                        "memory v2 maintenance result was not durably committed"
                                            .to_string(),
                                    ));
                                }
                                if let Err(error) = service.rebuild_markdown_projections().await {
                                    tracing::warn!(
                                        action = "memory_v2.projection",
                                        tenant_id = %tenant_id_for_task,
                                        job_id = %job_id_for_task,
                                        error = %error,
                                        "Memory v2 Markdown projection was not published"
                                    );
                                }
                                Ok(json!({
                                    "domain": if purpose_for_task == "project_consolidation" {
                                        "project_memory"
                                    } else {
                                        "global_memory"
                                    },
                                    "maintenance_job_id": job_id_for_task,
                                    "summary": summary,
                                }))
                            }
                            Err(error) => {
                                let failed_at = Utc::now().to_rfc3339();
                                let durable_status = if matches!(&error, AppError::Cancelled(_)) {
                                    "canceled"
                                } else if error.code() == "MEMORY_WORK_ORDER_STALE" {
                                    "stale"
                                } else {
                                    "failed"
                                };
                                let committed = store::finish_memory_v2_maintenance_job_sqlx(
                                    service.db.pool(),
                                    &tenant_id_for_task,
                                    &job_id_for_task,
                                    &owner_for_task,
                                    durable_status,
                                    Some(&error.code()),
                                    Some(&error.to_string()),
                                    error.retryable() && !matches!(&error, AppError::Cancelled(_)),
                                    &failed_at,
                                )
                                .await?;
                                if !committed {
                                    return Err(AppError::Conflict(
                                        "memory v2 maintenance lease is no longer owned"
                                            .to_string(),
                                    ));
                                }
                                Err(error)
                            }
                        }
                    });

            match spawn_result {
                Ok(crate::backend::runtime::tasks::SpawnOutcome::Started) => return Ok(1),
                Ok(crate::backend::runtime::tasks::SpawnOutcome::Existing) => {
                    let _ = store::finish_memory_v2_maintenance_job_sqlx(
                        &pool,
                        tenant_id,
                        &job_id,
                        &ownership_token,
                        "failed",
                        Some("TASK_ALREADY_EXISTS"),
                        Some("memory v2 maintenance task already exists"),
                        true,
                        &Utc::now().to_rfc3339(),
                    )
                    .await?;
                }
                Err(error) => {
                    let _ = store::finish_memory_v2_maintenance_job_sqlx(
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
        Ok(0)
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
