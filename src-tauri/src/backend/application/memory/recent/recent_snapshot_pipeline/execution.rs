use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::memory::RecentMemorySnapshotView;
use crate::backend::store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row, Sqlite};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};
impl AppService {
    pub(crate) async fn execute_recent_memory_snapshot_pipeline(
        &self,
        target_watermark_utc: DateTime<Utc>,
        window_hours: i64,
        result: MemoryGenerationResult,
    ) -> AppResult<RecentMemorySnapshotView> {
        let target = WatermarkTarget {
            target_watermark_utc,
            local_watermark_date: target_watermark_utc.format("%Y-%m-%d").to_string(),
            local_watermark_time: target_watermark_utc.format("%H:%M").to_string(),
            timezone_offset_minutes: 0,
            window_hours,
            window_start_utc: target_watermark_utc - Duration::hours(window_hours),
            window_end_utc: target_watermark_utc,
        };

        let (candidates, ref_map) = self
            .collect_recent_snapshot_candidates(target.target_watermark_utc, target.window_hours)
            .await?;

        // 校验门禁
        if let Err(e) = self.validate_memory_generation_result(&result, &candidates, &ref_map) {
            let (code, msg, retryable) = match &e {
                AppError::Domain {
                    code,
                    message,
                    retryable,
                    ..
                } => (code.clone(), message.clone(), *retryable),
                other => (
                    "VALIDATION_FAILED".to_string(),
                    other.to_string(),
                    other.retryable(),
                ),
            };
            self.record_recent_memory_failure(&code, &msg, retryable)
                .await?;
            return Err(e);
        }

        let skill_binding = self.get_active_generation_skill_binding().await?;

        let target_fingerprint = compute_target_fingerprint(
            self.tenant_id(),
            &target.target_watermark_utc,
            target.window_hours,
            &candidates,
            &[],
            &skill_binding,
        );

        let content_fingerprint = compute_content_fingerprint(
            target.window_hours,
            &candidates,
            &[],
            &[],
            &skill_binding,
            &[],
            &[],
        );

        let snapshot_view = self
            .commit_recent_memory_snapshot(
                &target,
                &skill_binding,
                result,
                &candidates,
                &ref_map,
                &target_fingerprint,
                &content_fingerprint,
                None,
                None,
            )
            .await?;

        Ok(snapshot_view)
    }

    /// 在单个 SQLite 事务内原子提交无变化复用的 L1 Snapshot (M35-L1-06)
    /// 推进水位与 last-success，保留原始 content_generated_at，且不增加晋升观察次数
    pub(crate) async fn commit_reused_memory_snapshot(
        &self,
        target: &WatermarkTarget,
        skill_binding: &MemorySkillBinding,
        prior_snapshot_id: &str,
        target_fingerprint: &str,
        content_fingerprint: &str,
    ) -> AppResult<RecentMemorySnapshotView> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();

        let mut tx = pool.begin().await.map_err(AppError::external)?;

        let prior_row = sqlx::query(
            "SELECT content_generated_at FROM recent_memory_snapshots WHERE tenant_id = ?1 AND id = ?2",
        )
        .bind(tenant_id)
        .bind(prior_snapshot_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::external)?;

        let content_generated_at: String = prior_row.get("content_generated_at");

        let snapshot_id = format!("snap-{}", uuid::Uuid::new_v4());
        let now = Utc::now();
        let published_at = now.to_rfc3339();

        let window_start_utc = target.window_start_utc.to_rfc3339();
        let window_end_utc = target.window_end_utc.to_rfc3339();
        let target_watermark_str = target.target_watermark_utc.to_rfc3339();

        // 清理同一 watermark 的历史快照冲突（若存在，排除 prior_snapshot_id）
        Self::remove_conflicting_recent_snapshots(
            &mut tx,
            tenant_id,
            &target_watermark_str,
            target.window_hours,
            Some(prior_snapshot_id),
        )
        .await?;

        let seq_row = sqlx::query(
            "SELECT COALESCE(MAX(sequence), 0) + 1 AS next_seq FROM recent_memory_snapshots WHERE tenant_id = ?1",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::external)?;
        let sequence: i64 = seq_row.get("next_seq");

