use super::memory_workflow_recent_project::RecentAndProjectWorkflowReport;
use super::types::MemoryRebuildResult;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::infrastructure::memory::storage::MemoryStorage;

impl AppService {
    /// 非破坏性重建：从项目累计状态树重新投影 Markdown 视图并重算近期工作快照。
    ///
    /// 这是 `force_rebuild` 的唯一合法实现语义。ADR-0018 的分层（Markdown 视图 +
    /// `.state/` JSON 状态树 + 薄 SQLite 水位）保持不变，本方法只做层内重算：
    /// 依据 `.state/projects/*.json` 里累计的主题树、分支节点与会话水位，
    /// 重写 `projects/<slug>.md`、`.state/recent.json` 与 `recent/latest.md`。
    /// 严禁物理删除任何累计状态或全局长期记忆 `MEMORY.md`——那些是跨周累积、
    /// 无法由流水线重跑再生的事实。
    pub async fn reproject_memory_views(
        &self,
        storage: &MemoryStorage,
    ) -> AppResult<RecentAndProjectWorkflowReport> {
        // 只补齐缺失的目录层级，不删除任何既有文件。
        storage.init_dirs().map_err(AppError::from)?;
        self.refresh_recent_memory_state_in(None, storage).await?;
        self.run_recent_and_project_workflow(storage).await
    }

    pub async fn rebuild_memory_scope(
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

        if self.is_memory_pipeline_active() {
            return Err(AppError::Conflict("Memory 流水线已在运行中".to_string()));
        }

        let pipeline_handle = self
            .start_memory_pipeline(super::memory_pipeline::MemoryPipelineParams {
                force_rebuild: params.force,
                target: Some(target),
                project_path: project_path.map(str::to_string),
                reason: params.reason,
            })
            .await?;

        Ok(MemoryRebuildResult {
            accepted: true,
            scheduled_task_ids: vec![pipeline_handle.task_id],
            target_watermark: None,
            reused: false,
        })
    }
}

#[cfg(test)]
#[path = "memory_rebuild_tests.rs"]
mod tests;
