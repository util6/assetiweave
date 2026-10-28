use super::types::MemoryRebuildResult;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;

impl AppService {
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
