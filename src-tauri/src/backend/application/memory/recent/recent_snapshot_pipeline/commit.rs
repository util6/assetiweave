use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::memory::{
    MemoryItemStatus, MemoryPromotionNomination, RecentMemorySnapshotView,
};
use crate::backend::store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row, Sqlite};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

impl AppService {
    pub(crate) async fn commit_recent_memory_snapshot(
        &self,
        target: &WatermarkTarget,
        skill_binding: &MemorySkillBinding,
        result: MemoryGenerationResult,
        candidates: &[CandidateSession],
        ref_map: &HashMap<String, ResolvedEvidenceRef>,
        target_fingerprint: &str,
        content_fingerprint: &str,
        expected_long_term_revisions: Option<&[String]>,
        job_lease: Option<(&str, &str)>,
    ) -> AppResult<RecentMemorySnapshotView> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();

        let mut tx = pool.begin().await.map_err(AppError::external)?;

        if let Some((job_id, ownership_token)) = job_lease {
            let now = Utc::now().to_rfc3339();
            let owned = sqlx::query(
                "SELECT 1 FROM recent_memory_jobs WHERE tenant_id = ?1 AND id = ?2 \
                 AND status = 'running' AND ownership_token = ?3 \
                 AND lease_expires_at > ?4",
            )
            .bind(tenant_id)
            .bind(job_id)
            .bind(ownership_token)
            .bind(now)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::external)?
            .is_some();
            if !owned {
                return Err(AppError::Conflict(
                    "recent memory job lease is no longer owned".to_string(),
                ));
            }
        }

        let snapshot_id = format!("snap-{}", uuid::Uuid::new_v4());
        let now = Utc::now();
        let published_at = now.to_rfc3339();
        let content_generated_at = published_at.clone();

        let window_start_utc = target.window_start_utc.to_rfc3339();
        let window_end_utc = target.window_end_utc.to_rfc3339();
        let target_watermark_str = target.target_watermark_utc.to_rfc3339();

        let local_watermark_date = &target.local_watermark_date;
        let local_watermark_time = &target.local_watermark_time;
        let timezone_offset_minutes = target.timezone_offset_minutes;
        let window_hours = target.window_hours;

        // 清理同一 watermark 的历史快照冲突（若存在），支持用户重试任务/重新生成无缝覆盖
        Self::remove_conflicting_recent_snapshots(
            &mut tx,
            tenant_id,
            &target_watermark_str,
            window_hours,
            None,
        )
        .await?;

        // Sequence
        let seq_row = sqlx::query(
            "SELECT COALESCE(MAX(sequence), 0) + 1 AS next_seq FROM recent_memory_snapshots WHERE tenant_id = ?1",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::external)?;
        let sequence: i64 = seq_row.get("next_seq");

        // 1. 插入 recent_memory_snapshots
        sqlx::query(
            "INSERT INTO recent_memory_snapshots (\
                tenant_id, id, sequence, target_watermark_utc, local_watermark_date, \
                local_watermark_time, timezone_offset_minutes, window_hours, window_start_utc, \
                window_end_utc, publication_kind, reused_from_snapshot_id, target_fingerprint, \
                content_fingerprint, generation_skill_asset_id, generation_skill_revision, \
                generation_skill_content_hash, contract_version, budget_policy_version, \
                projection_policy_version, content_generated_at, published_at\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'generated', NULL, ?11, ?12, ?13, ?14, ?15, 'memory.contract.v2', 'budget.v1', 'projection.v2', ?16, ?17)",
        )
        .bind(tenant_id)
        .bind(&snapshot_id)
        .bind(sequence)
        .bind(&target_watermark_str)
        .bind(local_watermark_date)
        .bind(local_watermark_time)
        .bind(timezone_offset_minutes)
        .bind(window_hours)
        .bind(&window_start_utc)
        .bind(&window_end_utc)
        .bind(target_fingerprint)
        .bind(content_fingerprint)
        .bind(Some(skill_binding.asset_id.as_str()))
        .bind(skill_binding.asset_revision)
        .bind(Some(skill_binding.content_hash.as_str()))
        .bind(&content_generated_at)
        .bind(&published_at)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;

        // 2. 插入 Project & Items
        for (proj_idx, project) in result.projects.into_iter().enumerate() {
            let project_row_id = format!("{}-{}", snapshot_id, proj_idx + 1);

            // 计算该项目的实际 candidate session count 与 latest activity
            let project_candidates = candidates
                .iter()
                .filter(|c| c.project_key == project.project_key)
                .collect::<Vec<_>>();

            let actual_session_count = project_candidates.len() as i64;
            let latest_activity_at = project_candidates
                .iter()
                .map(|c| c.last_activity_at.as_str())
                .max()
                .unwrap_or(&target_watermark_str);

            let project_path = project_candidates
                .first()
                .and_then(|c| c.project_path.clone());

            let project_title = if project.project_key == "unassigned" {
                "未归属会话".to_string()
            } else {
                project_path
                    .as_deref()
                    .and_then(|p| std::path::Path::new(p).file_name()?.to_str())
                    .unwrap_or(&project.project_key)
                    .to_string()
            };

            let summary = if project.no_material_change && project.summary.trim().is_empty() {
                "本时间窗口内无重大变更".to_string()
            } else {
                project.summary
            };

            sqlx::query(
                "INSERT INTO recent_memory_snapshot_projects (\
                    tenant_id, id, snapshot_id, project_key, project_title, project_path, summary, \
                    no_material_change, latest_activity_at, source_session_count, sort_order\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )
            .bind(tenant_id)
            .bind(&project_row_id)
            .bind(&snapshot_id)
            .bind(&project.project_key)
            .bind(&project_title)
            .bind(&project_path)
            .bind(&summary)
            .bind(if project.no_material_change { 1 } else { 0 })
            .bind(latest_activity_at)
            .bind(actual_session_count)
            .bind(proj_idx as i64)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;

            for (item_idx, item) in project.items.into_iter().enumerate() {
                let mut resolved_item_id = None;
                let mut resolved_rev_num = 1i64;
                let mut resolved_supersedes_id = None;
                let mut first_seen_str = target_watermark_str.clone();

                if let Some(ref prior_id) = item.continues_item_id {
                    let prior_row = sqlx::query(
                        "SELECT mi.id, mi.project_key, mi.lifecycle, mi.first_seen_at, mir.revision_number, mir.status, mir.id as rev_id \
                         FROM memory_items mi \
                         JOIN memory_item_revisions mir ON mi.current_revision_id = mir.id \
                         WHERE mi.tenant_id = ?1 AND mi.id = ?2",
                    )
                    .bind(tenant_id)
                    .bind(prior_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(AppError::external)?;

                    if let Some(row) = prior_row {
                        let prior_proj: Option<String> = row.get("project_key");
                        let prior_lifecycle: String = row.get("lifecycle");
                        let prior_first_seen: String = row.get("first_seen_at");
                        let prior_status: String = row.get("status");
                        let prior_rev: i64 = row.get("revision_number");
                        let prior_rev_id: String = row.get("rev_id");

                        let proj_match = prior_proj.as_deref().unwrap_or("unassigned")
                            == project.project_key.as_str();
                        let is_current = prior_lifecycle == "current";
                        let was_continuable_status =
                            matches!(prior_status.as_str(), "active" | "blocked" | "waiting");

                        let within_7_days = match (
                            DateTime::parse_from_rfc3339(&target_watermark_str),
                            DateTime::parse_from_rfc3339(&prior_first_seen),
                        ) {
                            (Ok(tw), Ok(fs)) => {
                                (tw.signed_duration_since(fs)).num_seconds() <= 7 * 86400
                            }
                            _ => false,
                        };

                        if proj_match && is_current && was_continuable_status && within_7_days {
                            resolved_item_id = Some(prior_id.clone());
                            resolved_rev_num = prior_rev + 1;
                            resolved_supersedes_id = Some(prior_rev_id);
                            first_seen_str = prior_first_seen;
                        } else {
                            // M35-L1-08 §7.2: 超过 7 天上限或原条目已终态/项目不匹配：旧条目 retired
                            sqlx::query(
                                "UPDATE memory_items SET lifecycle = 'retired', updated_at = ?3 \
                                 WHERE tenant_id = ?1 AND id = ?2",
                            )
                            .bind(tenant_id)
                            .bind(prior_id)
                            .bind(&published_at)
                            .execute(&mut *tx)
                            .await
                            .map_err(AppError::external)?;
                        }
                    }
                }

                let item_id =
                    resolved_item_id.unwrap_or_else(|| format!("item-{}", uuid::Uuid::new_v4()));
                let revision_id = format!("rev-{}", uuid::Uuid::new_v4());

                // M35-L1-09: 终态条目在当前 Snapshot 展示一次后 lifecycle 标记为 retired/superseded，退出 L1
                let new_lifecycle = if item.status == MemoryItemStatus::Superseded {
                    "superseded"
                } else if item.status.is_terminal() {
                    "retired"
                } else {
                    "current"
                };

                // Upsert memory_items
                sqlx::query(
                    "INSERT INTO memory_items (\
                        tenant_id, id, layer, project_key, current_revision_id, lifecycle, \
                        first_seen_at, last_seen_at, created_at, updated_at\
                     ) VALUES (?1, ?2, 'l1', ?3, ?4, ?5, ?6, ?7, ?8, ?8) \
                     ON CONFLICT (tenant_id, id) DO UPDATE SET \
                        current_revision_id = excluded.current_revision_id, \
                        lifecycle = excluded.lifecycle, \
                        last_seen_at = excluded.last_seen_at, \
                        updated_at = excluded.updated_at",
                )
                .bind(tenant_id)
                .bind(&item_id)
                .bind(&project.project_key)
                .bind(&revision_id)
                .bind(new_lifecycle)
                .bind(&first_seen_str)
                .bind(&target_watermark_str)
                .bind(&published_at)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                // Evidence fingerprint for revision
                let mut rev_hasher = Sha256::new();
                rev_hasher.update(item.title.as_bytes());
                rev_hasher.update(item.summary.as_bytes());
                for r in &item.source_refs {
                    rev_hasher.update(r.as_bytes());
                }
                let evidence_fingerprint = format!("{:x}", rev_hasher.finalize());

                // Insert memory_item_revisions
                sqlx::query(
                    "INSERT INTO memory_item_revisions (\
                        tenant_id, id, item_id, revision_number, category, status, title, summary, \
                        rationale, recommendation_rank, promotion_nomination, occurred_at, \
                        evidence_fingerprint, generated_by_snapshot_id, supersedes_revision_id, created_at\
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                )
                .bind(tenant_id)
                .bind(&revision_id)
                .bind(&item_id)
                .bind(resolved_rev_num)
                .bind(item.category.as_str())
                .bind(item.status.as_str())
                .bind(&item.title)
                .bind(&item.summary)
                .bind(&item.rationale)
                .bind(item.recommendation_rank)
                .bind(item.promotion_nomination.as_str())
                .bind(&item.occurred_at)
                .bind(&evidence_fingerprint)
                .bind(&snapshot_id)
                .bind(resolved_supersedes_id)
                .bind(&published_at)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                // Insert recent_memory_snapshot_items
                let snapshot_item_row_id = format!("{}-{}", snapshot_id, item_id);
                let display_date: String = item.occurred_at.chars().take(10).collect();
                sqlx::query(
                    "INSERT INTO recent_memory_snapshot_items (\
                        tenant_id, id, snapshot_id, project_key, item_id, item_revision_id, display_date, sort_order\
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )
                .bind(tenant_id)
                .bind(&snapshot_item_row_id)
                .bind(&snapshot_id)
                .bind(&project.project_key)
                .bind(&item_id)
                .bind(&revision_id)
                .bind(&display_date)
                .bind(item_idx as i64)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                // Insert source references (去重插入)
                let mut seen_refs = HashSet::new();
                for ref_key in item.source_refs {
                    if !seen_refs.insert(ref_key.clone()) {
                        continue;
                    }
                    if let Some(resolved) = ref_map.get(&ref_key) {
                        let ref_id = format!("ref-{}", uuid::Uuid::new_v4());
                        sqlx::query(
                            "INSERT INTO memory_item_source_references (\
                                tenant_id, id, item_revision_id, record_kind, source_id, session_id, \
                                question_id, turn_id, part_id, node_id, node_order, reference_key, \
                                source_revision, availability, unavailable_reason, unavailable_at, created_at\
                             ) VALUES (?1, ?2, ?3, 'session', ?4, ?5, ?6, ?7, NULL, ?8, NULL, ?9, ?10, 'available', NULL, NULL, ?11)",
                        )
                        .bind(tenant_id)
                        .bind(&ref_id)
                        .bind(&revision_id)
                        .bind(&resolved.source_id)
                        .bind(&resolved.session_id)
                        .bind(&resolved.question_id)
                        .bind(&resolved.turn_id)
                        .bind(&resolved.node_id)
                        .bind(&resolved.reference_key)
                        .bind(resolved.source_revision)
                        .bind(&published_at)
                        .execute(&mut *tx)
                        .await
                        .map_err(AppError::external)?;
                    }
                }

                // 晋升观察 (非 unassigned 且有 nomination)
                if item.promotion_nomination != MemoryPromotionNomination::None
                    && project.project_key != "unassigned"
                {
                    let obs_id = format!("obs-{}", uuid::Uuid::new_v4());
                    sqlx::query(
                        "INSERT INTO memory_promotion_observations (\
                            tenant_id, id, item_id, item_revision_id, snapshot_id, nomination, \
                            evidence_fingerprint, project_key, observed_at\
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    )
                    .bind(tenant_id)
                    .bind(&obs_id)
                    .bind(&item_id)
                    .bind(&revision_id)
                    .bind(&snapshot_id)
                    .bind(item.promotion_nomination.as_str())
                    .bind(&evidence_fingerprint)
                    .bind(&project.project_key)
                    .bind(&published_at)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::external)?;
                }
            }
        }

        // 计算提交后有效内容指纹 (包含当前生成条目状态)，确保后续无变化水位能够精准触发复用
        let settings = self.app_settings_value();
        let memory_settings = settings
            .get("memory")
            .and_then(|v| {
                serde_json::from_value::<
                        crate::backend::infrastructure::app_settings::MemorySettings,
                    >(v.clone())
                    .ok()
            })
            .unwrap_or_default();

        let (effective_content_fp, current_long_term_revisions) = self
            .compute_post_commit_content_fingerprint(
                &mut *tx,
                tenant_id,
                target,
                candidates,
                skill_binding,
                &memory_settings,
            )
            .await?;

        if let Some(expected) = expected_long_term_revisions {
            if expected != current_long_term_revisions.as_slice() {
                return Err(AppError::Domain {
                    code: "MEMORY_RESULT_STALE".to_string(),
                    message: "Long-term memory changed before publication".to_string(),
                    retryable: false,
                    details: None,
                });
            }
        }

        sqlx::query(
            "UPDATE recent_memory_snapshots SET content_fingerprint = ?3 \
             WHERE tenant_id = ?1 AND id = ?2",
        )
        .bind(tenant_id)
        .bind(&snapshot_id)
        .bind(&effective_content_fp)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;

        // 3. 更新 recent_memory_state (原子更新 last-success，清除错误)
        let state_id = format!("state-{}", tenant_id);
        sqlx::query(
            "INSERT INTO recent_memory_state (\
                tenant_id, id, last_successful_snapshot_id, latest_attempt_task_id, \
                latest_attempt_error_code, latest_attempt_error_message, latest_attempt_error_retryable, created_at, updated_at\
             ) VALUES (?1, ?2, ?3, NULL, NULL, NULL, NULL, ?4, ?4) \
             ON CONFLICT (tenant_id) DO UPDATE SET \
                last_successful_snapshot_id = excluded.last_successful_snapshot_id, \
                latest_attempt_error_code = NULL, \
                latest_attempt_error_message = NULL, \
                latest_attempt_error_retryable = NULL, \
                updated_at = excluded.updated_at",
        )
        .bind(tenant_id)
        .bind(&state_id)
        .bind(&snapshot_id)
        .bind(&published_at)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;

        if let Some((job_id, ownership_token)) = job_lease {
            let finalized = sqlx::query(
                "UPDATE recent_memory_jobs SET status = 'succeeded', retry_at = NULL, \
                 last_error_code = NULL, last_error_message = NULL, finished_at = ?1, \
                 ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, updated_at = ?1 \
                 WHERE tenant_id = ?2 AND id = ?3 AND status = 'running' \
                 AND ownership_token = ?4 AND lease_expires_at > ?1",
            )
            .bind(&published_at)
            .bind(tenant_id)
            .bind(job_id)
            .bind(ownership_token)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;
            if finalized.rows_affected() != 1 {
                return Err(AppError::Conflict(
                    "recent memory job lease is no longer owned".to_string(),
                ));
            }
        }

        tx.commit().await.map_err(AppError::external)?;

        let loaded = crate::backend::application::memory::recent::load_recent_snapshot_view_by_id(
            pool,
            tenant_id,
            &snapshot_id,
        )
        .await?
        .ok_or_else(|| {
            AppError::external("Committed snapshot not found immediately after commit")
        })?;

        Ok(loaded)
    }
}
