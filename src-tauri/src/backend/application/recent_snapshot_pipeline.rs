use super::prelude::*;
use super::recent::resolve_project_directory;
use crate::backend::{
    dto::{RecentMemorySnapshotView, RecentSnapshotPublicationKind},
    models::{
        CandidateSession, CandidateSessionSummary, ContinuableMemoryItemView,
        MemoryGenerationResultV2, MemoryItemCategory, MemoryItemStatus, MemoryPromotionNomination,
        MemorySkillBinding, RecentSnapshotWorkOrderEvidencePack, ResolvedEvidenceRef,
        ALLOWED_MEMORY_GENERATION_TOOLS,
    },
    runtime::{AppError, AppResult},
    store,
};
use chrono::{DateTime, Duration, Offset, TimeZone, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WatermarkTarget {
    pub(crate) target_watermark_utc: DateTime<Utc>,
    pub(crate) local_watermark_date: String,
    pub(crate) local_watermark_time: String,
    pub(crate) timezone_offset_minutes: i64,
    pub(crate) window_hours: i64,
    pub(crate) window_start_utc: DateTime<Utc>,
    pub(crate) window_end_utc: DateTime<Utc>,
}

pub(crate) fn parse_hh_mm(time_str: &str) -> AppResult<chrono::NaiveTime> {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 2 {
        return Err(AppError::Validation(format!(
            "Invalid time format '{}', expected HH:MM",
            time_str
        )));
    }
    let h: u32 = parts[0].parse().map_err(|_| {
        AppError::Validation(format!(
            "Invalid hour '{}' in time '{}'",
            parts[0], time_str
        ))
    })?;
    let m: u32 = parts[1].parse().map_err(|_| {
        AppError::Validation(format!(
            "Invalid minute '{}' in time '{}'",
            parts[1], time_str
        ))
    })?;
    if parts[0].len() != 2 || parts[1].len() != 2 || h >= 24 || m >= 60 {
        return Err(AppError::Validation(format!(
            "Invalid time '{}', hour must be 00-23 and minute 00-59",
            time_str
        )));
    }
    chrono::NaiveTime::from_hms_opt(h, m, 0)
        .ok_or_else(|| AppError::Validation(format!("Invalid time '{}'", time_str)))
}

fn resolve_local_time_with_dst<Tz: chrono::TimeZone>(
    tz: &Tz,
    naive_dt: chrono::NaiveDateTime,
) -> Option<DateTime<Tz>> {
    match tz.from_local_datetime(&naive_dt) {
        chrono::LocalResult::Single(dt) => Some(dt),
        chrono::LocalResult::Ambiguous(earliest, _latest) => {
            // M35 spec §2.2: 歧义时间选择第一次出现的 instant
            Some(earliest)
        }
        chrono::LocalResult::None => {
            // M35 spec §2.2: 不存在的本地墙上时间顺延到该日期第一个有效 instant
            let mut probe = naive_dt + chrono::Duration::minutes(1);
            let end_of_day = naive_dt.date().and_hms_opt(23, 59, 59)?;
            while probe <= end_of_day {
                match tz.from_local_datetime(&probe) {
                    chrono::LocalResult::Single(dt) | chrono::LocalResult::Ambiguous(dt, _) => {
                        return Some(dt);
                    }
                    chrono::LocalResult::None => {
                        probe += chrono::Duration::minutes(1);
                    }
                }
            }
            None
        }
    }
}

pub(crate) fn resolve_target_watermark<Tz: chrono::TimeZone>(
    now: DateTime<Tz>,
    window_hours: u32,
    watermark_1: &str,
    watermark_2: &str,
) -> AppResult<WatermarkTarget> {
    if window_hours != 24 && window_hours != 48 && window_hours != 72 {
        return Err(AppError::Validation(format!(
            "MEMORY_SCHEDULE_INVALID: invalid window hours {}, allowed: 24, 48, 72",
            window_hours
        )));
    }
    if watermark_1 == watermark_2 {
        return Err(AppError::Validation(
            "MEMORY_SCHEDULE_INVALID: watermark times must be different".to_string(),
        ));
    }
    let t1 = parse_hh_mm(watermark_1)?;
    let t2 = parse_hh_mm(watermark_2)?;

    let tz = now.timezone();
    let today = now.date_naive();
    let yesterday = today - chrono::Duration::days(1);

    let naive_candidates = [
        yesterday.and_time(t1),
        yesterday.and_time(t2),
        today.and_time(t1),
        today.and_time(t2),
    ];

    let mut valid_candidates: Vec<DateTime<Tz>> = Vec::new();
    for ndt in naive_candidates {
        if let Some(dt) = resolve_local_time_with_dst(&tz, ndt) {
            if dt.with_timezone(&Utc) <= now.with_timezone(&Utc) {
                valid_candidates.push(dt);
            }
        }
    }

    valid_candidates.sort_by_key(|dt| dt.with_timezone(&Utc));

    let chosen_dt = valid_candidates.into_iter().last().ok_or_else(|| {
        AppError::Validation("No valid watermark candidate found <= current time".to_string())
    })?;

    let target_watermark_utc = chosen_dt.with_timezone(&Utc);
    let local_watermark_date = chosen_dt.naive_local().format("%Y-%m-%d").to_string();
    let local_watermark_time = chosen_dt.naive_local().format("%H:%M").to_string();
    let timezone_offset_minutes = chosen_dt.offset().fix().local_minus_utc() as i64 / 60;
    let window_end_utc = target_watermark_utc;
    let window_start_utc = target_watermark_utc - chrono::Duration::hours(window_hours as i64);

    Ok(WatermarkTarget {
        target_watermark_utc,
        local_watermark_date,
        local_watermark_time,
        timezone_offset_minutes,
        window_hours: window_hours as i64,
        window_start_utc,
        window_end_utc,
    })
}

#[derive(Debug, Serialize)]
struct TargetFingerprintSessionRef<'a> {
    session_id: &'a str,
    source_revision: i64,
}

#[derive(Debug, Serialize)]
struct TargetFingerprintItemRef<'a> {
    item_id: &'a str,
    revision_id: &'a str,
}

#[derive(Debug, Serialize)]
struct TargetFingerprintPayload<'a> {
    budget_policy_version: &'static str,
    candidate_sessions: Vec<TargetFingerprintSessionRef<'a>>,
    contract_version: &'static str,
    prior_active_items: Vec<TargetFingerprintItemRef<'a>>,
    projection_policy_version: &'static str,
    skill_asset_id: Option<&'a str>,
    skill_content_hash: Option<&'a str>,
    skill_revision: i64,
    target_watermark_utc: &'a str,
    tenant_id: &'a str,
    window_hours: i64,
}

#[derive(Debug, Serialize)]
struct ContentFingerprintSessionRef<'a> {
    last_activity_at: &'a str,
    session_id: &'a str,
    source_revision: i64,
}

#[derive(Debug, Serialize)]
struct ContentFingerprintItemRef<'a> {
    item_id: &'a str,
    remaining_lifetime_bucket: i64,
    revision_id: &'a str,
}

#[derive(Debug, Serialize)]
struct ContentFingerprintPayload<'a> {
    candidate_sessions: Vec<ContentFingerprintSessionRef<'a>>,
    carry_over_items: Vec<ContentFingerprintItemRef<'a>>,
    contract_version: &'static str,
    current_l2_l3_revisions: Vec<&'a str>,
    excluded_session_ids: &'a [String],
    excluded_source_ids: &'a [String],
    skill_asset_id: Option<&'a str>,
    skill_content_hash: Option<&'a str>,
    skill_revision: i64,
    window_hours: i64,
}

