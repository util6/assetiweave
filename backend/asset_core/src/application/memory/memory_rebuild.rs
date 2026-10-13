use super::types::MemoryRebuildResult;
use crate::backend::application::prelude::*;
use crate::backend::store;
use chrono::Utc;

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
            MemoryRebuildTarget::Project => {
                let path = project_path.ok_or_else(|| {
                    AppError::Validation("Project rebuild requires project_path".to_string())
                })?;
                (self.schedule_project_memory_rebuild(path, now).await?, None)
            }
            MemoryRebuildTarget::Recent
            | MemoryRebuildTarget::Global
            | MemoryRebuildTarget::All => {
                if project_path.is_some() {
                    return Err(AppError::Validation(format!(
                        "{:?} rebuild does not accept project_path",
                        target
                    )));
                }
                if self.is_memory_pipeline_active() {
                    return Err(AppError::Conflict("Memory 流水线已在运行中".to_string()));
                }
                let pipeline_handle = self
                    .start_memory_pipeline(super::memory_pipeline::MemoryPipelineParams::rebuild(
                        target,
                        None,
                        params.reason,
                    ))
                    .await?;
                (vec![pipeline_handle.task_id], None)
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

#[cfg(test)]
#[path = "memory_rebuild_tests.rs"]
mod tests;
