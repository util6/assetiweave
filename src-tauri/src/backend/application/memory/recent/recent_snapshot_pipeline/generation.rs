use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::memory::work_order::{
    MemoryJobPurpose, MemoryWindow, MemoryWorkOrder, MemoryWorkOrderScope,
    RecentSnapshotWorkOrderPayload,
};
use crate::backend::domain::memory::RecentMemorySnapshotView;
use crate::backend::infrastructure::agent_execution::AiExecutionPurpose;
use crate::backend::store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row, Sqlite};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};
impl AppService {
    pub(crate) async fn enqueue_recent_snapshot_generation(
        &self,
        preparation: &RecentSnapshotPreparation,
        now: DateTime<Utc>,
    ) -> AppResult<String> {
        let work_order = MemoryWorkOrder::new(
            format!("recent-snapshot-{}", Uuid::new_v4()),
            self.tenant_id().to_string(),
            MemoryJobPurpose::RecentSnapshot,
            preparation.target.target_watermark_utc.to_rfc3339(),
            MemoryWindow {
                start_utc: preparation.target.window_start_utc.to_rfc3339(),
                end_utc: preparation.target.window_end_utc.to_rfc3339(),
                hours: preparation.target.window_hours as u32,
            },
            MemoryWorkOrderScope { project_key: None },
            preparation.content_fingerprint.clone(),
            preparation.skill_binding.clone(),
            now.to_rfc3339(),
        );
        let payload = RecentSnapshotWorkOrderPayload {
            target_watermark_utc: preparation.target.target_watermark_utc.to_rfc3339(),
            local_watermark_date: preparation.target.local_watermark_date.clone(),
            local_watermark_time: preparation.target.local_watermark_time.clone(),
            timezone_offset_minutes: preparation.target.timezone_offset_minutes,
            window_hours: preparation.target.window_hours,
            window_start_utc: preparation.target.window_start_utc.to_rfc3339(),
            window_end_utc: preparation.target.window_end_utc.to_rfc3339(),
            target_fingerprint: preparation.target_fingerprint.clone(),
            content_fingerprint: preparation.content_fingerprint.clone(),
            skill: preparation.skill_binding.clone(),
            skill_text: preparation.skill_text.clone(),
            evidence: preparation.evidence.clone(),
        };
        let work_order_json = serde_json::to_string(&serde_json::json!({
            "workOrder": work_order,
            "payload": payload,
        }))
        .map_err(AppError::external)?;
        Ok(store::enqueue_recent_memory_job_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &format!("recent-memory-{}", Uuid::new_v4()),
            &preparation.target.target_watermark_utc.to_rfc3339(),
            preparation.target.window_hours,
            &preparation.target_fingerprint,
            &preparation.content_fingerprint,
            &work_order_json,
            &now.to_rfc3339(),
        )
        .await?)
    }

    pub(crate) async fn run_recent_snapshot_generation_job(
        &self,
        job: &store::RecentMemoryJob,
        cancellation: tokio_util::sync::CancellationToken,
        progress: Option<std::sync::Arc<dyn AiExecutionProgressSink>>,
        mut task_progress: Option<
            &mut crate::backend::application::memory::recent::recent_snapshot_task_progress::RecentSnapshotTaskProgress,
        >,
    ) -> AppResult<RecentMemorySnapshotView> {
        let envelope: serde_json::Value =
            serde_json::from_str(&job.work_order_json).map_err(|_| {
                AppError::Validation("MEMORY_WORK_ORDER_INVALID: invalid JSON".to_string())
            })?;
        let work_order: MemoryWorkOrder =
            serde_json::from_value(envelope.get("workOrder").cloned().ok_or_else(|| {
                AppError::Validation("MEMORY_WORK_ORDER_INVALID: missing work order".to_string())
            })?)
            .map_err(|_| {
                AppError::Validation("MEMORY_WORK_ORDER_INVALID: invalid work order".to_string())
            })?;
        let payload: RecentSnapshotWorkOrderPayload =
            serde_json::from_value(envelope.get("payload").cloned().ok_or_else(|| {
                AppError::Validation("MEMORY_WORK_ORDER_INVALID: missing payload".to_string())
            })?)
            .map_err(|_| {
                AppError::Validation("MEMORY_WORK_ORDER_INVALID: invalid payload".to_string())
            })?;
        if work_order.tenant_id != self.tenant_id()
            || work_order.purpose != MemoryJobPurpose::RecentSnapshot
            || work_order.scope.project_key.is_some()
            || work_order.target_watermark_utc != payload.target_watermark_utc
            || work_order.window.hours != payload.window_hours as u32
            || work_order.skill != payload.skill
            || work_order.contract_version != "memory.contract.v2"
            || work_order.allowed_tools
                != ALLOWED_MEMORY_GENERATION_TOOLS
                    .iter()
                    .map(|tool| (*tool).to_string())
                    .collect::<Vec<_>>()
        {
            return Err(AppError::Validation(
                "MEMORY_WORK_ORDER_INVALID: work order binding mismatch".to_string(),
            ));
        }
        let expected_input_fingerprint = MemoryWorkOrder::compute_input_fingerprint(
            work_order.purpose,
            &work_order.target_watermark_utc,
            work_order.window.hours,
            work_order.scope.project_key.as_deref(),
            &work_order.source_revision_set_hash,
            &work_order.skill.content_hash,
            &work_order.contract_version,
            &work_order.budget_policy_version,
            &work_order.projection_policy_version,
        );
        if expected_input_fingerprint != work_order.input_fingerprint {
            return Err(AppError::Validation(
                "MEMORY_WORK_ORDER_INVALID: input fingerprint mismatch".to_string(),
            ));
        }

        let target_watermark = DateTime::parse_from_rfc3339(&payload.target_watermark_utc)
            .map_err(|_| {
                AppError::Validation("MEMORY_WORK_ORDER_INVALID: invalid watermark".to_string())
            })?
            .with_timezone(&Utc);
        let target = WatermarkTarget {
            target_watermark_utc: target_watermark,
            local_watermark_date: payload.local_watermark_date.clone(),
            local_watermark_time: payload.local_watermark_time.clone(),
            timezone_offset_minutes: payload.timezone_offset_minutes,
            window_hours: payload.window_hours,
            window_start_utc: DateTime::parse_from_rfc3339(&payload.window_start_utc)
                .map_err(|_| {
                    AppError::Validation(
                        "MEMORY_WORK_ORDER_INVALID: invalid window start".to_string(),
                    )
                })?
                .with_timezone(&Utc),
            window_end_utc: DateTime::parse_from_rfc3339(&payload.window_end_utc)
                .map_err(|_| {
                    AppError::Validation(
                        "MEMORY_WORK_ORDER_INVALID: invalid window end".to_string(),
                    )
                })?
                .with_timezone(&Utc),
        };
        let current_skill = self.get_active_generation_skill_binding().await?;
        if current_skill != payload.skill {
            return Err(AppError::Domain {
                code: "MEMORY_RESULT_STALE".to_string(),
                message: "Memory Generation Skill changed after the job was queued".to_string(),
                retryable: false,
                details: None,
            });
        }
        let (candidates, ref_map) = self
            .collect_recent_snapshot_candidates(target.target_watermark_utc, target.window_hours)
            .await?;
        let continuable_items = self
            .collect_continuable_items(&target.target_watermark_utc)
            .await?;
        let prior_items = continuable_items
            .iter()
            .map(|item| (item.item_id.as_str(), item.current_revision_id.as_str()))
            .collect::<Vec<_>>();
        let carry_over = continuable_items
            .iter()
            .map(|item| {
                (
                    item.item_id.as_str(),
                    item.current_revision_id.as_str(),
                    item.remaining_days,
                )
            })
            .collect::<Vec<_>>();
        let current_l2_l3_revisions = self.load_current_long_term_revision_ids().await?;
        let current_l2_l3_revision_refs = current_l2_l3_revisions
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let current_frozen_context = self
            .load_recent_snapshot_frozen_context(&target, &candidates)
            .await?;
        if !has_complete_recent_snapshot_evidence(&candidates, &current_frozen_context) {
            return Err(AppError::Domain {
                code: "MEMORY_COVERAGE_INCOMPLETE".to_string(),
                message: "Recent Snapshot requires successful Phase-1 facts and canonical references for every candidate session".to_string(),
                retryable: true,
                details: None,
            });
        }
        let current_evidence = self.build_recent_snapshot_work_order_evidence_pack_with_context(
            &target,
            &candidates,
            &continuable_items,
            &current_frozen_context,
        );
        let settings = self.app_settings_value();
        let memory_settings = settings
            .get("memory")
            .and_then(|value| {
                serde_json::from_value::<
                        crate::backend::infrastructure::app_settings::MemorySettings,
                    >(value.clone())
                    .ok()
            })
            .unwrap_or_default();
        let current_target_fingerprint = compute_target_fingerprint(
            self.tenant_id(),
            &target.target_watermark_utc,
            target.window_hours,
            &candidates,
            &prior_items,
            &current_skill,
        );
        let current_base_content_fingerprint = compute_content_fingerprint(
            target.window_hours,
            &candidates,
            &carry_over,
            &current_l2_l3_revision_refs,
            &current_skill,
            &memory_settings.excluded_session_ids,
            &memory_settings.excluded_source_ids,
        );
        let current_content_fingerprint = compute_recent_snapshot_evidence_fingerprint(
            &current_base_content_fingerprint,
            &current_evidence,
        );
        if current_target_fingerprint != payload.target_fingerprint
            || current_content_fingerprint != payload.content_fingerprint
        {
            return Err(AppError::Domain {
                code: "MEMORY_RESULT_STALE".to_string(),
                message: "Memory evidence changed after the job was queued".to_string(),
                retryable: false,
                details: None,
            });
        }

        if cancellation.is_cancelled() {
            return Err(AppError::Cancelled(
                "Recent memory generation was cancelled before execution".to_string(),
            ));
        }

        let mut result = if candidates.is_empty() {
            if let Some(task_progress) = task_progress.as_deref_mut() {
                task_progress.transition("agent_execution");
                task_progress.skip_current_and_transition("validation");
            }
            MemoryGenerationResult {
                schema_version: 2,
                projects: Vec::new(),
                coverage: Default::default(),
                unknowns: Vec::new(),
            }
        } else {
            if let Some(task_progress) = task_progress.as_deref_mut() {
                task_progress.transition("agent_execution");
            }
            let prompt = build_recent_generation_prompt(&envelope, &payload)?;
            let settings = self.app_settings_value();
            let (agent_id, model) =
                crate::backend::application::agents::composition::resolve_agent_for(
                    &crate::backend::domain::agents::ActionId::new("memory.generation"),
                    &settings,
                )?;
            let request = AiExecutionRequest {
                execution_id: format!("recent-memory-execution-{}", job.id),
                agent_id,
                purpose: AiExecutionPurpose::MemoryGeneration,
                session_mode: AgentSessionMode::OneShot,
                prompt,
                model,
                limits: AiExecutionLimits {
                    initialize_timeout: std::time::Duration::from_secs(30),
                    ..AiExecutionLimits::default()
                },
                cancellation: AiExecutionCancellation::from_token(cancellation.clone()),
                progress,
                tenant_id: Some(job.tenant_id.clone()),
                execution_context_key: None,
                binding: None,
                replay: false,
                restore_only: false,
                recall_tools: None,
                memory_generation_tools: Some(memory_generation_tools_for_job(job, &self.db_path)?),
            };
            let execution = execute_agent(self.agent_runtime.clone(), request)
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
            if let Some(task_progress) = task_progress.as_deref_mut() {
                task_progress.transition("validation");
            }
            parse_memory_generation_output(&execution.text)?
        };

        normalize_agent_memory_generation_result(&mut result, &candidates, &ref_map);

        self.validate_memory_generation_result(&result, &candidates, &ref_map)?;
        if cancellation.is_cancelled() {
            return Err(AppError::Cancelled(
                "Recent memory generation was cancelled before publication".to_string(),
            ));
        }
        if let Some(task_progress) = task_progress.as_deref_mut() {
            task_progress.transition("publish");
        }
        let snapshot = self
            .commit_recent_memory_snapshot(
                &target,
                &payload.skill,
                result,
                &candidates,
                &ref_map,
                &payload.target_fingerprint,
                &payload.content_fingerprint,
                Some(&current_l2_l3_revisions),
                Some((
                    job.id.as_str(),
                    job.ownership_token.as_deref().ok_or_else(|| {
                        AppError::Conflict(
                            "recent memory job has no active ownership token".to_string(),
                        )
                    })?,
                )),
            )
            .await?;
        if let Some(task_progress) = task_progress.as_deref_mut() {
            task_progress.transition("cleanup_session");
            task_progress.finish();
        }
        Ok(snapshot)
    }
}