pub(crate) fn compute_target_fingerprint(
    tenant_id: &str,
    target_watermark_utc: &DateTime<Utc>,
    window_hours: i64,
    candidates: &[CandidateSession],
    prior_active_items: &[(&str, &str)],
    skill_binding: &MemorySkillBinding,
) -> String {
    let mut sorted_candidates: Vec<TargetFingerprintSessionRef> = candidates
        .iter()
        .map(|c| TargetFingerprintSessionRef {
            session_id: &c.session_id,
            source_revision: c.source_revision,
        })
        .collect();
    sorted_candidates.sort_by_key(|c| c.session_id);

    let mut sorted_items: Vec<TargetFingerprintItemRef> = prior_active_items
        .iter()
        .map(|(item_id, rev_id)| TargetFingerprintItemRef {
            item_id,
            revision_id: rev_id,
        })
        .collect();
    sorted_items.sort_by_key(|i| i.item_id);

    let target_watermark_str = target_watermark_utc.to_rfc3339();

    let payload = TargetFingerprintPayload {
        budget_policy_version: "budget.v1",
        candidate_sessions: sorted_candidates,
        contract_version: "memory.contract.v2",
        prior_active_items: sorted_items,
        projection_policy_version: "projection.v2",
        skill_asset_id: Some(skill_binding.asset_id.as_str()),
        skill_content_hash: Some(skill_binding.content_hash.as_str()),
        skill_revision: skill_binding.asset_revision,
        target_watermark_utc: &target_watermark_str,
        tenant_id,
        window_hours,
    };

    let canonical_json = serde_json::to_string(&payload).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(canonical_json.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn compute_content_fingerprint(
    window_hours: i64,
    candidates: &[CandidateSession],
    carry_over_items: &[(&str, &str, i64)],
    current_l2_l3_revisions: &[&str],
    skill_binding: &MemorySkillBinding,
    excluded_session_ids: &[String],
    excluded_source_ids: &[String],
) -> String {
    let mut sorted_candidates: Vec<ContentFingerprintSessionRef> = candidates
        .iter()
        .map(|c| ContentFingerprintSessionRef {
            session_id: &c.session_id,
            last_activity_at: &c.last_activity_at,
            source_revision: c.source_revision,
        })
        .collect();
    sorted_candidates.sort_by_key(|c| c.session_id);

    let mut sorted_items: Vec<ContentFingerprintItemRef> = carry_over_items
        .iter()
        .map(|(item_id, rev_id, bucket)| ContentFingerprintItemRef {
            item_id,
            revision_id: rev_id,
            remaining_lifetime_bucket: *bucket,
        })
        .collect();
    sorted_items.sort_by_key(|i| i.item_id);

    let mut sorted_l2_l3 = current_l2_l3_revisions.to_vec();
    sorted_l2_l3.sort();

    let payload = ContentFingerprintPayload {
        candidate_sessions: sorted_candidates,
        carry_over_items: sorted_items,
        contract_version: "memory.contract.v2",
        current_l2_l3_revisions: sorted_l2_l3,
        excluded_session_ids,
        excluded_source_ids,
        skill_asset_id: Some(skill_binding.asset_id.as_str()),
        skill_content_hash: Some(skill_binding.content_hash.as_str()),
        skill_revision: skill_binding.asset_revision,
        window_hours,
    };

    let canonical_json = serde_json::to_string(&payload).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(canonical_json.as_bytes());
    format!("{:x}", hasher.finalize())
}

impl AppService {
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
            crate::backend::ai_execution::agent_execution_workspace_root(&self.db_path);

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
                serde_json::from_value::<crate::backend::app_settings::MemorySettings>(v.clone())
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

            let last_activity_at = match crate::backend::models::parse_conversation_timestamp(
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

        let mut ref_map = HashMap::new();
        for (idx, candidate) in candidates.iter_mut().enumerate() {
            let short_ref = format!("s{}", idx + 1);
            candidate.short_ref = short_ref.clone();

            ref_map.insert(
                short_ref.clone(),
                ResolvedEvidenceRef {
                    short_ref,
                    source_id: candidate.source_id.clone(),
                    session_id: candidate.session_id.clone(),
                    session_title: candidate.session_title.clone(),
                    source_agent: candidate.source_agent.clone(),
                    last_activity_at: candidate.last_activity_at.clone(),
                    reference_key: format!("{}/{}", candidate.source_id, candidate.session_id),
                    source_revision: candidate.source_revision,
                    question_id: None,
                    turn_id: None,
                    node_id: None,
                },
            );
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
                serde_json::from_value::<crate::backend::app_settings::MemorySettings>(v.clone())
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

    /// M35-L1-10 / M35-L1-11: 构建 Work Order 证据首包，包含结构化候选 Session 与可续接条目，绝不读取 Markdown
    pub(crate) fn build_recent_snapshot_work_order_evidence_pack(
        &self,
        target: &WatermarkTarget,
        candidates: &[CandidateSession],
        continuable_items: &[ContinuableMemoryItemView],
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
            continuable_items: continuable_items.to_vec(),
            allowed_tools,
            output_schema_version: 2,
        }
    }

    /// 校验 Agent 输出是否符合准入与质量门禁 (M35-L1-07/12, Schema, Coverage, Refs)
    pub(crate) fn validate_memory_generation_result(
        &self,
        result: &MemoryGenerationResultV2,
        candidates: &[CandidateSession],
        ref_map: &HashMap<String, ResolvedEvidenceRef>,
    ) -> AppResult<()> {
        if result.schema_version != 2 {
            return Err(AppError::Validation(format!(
                "Unsupported memory generation schema version: {}",
                result.schema_version
            )));
        }

        // 1. Coverage 门禁: 预算耗尽或不可读 Session 发生时直接拒绝发布
        if result.coverage.budget_exhausted {
            return Err(AppError::Domain {
                code: "MEMORY_BUDGET_EXHAUSTED".to_string(),
                message: "Memory generation budget exhausted, refusing publication".to_string(),
                retryable: false,
                details: None,
            });
        }
        if !result.coverage.unreadable_sessions.is_empty() {
            return Err(AppError::Domain {
                code: "MEMORY_COVERAGE_INCOMPLETE".to_string(),
                message: format!(
                    "Unreadable sessions detected: {:?}",
                    result.coverage.unreadable_sessions
                ),
                retryable: true,
                details: None,
            });
        }

        // 所有 candidate 必须被 covered_sessions 或 no_memory_sessions 覆盖
        let covered_set = result
            .coverage
            .covered_sessions
            .iter()
            .chain(result.coverage.no_memory_sessions.iter())
            .cloned()
            .collect::<HashSet<_>>();

        for candidate in candidates {
            if !covered_set.contains(&candidate.short_ref) {
                return Err(AppError::Domain {
                    code: "MEMORY_COVERAGE_INCOMPLETE".to_string(),
                    message: format!(
                        "Candidate session {} (short ref {}) not covered in generation result",
                        candidate.session_id, candidate.short_ref
                    ),
                    retryable: true,
                    details: None,
                });
            }
        }

        // 2. Project & Item 校验
        let candidate_project_keys = candidates
            .iter()
            .map(|c| c.project_key.as_str())
            .collect::<HashSet<_>>();

        for project in &result.projects {
            let key = project.project_key.trim();
            if key.is_empty() {
                return Err(AppError::Validation(
                    "Project key in memory generation result cannot be empty".to_string(),
                ));
            }

            // 项目 key 必须来自候选集或为 unassigned
            if !candidate_project_keys.contains(key)
                && key != "unassigned"
                && !candidates.is_empty()
            {
                return Err(AppError::Validation(format!(
                    "Project key '{}' not present in candidate work order",
                    key
                )));
            }

            // 摘要长度
            if !project.no_material_change
                && (project.summary.trim().is_empty() || project.summary.len() > 2000)
            {
                return Err(AppError::Validation(format!(
                    "Project '{}' summary must be 1..2000 chars when no_material_change is false",
                    key
                )));
            }

            // 建议排序 (0..3 个 recommendation_rank, 1..=3 且不重复)
            let mut rank_set = HashSet::new();
            for item in &project.items {
                if let Some(rank) = item.recommendation_rank {
                    if !(1..=3).contains(&rank) {
                        return Err(AppError::Validation(format!(
                            "Recommendation rank {} in project '{}' is out of range 1..=3",
                            rank, key
                        )));
                    }
                    if !rank_set.insert(rank) {
                        return Err(AppError::Validation(format!(
                            "Duplicate recommendation rank {} in project '{}'",
                            rank, key
                        )));
                    }
                    if item.source_refs.is_empty() {
                        return Err(AppError::Validation(format!(
                            "Recommendation item '{}' in project '{}' must have at least one source ref",
                            item.title, key
                        )));
                    }
                }

                // 字段长度
                let title = item.title.trim();
                if title.is_empty() || title.len() > 200 {
                    return Err(AppError::Validation(format!(
                        "Item title must be 1..200 chars, got length {}",
                        title.len()
                    )));
                }
                if item.summary.trim().is_empty() || item.summary.len() > 2000 {
                    return Err(AppError::Validation(format!(
                        "Item summary must be 1..2000 chars, got length {}",
                        item.summary.len()
                    )));
                }
                if item.rationale.trim().is_empty() || item.rationale.len() > 2000 {
                    return Err(AppError::Validation(format!(
                        "Item rationale must be 1..2000 chars, got length {}",
                        item.rationale.len()
                    )));
                }

                // 引用校验: 所有 source_refs 必须合法在 ref_map 中存在
                for ref_key in &item.source_refs {
                    if !ref_map.contains_key(ref_key) {
                        return Err(AppError::Validation(format!(
                            "Unknown source reference '{}' in item '{}'",
                            ref_key, item.title
                        )));
                    }
                }

                // M35-L1-12: unassigned 项目不能被提名晋升 L2
                if key == "unassigned"
                    && item.promotion_nomination != MemoryPromotionNomination::None
                {
                    return Err(AppError::Validation(
                        "Items in unassigned project cannot be nominated for promotion".to_string(),
                    ));
                }
            }

            if rank_set.len() > 3 {
                return Err(AppError::Validation(format!(
                    "Project '{}' exceeds maximum 3 recommendations",
                    key
                )));
            }
        }

        Ok(())
    }

    /// 在单个 SQLite 事务内原子提交生成的 L1 Snapshot，并推进 last-success (M35-AUTH-01/05, M35-L1-07/12)
    pub(crate) async fn commit_recent_memory_snapshot(
        &self,
        target: &WatermarkTarget,
        skill_binding: &MemorySkillBinding,
        result: MemoryGenerationResultV2,
        candidates: &[CandidateSession],
        ref_map: &HashMap<String, ResolvedEvidenceRef>,
        target_fingerprint: &str,
        content_fingerprint: &str,
    ) -> AppResult<RecentMemorySnapshotView> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();

        let mut tx = pool.begin().await.map_err(AppError::external)?;

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
                serde_json::from_value::<crate::backend::app_settings::MemorySettings>(v.clone())
                    .ok()
            })
            .unwrap_or_default();

        let active_rows = sqlx::query(
            "SELECT mi.id as item_id, mir.id as rev_id, mi.first_seen_at \
             FROM memory_items mi \
             JOIN memory_item_revisions mir ON mi.current_revision_id = mir.id \
             WHERE mi.tenant_id = ?1 \
               AND mi.layer = 'l1' \
               AND mi.lifecycle = 'current' \
               AND mir.status IN ('active', 'blocked', 'waiting')",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::external)?;

        let mut carry_over: Vec<(String, String, i64)> = Vec::new();
        for r in active_rows {
            let item_id: String = r.get("item_id");
            let rev_id: String = r.get("rev_id");
            let fs: String = r.get("first_seen_at");
            let days_elapsed = match DateTime::parse_from_rfc3339(&fs) {
                Ok(dt) => {
                    (target.target_watermark_utc - dt.with_timezone(&Utc))
                        .num_seconds()
                        .max(0)
                        / 86400
                }
                Err(_) => 0,
            };
            let rem_bucket = (7 - days_elapsed).max(0);
            carry_over.push((item_id, rev_id, rem_bucket));
        }

        let carry_over_refs: Vec<(&str, &str, i64)> = carry_over
            .iter()
            .map(|(i, r, b)| (i.as_str(), r.as_str(), *b))
            .collect();

        let effective_content_fp = compute_content_fingerprint(
            target.window_hours,
            candidates,
            &carry_over_refs,
            &[],
            skill_binding,
            &memory_settings.excluded_session_ids,
            &memory_settings.excluded_source_ids,
        );

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
                latest_attempt_error_code, latest_attempt_error_message, created_at, updated_at\
             ) VALUES (?1, ?2, ?3, NULL, NULL, NULL, ?4, ?4) \
             ON CONFLICT (tenant_id) DO UPDATE SET \
                last_successful_snapshot_id = excluded.last_successful_snapshot_id, \
                latest_attempt_error_code = NULL, \
                latest_attempt_error_message = NULL, \
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

        let loaded = store::load_recent_snapshot_by_id_sqlx(pool, tenant_id, &snapshot_id)
            .await?
            .ok_or_else(|| {
                AppError::external("Committed snapshot not found immediately after commit")
            })?;

        Ok(loaded)
    }

    /// 执行固定水位 L1 Snapshot 管线并记录失败审计 (M35-L1-01/03/04/07/12)
    pub(crate) async fn execute_recent_memory_snapshot_pipeline(
        &self,
        target_watermark_utc: DateTime<Utc>,
        window_hours: i64,
        result: MemoryGenerationResultV2,
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
            let (code, msg) = match &e {
                AppError::Domain { code, message, .. } => (code.clone(), message.clone()),
                other => ("VALIDATION_FAILED".to_string(), other.to_string()),
            };
            self.record_recent_memory_failure(&code, &msg).await?;
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
                latest_attempt_error_code, latest_attempt_error_message, created_at, updated_at\
             ) VALUES (?1, ?2, ?3, NULL, NULL, NULL, ?4, ?4) \
             ON CONFLICT (tenant_id) DO UPDATE SET \
                last_successful_snapshot_id = excluded.last_successful_snapshot_id, \
                latest_attempt_error_code = NULL, \
                latest_attempt_error_message = NULL, \
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

        let loaded = store::load_recent_snapshot_by_id_sqlx(pool, tenant_id, &snapshot_id)
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
        mock_result: Option<MemoryGenerationResultV2>,
    ) -> AppResult<Option<RecentMemorySnapshotView>> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let settings = self.app_settings_value();

        let memory_settings = settings
            .get("memory")
            .and_then(|v| {
                serde_json::from_value::<crate::backend::app_settings::MemorySettings>(v.clone())
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

        let state = store::load_recent_memory_state_sqlx(pool, tenant_id).await?;

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
            &[],
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
                let (code, msg) = match &e {
                    AppError::Domain { code, message, .. } => (code.clone(), message.clone()),
                    other => ("VALIDATION_FAILED".to_string(), other.to_string()),
                };
                self.record_recent_memory_failure(&code, &msg).await?;
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
    ) -> AppResult<()> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let state_id = format!("state-{}", tenant_id);
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO recent_memory_state (\
                tenant_id, id, last_successful_snapshot_id, latest_attempt_task_id, \
                latest_attempt_error_code, latest_attempt_error_message, created_at, updated_at\
             ) VALUES (?1, ?2, NULL, NULL, ?3, ?4, ?5, ?5) \
             ON CONFLICT (tenant_id) DO UPDATE SET \
                latest_attempt_error_code = excluded.latest_attempt_error_code, \
                latest_attempt_error_message = excluded.latest_attempt_error_message, \
                updated_at = excluded.updated_at",
        )
        .bind(tenant_id)
        .bind(&state_id)
        .bind(error_code)
        .bind(error_message)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(AppError::external)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::dto::RecentMemoryStatus;
    use crate::backend::models::{
        MemoryGenerationCoverageV2, MemoryGenerationItemV2, MemoryGenerationProjectV2,
        MemoryGenerationResultV2, MemoryItemCategory, MemoryItemStatus, MemoryPromotionNomination,
    };
    use std::fs;

    async fn setup_test_service() -> (AppService, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-snapshot-pipeline-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create test root");
        let db_path = root.join("app.db");

        let service = AppService::open_with_db_path(db_path)
            .await
            .expect("open service");

        (service, root)
    }

    async fn seed_test_conversation_data(service: &AppService) {
        let pool = service.db.pool();

        // 1. Insert conversation source
        sqlx::query(
            r#"
            INSERT INTO conversation_sources (
                tenant_id, id, adapter_id, name, kind, location, config_json, enabled,
                last_synced_at, last_sync_status, created_at, updated_at
            ) VALUES (
                'default', 'source-alpha', 'adapter-claude', 'Alpha Source', 'local_folder',
                '/tmp/source', '{}', 1, '2026-09-14T00:00:00Z', 'idle',
                '2026-09-14T00:00:00Z', '2026-09-14T00:00:00Z'
            )
            "#,
        )
        .execute(pool)
        .await
        .expect("insert source");

        // Target watermark: 2026-09-15T12:00:00Z, 48h cutoff: 2026-09-13T12:00:00Z

        // Session 1: in window (10h before watermark), project alpha
        insert_test_session(
            pool,
            "session-in-alpha",
            "source-alpha",
            "Alpha In-Window",
            Some("/tmp/alpha-project"),
            "2026-09-15T02:00:00Z",
        )
        .await;

        // Session 2: in window (30h before watermark), unassigned (no project)
        insert_test_session(
            pool,
            "session-in-unassigned",
            "source-alpha",
            "Unassigned In-Window",
            None,
            "2026-09-14T06:00:00Z",
        )
        .await;

        // Session 3: too old (outside 48h window: 50h before watermark)
        insert_test_session(
            pool,
            "session-too-old",
            "source-alpha",
            "Too Old Session",
            Some("/tmp/alpha-project"),
            "2026-09-13T10:00:00Z",
        )
        .await;

        // Session 4: in future (after watermark)
        insert_test_session(
            pool,
            "session-in-future",
            "source-alpha",
            "Future Session",
            Some("/tmp/alpha-project"),
            "2026-09-15T13:00:00Z",
        )
        .await;
    }

    async fn insert_test_session(
        pool: &sqlx::SqlitePool,
        session_id: &str,
        source_id: &str,
        title: &str,
        project_path: Option<&str>,
        activity_at: &str,
    ) {
        sqlx::query(
            r#"
            INSERT INTO conversation_sessions (
                tenant_id, id, source_id, adapter_id, external_id, title,
                project_path, started_at, updated_at, source_locator,
                source_fingerprint, missing, user_visible, created_at, imported_at
            ) VALUES (
                'default', ?1, ?2, 'adapter-claude', ?1, ?3, ?4,
                ?5, ?5, 'loc', 'fp', 0, 1, ?5, ?5
            )
            "#,
        )
        .bind(session_id)
        .bind(source_id)
        .bind(title)
        .bind(project_path)
        .bind(activity_at)
        .execute(pool)
        .await
        .expect("insert test session");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recent_snapshot_pipeline_candidate_selection() {
        let (service, _root) = setup_test_service().await;
        seed_test_conversation_data(&service).await;

        // Session 5: in window (12h before watermark), but will be excluded via settings
        insert_test_session(
            service.db.pool(),
            "session-excluded",
            "source-alpha",
            "Excluded Session",
            Some("/tmp/alpha-project"),
            "2026-09-15T00:00:00Z",
        )
        .await;

        // Configure exclusion in settings
        service
            .save_app_settings(serde_json::json!({
                "memory": {
                    "generationEnabled": true,
                    "usageEnabled": true,
                    "recentWindowHours": 48,
                    "watermarkTime1": "02:00",
                    "watermarkTime2": "14:00",
                    "generationSkillAssetId": null,
                    "excludedSessionIds": ["session-excluded"],
                    "excludedSourceIds": []
                }
            }))
            .await
            .expect("save settings");

        let target_watermark: DateTime<Utc> = "2026-09-15T12:00:00Z".parse().unwrap();
        let (candidates, ref_map) = service
            .collect_recent_snapshot_candidates(target_watermark, 48)
            .await
            .expect("collect candidates");

        // Exactly 2 candidates: session-in-alpha and session-in-unassigned
        assert_eq!(candidates.len(), 2);

        let alpha_candidate = candidates
            .iter()
            .find(|c| c.session_id == "session-in-alpha")
            .expect("find alpha candidate");
        assert_eq!(alpha_candidate.project_key, "/tmp/alpha-project");
        assert_eq!(
            alpha_candidate.project_path.as_deref(),
            Some("/tmp/alpha-project")
        );
        assert!(!alpha_candidate.short_ref.is_empty());

        let unassigned_candidate = candidates
            .iter()
            .find(|c| c.session_id == "session-in-unassigned")
            .expect("find unassigned candidate");
        assert_eq!(unassigned_candidate.project_key, "unassigned");
        assert_eq!(unassigned_candidate.project_path, None);
        assert!(!unassigned_candidate.short_ref.is_empty());

        // Verify ref_map has both short refs
        assert!(ref_map.contains_key(&alpha_candidate.short_ref));
        assert!(ref_map.contains_key(&unassigned_candidate.short_ref));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recent_snapshot_pipeline_quality_gates() {
        let (service, _root) = setup_test_service().await;
        seed_test_conversation_data(&service).await;

        let target_watermark: DateTime<Utc> = "2026-09-15T12:00:00Z".parse().unwrap();
        let (candidates, ref_map) = service
            .collect_recent_snapshot_candidates(target_watermark, 48)
            .await
            .expect("collect candidates");
        assert_eq!(candidates.len(), 2);

        let ref1 = &candidates[0].short_ref;
        let ref2 = &candidates[1].short_ref;

        // Gate 1: Coverage Incomplete (omitted s2)
        let incomplete_result = MemoryGenerationResultV2 {
            schema_version: 2,
            projects: vec![MemoryGenerationProjectV2 {
                project_key: candidates[0].project_key.clone(),
                summary: "Summary".to_string(),
                no_material_change: false,
                source_sessions: vec![ref1.clone()],
                items: vec![],
            }],
            coverage: MemoryGenerationCoverageV2 {
                covered_sessions: vec![ref1.clone()],
                no_memory_sessions: vec![],
                unreadable_sessions: vec![],
                budget_exhausted: false,
            },
            unknowns: vec![],
        };
        let err = service
            .execute_recent_memory_snapshot_pipeline(target_watermark, 48, incomplete_result)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not covered"));

        // Verify state has failure and last-success is None
        let state = service
            .get_recent_memory_snapshot()
            .await
            .expect("get state");
        assert_eq!(state.status, RecentMemoryStatus::UpdateFailed);
        assert!(state.snapshot.is_none());
        assert!(state.latest_attempt_error.is_some());

        // Gate 2: Budget exhausted
        let budget_exhausted_result = MemoryGenerationResultV2 {
            schema_version: 2,
            projects: vec![],
            coverage: MemoryGenerationCoverageV2 {
                covered_sessions: vec![ref1.clone(), ref2.clone()],
                no_memory_sessions: vec![],
                unreadable_sessions: vec![],
                budget_exhausted: true,
            },
            unknowns: vec![],
        };
        let err = service
            .validate_memory_generation_result(&budget_exhausted_result, &candidates, &ref_map)
            .unwrap_err();
        assert!(err.to_string().contains("budget exhausted"));

        // Gate 3: Invalid reference key
        let invalid_ref_result = MemoryGenerationResultV2 {
            schema_version: 2,
            projects: vec![MemoryGenerationProjectV2 {
                project_key: candidates[0].project_key.clone(),
                summary: "Summary".to_string(),
                no_material_change: false,
                source_sessions: vec![ref1.clone()],
                items: vec![MemoryGenerationItemV2 {
                    continues_item_id: None,
                    category: MemoryItemCategory::Progress,
                    status: MemoryItemStatus::Active,
                    title: "Item 1".to_string(),
                    summary: "Item summary".to_string(),
                    rationale: "Rationale".to_string(),
                    occurred_at: "2026-09-15T01:00:00Z".to_string(),
                    recommendation_rank: None,
                    source_refs: vec!["unknown-ref-999".to_string()],
                    promotion_nomination: MemoryPromotionNomination::None,
                }],
            }],
            coverage: MemoryGenerationCoverageV2 {
                covered_sessions: vec![ref1.clone(), ref2.clone()],
                no_memory_sessions: vec![],
                unreadable_sessions: vec![],
                budget_exhausted: false,
            },
            unknowns: vec![],
        };
        let err = service
            .validate_memory_generation_result(&invalid_ref_result, &candidates, &ref_map)
            .unwrap_err();
        assert!(err.to_string().contains("Unknown source reference"));

        // Gate 4: Duplicate recommendation rank
        let duplicate_rank_result = MemoryGenerationResultV2 {
            schema_version: 2,
            projects: vec![MemoryGenerationProjectV2 {
                project_key: candidates[0].project_key.clone(),
                summary: "Summary".to_string(),
                no_material_change: false,
                source_sessions: vec![ref1.clone()],
                items: vec![
                    MemoryGenerationItemV2 {
                        continues_item_id: None,
                        category: MemoryItemCategory::Decision,
                        status: MemoryItemStatus::Verified,
                        title: "Rec 1".to_string(),
                        summary: "Summary".to_string(),
                        rationale: "Rationale".to_string(),
                        occurred_at: "2026-09-15T01:00:00Z".to_string(),
                        recommendation_rank: Some(1),
                        source_refs: vec![ref1.clone()],
                        promotion_nomination: MemoryPromotionNomination::None,
                    },
                    MemoryGenerationItemV2 {
                        continues_item_id: None,
                        category: MemoryItemCategory::FollowUp,
                        status: MemoryItemStatus::Active,
                        title: "Rec 2".to_string(),
                        summary: "Summary".to_string(),
                        rationale: "Rationale".to_string(),
                        occurred_at: "2026-09-15T01:00:00Z".to_string(),
                        recommendation_rank: Some(1), // duplicate!
                        source_refs: vec![ref1.clone()],
                        promotion_nomination: MemoryPromotionNomination::None,
                    },
                ],
            }],
            coverage: MemoryGenerationCoverageV2 {
                covered_sessions: vec![ref1.clone(), ref2.clone()],
                no_memory_sessions: vec![],
                unreadable_sessions: vec![],
                budget_exhausted: false,
            },
            unknowns: vec![],
        };
        let err = service
            .validate_memory_generation_result(&duplicate_rank_result, &candidates, &ref_map)
            .unwrap_err();
        assert!(err.to_string().contains("Duplicate recommendation rank"));

        // Gate 5: M35-L1-12 Unassigned project cannot nominate promotion
        let unassigned_nomination_result = MemoryGenerationResultV2 {
            schema_version: 2,
            projects: vec![
                MemoryGenerationProjectV2 {
                    project_key: "/tmp/alpha-project".to_string(),
                    summary: "Alpha summary".to_string(),
                    no_material_change: false,
                    source_sessions: vec![ref1.clone()],
                    items: vec![],
                },
                MemoryGenerationProjectV2 {
                    project_key: "unassigned".to_string(),
                    summary: "Unassigned summary".to_string(),
                    no_material_change: false,
                    source_sessions: vec![ref2.clone()],
                    items: vec![MemoryGenerationItemV2 {
                        continues_item_id: None,
                        category: MemoryItemCategory::Decision,
                        status: MemoryItemStatus::Verified,
                        title: "Unassigned decision".to_string(),
                        summary: "Summary".to_string(),
                        rationale: "Rationale".to_string(),
                        occurred_at: "2026-09-15T01:00:00Z".to_string(),
                        recommendation_rank: None,
                        source_refs: vec![ref2.clone()],
                        promotion_nomination: MemoryPromotionNomination::ProjectDecision, // Not allowed!
                    }],
                },
            ],
            coverage: MemoryGenerationCoverageV2 {
                covered_sessions: vec![ref1.clone(), ref2.clone()],
                no_memory_sessions: vec![],
                unreadable_sessions: vec![],
                budget_exhausted: false,
            },
            unknowns: vec![],
        };
        let err = service
            .validate_memory_generation_result(&unassigned_nomination_result, &candidates, &ref_map)
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("unassigned project cannot be nominated"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recent_snapshot_pipeline_success_atomicity_and_preservation() {
        let (service, _root) = setup_test_service().await;
        seed_test_conversation_data(&service).await;

        let target_watermark: DateTime<Utc> = "2026-09-15T12:00:00Z".parse().unwrap();
        let (candidates, _ref_map) = service
            .collect_recent_snapshot_candidates(target_watermark, 48)
            .await
            .expect("collect candidates");
        assert_eq!(candidates.len(), 2);

        let alpha_candidate = candidates
            .iter()
            .find(|c| c.project_key == "/tmp/alpha-project")
            .unwrap();
        let unassigned_candidate = candidates
            .iter()
            .find(|c| c.project_key == "unassigned")
            .unwrap();

        let valid_result = MemoryGenerationResultV2 {
            schema_version: 2,
            projects: vec![
                MemoryGenerationProjectV2 {
                    project_key: "/tmp/alpha-project".to_string(),
                    summary: "Alpha project updates and next steps.".to_string(),
                    no_material_change: false,
                    source_sessions: vec![alpha_candidate.short_ref.clone()],
                    items: vec![
                        MemoryGenerationItemV2 {
                            continues_item_id: None,
                            category: MemoryItemCategory::Progress,
                            status: MemoryItemStatus::Active,
                            title: "Implemented database migrations".to_string(),
                            summary: "Completed schema foundation tables".to_string(),
                            rationale: "Needed for persistence".to_string(),
                            occurred_at: "2026-09-15T02:00:00Z".to_string(),
                            recommendation_rank: None,
                            source_refs: vec![alpha_candidate.short_ref.clone()],
                            promotion_nomination: MemoryPromotionNomination::None,
                        },
                        MemoryGenerationItemV2 {
                            continues_item_id: None,
                            category: MemoryItemCategory::Decision,
                            status: MemoryItemStatus::Verified,
                            title: "Decided to adopt single-tier symlinks".to_string(),
                            summary: "Direct symlinks from app dir to sources".to_string(),
                            rationale: "Reduces intermediate link complexity".to_string(),
                            occurred_at: "2026-09-15T02:00:00Z".to_string(),
                            recommendation_rank: Some(1),
                            source_refs: vec![alpha_candidate.short_ref.clone()],
                            promotion_nomination: MemoryPromotionNomination::ProjectDecision,
                        },
                    ],
                },
                MemoryGenerationProjectV2 {
                    project_key: "unassigned".to_string(),
                    summary: "Unassigned exploratory sessions.".to_string(),
                    no_material_change: false,
                    source_sessions: vec![unassigned_candidate.short_ref.clone()],
                    items: vec![MemoryGenerationItemV2 {
                        continues_item_id: None,
                        category: MemoryItemCategory::Blocker,
                        status: MemoryItemStatus::Blocked,
                        title: "Waiting on external API key".to_string(),
                        summary: "Cannot run tests without API key".to_string(),
                        rationale: "Third party vendor delay".to_string(),
                        occurred_at: "2026-09-14T06:00:00Z".to_string(),
                        recommendation_rank: Some(1),
                        source_refs: vec![unassigned_candidate.short_ref.clone()],
                        promotion_nomination: MemoryPromotionNomination::None,
                    }],
                },
            ],
            coverage: MemoryGenerationCoverageV2 {
                covered_sessions: vec![
                    alpha_candidate.short_ref.clone(),
                    unassigned_candidate.short_ref.clone(),
                ],
                no_memory_sessions: vec![],
                unreadable_sessions: vec![],
                budget_exhausted: false,
            },
            unknowns: vec![],
        };

        // 1. Commit snapshot successfully
        let snapshot_view = service
            .execute_recent_memory_snapshot_pipeline(target_watermark, 48, valid_result)
            .await
            .expect("execute pipeline successfully");

        assert_eq!(snapshot_view.window_hours, 48);
        assert_eq!(snapshot_view.projects.len(), 2);

        let alpha_proj = snapshot_view
            .projects
            .iter()
            .find(|p| p.project_key == "/tmp/alpha-project")
            .unwrap();
        assert_eq!(alpha_proj.items.len(), 2);
        let rec_item = alpha_proj
            .items
            .iter()
            .find(|i| i.recommendation_rank == Some(1))
            .unwrap();
        assert_eq!(rec_item.title, "Decided to adopt single-tier symlinks");
        assert_eq!(rec_item.session_references.len(), 1);
        assert_eq!(
            rec_item.session_references[0].session_id,
            "session-in-alpha"
        );

        let unassigned_proj = snapshot_view
            .projects
            .iter()
            .find(|p| p.project_key == "unassigned")
            .unwrap();
        assert_eq!(unassigned_proj.items.len(), 1);

        // Verify promotion observations in DB
        let obs_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM memory_promotion_observations WHERE tenant_id = 'default'",
        )
        .fetch_one(service.db.pool())
        .await
        .expect("count observations");
        // Only alpha project item had promotion nomination (unassigned had None)
        assert_eq!(obs_count.0, 1);

        // 2. Query state via public get_recent_memory_snapshot API
        let state = service
            .get_recent_memory_snapshot()
            .await
            .expect("get state");
        assert_eq!(state.status, RecentMemoryStatus::Ready);
        assert!(state.latest_attempt_error.is_none());
        let current_snap = state.snapshot.expect("has snapshot");
        assert_eq!(current_snap.snapshot_id, snapshot_view.snapshot_id);

        // 3. Trigger a failure on subsequent pipeline run -> last success must be preserved!
        let failing_result = MemoryGenerationResultV2 {
            schema_version: 2,
            projects: vec![],
            coverage: MemoryGenerationCoverageV2 {
                covered_sessions: vec![],
                no_memory_sessions: vec![],
                unreadable_sessions: vec!["unreadable".to_string()],
                budget_exhausted: false,
            },
            unknowns: vec![],
        };

        let err = service
            .execute_recent_memory_snapshot_pipeline(target_watermark, 48, failing_result)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Unreadable sessions"));

        let state_after_fail = service
            .get_recent_memory_snapshot()
            .await
            .expect("get state after fail");
        assert_eq!(state_after_fail.status, RecentMemoryStatus::UpdateFailed);
        assert!(state_after_fail.latest_attempt_error.is_some());
        let preserved_snap = state_after_fail.snapshot.expect("preserved snapshot");
        assert_eq!(preserved_snap.snapshot_id, snapshot_view.snapshot_id);
    }

    #[test]
    fn test_watermark_resolution_defaults_and_custom() {
        use chrono::FixedOffset;

        let tz = FixedOffset::east_opt(8 * 3600).unwrap(); // UTC+8

        // Test default 02:00 / 14:00 at various times of day:
        // 1. At 01:59 UTC+8 on 2026-09-15 -> should pick yesterday 14:00 UTC+8 (2026-09-14 14:00)
        let now_0159 = tz.with_ymd_and_hms(2026, 9, 15, 1, 59, 0).unwrap();
        let target = resolve_target_watermark(now_0159, 48, "02:00", "14:00").unwrap();
        assert_eq!(target.local_watermark_date, "2026-09-14");
        assert_eq!(target.local_watermark_time, "14:00");
        assert_eq!(target.timezone_offset_minutes, 480);
        assert_eq!(target.window_hours, 48);

        // 2. Exactly at 02:00 UTC+8 on 2026-09-15 -> picks today 02:00
        let now_0200 = tz.with_ymd_and_hms(2026, 9, 15, 2, 0, 0).unwrap();
        let target = resolve_target_watermark(now_0200, 48, "02:00", "14:00").unwrap();
        assert_eq!(target.local_watermark_date, "2026-09-15");
        assert_eq!(target.local_watermark_time, "02:00");

        // 3. At 13:59 UTC+8 on 2026-09-15 -> still picks today 02:00
        let now_1359 = tz.with_ymd_and_hms(2026, 9, 15, 13, 59, 0).unwrap();
        let target = resolve_target_watermark(now_1359, 48, "02:00", "14:00").unwrap();
        assert_eq!(target.local_watermark_date, "2026-09-15");
        assert_eq!(target.local_watermark_time, "02:00");

        // 4. At 14:00 UTC+8 on 2026-09-15 -> picks today 14:00
        let now_1400 = tz.with_ymd_and_hms(2026, 9, 15, 14, 0, 0).unwrap();
        let target = resolve_target_watermark(now_1400, 48, "02:00", "14:00").unwrap();
        assert_eq!(target.local_watermark_date, "2026-09-15");
        assert_eq!(target.local_watermark_time, "14:00");

        // 5. At 23:59 UTC+8 on 2026-09-15 -> picks today 14:00
        let now_2359 = tz.with_ymd_and_hms(2026, 9, 15, 23, 59, 0).unwrap();
        let target = resolve_target_watermark(now_2359, 48, "02:00", "14:00").unwrap();
        assert_eq!(target.local_watermark_date, "2026-09-15");
        assert_eq!(target.local_watermark_time, "14:00");

        // Test custom watermarks: 03:30 and 15:30 with window 24h
        let target = resolve_target_watermark(now_1400, 24, "03:30", "15:30").unwrap();
        assert_eq!(target.local_watermark_date, "2026-09-15");
        assert_eq!(target.local_watermark_time, "03:30");
        assert_eq!(target.window_hours, 24);

        // Validation errors
        assert!(resolve_target_watermark(now_1400, 36, "02:00", "14:00").is_err());
        assert!(resolve_target_watermark(now_1400, 48, "02:00", "02:00").is_err());
        assert!(resolve_target_watermark(now_1400, 48, "24:00", "14:00").is_err());
        assert!(resolve_target_watermark(now_1400, 48, "2:00", "14:00").is_err());
    }

    #[test]
    fn test_watermark_missed_and_dst() {
        use chrono::FixedOffset;

        let tz = FixedOffset::east_opt(8 * 3600).unwrap();

        // M35-L1-05: Missed watermark (app offline for 5 days)
        // System comes online at 2026-09-20 16:00
        // Should only resolve the latest expired watermark (2026-09-20 14:00), ignoring previous missed ones
        let now_online = tz.with_ymd_and_hms(2026, 9, 20, 16, 0, 0).unwrap();
        let target = resolve_target_watermark(now_online, 48, "02:00", "14:00").unwrap();
        assert_eq!(target.local_watermark_date, "2026-09-20");
        assert_eq!(target.local_watermark_time, "14:00");

        // DST simulation: resolve_local_time_with_dst
        let naive = chrono::NaiveDate::from_ymd_opt(2026, 3, 29)
            .unwrap()
            .and_hms_opt(2, 0, 0)
            .unwrap();

        let resolved = resolve_local_time_with_dst(&tz, naive);
        assert!(resolved.is_some());
    }

    #[test]
    fn test_fingerprint_separation() {
        let skill = MemorySkillBinding {
            asset_id: "skill-gen".to_string(),
            asset_revision: 1,
            content_hash: "hash-123".to_string(),
            entry_hash: "entry-123".to_string(),
        };

        let cand1 = CandidateSession {
            tenant_id: "default".to_string(),
            session_id: "s1".to_string(),
            source_id: "src1".to_string(),
            session_title: "Session 1".to_string(),
            source_agent: "agent".to_string(),
            project_path: None,
            project_key: "unassigned".to_string(),
            last_activity_at: "2026-09-15T01:00:00Z".to_string(),
            source_revision: 1,
            short_ref: "s1".to_string(),
        };

        let dt1: DateTime<Utc> = "2026-09-15T02:00:00Z".parse().unwrap();
        let dt2: DateTime<Utc> = "2026-09-15T14:00:00Z".parse().unwrap();

        // 1. Same candidate sessions, different target watermarks:
        let target_fp_1 =
            compute_target_fingerprint("default", &dt1, 48, &[cand1.clone()], &[], &skill);
        let target_fp_2 =
            compute_target_fingerprint("default", &dt2, 48, &[cand1.clone()], &[], &skill);
        // Target fingerprints MUST DIFFER because target_watermark_utc differed:
        assert_ne!(target_fp_1, target_fp_2);

        // Content fingerprints MUST BE IDENTICAL because target_watermark_utc is NOT in content fingerprint:
        let content_fp_1 =
            compute_content_fingerprint(48, &[cand1.clone()], &[], &[], &skill, &[], &[]);
        let content_fp_2 =
            compute_content_fingerprint(48, &[cand1.clone()], &[], &[], &skill, &[], &[]);
        assert_eq!(content_fp_1, content_fp_2);

        // 2. Modifying candidate activity time changes content fingerprint:
        let mut cand2 = cand1.clone();
        cand2.last_activity_at = "2026-09-15T01:30:00Z".to_string();
        let content_fp_changed =
            compute_content_fingerprint(48, &[cand2], &[], &[], &skill, &[], &[]);
        assert_ne!(content_fp_1, content_fp_changed);

        // 3. Modifying exclusion changes content fingerprint:
        let content_fp_excluded = compute_content_fingerprint(
            48,
            &[cand1.clone()],
            &[],
            &[],
            &skill,
            &["s-other".to_string()],
            &[],
        );
        assert_ne!(content_fp_1, content_fp_excluded);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_dual_watermark_reuse_pipeline() {
        use chrono::FixedOffset;

        let (service, _root) = setup_test_service().await;

        sqlx::query(
            r#"
            INSERT INTO conversation_sources (
                tenant_id, id, adapter_id, name, kind, location, config_json, enabled,
                last_synced_at, last_sync_status, created_at, updated_at
            ) VALUES (
                'default', 'source-alpha', 'adapter-claude', 'Alpha Source', 'local_folder',
                '/tmp/source', '{}', 1, '2026-09-14T00:00:00Z', 'idle',
                '2026-09-14T00:00:00Z', '2026-09-14T00:00:00Z'
            )
            "#,
        )
        .execute(service.db.pool())
        .await
        .expect("insert source");

        // Insert two stable sessions whose activity falls within both the 02:00 and 14:00 48h windows
        insert_test_session(
            service.db.pool(),
            "session-alpha",
            "source-alpha",
            "Alpha Active Session",
            Some("/tmp/alpha-project"),
            "2026-09-14T12:00:00Z",
        )
        .await;

        insert_test_session(
            service.db.pool(),
            "session-unassigned",
            "source-alpha",
            "Unassigned Session",
            None,
            "2026-09-14T15:00:00Z",
        )
        .await;

        let tz = FixedOffset::east_opt(8 * 3600).unwrap();

        // Step 1: At 02:05 UTC+8, first watermark 02:00 has passed -> Generates snapshot 1
        let now_0205 = tz.with_ymd_and_hms(2026, 9, 15, 2, 5, 0).unwrap();

        let (candidates, _) = service
            .collect_recent_snapshot_candidates("2026-09-14T18:00:00Z".parse().unwrap(), 48)
            .await
            .expect("collect candidates");

        let alpha_cand = candidates
            .iter()
            .find(|c| c.project_key != "unassigned")
            .unwrap();
        let unassigned_cand = candidates
            .iter()
            .find(|c| c.project_key == "unassigned")
            .unwrap();

        let initial_result = MemoryGenerationResultV2 {
            schema_version: 2,
            projects: vec![
                MemoryGenerationProjectV2 {
                    project_key: alpha_cand.project_key.clone(),
                    summary: "Alpha project summary.".to_string(),
                    no_material_change: false,
                    source_sessions: vec![alpha_cand.short_ref.clone()],
                    items: vec![MemoryGenerationItemV2 {
                        continues_item_id: None,
                        category: MemoryItemCategory::Progress,
                        status: MemoryItemStatus::Active,
                        title: "Initial work".to_string(),
                        summary: "Summary of work".to_string(),
                        rationale: "Rationale".to_string(),
                        occurred_at: "2026-09-14T20:00:00Z".to_string(),
                        recommendation_rank: Some(1),
                        source_refs: vec![alpha_cand.short_ref.clone()],
                        promotion_nomination: MemoryPromotionNomination::ProjectDecision,
                    }],
                },
                MemoryGenerationProjectV2 {
                    project_key: "unassigned".to_string(),
                    summary: "Unassigned summary.".to_string(),
                    no_material_change: true,
                    source_sessions: vec![unassigned_cand.short_ref.clone()],
                    items: vec![],
                },
            ],
            coverage: MemoryGenerationCoverageV2 {
                covered_sessions: vec![
                    alpha_cand.short_ref.clone(),
                    unassigned_cand.short_ref.clone(),
                ],
                no_memory_sessions: vec![],
                unreadable_sessions: vec![],
                budget_exhausted: false,
            },
            unknowns: vec![],
        };

        let snap1 = service
            .evaluate_and_run_recent_snapshot(Some(now_0205), Some(initial_result))
            .await
            .expect("evaluate snap 1")
            .expect("must produce snapshot 1");

        assert_eq!(
            snap1.publication_kind,
            RecentSnapshotPublicationKind::Generated
        );
        assert_eq!(snap1.reused_from_snapshot_id, None);
        let content_gen_at_1 = snap1.content_generated_at.clone();

        // Observation count in DB should be 1
        let obs_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM memory_promotion_observations WHERE tenant_id = 'default'",
        )
        .fetch_one(service.db.pool())
        .await
        .expect("count obs");
        assert_eq!(obs_count.0, 1);

        // Step 2: Running again at 02:10 UTC+8 (same watermark 02:00) -> Target idempotency, returns None
        let now_0210 = tz.with_ymd_and_hms(2026, 9, 15, 2, 10, 0).unwrap();
        let res_repeat = service
            .evaluate_and_run_recent_snapshot::<FixedOffset>(Some(now_0210), None)
            .await
            .expect("repeat target check");
        assert!(res_repeat.is_none());

        // Step 3: At 14:05 UTC+8, watermark advances to 14:00!
        // No session was added, so content_fingerprint is IDENTICAL!
        // Agent must NOT be called (mock_result is None)
        let now_1405 = tz.with_ymd_and_hms(2026, 9, 15, 14, 5, 0).unwrap();
        let snap2 = service
            .evaluate_and_run_recent_snapshot::<FixedOffset>(Some(now_1405), None)
            .await
            .expect("evaluate snap 2")
            .expect("must produce reused snapshot 2");

        // Verification of M35-L1-06 (Reuse):
        assert_eq!(
            snap2.publication_kind,
            RecentSnapshotPublicationKind::Reused
        );
        assert_eq!(
            snap2.reused_from_snapshot_id,
            Some(snap1.snapshot_id.clone())
        );
        // M35-L1-06: Preserves original content_generated_at!
        assert_eq!(snap2.content_generated_at, content_gen_at_1);
        // Sequence incremented
        assert_eq!(snap2.sequence, snap1.sequence + 1);
        // Projects and items copied
        assert_eq!(snap2.projects.len(), snap1.projects.len());
        let alpha_snap2 = snap2
            .projects
            .iter()
            .find(|p| p.project_key == alpha_cand.project_key)
            .unwrap();
        assert_eq!(alpha_snap2.items.len(), 1);
        assert_eq!(alpha_snap2.items[0].title, "Initial work");

        // M35-L1-06 / M35-L2-04: reused snapshot does NOT increment promotion observation count!
        let obs_count_after: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM memory_promotion_observations WHERE tenant_id = 'default'",
        )
        .fetch_one(service.db.pool())
        .await
        .expect("count obs after reuse");
        assert_eq!(obs_count_after.0, 1);

        // State check: Ready, pointing to snap2
        let state = service
            .get_recent_memory_snapshot()
            .await
            .expect("get state");
        assert_eq!(state.status, RecentMemoryStatus::Ready);
        assert_eq!(state.snapshot.unwrap().snapshot_id, snap2.snapshot_id);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_l1_item_continuation_and_7_day_limit() {
        use chrono::FixedOffset;

        let (service, _root) = setup_test_service().await;
        let pool = service.db.pool();

        // 1. Setup source and session
        sqlx::query(
            r#"
            INSERT INTO conversation_sources (
                tenant_id, id, adapter_id, name, kind, location, config_json, enabled,
                last_synced_at, last_sync_status, created_at, updated_at
            ) VALUES (
                'default', 'source-alpha', 'adapter-claude', 'Alpha Source', 'local_folder',
                '/tmp/source', '{}', 1, '2026-09-10T00:00:00Z', 'idle',
                '2026-09-10T00:00:00Z', '2026-09-10T00:00:00Z'
            )
            "#,
        )
        .execute(pool)
        .await
        .expect("insert source");

        insert_test_session(
            pool,
            "session-alpha",
            "source-alpha",
            "Alpha Session",
            Some("/tmp/alpha-project"),
            "2026-09-10T01:00:00Z",
        )
        .await;

        let tz = FixedOffset::east_opt(0).unwrap();

        // Step 1: Day 0 (2026-09-10T02:05:00Z) -> Generates Snapshot 1 with Active Item
        let now_day0 = tz.with_ymd_and_hms(2026, 9, 10, 2, 5, 0).unwrap();
        let (candidates, _) = service
            .collect_recent_snapshot_candidates("2026-09-10T02:00:00Z".parse().unwrap(), 48)
            .await
            .expect("collect candidates");

        let alpha_cand = candidates
            .iter()
            .find(|c| c.project_key != "unassigned")
            .unwrap();

        let initial_result = MemoryGenerationResultV2 {
            schema_version: 2,
            projects: vec![MemoryGenerationProjectV2 {
                project_key: alpha_cand.project_key.clone(),
                summary: "Initial day 0 work.".to_string(),
                no_material_change: false,
                source_sessions: vec![alpha_cand.short_ref.clone()],
                items: vec![MemoryGenerationItemV2 {
                    continues_item_id: None,
                    category: MemoryItemCategory::Progress,
                    status: MemoryItemStatus::Active,
                    title: "Feature A initial work".to_string(),
                    summary: "Started feature A".to_string(),
                    rationale: "Needed for milestone".to_string(),
                    occurred_at: "2026-09-10T01:30:00Z".to_string(),
                    recommendation_rank: None,
                    source_refs: vec![alpha_cand.short_ref.clone()],
                    promotion_nomination: MemoryPromotionNomination::None,
                }],
            }],
            coverage: MemoryGenerationCoverageV2 {
                covered_sessions: vec![alpha_cand.short_ref.clone()],
                no_memory_sessions: vec![],
                unreadable_sessions: vec![],
                budget_exhausted: false,
            },
            unknowns: vec![],
        };

        let snap1 = service
            .evaluate_and_run_recent_snapshot(Some(now_day0), Some(initial_result))
            .await
            .expect("snap 1")
            .expect("snap 1 some");

        let item1_id = snap1.projects[0].items[0].item_id.clone();

        // Verify Item 1 in DB
        let row1: (String, String, String, i64) = sqlx::query_as(
            "SELECT mi.first_seen_at, mi.last_seen_at, mi.lifecycle, mir.revision_number \
             FROM memory_items mi JOIN memory_item_revisions mir ON mi.current_revision_id = mir.id \
             WHERE mi.tenant_id = 'default' AND mi.id = ?1",
        )
        .bind(&item1_id)
        .fetch_one(pool)
        .await
        .expect("query item 1");

        assert_eq!(row1.2, "current");
        assert_eq!(row1.3, 1);
        let first_seen_at_day0 = row1.0.clone();

        // Step 2: Day 2 (2026-09-12T02:05:00Z)
        // Check collect_continuable_items at Day 2: item 1 must be present with remaining_days = 5
        let now_day2 = tz.with_ymd_and_hms(2026, 9, 12, 2, 5, 0).unwrap();
        let continuable_day2 = service
            .collect_continuable_items(&"2026-09-12T02:00:00Z".parse().unwrap())
            .await
            .expect("collect continuable day 2");

        let cont1 = continuable_day2
            .iter()
            .find(|i| i.item_id == item1_id)
            .expect("must find item1");
        assert_eq!(cont1.days_since_first_seen, 2);
        assert_eq!(cont1.remaining_days, 5);

        // Insert new activity at Day 2
        insert_test_session(
            pool,
            "session-alpha-day2",
            "source-alpha",
            "Alpha Session Day 2",
            Some("/tmp/alpha-project"),
            "2026-09-12T01:00:00Z",
        )
        .await;

        let (candidates_day2, _) = service
            .collect_recent_snapshot_candidates("2026-09-12T02:00:00Z".parse().unwrap(), 48)
            .await
            .expect("collect candidates day 2");
        let alpha_cand_day2 = candidates_day2
            .iter()
            .find(|c| c.project_key != "unassigned")
            .unwrap();

        // Agent continues item 1, updates status to Blocked
        let day2_result = MemoryGenerationResultV2 {
            schema_version: 2,
            projects: vec![MemoryGenerationProjectV2 {
                project_key: alpha_cand_day2.project_key.clone(),
                summary: "Day 2 work blocked.".to_string(),
                no_material_change: false,
                source_sessions: vec![alpha_cand_day2.short_ref.clone()],
                items: vec![MemoryGenerationItemV2 {
                    continues_item_id: Some(item1_id.clone()),
                    category: MemoryItemCategory::Blocker,
                    status: MemoryItemStatus::Blocked,
                    title: "Feature A blocked by dependency".to_string(),
                    summary: "Feature A is now blocked".to_string(),
                    rationale: "Upstream API change".to_string(),
                    occurred_at: "2026-09-12T01:30:00Z".to_string(),
                    recommendation_rank: None,
                    source_refs: vec![alpha_cand_day2.short_ref.clone()],
                    promotion_nomination: MemoryPromotionNomination::None,
                }],
            }],
            coverage: MemoryGenerationCoverageV2 {
                covered_sessions: vec![alpha_cand_day2.short_ref.clone()],
                no_memory_sessions: vec![],
                unreadable_sessions: vec![],
                budget_exhausted: false,
            },
            unknowns: vec![],
        };

        let snap2 = service
            .evaluate_and_run_recent_snapshot(Some(now_day2), Some(day2_result))
            .await
            .expect("snap 2")
            .expect("snap 2 some");

        assert_eq!(snap2.projects[0].items[0].item_id, item1_id);

        // Verify revision 2 and first_seen_at preserved
        let row2: (String, String, String, i64, Option<String>) = sqlx::query_as(
            "SELECT mi.first_seen_at, mi.last_seen_at, mi.lifecycle, mir.revision_number, mir.supersedes_revision_id \
             FROM memory_items mi JOIN memory_item_revisions mir ON mi.current_revision_id = mir.id \
             WHERE mi.tenant_id = 'default' AND mi.id = ?1",
        )
        .bind(&item1_id)
        .fetch_one(pool)
        .await
        .expect("query item 1 after day 2");

        // M35-L1-08: first_seen_at remains Day 0!
        assert_eq!(row2.0, first_seen_at_day0);
        // last_seen_at is updated to Day 2!
        assert_eq!(row2.1, "2026-09-12T02:00:00+00:00");
        assert_eq!(row2.2, "current");
        assert_eq!(row2.3, 2); // revision 2
        assert!(row2.4.is_some()); // supersedes rev 1

        // Step 3: Day 8 (> 7 days after first_seen_at)
        // 2026-09-18T02:00:00Z is 8 days after 2026-09-10T02:00:00Z
        let continuable_day8 = service
            .collect_continuable_items(&"2026-09-18T02:00:00Z".parse().unwrap())
            .await
            .expect("collect continuable day 8");

        // M35-L1-08: item 1 must be retired and NOT in continuable items!
        assert!(continuable_day8.iter().all(|i| i.item_id != item1_id));

        let lifecycle_day8: (String,) = sqlx::query_as(
            "SELECT lifecycle FROM memory_items WHERE tenant_id = 'default' AND id = ?1",
        )
        .bind(&item1_id)
        .fetch_one(pool)
        .await
        .expect("query item 1 lifecycle day 8");
        assert_eq!(lifecycle_day8.0, "retired");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_l1_terminal_state_displays_once_and_exits() {
        use chrono::FixedOffset;

        let (service, _root) = setup_test_service().await;
        let pool = service.db.pool();

        sqlx::query(
            r#"
            INSERT INTO conversation_sources (
                tenant_id, id, adapter_id, name, kind, location, config_json, enabled,
                last_synced_at, last_sync_status, created_at, updated_at
            ) VALUES (
                'default', 'source-beta', 'adapter-claude', 'Beta Source', 'local_folder',
                '/tmp/source', '{}', 1, '2026-09-10T00:00:00Z', 'idle',
                '2026-09-10T00:00:00Z', '2026-09-10T00:00:00Z'
            )
            "#,
        )
        .execute(pool)
        .await
        .expect("insert source");

        insert_test_session(
            pool,
            "session-beta-1",
            "source-beta",
            "Beta Session 1",
            Some("/tmp/beta-project"),
            "2026-09-10T01:00:00Z",
        )
        .await;

        let tz = FixedOffset::east_opt(0).unwrap();

        // Step 1: Snapshot 1: Item created as Active
        let now_1 = tz.with_ymd_and_hms(2026, 9, 10, 2, 5, 0).unwrap();
        let (cand1, _) = service
            .collect_recent_snapshot_candidates("2026-09-10T02:00:00Z".parse().unwrap(), 48)
            .await
            .expect("cand1");
        let beta_cand1 = cand1
            .iter()
            .find(|c| c.project_key != "unassigned")
            .unwrap();

        let res1 = MemoryGenerationResultV2 {
            schema_version: 2,
            projects: vec![MemoryGenerationProjectV2 {
                project_key: beta_cand1.project_key.clone(),
                summary: "Beta active task.".to_string(),
                no_material_change: false,
                source_sessions: vec![beta_cand1.short_ref.clone()],
                items: vec![MemoryGenerationItemV2 {
                    continues_item_id: None,
                    category: MemoryItemCategory::Progress,
                    status: MemoryItemStatus::Active,
                    title: "Feature B development".to_string(),
                    summary: "Under development".to_string(),
                    rationale: "Milestone B".to_string(),
                    occurred_at: "2026-09-10T01:30:00Z".to_string(),
                    recommendation_rank: None,
                    source_refs: vec![beta_cand1.short_ref.clone()],
                    promotion_nomination: MemoryPromotionNomination::None,
                }],
            }],
            coverage: MemoryGenerationCoverageV2 {
                covered_sessions: vec![beta_cand1.short_ref.clone()],
                no_memory_sessions: vec![],
                unreadable_sessions: vec![],
                budget_exhausted: false,
            },
            unknowns: vec![],
        };

        let snap1 = service
            .evaluate_and_run_recent_snapshot(Some(now_1), Some(res1))
            .await
            .expect("snap 1")
            .unwrap();
        let item_id = snap1.projects[0].items[0].item_id.clone();

        // Step 2: Snapshot 2: Item is Completed (terminal)
        insert_test_session(
            pool,
            "session-beta-2",
            "source-beta",
            "Beta Session 2",
            Some("/tmp/beta-project"),
            "2026-09-10T13:00:00Z",
        )
        .await;

        let now_2 = tz.with_ymd_and_hms(2026, 9, 10, 14, 5, 0).unwrap();
        let (cand2, _) = service
            .collect_recent_snapshot_candidates("2026-09-10T14:00:00Z".parse().unwrap(), 48)
            .await
            .expect("cand2");
        let beta_cand2 = cand2
            .iter()
            .find(|c| c.project_key != "unassigned")
            .unwrap();

        let res2 = MemoryGenerationResultV2 {
            schema_version: 2,
            projects: vec![MemoryGenerationProjectV2 {
                project_key: beta_cand2.project_key.clone(),
                summary: "Beta task completed.".to_string(),
                no_material_change: false,
                source_sessions: vec![beta_cand2.short_ref.clone()],
                items: vec![MemoryGenerationItemV2 {
                    continues_item_id: Some(item_id.clone()),
                    category: MemoryItemCategory::Progress,
                    status: MemoryItemStatus::Completed,
                    title: "Feature B development finished".to_string(),
                    summary: "Completed and merged".to_string(),
                    rationale: "Done".to_string(),
                    occurred_at: "2026-09-10T13:30:00Z".to_string(),
                    recommendation_rank: None,
                    source_refs: vec![beta_cand2.short_ref.clone()],
                    promotion_nomination: MemoryPromotionNomination::None,
                }],
            }],
            coverage: MemoryGenerationCoverageV2 {
                covered_sessions: cand2.iter().map(|c| c.short_ref.clone()).collect(),
                no_memory_sessions: vec![],
                unreadable_sessions: vec![],
                budget_exhausted: false,
            },
            unknowns: vec![],
        };

        let snap2 = service
            .evaluate_and_run_recent_snapshot(Some(now_2), Some(res2))
            .await
            .expect("snap 2")
            .unwrap();

        // M35-L1-09: In the snapshot where it becomes terminal, it displays once!
        assert_eq!(snap2.projects[0].items.len(), 1);
        assert_eq!(snap2.projects[0].items[0].item_id, item_id);
        assert_eq!(snap2.projects[0].items[0].status, "completed");

        // In DB, its lifecycle is now 'retired'
        let lifecycle: (String,) = sqlx::query_as(
            "SELECT lifecycle FROM memory_items WHERE tenant_id = 'default' AND id = ?1",
        )
        .bind(&item_id)
        .fetch_one(pool)
        .await
        .expect("lifecycle check");
        assert_eq!(lifecycle.0, "retired");

        // Step 3: Snapshot 3: Next day
        // collect_continuable_items must NOT return the completed item!
        let continuable_step3 = service
            .collect_continuable_items(&"2026-09-11T02:00:00Z".parse().unwrap())
            .await
            .expect("continuable step 3");
        assert!(continuable_step3.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_source_invalidation_retires_unpromoted_l1_item() {
        use chrono::FixedOffset;

        let (service, _root) = setup_test_service().await;
        let pool = service.db.pool();

        sqlx::query(
            r#"
            INSERT INTO conversation_sources (
                tenant_id, id, adapter_id, name, kind, location, config_json, enabled,
                last_synced_at, last_sync_status, created_at, updated_at
            ) VALUES (
                'default', 'source-gamma', 'adapter-claude', 'Gamma Source', 'local_folder',
                '/tmp/source', '{}', 1, '2026-09-10T00:00:00Z', 'idle',
                '2026-09-10T00:00:00Z', '2026-09-10T00:00:00Z'
            )
            "#,
        )
        .execute(pool)
        .await
        .expect("insert source");

        insert_test_session(
            pool,
            "session-gamma",
            "source-gamma",
            "Gamma Session",
            Some("/tmp/gamma-project"),
            "2026-09-10T01:00:00Z",
        )
        .await;

        let tz = FixedOffset::east_opt(0).unwrap();
        let now = tz.with_ymd_and_hms(2026, 9, 10, 2, 5, 0).unwrap();

        let (candidates, _) = service
            .collect_recent_snapshot_candidates("2026-09-10T02:00:00Z".parse().unwrap(), 48)
            .await
            .expect("candidates");
        let gamma_cand = candidates
            .iter()
            .find(|c| c.project_key != "unassigned")
            .unwrap();

        let res = MemoryGenerationResultV2 {
            schema_version: 2,
            projects: vec![MemoryGenerationProjectV2 {
                project_key: gamma_cand.project_key.clone(),
                summary: "Gamma task.".to_string(),
                no_material_change: false,
                source_sessions: vec![gamma_cand.short_ref.clone()],
                items: vec![MemoryGenerationItemV2 {
                    continues_item_id: None,
                    category: MemoryItemCategory::Progress,
                    status: MemoryItemStatus::Active,
                    title: "Gamma Item".to_string(),
                    summary: "Gamma summary".to_string(),
                    rationale: "Gamma rationale".to_string(),
                    occurred_at: "2026-09-10T01:30:00Z".to_string(),
                    recommendation_rank: None,
                    source_refs: vec![gamma_cand.short_ref.clone()],
                    promotion_nomination: MemoryPromotionNomination::None,
                }],
            }],
            coverage: MemoryGenerationCoverageV2 {
                covered_sessions: vec![gamma_cand.short_ref.clone()],
                no_memory_sessions: vec![],
                unreadable_sessions: vec![],
                budget_exhausted: false,
            },
            unknowns: vec![],
        };

        let snap = service
            .evaluate_and_run_recent_snapshot(Some(now), Some(res))
            .await
            .expect("eval")
            .unwrap();
        let l1_item_id = snap.projects[0].items[0].item_id.clone();

        // Also insert an L2 item to verify M35-L3-04 (L2 item is NOT retired on source invalidation)
        let l2_item_id = "item-l2-test".to_string();
        let l2_rev_id = "rev-l2-test".to_string();
        sqlx::query(
            "INSERT INTO memory_items (\
                tenant_id, id, layer, project_key, current_revision_id, lifecycle, \
                first_seen_at, last_seen_at, created_at, updated_at\
             ) VALUES ('default', ?1, 'l2', 'gamma', ?2, 'current', '2026-09-10T00:00:00Z', '2026-09-10T00:00:00Z', '2026-09-10T00:00:00Z', '2026-09-10T00:00:00Z')",
        )
        .bind(&l2_item_id)
        .bind(&l2_rev_id)
        .execute(pool)
        .await
        .expect("insert l2 item");

        sqlx::query(
            "INSERT INTO memory_item_revisions (\
                tenant_id, id, item_id, revision_number, category, status, title, summary, \
                rationale, recommendation_rank, promotion_nomination, occurred_at, \
                evidence_fingerprint, generated_by_snapshot_id, created_at\
             ) VALUES ('default', ?1, ?2, 1, 'decision', 'active', 'L2 Rule', 'L2 summary', 'L2 rat', NULL, 'none', '2026-09-10T00:00:00Z', 'fp', NULL, '2026-09-10T00:00:00Z')",
        )
        .bind(&l2_rev_id)
        .bind(&l2_item_id)
        .execute(pool)
        .await
        .expect("insert l2 rev");

        sqlx::query(
            "INSERT INTO memory_item_source_references (\
                tenant_id, id, item_revision_id, record_kind, source_id, session_id, \
                reference_key, source_revision, availability, created_at\
             ) VALUES ('default', 'ref-l2', ?1, 'session', 'source-gamma', 'session-gamma', 'source-gamma/session-gamma', 1, 'available', '2026-09-10T00:00:00Z')",
        )
        .bind(&l2_rev_id)
        .execute(pool)
        .await
        .expect("insert l2 ref");

        // Now disable source-gamma:
        sqlx::query("UPDATE conversation_sources SET enabled = 0 WHERE id = 'source-gamma'")
            .execute(pool)
            .await
            .expect("disable source");

        // Run sync
        service
            .sync_source_availability_and_retire_unpromoted_items("2026-09-11T00:00:00Z")
            .await
            .expect("sync source availability");

        // 1. Unpromoted L1 item's references are unavailable
        let l1_ref_status: (String, Option<String>) = sqlx::query_as(
            "SELECT availability, unavailable_reason FROM memory_item_source_references WHERE item_revision_id = (SELECT current_revision_id FROM memory_items WHERE id = ?1)",
        )
        .bind(&l1_item_id)
        .fetch_one(pool)
        .await
        .expect("l1 ref status");
        assert_eq!(l1_ref_status.0, "unavailable");
        assert_eq!(l1_ref_status.1.as_deref(), Some("source_disabled"));

        // 2. Unpromoted L1 item is retired
        let l1_lifecycle: (String,) =
            sqlx::query_as("SELECT lifecycle FROM memory_items WHERE id = ?1")
                .bind(&l1_item_id)
                .fetch_one(pool)
                .await
                .expect("l1 lifecycle");
        assert_eq!(l1_lifecycle.0, "retired");

        // 3. M35-L3-04: L2 item reference is unavailable, but L2 item lifecycle is STILL 'current'
        let l2_ref_status: (String, Option<String>) = sqlx::query_as(
            "SELECT availability, unavailable_reason FROM memory_item_source_references WHERE item_revision_id = ?1",
        )
        .bind(&l2_rev_id)
        .fetch_one(pool)
        .await
        .expect("l2 ref status");
        assert_eq!(l2_ref_status.0, "unavailable");
        assert_eq!(l2_ref_status.1.as_deref(), Some("source_disabled"));

        let l2_lifecycle: (String,) =
            sqlx::query_as("SELECT lifecycle FROM memory_items WHERE id = ?1")
                .bind(&l2_item_id)
                .fetch_one(pool)
                .await
                .expect("l2 lifecycle");
        assert_eq!(l2_lifecycle.0, "current");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_work_order_evidence_pack_is_structured_facts_without_markdown() {
        let (service, _root) = setup_test_service().await;
        let target = WatermarkTarget {
            target_watermark_utc: "2026-09-15T02:00:00Z".parse().unwrap(),
            local_watermark_date: "2026-09-15".to_string(),
            local_watermark_time: "02:00".to_string(),
            timezone_offset_minutes: 0,
            window_hours: 48,
            window_start_utc: "2026-09-13T02:00:00Z".parse().unwrap(),
            window_end_utc: "2026-09-15T02:00:00Z".parse().unwrap(),
        };

        let candidates = vec![CandidateSession {
            tenant_id: "default".to_string(),
            session_id: "s1".to_string(),
            source_id: "src1".to_string(),
            session_title: "Session 1".to_string(),
            source_agent: "agent".to_string(),
            project_key: "proj1".to_string(),
            project_path: None,
            last_activity_at: "2026-09-14T10:00:00Z".to_string(),
            source_revision: 1,
            short_ref: "SES-1".to_string(),
        }];

        let continuable = vec![ContinuableMemoryItemView {
            item_id: "item1".to_string(),
            project_key: "proj1".to_string(),
            category: MemoryItemCategory::Blocker,
            status: MemoryItemStatus::Blocked,
            title: "Task Blocked".to_string(),
            summary: "Blocked summary".to_string(),
            rationale: "Rationale".to_string(),
            first_seen_at: "2026-09-13T02:00:00Z".to_string(),
            last_seen_at: "2026-09-14T02:00:00Z".to_string(),
            days_since_first_seen: 1,
            remaining_days: 6,
            current_revision_id: "rev1".to_string(),
            current_revision_number: 1,
            evidence_fingerprint: "fp1".to_string(),
            source_refs: vec!["SES-1".to_string()],
        }];

        let pack = service.build_recent_snapshot_work_order_evidence_pack(
            &target,
            &candidates,
            &continuable,
        );

        assert_eq!(pack.project_keys, vec!["proj1".to_string()]);
        assert_eq!(pack.candidate_sessions.len(), 1);
        assert_eq!(pack.candidate_sessions[0].session_ref, "SES-1");
        assert_eq!(pack.continuable_items.len(), 1);
        assert_eq!(pack.continuable_items[0].remaining_days, 6);
        assert_eq!(pack.allowed_tools.len(), 4);

        // M35-L1-10: Verify serialized JSON has zero markdown fields
        let json_str = serde_json::to_string(&pack).expect("serialize pack");
        assert!(!json_str.contains(".md"));
        assert!(!json_str.contains("memory_summary"));
    }
}
