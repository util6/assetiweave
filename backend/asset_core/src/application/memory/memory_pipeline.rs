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
            let storage = crate::backend::domain::memory::storage::MemoryStorage::new(&tenant_id);
            let default_extractor =
                super::session_branch_workflow::HeuristicSessionBranchExtractorAgent;

            if params.force_rebuild {
                let _ = store::clear_watermarks_sqlx(&pool, &tenant_id).await;
            }

            // 阶段 1: SESSION_FACTS
            let session_report = {
                let mut guard = cx.enter_stage(stages::SESSION_FACTS);
                if cx.is_cancelled() {
                    return Err(InfraError::Cancelled("memory pipeline cancelled".into()));
                }
                guard.progress(
                    0,
                    Some(1),
                    Some("正在扫描未处理会话并提取思维分叉...".into()),
                );
                let report = service
                    .run_session_branch_workflow(&default_extractor, Some(&storage))
                    .await
                    .map_err(|e| InfraError::External(format!("会话思维分叉提取失败: {e}")))?;
                guard.record_metric("sessions_scanned", report.scanned_count as u64);
                guard.record_metric("sessions_processed", report.processed_count as u64);
                guard.record_step(
                    "session_facts_extracted",
                    Some(format!(
                        "扫描 {} 个会话，提取 {} 个分支，跳过 {} 个会话",
                        report.scanned_count, report.processed_count, report.skipped_count
                    )),
                    Some(1),
                    Some(1),
                );
                guard.progress(1, Some(1), Some("会话思维分叉提取完成".into()));
                guard.finish_with_status(StageStatus::Succeeded);
                report
            };

            // 阶段 2: RECENT_SNAPSHOT
            let recent_report = {
                let mut guard = cx.enter_stage(stages::RECENT_SNAPSHOT);
                if cx.is_cancelled() {
                    return Err(InfraError::Cancelled("memory pipeline cancelled".into()));
                }
                guard.progress(
                    0,
                    Some(1),
                    Some("正在生成近期工作快照并消融项目叶子...".into()),
                );
                let report = service
                    .run_recent_and_project_workflow(&storage)
                    .await
                    .map_err(|e| {
                        InfraError::External(format!("近期快照与项目叶子消融失败: {e}"))
                    })?;
                guard.record_metric("recent_sessions", report.session_count as u64);
                guard.record_metric("projects_updated", report.project_count as u64);
                guard.record_step(
                    "recent_snapshot_consolidated",
                    Some(format!(
                        "近期快照已更新（覆盖 {} 个会话），同步更新 {} 个项目记忆",
                        report.session_count, report.project_count
                    )),
                    Some(1),
                    Some(1),
                );
                guard.progress(1, Some(1), Some("近期快照与项目消融完成".into()));
                guard.finish_with_status(StageStatus::Succeeded);
                report
            };

            // 阶段 3: GLOBAL_CONSOLIDATION
            let global_report = {
                let mut guard = cx.enter_stage(stages::GLOBAL_CONSOLIDATION);
                if cx.is_cancelled() {
                    return Err(InfraError::Cancelled("memory pipeline cancelled".into()));
                }
                guard.progress(
                    0,
                    Some(1),
                    Some("正在沉淀全局长期经验至 MEMORY.md...".into()),
                );
                let report = service
                    .run_global_memory_workflow(&storage)
                    .await
                    .map_err(|e| InfraError::External(format!("全局记忆沉淀失败: {e}")))?;
                guard.record_metric("global_updated", if report.updated { 1 } else { 0 });
                guard.record_step(
                    "global_consolidated",
                    Some("全局长期记忆沉淀完成".into()),
                    Some(1),
                    Some(1),
                );
                guard.progress(1, Some(1), Some("全局长期记忆维护完成".into()));
                guard.finish_with_status(StageStatus::Succeeded);
                report
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
