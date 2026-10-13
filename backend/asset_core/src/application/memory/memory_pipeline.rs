use chrono::{DateTime, Utc};
use futures::stream::{FuturesUnordered, StreamExt};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::memory::work_order::MemoryJobPurpose;
use crate::backend::infrastructure::tasks::{
    stages, ConflictPolicy, InfraError, PipelineDescriptor, StageStatus, TaskAdmission,
    TaskCapabilities, TaskContext, TaskHandle, TaskKind, TaskOutput, TaskSpec,
};
use crate::backend::store;

use super::recent::recent_snapshot_task_progress::RecentSnapshotTaskProgress;

/// Memory 流水线执行参数
#[derive(Debug, Clone, Default)]
pub struct MemoryPipelineParams {
    pub force_rebuild: bool,
    pub target: Option<MemoryRebuildTarget>,
    pub project_path: Option<String>,
    pub reason: Option<MemoryRebuildReason>,
}

impl MemoryPipelineParams {
    pub fn background() -> Self {
        Self::default()
    }

    pub fn rebuild(
        target: MemoryRebuildTarget,
        project_path: Option<String>,
        reason: Option<MemoryRebuildReason>,
    ) -> Self {
        Self {
            force_rebuild: true,
            target: Some(target),
            project_path,
            reason,
        }
    }
}

impl AppService {
    /// 检查当前租户的单一 Memory 知识流水线是否处于活跃运行状态
    pub fn is_memory_pipeline_active(&self) -> bool {
        let task_id = format!("memory-pipeline:{}", self.tenant_id());
        self.runtime
            .task_runtime()
            .get_for_tenant(self.tenant_id(), &task_id)
            .map(|s| s.state.is_active())
            .unwrap_or(false)
    }

    /// 检查当前租户是否存在待处理的 Memory 任务或事实
    pub async fn has_pending_memory_work(&self, now: DateTime<Utc>) -> AppResult<bool> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let now_text = now.to_rfc3339();

        if !store::list_session_memory_job_ids_for_scheduler_sqlx(pool, tenant_id, &now_text, 1)
            .await?
            .is_empty()
            || !store::list_recent_memory_job_ids_for_scheduler_sqlx(pool, tenant_id, &now_text, 1)
                .await?
                .is_empty()
            || !store::list_memory_maintenance_job_ids_for_scheduler_sqlx(
                pool, tenant_id, &now_text, 1,
            )
            .await?
            .is_empty()
        {
            return Ok(true);
        }

