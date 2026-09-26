use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row, Sqlite};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

impl AppService {
    pub(crate) fn build_recent_snapshot_work_order_evidence_pack(
        &self,
        target: &WatermarkTarget,
        candidates: &[CandidateSession],
        continuable_items: &[ContinuableMemoryItemView],
    ) -> RecentSnapshotWorkOrderEvidencePack {
        self.build_recent_snapshot_work_order_evidence_pack_with_context(
            target,
            candidates,
            continuable_items,
            &RecentSnapshotFrozenContext::default(),
        )
    }

    pub(crate) fn build_recent_snapshot_work_order_evidence_pack_with_context(
        &self,
        target: &WatermarkTarget,
        candidates: &[CandidateSession],
        continuable_items: &[ContinuableMemoryItemView],
        context: &RecentSnapshotFrozenContext,
    ) -> RecentSnapshotWorkOrderEvidencePack {
        let candidate_sessions = candidates
            .iter()
            .map(|c| CandidateSessionSummary {
                session_ref: c.short_ref.clone(),
                session_id: c.session_id.clone(),
                project_key: c.project_key.clone(),
                title: c.session_title.clone(),
                last_activity_at: c.last_activity_at.clone(),
                source_id: c.source_id.clone(),
                source_agent: c.source_agent.clone(),
                source_revision: c.source_revision,
            })
            .collect::<Vec<_>>();

        let session_evidence = candidates
            .iter()
            .map(|candidate| {
                let memory = context.session_memories.get(&candidate.session_id);
                RecentSnapshotSessionEvidence {
                    candidate: CandidateSessionSummary {
                        session_ref: candidate.short_ref.clone(),
                        session_id: candidate.session_id.clone(),
                        project_key: candidate.project_key.clone(),
                        title: candidate.session_title.clone(),
                        last_activity_at: candidate.last_activity_at.clone(),
                        source_id: candidate.source_id.clone(),
                        source_agent: candidate.source_agent.clone(),
                        source_revision: candidate.source_revision,
                    },
                    memory_source_revision: memory.map(|value| value.source_revision),
                    summary: memory.map(|value| value.summary.clone()),
                    goal: memory.map(|value| value.goal.clone()),
                    result: memory.map(|value| value.result.clone()),
                    decisions: memory
                        .map(|value| value.decisions.clone())
                        .unwrap_or_default(),
                    verification: memory
                        .map(|value| value.verification.clone())
                        .unwrap_or_default(),
                    blockers: memory
                        .map(|value| value.blockers.clone())
                        .unwrap_or_default(),
                    follow_up: memory
                        .map(|value| value.follow_up.clone())
                        .unwrap_or_default(),
                    topics: memory.map(|value| value.topics.clone()).unwrap_or_default(),
                    source_references: context
                        .source_references
                        .get(&candidate.session_id)
                        .cloned()
                        .unwrap_or_default(),
                    recent_events: context
                        .recent_events
                        .get(&candidate.session_id)
                        .cloned()
                        .unwrap_or_default(),
                }
            })
            .collect();

        let mut project_keys: Vec<String> = candidates
            .iter()
            .map(|c| c.project_key.clone())
            .chain(continuable_items.iter().map(|i| i.project_key.clone()))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        project_keys.sort();

        let allowed_tools = ALLOWED_MEMORY_GENERATION_TOOLS
            .iter()
            .map(|&s| s.to_string())
            .collect();

        RecentSnapshotWorkOrderEvidencePack {
            target_watermark_utc: target.target_watermark_utc.to_rfc3339(),
            window_start_utc: target.window_start_utc.to_rfc3339(),
            window_end_utc: target.window_end_utc.to_rfc3339(),
            window_hours: target.window_hours as u32,
            project_keys,
            candidate_sessions,
            session_evidence,
            continuable_items: continuable_items.to_vec(),
            current_l2_projects: context.current_l2_projects.clone(),
            current_l3_items: context.current_l3_items.clone(),
            allowed_tools,
            output_schema_version: 2,
        }
    }