        sqlx::query(
            "INSERT INTO recent_memory_snapshots (\
                tenant_id, id, sequence, target_watermark_utc, local_watermark_date, \
                local_watermark_time, timezone_offset_minutes, window_hours, window_start_utc, \
                window_end_utc, publication_kind, reused_from_snapshot_id, target_fingerprint, \
                content_fingerprint, generation_skill_asset_id, generation_skill_revision, \
                generation_skill_content_hash, contract_version, budget_policy_version, \
                projection_policy_version, content_generated_at, published_at\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'reused', ?11, ?12, ?13, ?14, ?15, ?16, 'memory.contract.v2', 'budget.v1', 'projection.v2', ?17, ?18)",
        )
        .bind(tenant_id)
        .bind(&snapshot_id)
        .bind(sequence)
        .bind(&target_watermark_str)
        .bind(&target.local_watermark_date)
        .bind(&target.local_watermark_time)
        .bind(target.timezone_offset_minutes)
        .bind(target.window_hours)
        .bind(&window_start_utc)
        .bind(&window_end_utc)
        .bind(Some(prior_snapshot_id))
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

        // 复制 project rows
        sqlx::query(
            "INSERT INTO recent_memory_snapshot_projects (\
                tenant_id, id, snapshot_id, project_key, project_title, project_path, summary, \
                no_material_change, latest_activity_at, source_session_count, sort_order\
             ) SELECT tenant_id, ?1 || '-' || sort_order, ?1, project_key, project_title, project_path, \
                      summary, no_material_change, latest_activity_at, source_session_count, sort_order \
               FROM recent_memory_snapshot_projects WHERE tenant_id = ?2 AND snapshot_id = ?3",
        )
        .bind(&snapshot_id)
        .bind(tenant_id)
        .bind(prior_snapshot_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;

        // 复制 item rows
        sqlx::query(
            "INSERT INTO recent_memory_snapshot_items (\
                tenant_id, id, snapshot_id, project_key, item_id, item_revision_id, display_date, sort_order\
             ) SELECT tenant_id, ?1 || '-' || sort_order, ?1, project_key, item_id, item_revision_id, display_date, sort_order \
               FROM recent_memory_snapshot_items WHERE tenant_id = ?2 AND snapshot_id = ?3",
        )
        .bind(&snapshot_id)
        .bind(tenant_id)
        .bind(prior_snapshot_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;

        // M35-L1-06 / M35-L2-04: reused Snapshot 不增加 L2/L3 晋升观察次数

        // 推进 last-success
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

        tx.commit().await.map_err(AppError::external)?;

        let loaded = crate::backend::application::memory::recent::load_recent_snapshot_view_by_id(
            pool,
            tenant_id,
            &snapshot_id,
        )
        .await?
        .ok_or_else(|| {
            AppError::external("Committed reused snapshot not found immediately after commit")
        })?;

        Ok(loaded)
    }

    /// 评估并执行双水位调度与无变化复用 (M35-L1-01–06)
    pub(crate) async fn evaluate_and_run_recent_snapshot<Tz: chrono::TimeZone>(
        &self,
        now: Option<DateTime<Tz>>,
        mock_result: Option<MemoryGenerationResult>,
    ) -> AppResult<Option<RecentMemorySnapshotView>> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
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

        if !memory_settings.generation_enabled {
            return Ok(None);
        }

        let target = if let Some(custom_now) = now {
            resolve_target_watermark(
                custom_now,
                memory_settings.recent_window_hours,
                &memory_settings.watermark_time_1,
                &memory_settings.watermark_time_2,
            )?
        } else {
            let utc_now = Utc::now();
            resolve_target_watermark(
                utc_now,
                memory_settings.recent_window_hours,
                &memory_settings.watermark_time_1,
                &memory_settings.watermark_time_2,
            )?
        };

        let state = crate::backend::application::memory::recent::load_recent_memory_state_view(
            pool, tenant_id,
        )
        .await?;

        let (candidates, ref_map) = self
            .collect_recent_snapshot_candidates(target.target_watermark_utc, target.window_hours)
            .await?;

        let skill_binding = self.get_active_generation_skill_binding().await?;

        let continuable_items = self
            .collect_continuable_items(&target.target_watermark_utc)
            .await?;

        let prior_items_for_target: Vec<(&str, &str)> = continuable_items
            .iter()
            .map(|i| (i.item_id.as_str(), i.current_revision_id.as_str()))
            .collect();

        let carry_over_items_for_content: Vec<(&str, &str, i64)> = continuable_items
            .iter()
            .map(|i| {
                (
                    i.item_id.as_str(),
                    i.current_revision_id.as_str(),
                    i.remaining_days,
                )
            })
            .collect();
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
            &prior_items_for_target,
            &skill_binding,
        );

