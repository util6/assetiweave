use chrono::{DateTime, Utc};
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

pub use super::params::MemoryPipelineParams;

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
            "domain": "memory_pipeline", "tenant_id": self.tenant_id(),
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
            let (s1_ok, s1_fail) = {
                let mut guard = cx.enter_stage(stages::SESSION_FACTS);
                if cx.is_cancelled() {
                    return Err(InfraError::Cancelled("memory pipeline cancelled".into()));
                }
                service
                    .run_session_facts_pipeline_stage(&mut guard, now, params.force_rebuild, &cx)
                    .await?
            };

            // 阶段 2: RECENT_SNAPSHOT
            let mut snapshot_updated = false;
            {
                let mut guard = cx.enter_stage(stages::RECENT_SNAPSHOT);
                if cx.is_cancelled() {
                    return Err(InfraError::Cancelled("memory pipeline cancelled".into()));
                }
                let _ = store::recover_expired_recent_memory_leases_sqlx(
                    &pool, &tenant_id, &now_text,
                )
                .await;

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
                            let mut claimed = store::claim_recent_memory_job_with_lease_sqlx(
                                &pool, &tenant_id, &jid, &owner, &now_text,
                            )
                            .await
                            .unwrap_or(false);

                            if !claimed {
                                if let Ok(Some(existing_job)) =
                                    store::load_recent_memory_job_sqlx(&pool, &tenant_id, &jid).await
                                {
                                    if existing_job.status == "failed" {
                                        guard.record_step(
                                            "reopen_job",
                                            Some(format!("发现处于失败终态的快照任务 [{}], 自动重置并重新执行", jid)),
                                            None,
                                            None,
                                        );
                                        let _ = store::restart_recent_memory_job_for_rebuild_sqlx(
                                            &pool, &tenant_id, &jid, &now_text,
                                        ).await;
                                        claimed = store::claim_recent_memory_job_with_lease_sqlx(
                                            &pool, &tenant_id, &jid, &owner, &now_text,
                                        )
                                        .await
                                        .unwrap_or(false);
                                    } else if existing_job.status == "running" {
                                        guard.record_step(
                                            "recover_orphaned_job",
                                            Some(format!("接管遗留的运行态孤儿快照任务 [{}], 重置并重新执行", jid)),
                                            None,
                                            None,
                                        );
                                        let _ = store::restart_recent_memory_job_for_rebuild_sqlx(
                                            &pool, &tenant_id, &jid, &now_text,
                                        ).await;
                                        claimed = store::claim_recent_memory_job_with_lease_sqlx(
                                            &pool, &tenant_id, &jid, &owner, &now_text,
                                        )
                                        .await
                                        .unwrap_or(false);
                                    }
                                }
                            }

                            if claimed {
                                if let Ok(Some(job)) =
                                    store::load_recent_memory_job_sqlx(&pool, &tenant_id, &jid)
                                        .await
                                {
                                    guard.activity("recent_snapshot", "生成近期工作快照", Some(job.id.clone()));
                                    guard.progress(1, Some(2), Some("正在生成近期快照...".into()));

                                    let (agent_id, model) = service
                                        .resolve_available_agent_for(
                                            &crate::backend::domain::agents::ActionId::new("memory.generation"),
                                        )
                                        .map(|(id, m)| (id.to_string(), m))
                                        .unwrap_or_else(|_| ("builtin:assistant".to_string(), None));

                                    let memory_session = crate::backend::application::memory::memory_agent_session::ActiveMemoryAgentSession::start(
                                        &service.runtime,
                                        crate::backend::application::memory::memory_agent_session::MemoryAgentSessionParams {
                                            tenant_id: &tenant_id,
                                            scope: "recent",
                                            job_id: &job.id,
                                            task_id: Some(cx.task_id()),
                                            agent_id: &agent_id,
                                            display_name: Some("Recent Memory Snapshot Agent".to_string()),
                                            model,
                                            prompt_summary: "生成近期工作快照",
                                            custom_session_ref: Some(crate::backend::domain::agents::AgentSessionRef::new(
                                                format!("recent-snapshot-{}", job.id),
                                            )),
                                            persistent: false,
                                        },
                                    );
                                    guard.set_agent_session_ref(Some(memory_session.session_ref.clone()));

                                    let watchdog_res = super::activity_watchdog::run_with_activity_watchdog(
                                        service.run_recent_snapshot_generation_job(
                                            &job,
                                            cx.cancellation().clone(),
                                            Some(memory_session.sink.clone()),
                                            None,
                                        ),
                                        cx.cancellation().clone(),
                                        {
                                            let s = memory_session.clone();
                                            move || s.elapsed_since_active()
                                        },
                                        super::activity_watchdog::ActivityWatchdogConfig::default(),
                                    )
                                    .await;

                                    match watchdog_res {
                                        Ok(Ok(_)) => {
                                            memory_session.finish_succeeded(&service.runtime);
                                            snapshot_updated = true;
                                            guard.remove_activity("recent_snapshot");
                                            guard.record_metric("recent_snapshot_updated", 1);
                                            guard.record_step("snapshot_generated", Some("近期记忆快照已成功更新".into()), Some(2), Some(2));
                                            guard.progress(2, Some(2), Some("近期快照生成完成".into()));
                                            guard.finish_with_status(StageStatus::Succeeded);
                                        }
                                        Ok(Err(e)) => {
                                            guard.remove_activity("recent_snapshot");
                                            let err_msg = e.to_string();
                                            let (err_code, retryable) = match &e {
                                                AppError::Domain {
                                                    code, retryable, ..
                                                } => (code.as_str(), *retryable),
                                                _ => ("recent_snapshot_failed", false),
                                            };
                                            memory_session.finish_failed(&service.runtime, err_code, &err_msg, retryable);
                                            let _ = store::finish_recent_memory_job_sqlx(
                                                &pool,
                                                &tenant_id,
                                                &jid,
                                                &owner,
                                                "failed",
                                                Some(err_code),
                                                Some(&err_msg),
                                                retryable,
                                                &now_text,
                                            )
                                            .await;
                                            guard.record_failure(err_code, &err_msg, retryable);
                                            guard.record_step("snapshot_failed", Some(format!("近期记忆快照失败: {err_msg}")), None, None);
                                            guard.finish_with_status(StageStatus::Failed);
                                            return Err(InfraError::External(format!(
                                                "Memory 流水线阶段 2 (近期快照) 失败: {err_msg}"
                                            )));
                                        }
                                        Err(timeout_kind) => {
                                            let err_code = timeout_kind.error_code();
                                            let err_msg = timeout_kind.display_message("生成近期快照");
                                            memory_session.finish_failed(&service.runtime, err_code, &err_msg, true);
                                            guard.remove_activity("recent_snapshot");
                                            let _ = store::finish_recent_memory_job_sqlx(
                                                &pool,
                                                &tenant_id,
                                                &jid,
                                                &owner,
                                                "failed",
                                                Some(err_code),
                                                Some(&err_msg),
                                                true,
                                                &now_text,
                                            )
                                            .await;
                                            guard.record_failure(err_code, &err_msg, true);
                                            guard.record_step("snapshot_timeout", Some(err_msg.clone()), None, None);
                                            guard.finish_with_status(StageStatus::Failed);
                                            return Err(InfraError::External(format!(
                                                "Memory 流水线阶段 2 (近期快照) 超时: {err_msg}"
                                            )));
                                        }
                                    }
                                } else {
                                    guard.record_failure(
                                        "recent_job_not_found",
                                        "Missing job record",
                                        false,
                                    );
                                    guard.record_step("snapshot_job_missing", Some(format!("找不到快照任务记录 [{jid}]")), None, None);
                                    guard.finish_with_status(StageStatus::Failed);
                                    return Err(InfraError::External(format!("Memory 快照任务记录不存在: {jid}")));
                                }
                            } else {
                                if let Ok(Some(existing_job)) =
                                    store::load_recent_memory_job_sqlx(&pool, &tenant_id, &jid).await
                                {
                                    if matches!(existing_job.status.as_str(), "succeeded" | "reused") {
                                        snapshot_updated = true;
                                        guard.record_metric("recent_snapshot_reused", 1);
                                        guard.record_step("snapshot_reused", Some("近期快照内容无变动，复用现有结果".into()), Some(1), Some(1));
                                        guard.finish_with_status(StageStatus::Succeeded);
                                    } else {
                                        guard.record_failure(
                                            "recent_job_claim_failed",
                                            format!("Job status '{}' cannot be claimed", existing_job.status),
                                            false,
                                        );
                                        guard.record_step(
                                            "claim_failed",
                                            Some(format!("无法认领快照任务 [{}]: 状态为 '{}'", jid, existing_job.status)),
                                            None,
                                            None,
                                        );
                                        guard.finish_with_status(StageStatus::Failed);
                                        return Err(InfraError::External(format!(
                                            "Memory 阶段 2 快照认领失败: 状态为 '{}'",
                                            existing_job.status
                                        )));
                                    }
                                } else {
                                    guard.record_failure(
                                        "recent_job_claim_failed",
                                        "Claim conflict",
                                        false,
                                    );
                                    guard.record_step("claim_conflict", Some(format!("认领快照任务 [{jid}] 发生并发冲突")), None, None);
                                    guard.finish_with_status(StageStatus::Failed);
                                    return Err(InfraError::External("Memory 阶段 2 快照认领并发冲突".into()));
                                }
                            }
                        }
                        Err(e) => {
                            guard.record_failure(
                                "recent_snapshot_enqueue_failed",
                                e.to_string(),
                                true,
                            );
                            guard.record_step("enqueue_failed", Some(format!("快照工单入队失败: {e}")), None, None);
                            guard.finish_with_status(StageStatus::Failed);
                            return Err(InfraError::External(format!("Memory 阶段 2 快照工单入队失败: {e}")));
                        }
                    },
                    Ok(None) => {
                        let _ = sqlx::query("UPDATE recent_memory_jobs SET status = 'reused', finished_at = ?1, updated_at = ?1 WHERE tenant_id = ?2 AND status = 'queued'")
                            .bind(&now_text).bind(&tenant_id).execute(&pool).await;
                        guard.record_metric("skipped", 1);
                        guard.record_skipped("no_change", "近期记忆内容无变动，复用现有快照");
                        guard.record_step("snapshot_skipped", Some("近期记忆内容无变动，复用现有快照".into()), Some(1), Some(1));
                        guard.finish_with_status(StageStatus::Succeeded);
                    }
                    Err(e) => {
                        guard.record_failure("recent_snapshot_prep_failed", e.to_string(), true);
                        guard.record_step("prep_failed", Some(format!("准备快照工单失败: {e}")), None, None);
                        guard.finish_with_status(StageStatus::Failed);
                        return Err(InfraError::External(format!("Memory 阶段 2 准备快照失败: {e}")));
                    }
                }
            }

            // 阶段 3: GLOBAL_CONSOLIDATION
            {
                let mut guard = cx.enter_stage(stages::GLOBAL_CONSOLIDATION);
                if cx.is_cancelled() {
                    return Err(InfraError::Cancelled("memory pipeline cancelled".into()));
                }
                let _ = store::recover_expired_memory_maintenance_leases_sqlx(
                    &pool, &tenant_id, &now_text,
                )
                .await;

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
                    guard.record_step("global_skipped", Some("无新增全局长期记忆整理需求，跳过维护".into()), Some(0), Some(0));
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
                            guard.activity("global_consolidation", "维护全局长期记忆", Some(jid.clone()));
                            match tokio::time::timeout(
                                std::time::Duration::from_secs(600),
                                super::memory_maintenance_runner::run_memory_maintenance_task(
                                    &service.runtime,
                                    &tenant_id,
                                    &jid,
                                    &owner,
                                    "global_consolidation",
                                    &cx,
                                ),
                            )
                            .await
                            {
                                Ok(Ok(_)) => {
                                    guard.remove_activity("global_consolidation");
                                    guard.record_metric("global_consolidated", 1);
                                    guard.record_step("global_consolidated", Some("全局长期记忆已整理完成".into()), Some(1), Some(1));
                                    guard.finish_with_status(StageStatus::Succeeded);
                                }
                                Ok(Err(e)) => {
                                    guard.remove_activity("global_consolidation");
                                    guard.record_failure("global_consolidation_failed", e.to_string(), false);
                                    guard.record_step("global_failed", Some(format!("全局维护失败: {e}")), None, None);
                                    guard.finish_with_status(StageStatus::Failed);
                                }
                                Err(_) => {
                                    guard.remove_activity("global_consolidation");
                                    guard.record_failure("global_consolidation_timeout", "全局维护超时(180s)", false);
                                    guard.record_step("global_timeout", Some("全局维护超时".into()), None, None);
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
                        guard.record_step("markdown_projected", Some("Markdown 记忆投影已更新".into()), Some(1), Some(1));
                        guard.progress(1, Some(1), Some("Markdown 投影更新完成".into()));
                        guard.finish_with_status(StageStatus::Succeeded);
                    }
                    Err(e) => {
                        let err_msg = e.to_string();
                        guard.record_failure("markdown_projection_failed", &err_msg, false);
                        guard.finish_with_status(StageStatus::Failed);
                        return Err(InfraError::External(format!("Markdown 记忆投影更新失败: {err_msg}")));
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