    pub(crate) async fn load_recent_snapshot_frozen_context(
        &self,
        target: &WatermarkTarget,
        candidates: &[CandidateSession],
    ) -> AppResult<RecentSnapshotFrozenContext> {
        let session_ids = candidates
            .iter()
            .map(|candidate| candidate.session_id.clone())
            .collect::<Vec<_>>();
        let session_memories = store::list_active_session_memories_for_sessions_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &session_ids,
        )
        .await?;
        let source_references = store::list_session_memory_source_references_for_sessions_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &session_ids,
        )
        .await?;
        let recent_events = store::list_recent_memory_events_for_sessions_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &session_ids,
            &target.window_start_utc.to_rfc3339(),
            &target.window_end_utc.to_rfc3339(),
        )
        .await?;

        let project_keys = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT project_key FROM memory_items WHERE tenant_id = ?1 AND layer = 'l2' AND lifecycle = 'current' AND project_key IS NOT NULL ORDER BY project_key ASC",
        )
        .bind(self.tenant_id())
        .fetch_all(self.db.pool())
        .await
        .map_err(AppError::external)?;
        let mut current_l2_projects = Vec::new();
        for project_key in project_keys {
            if let Some(view) = crate::backend::application::memory::project_consolidation_pipeline::
                load_l2_project_memory_view(self.db.pool(), self.tenant_id(), &project_key)
                .await?
            {
                current_l2_projects.push(view);
            }
        }
        let current_l3_items =
            crate::backend::application::memory::global_consolidation_pipeline::get_global_memory_l3_view(
                self.db.pool(),
                self.tenant_id(),
            )
            .await?
            .map(|view| view.items)
            .unwrap_or_default();

        Ok(RecentSnapshotFrozenContext {
            session_memories,
            source_references,
            recent_events,
            current_l2_projects,
            current_l3_items,
        })
    }

    /// 准备一个 v2 Recent Snapshot 任务。该方法只负责确定性输入、指纹和
    /// reuse；真正的 Agent 调用由 durable job worker 执行。
    pub(crate) async fn prepare_recent_snapshot_generation<Tz: chrono::TimeZone>(
        &self,
        now: Option<DateTime<Tz>>,
    ) -> AppResult<Option<RecentSnapshotPreparation>> {
        self.prepare_recent_snapshot_generation_with_options(now, false)
            .await
    }

    pub(crate) async fn prepare_recent_snapshot_generation_with_options<Tz: chrono::TimeZone>(
        &self,
        now: Option<DateTime<Tz>>,
        force_rebuild: bool,
    ) -> AppResult<Option<RecentSnapshotPreparation>> {
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

        if !memory_settings.generation_enabled {
            return Ok(None);
        }

        let current = now
            .map(|value| value.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);
        let target = resolve_target_watermark(
            current,
            memory_settings.recent_window_hours,
            &memory_settings.watermark_time_1,
            &memory_settings.watermark_time_2,
        )?;
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let state = crate::backend::application::memory::recent::load_recent_memory_state_view(
            pool, tenant_id,
        )
        .await?;
        let (candidates, _ref_map) = self
            .collect_recent_snapshot_candidates(target.target_watermark_utc, target.window_hours)
            .await?;
        let skill_binding = self.get_active_generation_skill_binding().await?;
        let skill_text = self.load_active_generation_skill_text().await?;
        let continuable_items = self
            .collect_continuable_items(&target.target_watermark_utc)
            .await?;
        let prior_items: Vec<(&str, &str)> = continuable_items
            .iter()
            .map(|item| (item.item_id.as_str(), item.current_revision_id.as_str()))
            .collect();
        let carry_over: Vec<(&str, &str, i64)> = continuable_items
            .iter()
            .map(|item| {
                (
                    item.item_id.as_str(),
                    item.current_revision_id.as_str(),
                    item.remaining_days,
                )
            })
            .collect();
        let frozen_context = self
            .load_recent_snapshot_frozen_context(&target, &candidates)
            .await?;
        // Recent generation consumes successful Phase-1 facts and canonical
        // Conversation locators only. If Phase 1 is still pending, leave the
        // durable watermark untouched so the next coordinator pass can retry
        // after the prerequisite job reaches a terminal state.
        if !has_complete_recent_snapshot_evidence(&candidates, &frozen_context) {
            return Ok(None);
        }
        let evidence = self.build_recent_snapshot_work_order_evidence_pack_with_context(
            &target,
            &candidates,
            &continuable_items,
            &frozen_context,
        );
        let current_l2_l3_revisions = self.load_current_long_term_revision_ids().await?;
        let current_l2_l3_revision_refs = current_l2_l3_revisions
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let target_fingerprint = compute_target_fingerprint(
            tenant_id,
            &target.target_watermark_utc,
            target.window_hours,
            &candidates,
            &prior_items,
            &skill_binding,
        );
        let base_content_fingerprint = compute_content_fingerprint(
            target.window_hours,
            &candidates,
            &carry_over,
            &current_l2_l3_revision_refs,
            &skill_binding,
            &memory_settings.excluded_session_ids,
            &memory_settings.excluded_source_ids,
        );
        let content_fingerprint =
            compute_recent_snapshot_evidence_fingerprint(&base_content_fingerprint, &evidence);

        if !force_rebuild {
            if let Some(last_snapshot) = state.snapshot {
                if let Some(last_meta) = store::load_recent_snapshot_meta_by_id_sqlx(
                    pool,
                    tenant_id,
                    &last_snapshot.snapshot_id,
                )
                .await?
                {
                    if last_meta.content_fingerprint == content_fingerprint {
                        if last_meta.target_watermark_utc
                            == target.target_watermark_utc.to_rfc3339()
                            || last_meta.target_fingerprint == target_fingerprint
                        {
                            return Ok(None);
                        }
                        self.commit_reused_memory_snapshot(
                            &target,
                            &skill_binding,
                            &last_meta.id,
                            &target_fingerprint,
                            &content_fingerprint,
                        )
                        .await?;
                        return Ok(None);
                    }
                }
            }
        }

        Ok(Some(RecentSnapshotPreparation {
            target,
            skill_binding,
            target_fingerprint,
            content_fingerprint,
            evidence,
            skill_text,
        }))
    }
}
