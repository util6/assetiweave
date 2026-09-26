use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::conversations::ConversationSessionDetail;
use crate::backend::domain::memory::evidence::{BoundedEvidenceInitialPack, ShortEvidenceRef};
use crate::backend::domain::memory::{MemoryExecutionWorkOrder, SessionMemoryJob};
use crate::backend::store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration as StdDuration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
impl AppService {
    pub(crate) async fn execute_session_memory_agent(
        &self,
        job: &SessionMemoryJob,
        detail: &ConversationSessionDetail,
        cancellation: CancellationToken,
        progress: Option<Arc<dyn AiExecutionProgressSink>>,
    ) -> AppResult<SessionMemoryAgentExecutionResult> {
        let recipe = if let Some(work_order_json) = &job.work_order_json {
            if let Ok(wo) = serde_json::from_str::<MemoryExecutionWorkOrder>(work_order_json) {
                wo.recipe.to_recipe()
            } else {
                MemoryRecipe::default_builtin()
            }
        } else {
            MemoryRecipe::default_builtin()
        };

        let normalized = session_detail_to_normalized(detail);
        let budget = BoundedMemoryBudgetPolicy::default();
        let work_order = MemoryExecutionWorkOrder::new(
            format!("wo-{}", job.id),
            job.session_id.clone(),
            job.source_id.clone(),
            job.source_revision,
            job.source_fingerprint.clone(),
            &recipe,
            budget,
            Utc::now().to_rfc3339(),
        );

        let (pack, short_refs) = build_bounded_evidence_initial_pack(&normalized, &work_order);

        // 如果完全没有有效节点（所有节点全为空或不可用）
        if pack.nodes_count == 0 && pack.coverage.indexed_nodes == 0 {
            return Ok(SessionMemoryAgentExecutionResult {
                raw_text: String::new(),
                short_refs,
                pack,
                session_cleanup: SessionCleanupStatus::Skipped,
                is_empty: true,
            });
        }

        let prompt = build_bounded_evidence_prompt(&pack, &recipe)?;
        let settings = self.app_settings_value();
        let (agent_id, model) =
            crate::backend::application::agents::composition::resolve_agent_for(
                &crate::backend::domain::agents::ActionId::new(SESSION_MEMORY_ACTION),
                &settings,
            )?;
        let request = AiExecutionRequest {
            execution_id: format!("session-memory-execution-{}", job.id),
            agent_id,
            purpose: AiExecutionPurpose::SessionMemory,
            session_mode: AgentSessionMode::OneShot,
            prompt,
            model,
            limits: AiExecutionLimits {
                initialize_timeout: std::time::Duration::from_secs(30),
                ..AiExecutionLimits::default()
            },
            cancellation: AiExecutionCancellation::from_token(cancellation),
            progress,
            tenant_id: Some(job.tenant_id.clone()),
            execution_context_key: None,
            binding: None,
            replay: false,
            restore_only: false,
            recall_tools: None,
            memory_generation_tools: None,
        };
        let result = execute_agent(self.agent_runtime.clone(), request)
            .await
            .map_err(|error| {
                let view = error.to_view();
                AppError::Domain {
                    code: view.code,
                    message: view.message,
                    retryable: view.retryable,
                    details: None,
                }
            })?;
        if result.text.chars().count() > MAX_AGENT_OUTPUT_LENGTH {
            return Err(AppError::Validation(
                "Session Memory Agent output is too large".to_string(),
            ));
        }

        Ok(SessionMemoryAgentExecutionResult {
            raw_text: result.text,
            short_refs,
            pack,
            session_cleanup: result.session_cleanup,
            is_empty: false,
        })
    }
}
