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
    pub(crate) async fn load_current_long_term_revision_ids(&self) -> AppResult<Vec<String>> {
        sqlx::query_scalar(
            "SELECT current_revision_id FROM memory_items
             WHERE tenant_id = ?1 AND layer IN ('l2', 'l3')
               AND lifecycle = 'current' AND current_revision_id IS NOT NULL
             ORDER BY layer ASC, project_key ASC, id ASC",
        )
        .bind(self.tenant_id())
        .fetch_all(self.db.pool())
        .await
        .map_err(AppError::external)
    }

    /// Ensures that every Session candidate in the exact frozen Recent
    /// watermark window has Phase-1 work. The candidate set must come from
    /// the same watermark calculation as Phase 2; deriving a second rolling
    /// `now - 48h` window can silently omit sessions near the lower bound.
    pub(crate) async fn ensure_recent_snapshot_phase1_jobs_at<Tz: chrono::TimeZone>(
        &self,
        now: DateTime<Tz>,
        restart_terminal: bool,
    ) -> AppResult<Option<(WatermarkTarget, usize)>> {
        let memory_settings = self.backend_settings()?.memory.clone();
        if !memory_settings.generation_enabled {
            return Ok(None);
        }

        let now_text = now.with_timezone(&Utc).to_rfc3339();
        let target = resolve_target_watermark(
            now,
            memory_settings.recent_window_hours,
            &memory_settings.watermark_time_1,
            &memory_settings.watermark_time_2,
        )?;
        let (candidates, _) = self
            .collect_recent_snapshot_candidates(target.target_watermark_utc, target.window_hours)
            .await?;
        let session_ids = candidates
            .into_iter()
            .map(|candidate| candidate.session_id)
            .collect::<Vec<_>>();
        let internal_agent_workspace =
            crate::backend::infrastructure::agent_execution::agent_execution_workspace_root(
                &self.db_path,
            );
        let prepared = store::ensure_session_memory_jobs_for_sessions_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &session_ids,
            &internal_agent_workspace,
            &now_text,
            restart_terminal,
        )
        .await?;

        Ok(Some((target, prepared)))
    }

    /// 根据目标水位与窗口小时数收集候选 Session 及对应短引用映射表 (M35-L1-03/04)
    pub(crate) async fn collect_recent_snapshot_candidates(
        &self,
        target_watermark_utc: DateTime<Utc>,
        window_hours: i64,
    ) -> AppResult<(Vec<CandidateSession>, HashMap<String, ResolvedEvidenceRef>)> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let settings = self.app_settings_value();

        let cutoff = target_watermark_utc - Duration::hours(window_hours);
        let cutoff_str = cutoff.to_rfc3339();
        let watermark_str = target_watermark_utc.to_rfc3339();

        let internal_agent_workspace =
            crate::backend::infrastructure::agent_execution::agent_execution_workspace_root(
                &self.db_path,
            );

        let records = store::list_recent_conversation_sessions_sqlx(
            pool,
            tenant_id,
            &cutoff_str,
            &watermark_str,
            &internal_agent_workspace,
        )
        .await?;

        let registered_roots = store::load_sources_sqlx(pool, tenant_id)
            .await?
            .into_iter()
            .filter_map(|source| source.repo_root)
            .collect::<Vec<_>>();

        let memory_settings = settings
            .get("memory")
            .and_then(|v| {
                serde_json::from_value::<
                        crate::backend::infrastructure::app_settings::MemorySettings,
                    >(v.clone())
                    .ok()
            })
            .unwrap_or_default();

        let excluded_sessions = memory_settings
            .excluded_session_ids
            .into_iter()
            .collect::<HashSet<_>>();
        let excluded_sources = memory_settings
            .excluded_source_ids
            .into_iter()
            .collect::<HashSet<_>>();

        let mut candidates = Vec::new();
        for record in records {
            let session_id = &record.session.session.id;
            let source_id = &record.session.session.source_id;

            if excluded_sessions.contains(session_id) || excluded_sources.contains(source_id) {
                continue;
            }

            let last_activity_at = match crate::backend::domain::parse_conversation_timestamp(
                &record.last_activity_at,
            ) {
                Some(dt) => dt,
                None => continue,
            };

            // M35-L1-03: 左闭右闭 window_start_utc <= last_activity_at <= window_end_utc
            if last_activity_at < cutoff || last_activity_at > target_watermark_utc {
                continue;
            }

            let raw_project_path = record.cwd.as_deref().or(record
                .session
                .session
                .project_path
                .as_deref());

            let project_path = raw_project_path
                .and_then(|path| resolve_project_directory(path, &registered_roots));
            let project_key = project_path
                .clone()
                .unwrap_or_else(|| "unassigned".to_string());

            candidates.push(CandidateSession {
                tenant_id: tenant_id.to_string(),
                session_id: session_id.clone(),
                source_id: source_id.clone(),
                session_title: record.session.session.title.clone(),
                source_agent: record.source_agent.clone(),
                project_key,
                project_path,
                last_activity_at: last_activity_at.to_rfc3339(),
                source_revision: 1,
                short_ref: String::new(),
            });
        }

        // 稳定排序: project_key -> last_activity_at 降序 -> session_id 升序
        candidates.sort_by(|left, right| {
            left.project_key
                .cmp(&right.project_key)
                .then_with(|| right.last_activity_at.cmp(&left.last_activity_at))
                .then_with(|| left.session_id.cmp(&right.session_id))
        });

        let session_ids = candidates
            .iter()
            .map(|candidate| candidate.session_id.clone())
            .collect::<Vec<_>>();
        let source_references = store::list_session_memory_source_references_for_sessions_sqlx(
            pool,
            tenant_id,
            &session_ids,
        )
        .await?;

        let mut ref_map = HashMap::new();
        for (idx, candidate) in candidates.iter_mut().enumerate() {
            let short_ref = format!("s{}", idx + 1);
            candidate.short_ref = short_ref.clone();

            let canonical_reference = source_references
                .get(&candidate.session_id)
                .and_then(|references| references.first());
            if let Some(reference) = canonical_reference {
                candidate.source_revision = reference.source_revision;
            }

            ref_map.insert(
                short_ref.clone(),
                ResolvedEvidenceRef {
                    short_ref,
                    source_id: canonical_reference
                        .map(|reference| reference.source_id.clone())
                        .unwrap_or_else(|| candidate.source_id.clone()),
                    session_id: candidate.session_id.clone(),
                    project_key: candidate.project_key.clone(),
                    session_title: candidate.session_title.clone(),
                    source_agent: candidate.source_agent.clone(),
                    last_activity_at: candidate.last_activity_at.clone(),
                    reference_key: canonical_reference
                        .map(|reference| reference.reference_key.clone())
                        .unwrap_or_else(|| {
                            format!("{}/{}", candidate.source_id, candidate.session_id)
                        }),
                    source_revision: candidate.source_revision,
                    question_id: canonical_reference
                        .and_then(|reference| reference.question_id.clone()),
                    turn_id: canonical_reference.and_then(|reference| reference.turn_id.clone()),
                    node_id: canonical_reference.and_then(|reference| reference.node_id.clone()),
                },
            );
            if let Some(references) = source_references.get(&candidate.session_id) {
                extend_source_reference_aliases(&mut ref_map, candidate, references);
            }
        }

        Ok((candidates, ref_map))
    }

    /// M35-L3-04: 同步来源与会话有效性，并将来源已失效的未晋升 L1 条目退出 (lifecycle = 'retired')
    /// 注意：已晋升到 L2/L3 的条目保持知识不删，仅引用标记为 unavailable
    pub(crate) async fn sync_source_availability_and_retire_unpromoted_items(
        &self,
        now_str: &str,
    ) -> AppResult<()> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();

        // 1. conversation_source 被禁用 (enabled = 0)
        sqlx::query(
            "UPDATE memory_item_source_references \
             SET availability = 'unavailable', \
                 unavailable_reason = 'source_disabled', \
                 unavailable_at = COALESCE(unavailable_at, ?2) \
             WHERE tenant_id = ?1 \
               AND availability = 'available' \
               AND source_id IN (SELECT id FROM conversation_sources WHERE tenant_id = ?1 AND enabled = 0)",
        )
        .bind(tenant_id)
        .bind(now_str)
        .execute(pool)
        .await
        .map_err(AppError::external)?;

        // 2. conversation_session 缺失 (missing = 1)
        sqlx::query(
            "UPDATE memory_item_source_references \
             SET availability = 'unavailable', \
                 unavailable_reason = 'missing', \
                 unavailable_at = COALESCE(unavailable_at, ?2) \
             WHERE tenant_id = ?1 \
               AND availability = 'available' \
               AND session_id IN (SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND missing = 1)",
        )
        .bind(tenant_id)
        .bind(now_str)
        .execute(pool)
        .await
        .map_err(AppError::external)?;

        // 3. conversation_source 或 conversation_session 被删除
        sqlx::query(
            "UPDATE memory_item_source_references \
             SET availability = 'unavailable', \
                 unavailable_reason = 'deleted', \
                 unavailable_at = COALESCE(unavailable_at, ?2) \
             WHERE tenant_id = ?1 \
               AND availability = 'available' \
               AND ( \
                   source_id NOT IN (SELECT id FROM conversation_sources WHERE tenant_id = ?1) \
                   OR session_id NOT IN (SELECT id FROM conversation_sessions WHERE tenant_id = ?1) \
               )",
        )
        .bind(tenant_id)
        .bind(now_str)
        .execute(pool)
        .await
        .map_err(AppError::external)?;

        // 4. 设置中排除的 source 或 session
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

        for excluded_source in &memory_settings.excluded_source_ids {
            sqlx::query(
                "UPDATE memory_item_source_references \
                 SET availability = 'unavailable', \
                     unavailable_reason = 'excluded', \
                     unavailable_at = COALESCE(unavailable_at, ?2) \
                 WHERE tenant_id = ?1 \
                   AND availability = 'available' \
                   AND source_id = ?3",
            )
            .bind(tenant_id)
            .bind(now_str)
            .bind(excluded_source)
            .execute(pool)
            .await
            .map_err(AppError::external)?;
        }

        for excluded_session in &memory_settings.excluded_session_ids {
            sqlx::query(
                "UPDATE memory_item_source_references \
                 SET availability = 'unavailable', \
                     unavailable_reason = 'excluded', \
                     unavailable_at = COALESCE(unavailable_at, ?2) \
                 WHERE tenant_id = ?1 \
                   AND availability = 'available' \
                   AND session_id = ?3",
            )
            .bind(tenant_id)
            .bind(now_str)
            .bind(excluded_session)
            .execute(pool)
            .await
            .map_err(AppError::external)?;
        }

        // 5. M35-L3-04: 来源失效使未晋升 L1 退出 (layer = 'l1' AND lifecycle = 'current')
        // 已晋升的 L2/L3 保持不变，仅引用标记 unavailable
        sqlx::query(
            "UPDATE memory_items \
             SET lifecycle = 'retired', \
                 updated_at = ?2 \
             WHERE tenant_id = ?1 \
               AND layer = 'l1' \
               AND lifecycle = 'current' \
               AND NOT EXISTS ( \
                   SELECT 1 \
                   FROM memory_item_source_references sr \
                   JOIN memory_item_revisions mir ON sr.item_revision_id = mir.id \
                   WHERE mir.tenant_id = memory_items.tenant_id \
                     AND mir.item_id = memory_items.id \
                     AND sr.availability = 'available' \
               )",
        )
        .bind(tenant_id)
        .bind(now_str)
        .execute(pool)
        .await
        .map_err(AppError::external)?;

        Ok(())
    }

    /// M35-L1-08 / M35-L1-10: 收集未完成事项 (active/blocked/waiting)，跨窗口延续最长 7 天
    /// 超过 7 天则自动退休，输入完全基于结构化事实与引用，严禁读取旧 Markdown
    pub(crate) async fn collect_continuable_items(
        &self,
        target_watermark_utc: &DateTime<Utc>,
    ) -> AppResult<Vec<ContinuableMemoryItemView>> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let target_watermark_str = target_watermark_utc.to_rfc3339();

        // 1. 同步来源有效性并淘汰来源失效的未晋升 L1
        self.sync_source_availability_and_retire_unpromoted_items(&target_watermark_str)
            .await?;

        // 2. 查询活跃/阻塞/等待中的当前 L1 条目
        let rows = sqlx::query(
            "SELECT mi.id as item_id, mi.project_key, mi.first_seen_at, mi.last_seen_at, \
                    mir.id as rev_id, mir.revision_number, mir.category, mir.status, \
                    mir.title, mir.summary, mir.rationale, mir.evidence_fingerprint \
             FROM memory_items mi \
             JOIN memory_item_revisions mir ON mi.current_revision_id = mir.id \
             WHERE mi.tenant_id = ?1 \
               AND mi.layer = 'l1' \
               AND mi.lifecycle = 'current' \
               AND mir.status IN ('active', 'blocked', 'waiting') \
             ORDER BY mir.occurred_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;

        let mut continuable = Vec::new();

        for row in rows {
            let item_id: String = row.get("item_id");
            let first_seen_at: String = row.get("first_seen_at");
            let last_seen_at: String = row.get("last_seen_at");
            let rev_id: String = row.get("rev_id");
            let current_revision_number: i64 = row.get("revision_number");
            let project_key: Option<String> = row.get("project_key");
            let category_str: String = row.get("category");
            let status_str: String = row.get("status");
            let title: String = row.get("title");
            let summary: String = row.get("summary");
            let rationale: String = row.get("rationale");
            let evidence_fingerprint: String = row.get("evidence_fingerprint");

            // 计算生命周期 (最长 7 天)
            let fs_dt = match DateTime::parse_from_rfc3339(&first_seen_at) {
                Ok(dt) => dt.with_timezone(&Utc),
                Err(_) => continue,
            };

            let seconds_elapsed = (*target_watermark_utc - fs_dt).num_seconds();
            if seconds_elapsed > 7 * 86400 {
                // 超过 7 天上限，退休
                sqlx::query(
                    "UPDATE memory_items SET lifecycle = 'retired', updated_at = ?3 \
                     WHERE tenant_id = ?1 AND id = ?2",
                )
                .bind(tenant_id)
                .bind(&item_id)
                .bind(&target_watermark_str)
                .execute(pool)
                .await
                .map_err(AppError::external)?;
                continue;
            }

            let days_since_first_seen = (seconds_elapsed.max(0) / 86400).min(7);
            let remaining_days = (7 - days_since_first_seen).max(0);

            // 查询有效引用
            let ref_rows = sqlx::query(
                "SELECT reference_key FROM memory_item_source_references \
                 WHERE tenant_id = ?1 AND item_revision_id = ?2 AND availability = 'available'",
            )
            .bind(tenant_id)
            .bind(&rev_id)
            .fetch_all(pool)
            .await
            .map_err(AppError::external)?;

            let source_refs: Vec<String> = ref_rows
                .into_iter()
                .map(|r| r.get("reference_key"))
                .collect();

            let category = match category_str.as_str() {
                "decision" => MemoryItemCategory::Decision,
                "research" => MemoryItemCategory::Research,
                "verification" => MemoryItemCategory::Verification,
                "blocker" => MemoryItemCategory::Blocker,
                "follow_up" => MemoryItemCategory::FollowUp,
                _ => MemoryItemCategory::Progress,
            };

            let status = match status_str.as_str() {
                "blocked" => MemoryItemStatus::Blocked,
                "waiting" => MemoryItemStatus::Waiting,
                _ => MemoryItemStatus::Active,
            };

            continuable.push(ContinuableMemoryItemView {
                item_id,
                project_key: project_key.unwrap_or_else(|| "unassigned".to_string()),
                category,
                status,
                title,
                summary,
                rationale,
                first_seen_at,
                last_seen_at,
                days_since_first_seen,
                remaining_days,
                current_revision_id: rev_id,
                current_revision_number,
                evidence_fingerprint,
                source_refs,
            });
        }

        Ok(continuable)
    }
}