        let content_fingerprint = compute_content_fingerprint(
            target.window_hours,
            &candidates,
            &carry_over_items_for_content,
            &current_l2_l3_revision_refs,
            &skill_binding,
            &memory_settings.excluded_session_ids,
            &memory_settings.excluded_source_ids,
        );

        let _evidence_pack = self.build_recent_snapshot_work_order_evidence_pack(
            &target,
            &candidates,
            &continuable_items,
        );

        // 幂等检查 (M35-L1-05): 如果已有成功 Snapshot 且目标水位已处理或 target_fingerprint 相同，跳过
        if let Some(ref last_snap) = state.snapshot {
            if let Some(last_meta) =
                store::load_recent_snapshot_meta_by_id_sqlx(pool, tenant_id, &last_snap.snapshot_id)
                    .await?
            {
                if last_meta.target_watermark_utc == target.target_watermark_utc.to_rfc3339()
                    || last_meta.target_fingerprint == target_fingerprint
                {
                    return Ok(None);
                }

                // Reuse 检查: 如果 content_fingerprint 相同，执行无变化复用 (M35-L1-06)
                if last_meta.content_fingerprint == content_fingerprint {
                    let reused = self
                        .commit_reused_memory_snapshot(
                            &target,
                            &skill_binding,
                            &last_meta.id,
                            &target_fingerprint,
                            &content_fingerprint,
                        )
                        .await?;
                    return Ok(Some(reused));
                }
            }
        }

        // 内容变化或首次生成
        if let Some(result) = mock_result {
            if let Err(e) = self.validate_memory_generation_result(&result, &candidates, &ref_map) {
                let (code, msg, retryable) = match &e {
                    AppError::Domain {
                        code,
                        message,
                        retryable,
                        ..
                    } => (code.clone(), message.clone(), *retryable),
                    other => (
                        "VALIDATION_FAILED".to_string(),
                        other.to_string(),
                        other.retryable(),
                    ),
                };
                self.record_recent_memory_failure(&code, &msg, retryable)
                    .await?;
                return Err(e);
            }

            let snap = self
                .commit_recent_memory_snapshot(
                    &target,
                    &skill_binding,
                    result,
                    &candidates,
                    &ref_map,
                    &target_fingerprint,
                    &content_fingerprint,
                    None,
                    None,
                )
                .await?;
            return Ok(Some(snap));
        }

        Ok(None)
    }

    /// 记录最近生成失败（保持原有 last-success 不变）
    pub(crate) async fn record_recent_memory_failure(
        &self,
        error_code: &str,
        error_message: &str,
        retryable: bool,
    ) -> AppResult<()> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let state_id = format!("state-{}", tenant_id);
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO recent_memory_state (\
                tenant_id, id, last_successful_snapshot_id, latest_attempt_task_id, \
                latest_attempt_error_code, latest_attempt_error_message, latest_attempt_error_retryable, created_at, updated_at\
             ) VALUES (?1, ?2, NULL, NULL, ?3, ?4, ?5, ?6, ?6) \
             ON CONFLICT (tenant_id) DO UPDATE SET \
                latest_attempt_error_code = excluded.latest_attempt_error_code, \
                latest_attempt_error_message = excluded.latest_attempt_error_message, \
                latest_attempt_error_retryable = excluded.latest_attempt_error_retryable, \
                updated_at = excluded.updated_at",
        )
        .bind(tenant_id)
        .bind(&state_id)
        .bind(error_code)
        .bind(error_message)
        .bind(if retryable { 1 } else { 0 })
        .bind(&now)
        .execute(pool)
        .await
        .map_err(AppError::external)?;

        Ok(())
    }
}
