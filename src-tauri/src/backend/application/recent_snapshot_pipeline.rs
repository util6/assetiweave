use super::prelude::*;
use super::recent::resolve_project_directory;
use crate::backend::{
    dto::RecentMemorySnapshotView,
    models::{
        CandidateSession, MemoryGenerationResultV2, MemoryPromotionNomination,
        MemorySkillBinding, ResolvedEvidenceRef,
    },
    runtime::{AppError, AppResult},
    store,
};
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::{HashMap, HashSet};

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
            .and_then(|v| serde_json::from_value::<crate::backend::app_settings::MemorySettings>(v.clone()).ok())
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

            let raw_project_path = record
                .cwd
                .as_deref()
                .or(record.session.session.project_path.as_deref());

            let project_path =
                raw_project_path.and_then(|path| resolve_project_directory(path, &registered_roots));
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
            if !candidate_project_keys.contains(key) && key != "unassigned" && !candidates.is_empty() {
                return Err(AppError::Validation(format!(
                    "Project key '{}' not present in candidate work order",
                    key
                )));
            }

            // 摘要长度
            if !project.no_material_change && (project.summary.trim().is_empty() || project.summary.len() > 2000) {
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
        target_watermark_utc: DateTime<Utc>,
        window_hours: i64,
        skill_binding: &MemorySkillBinding,
        result: MemoryGenerationResultV2,
        candidates: &[CandidateSession],
        ref_map: &HashMap<String, ResolvedEvidenceRef>,
    ) -> AppResult<RecentMemorySnapshotView> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();

        let mut tx = pool.begin().await.map_err(AppError::external)?;

        let snapshot_id = format!("snap-{}", uuid::Uuid::new_v4());
        let now = Utc::now();
        let published_at = now.to_rfc3339();
        let content_generated_at = published_at.clone();

        let window_start_utc = (target_watermark_utc - Duration::hours(window_hours)).to_rfc3339();
        let window_end_utc = target_watermark_utc.to_rfc3339();
        let target_watermark_str = target_watermark_utc.to_rfc3339();

        let local_watermark_date = target_watermark_utc.format("%Y-%m-%d").to_string();
        let local_watermark_time = target_watermark_utc.format("%H:%M").to_string();
        let timezone_offset_minutes = 0i64;

        // Sequence
        let seq_row = sqlx::query(
            "SELECT COALESCE(MAX(sequence), 0) + 1 AS next_seq FROM recent_memory_snapshots WHERE tenant_id = ?1",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::external)?;
        let sequence: i64 = seq_row.get("next_seq");

        // 计算 fingerprints
        let mut target_hasher = Sha256::new();
        target_hasher.update(tenant_id.as_bytes());
        target_hasher.update(target_watermark_str.as_bytes());
        target_hasher.update(&window_hours.to_le_bytes());
        for c in candidates {
            target_hasher.update(c.session_id.as_bytes());
        }
        let target_fingerprint = format!("{:x}", target_hasher.finalize());

        let mut content_hasher = Sha256::new();
        content_hasher.update(&window_hours.to_le_bytes());
        for c in candidates {
            content_hasher.update(c.session_id.as_bytes());
            content_hasher.update(c.last_activity_at.as_bytes());
        }
        let content_fingerprint = format!("{:x}", content_hasher.finalize());

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
        .bind(&local_watermark_date)
        .bind(&local_watermark_time)
        .bind(timezone_offset_minutes)
        .bind(window_hours)
        .bind(&window_start_utc)
        .bind(&window_end_utc)
        .bind(&target_fingerprint)
        .bind(&content_fingerprint)
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
                let item_id = item
                    .continues_item_id
                    .unwrap_or_else(|| format!("item-{}", uuid::Uuid::new_v4()));
                let revision_id = format!("rev-{}", uuid::Uuid::new_v4());

                // Upsert memory_items
                sqlx::query(
                    "INSERT INTO memory_items (\
                        tenant_id, id, layer, project_key, current_revision_id, lifecycle, \
                        first_seen_at, last_seen_at, created_at, updated_at\
                     ) VALUES (?1, ?2, 'l1', ?3, ?4, 'current', ?5, ?5, ?6, ?6) \
                     ON CONFLICT (tenant_id, id) DO UPDATE SET \
                        current_revision_id = excluded.current_revision_id, \
                        last_seen_at = excluded.last_seen_at, \
                        updated_at = excluded.updated_at",
                )
                .bind(tenant_id)
                .bind(&item_id)
                .bind(&project.project_key)
                .bind(&revision_id)
                .bind(&item.occurred_at)
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
                        evidence_fingerprint, generated_by_snapshot_id, created_at\
                     ) VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                )
                .bind(tenant_id)
                .bind(&revision_id)
                .bind(&item_id)
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
        let (candidates, ref_map) = self
            .collect_recent_snapshot_candidates(target_watermark_utc, window_hours)
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

        let snapshot_view = self
            .commit_recent_memory_snapshot(
                target_watermark_utc,
                window_hours,
                &skill_binding,
                result,
                &candidates,
                &ref_map,
            )
            .await?;

        Ok(snapshot_view)
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
        assert_eq!(alpha_candidate.project_path.as_deref(), Some("/tmp/alpha-project"));
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
        assert!(err.to_string().contains("unassigned project cannot be nominated"));
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

        // Verify state preserves last success snapshot!
        let state_after_fail = service
            .get_recent_memory_snapshot()
            .await
            .expect("get state after fail");
        assert_eq!(state_after_fail.status, RecentMemoryStatus::UpdateFailed);
        assert!(state_after_fail.latest_attempt_error.is_some());
        let preserved_snap = state_after_fail.snapshot.expect("preserved snapshot");
        assert_eq!(preserved_snap.snapshot_id, snapshot_view.snapshot_id);
    }
}
