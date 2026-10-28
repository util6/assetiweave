use chrono::Utc;
use serde_json::{json, Value};

use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::infrastructure::tasks::{
    stages, ConflictPolicy, InfraError, PipelineDescriptor, StageStatus, TaskAdmission,
    TaskCapabilities, TaskHandle, TaskKind, TaskOutput, TaskSpec,
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

    /// 启动单一租户级 Memory 知识流水线 (Issue #85 / ADR-0017 / Issue #135)
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
            let tenant_id = service.tenant_id().to_string();
            let pool = service.db.pool().clone();
            let storage =
                crate::backend::infrastructure::memory::storage::MemoryStorage::new(&tenant_id);

            if params.force_rebuild {
                let _ = store::clear_watermarks_sqlx(&pool, &tenant_id).await;
                if let Err(e) = storage.clear_all() {
                    tracing::warn!(tenant = %tenant_id, error = %e, "Failed to clear memory storage during force rebuild");
                }
            }

            // 阶段 1: SESSION_FACTS
            let session_report = {
                let mut guard = cx.enter_stage(stages::SESSION_FACTS);
                if cx.is_cancelled() {
                    return Err(InfraError::Cancelled("memory pipeline cancelled".into()));
                }
                guard.progress(0, Some(1), Some("正在扫描未处理会话...".into()));
                let report = service
                    .run_session_branch_workflow(None, Some(&storage))
                    .await
                    .map_err(|e| InfraError::External(format!("会话扫描失败: {e}")))?;
                guard.record_metric("sessions_scanned", report.scanned_count as u64);
                guard.record_metric("sessions_processed", report.processed_count as u64);
                guard.record_step(
                    "session_facts_scanned",
                    Some(format!(
                        "扫描 {} 个会话，未注入认知提取器，跳过写入 {} 个会话",
                        report.scanned_count, report.skipped_count
                    )),
                    Some(1),
                    Some(1),
                );
                guard.progress(1, Some(1), Some("会话扫描完成".into()));
                guard.finish_with_status(StageStatus::Succeeded);
                report
            };

            // 阶段 2: RECENT_SNAPSHOT
            let recent_report = {
                let mut guard = cx.enter_stage(stages::RECENT_SNAPSHOT);
                if cx.is_cancelled() {
                    return Err(InfraError::Cancelled("memory pipeline cancelled".into()));
                }
                guard.progress(0, Some(1), Some("检查近期快照与项目状态...".into()));
                guard.record_metric("recent_sessions", 0);
                guard.record_metric("projects_updated", 0);
                guard.record_step(
                    "recent_snapshot_skipped",
                    Some("未注入有效认知提取器，跳过近期快照自动写入以防止产物污染".into()),
                    Some(1),
                    Some(1),
                );
                guard.progress(1, Some(1), Some("近期快照维护完成".into()));
                guard.finish_with_status(StageStatus::Succeeded);
                super::memory_workflow_recent_project::RecentAndProjectWorkflowReport::default()
            };

            // 阶段 3: GLOBAL_CONSOLIDATION
            let global_report = {
                let mut guard = cx.enter_stage(stages::GLOBAL_CONSOLIDATION);
                if cx.is_cancelled() {
                    return Err(InfraError::Cancelled("memory pipeline cancelled".into()));
                }
                guard.progress(0, Some(1), Some("检查全局长期经验状态...".into()));
                guard.record_metric("global_updated", 0);
                guard.record_step(
                    "global_skipped",
                    Some("未注入有效认知提取器，跳过全局长期记忆自动写入以防止产物污染".into()),
                    Some(1),
                    Some(1),
                );
                guard.progress(1, Some(1), Some("全局长期记忆维护完成".into()));
                guard.finish_with_status(StageStatus::Succeeded);
                super::memory_workflow_global::GlobalMemoryWorkflowReport::default()
            };

            let summary = format!(
                "Memory 流水线执行完毕 (扫描会话: {}, 近期会话: {}, 更新项目: {})",
                session_report.scanned_count,
                recent_report.session_count,
                recent_report.project_count
            );
            Ok(TaskOutput::with_summary(
                json!({
                    "domain": "memory_pipeline",
                    "sessions_scanned": session_report.scanned_count,
                    "sessions_processed": session_report.processed_count,
                    "recent_sessions": recent_report.session_count,
                    "projects_updated": recent_report.project_count,
                    "global_updated": global_report.updated,
                }),
                summary,
            ))
        })
    }
}

#[cfg(test)]
#[path = "memory_pipeline_tests.rs"]
mod tests;