        Ok(self
            .ensure_recent_snapshot_phase1_jobs_at(now, false)
            .await?
            .is_some())
    }

    /// 启动单一租户级 Memory 知识流水线 (Issue #85 / ADR-0017)
    pub async fn start_memory_pipeline(
        &self,
        params: MemoryPipelineParams,
    ) -> AppResult<TaskHandle<Value>> {
        if !self.backend_settings()?.is_memory_generation_enabled() {
            return Err(AppError::Validation("Memory generation is disabled".into()));
        }
        if self.is_memory_pipeline_active() {
            return Err(AppError::Conflict("Memory 流水线已在运行中".into()));
        }

        let pipeline = PipelineDescriptor::builder()
            .stage(stages::SESSION_FACTS, "提取会话事实")
            .stage(stages::RECENT_SNAPSHOT, "近期记忆快照")
            .stage(stages::GLOBAL_CONSOLIDATION, "全局长期记忆维护")
            .stage(stages::MARKDOWN_PROJECTION, "Markdown 记忆投影")
            .build();

        let task_id = format!("memory-pipeline:{}", self.tenant_id());
        let _ = self.runtime.task_runtime().remove_terminal(&task_id);

        let mut spec = TaskSpec::new(
            TaskKind::Memory,
            Some(format!("memory-pipeline-task:{}", self.tenant_id())),
        )
        .with_task_id(task_id.clone())
        .with_tenant_id(self.tenant_id().to_string())
        .with_title("Memory 知识流水线")
        .with_admission(TaskAdmission::Managed)
        .with_user_visible(true)
        .with_capabilities(TaskCapabilities {
            cancellable: true,
            retryable: true,
            clearable: true,
        })
        .with_conflict_key(format!("memory:{}", self.tenant_id()))
        .with_conflict_policy(ConflictPolicy::Reject)
        .with_pipeline(pipeline);

        spec.detail = json!({
            "domain": "memory_pipeline",
            "tenant_id": self.tenant_id(),
            "force_rebuild": params.force_rebuild,
            "target": params.target.as_ref().map(|t| format!("{:?}", t)),
            "reason": params.reason.as_ref().map(|r| format!("{:?}", r)),
        });

        let service = self.clone();
        self.spawn_pipeline_task(spec, move |cx| async move {
            let now = Utc::now();
            let now_text = now.to_rfc3339();
            let tenant_id = service.tenant_id().to_string();
            let pool = service.db.pool().clone();

            // 阶段 1: SESSION_FACTS
            let mut s1_ok = 0usize;
            let mut s1_fail = 0usize;
            {
                let mut guard = cx.enter_stage(stages::SESSION_FACTS);
                if cx.is_cancelled() {
                    return Err(InfraError::Cancelled("memory pipeline cancelled".into()));
                }
                let _ = service
                    .ensure_recent_snapshot_phase1_jobs_at(now, params.force_rebuild)
                    .await;
                let _ = store::recover_expired_session_memory_leases_sqlx(
                    &pool, &tenant_id, &now_text,
                )
                .await;

                let job_ids = store::list_session_memory_job_ids_for_scheduler_sqlx(
                    &pool, &tenant_id, &now_text, 64,
                )
                .await
                .unwrap_or_default();
                let total = job_ids.len();
                guard.record_metric("sessions_total", total as u64);

                if total > 0 {
                    let sem = Arc::new(tokio::sync::Semaphore::new(4));
                    let mut futs = FuturesUnordered::new();
                    for jid in job_ids {
                        let (sem, s, tid, cancel) = (
                            sem.clone(),
                            service.clone(),
                            tenant_id.clone(),
                            cx.cancellation().clone(),
                        );
                        futs.push(async move {
                            let _p = sem
                                .acquire()
                                .await
                                .map_err(|_| (jid.clone(), "semaphore closed".into()))?;
                            if cancel.is_cancelled() {
                                return Err((jid, "Task cancelled".into()));
                            }
                            match tokio::time::timeout(
                                std::time::Duration::from_secs(60),
                                s.run_session_memory_phase1_for_tenant_at(
                                    &tid,
                                    &jid,
                                    now,
                                    TaskContext::detached("memory_pipeline_session_worker"),
                                ),
                            )
                            .await
                            {
                                Ok(Ok(_)) => Ok(jid),
                                Ok(Err(e)) => Err((jid, e.to_string())),
                                Err(_) => Err((jid, "Timeout after 60s".into())),
                            }
                        });
                    }

                    let mut processed = 0u64;
                    while let Some(res) = futs.next().await {
                        processed += 1;
                        match res {
                            Ok(_) => s1_ok += 1,
                            Err((jid, msg)) => {
                                s1_fail += 1;
                                guard.record_failure(
                                    "session_fact_failed",
                                    format!("Session {jid} failed: {msg}"),
                                    true,
                                );
                            }
                        }
                        guard.progress(
                            processed,
                            Some(total as u64),
                            Some(format!("提取会话事实: {processed}/{total}")),
                        );
                    }
                }
                guard.record_metric("sessions_succeeded", s1_ok as u64);
                guard.record_metric("sessions_failed", s1_fail as u64);

                let existing: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM session_memories WHERE tenant_id = ?1 AND status = 'active'",
                )
                .bind(&tenant_id)
                .fetch_one(&pool)
                .await
                .unwrap_or(0);

                if s1_fail > 0 && (s1_ok > 0 || existing > 0) {
                    guard.finish_with_status(StageStatus::PartialSuccess);
                } else if s1_fail > 0 && s1_ok == 0 && existing == 0 {
                    guard.finish_with_status(StageStatus::Failed);
                } else {
                    guard.finish_with_status(StageStatus::Succeeded);
                }
            }

            // 阶段 2: RECENT_SNAPSHOT
            let mut snapshot_updated = false;
            {
                let mut guard = cx.enter_stage(stages::RECENT_SNAPSHOT);
                if cx.is_cancelled() {
                    return Err(InfraError::Cancelled("memory pipeline cancelled".into()));
                }
                match service
                    .prepare_recent_snapshot_generation_for_pipeline(
                        Some(now),
                        params.force_rebuild,
                    )
                    .await
                {
                    Ok(Some(prep)) => match service
                        .enqueue_recent_snapshot_generation(&prep, now)
                        .await
                    {
                        Ok(jid) => {
                            let owner = format!("memory-pipeline-owner-{}", Uuid::new_v4());
                            let claimed = store::claim_recent_memory_job_with_lease_sqlx(
                                &pool, &tenant_id, &jid, &owner, &now_text,
                            )
                            .await
                            .unwrap_or(false);

                            if claimed {
                                if let Ok(Some(job)) =
                                    store::load_recent_memory_job_sqlx(&pool, &tenant_id, &jid)
                                        .await
                                {
                                    let mut tp = RecentSnapshotTaskProgress::start(cx.progress());
                                    match service
                                        .run_recent_snapshot_generation_job(
                                            &job,
                                            cx.cancellation().clone(),
                                            None,
                                            Some(&mut tp),
                                        )
                                        .await
                                    {
                                        Ok(_) => {
                                            snapshot_updated = true;
                                            guard.record_metric("recent_snapshot_updated", 1);
                                            guard.finish_with_status(StageStatus::Succeeded);
                                        }
                                        Err(e) => {
                                            guard.record_failure(
                                                "recent_snapshot_failed",
                                                e.to_string(),
                                                true,
                                            );
                                            guard.finish_with_status(StageStatus::Failed);
                                        }
                                    }
                                } else {
                                    guard.record_failure(
                                        "recent_job_not_found",
                                        "Missing job record",
                                        false,
                                    );
                                    guard.finish_with_status(StageStatus::Failed);
                                }
                            } else {
                                guard.record_failure(
                                    "recent_job_claim_failed",
                                    "Claim conflict",
                                    false,
                                );
                                guard.finish_with_status(StageStatus::Failed);
                            }
                        }
                        Err(e) => {
                            guard.record_failure(
                                "recent_snapshot_enqueue_failed",
                                e.to_string(),
                                true,
                            );
                            guard.finish_with_status(StageStatus::Failed);
                        }
                    },
                    Ok(None) => {
                        guard.record_metric("skipped", 1);
                        guard.record_skipped("no_change", "近期记忆内容无变动，复用现有快照");
                        guard.finish_with_status(StageStatus::Succeeded);
                    }
                    Err(e) => {
                        guard.record_failure("recent_snapshot_prep_failed", e.to_string(), true);
                        guard.finish_with_status(StageStatus::Failed);
                    }
                }
            }

            // 阶段 3: GLOBAL_CONSOLIDATION
            {
                let mut guard = cx.enter_stage(stages::GLOBAL_CONSOLIDATION);
                if cx.is_cancelled() {
                    return Err(InfraError::Cancelled("memory pipeline cancelled".into()));
                }
                let is_global = matches!(
                    params.target,
                    Some(MemoryRebuildTarget::Global) | Some(MemoryRebuildTarget::All)
                );
                let pending_maint = store::list_memory_maintenance_job_ids_for_scheduler_sqlx(
                    &pool, &tenant_id, &now_text, 1,
                )
                .await
                .unwrap_or_default();

                if !is_global && pending_maint.is_empty() {
                    guard.skip("not_needed", "无新增全局长期记忆整理需求");
                } else {
                    let jid = if let Some(first_id) = pending_maint.into_iter().next() {
                        first_id
                    } else {
                        service
                            .enqueue_memory_maintenance_job(
                                MemoryJobPurpose::GlobalConsolidation,
                                None,
                                None,
                                now,
                            )
                            .await
                            .unwrap_or_default()
                    };

                    if !jid.is_empty() {
                        let owner = format!("memory-pipeline-owner-{}", Uuid::new_v4());
                        if store::claim_memory_maintenance_job_with_lease_sqlx(
                            &pool, &tenant_id, &jid, &owner, &now_text,
                        )
                        .await
                        .unwrap_or(false)
                        {
                            match super::memory_maintenance_runner::run_memory_maintenance_task(
                                &service.runtime,
                                &tenant_id,
                                &jid,
                                &owner,
                                "global_consolidation",
                                &cx,
                            )
                            .await
                            {
                                Ok(_) => {
                                    guard.record_metric("global_consolidated", 1);
                                    guard.finish_with_status(StageStatus::Succeeded);
                                }
                                Err(e) => {
                                    guard.record_failure(
                                        "global_consolidation_failed",
                                        e.to_string(),
                                        false,
                                    );
                                    guard.finish_with_status(StageStatus::Failed);
                                }
                            }
                        } else {
                            guard.record_skipped("lease_not_acquired", "全局维护任务已被占用");
                            guard.finish_with_status(StageStatus::Succeeded);
                        }
                    }
                }
            }

            // 阶段 4: MARKDOWN_PROJECTION
            {
                let mut guard = cx.enter_stage(stages::MARKDOWN_PROJECTION);
                if cx.is_cancelled() {
                    return Err(InfraError::Cancelled("memory pipeline cancelled".into()));
                }
                match service.rebuild_markdown_projections().await {
                    Ok(_) => {
                        guard.record_metric("projections_written", 1);
                        guard.progress(1, Some(1), Some("Markdown 投影更新完成".into()));
                        guard.finish_with_status(StageStatus::Succeeded);
                    }
                    Err(e) => {
                        guard.record_failure("markdown_projection_failed", e.to_string(), false);
                        guard.finish_with_status(StageStatus::Failed);
                    }
                }
            }

            let summary = format!(
                "Memory 流水线执行完毕 (会话成功: {s1_ok}, 失败: {s1_fail}, 快照已更新: {snapshot_updated})"
            );
            Ok(TaskOutput::with_summary(
                json!({
                    "domain": "memory_pipeline",
                    "sessions_succeeded": s1_ok,
                    "sessions_failed": s1_fail,
                    "recent_snapshot_updated": snapshot_updated,
                }),
                summary,
            ))
        })
    }
}

#[cfg(test)]
#[path = "memory_pipeline_tests.rs"]
mod tests;
