use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::{
    L2CandidateReferenceView, L2PromotionCandidate, MemoryItemCategory, MemoryItemStatus,
    MemoryPromotionNomination,
};
use sqlx::{Row, SqlitePool};
use std::collections::{HashMap, HashSet};

pub(crate) fn resolve_project_reference(
    references: &HashMap<String, L2CandidateReferenceView>,
    reference_key: &str,
    strict: bool,
) -> AppResult<L2CandidateReferenceView> {
    if let Some(reference) = references.get(reference_key) {
        return Ok(reference.clone());
    }
    if strict {
        return Err(AppError::Validation(format!(
            "MEMORY_OUTPUT_INVALID: project reference is outside the Work Order: {reference_key}"
        )));
    }

    // 保留旧的测试/兼容入口：没有 Agent Work Order 时，历史调用方只提供
    // reference_key，继续写入显式的系统占位来源；生产 Agent 路径始终走上面的
    // strict 分支，禁止伪造 source/session locator。
    Ok(L2CandidateReferenceView {
        source_id: "system".to_string(),
        session_id: "consolidation".to_string(),
        reference_key: reference_key.to_string(),
        role: None,
        question_id: None,
        turn_id: None,
        part_id: None,
        node_id: None,
        source_revision: 0,
        available: true,
    })
}

pub(crate) fn parse_promotion_nomination(value: &str) -> MemoryPromotionNomination {
    match value {
        "project_decision" => MemoryPromotionNomination::ProjectDecision,
        "project_constraint" => MemoryPromotionNomination::ProjectConstraint,
        "recurring_blocker" => MemoryPromotionNomination::RecurringBlocker,
        "recurring_todo" => MemoryPromotionNomination::RecurringTodo,
        "research_conclusion" => MemoryPromotionNomination::ResearchConclusion,
        "global_rule" => MemoryPromotionNomination::GlobalRule,
        "cross_project_pattern" => MemoryPromotionNomination::CrossProjectPattern,
        _ => MemoryPromotionNomination::None,
    }
}

pub(crate) fn promotion_nomination_for_source_refs(
    candidates: &[L2PromotionCandidate],
    source_refs: &[String],
    fallback: MemoryPromotionNomination,
) -> MemoryPromotionNomination {
    // An operation may summarize multiple admitted candidates. Keep the
    // strongest explicit nomination instead of silently resetting L2 to none.
    let rank =
        |nomination: MemoryPromotionNomination| match nomination {
            MemoryPromotionNomination::None => 0,
            MemoryPromotionNomination::RecurringTodo
            | MemoryPromotionNomination::RecurringBlocker => 10,
            MemoryPromotionNomination::ResearchConclusion => 20,
            MemoryPromotionNomination::GlobalRule
            | MemoryPromotionNomination::CrossProjectPattern => 30,
            MemoryPromotionNomination::ProjectDecision
            | MemoryPromotionNomination::ProjectConstraint => 40,
        };
    let mut selected = fallback;
    for candidate in candidates {
        if candidate.nomination == MemoryPromotionNomination::None
            || !candidate.session_references.iter().any(|reference| {
                reference.available && source_refs.contains(&reference.reference_key)
            })
        {
            continue;
        }
        if rank(candidate.nomination) > rank(selected) {
            selected = candidate.nomination;
        }
    }
    selected
}

/// L2 晋升候选准入评估 (M35-L2-01 ~ M35-L2-05)
///
/// 评估 L1 条目是否满足晋升为 L2 项目长期记忆的资格：
/// - M35-L2-01: 普通 progress/completion 不进入
/// - M35-L2-02: unassigned 项目永不晋升
/// - M35-L2-03: 用户明确决定 (project_decision) 或约束 (project_constraint) 1 次有效 generated 观察可候选
/// - M35-L2-04: recurring_blocker / recurring_todo / research_conclusion 需 2 次带新证据且指纹变化的 generated 观察
/// - M35-L2-05: completion 和普通 progress 不能仅因重复而晋升
pub(crate) async fn evaluate_l2_candidates(
    pool: &SqlitePool,
    tenant_id: &str,
    project_key: &str,
) -> AppResult<Vec<L2PromotionCandidate>> {
    // 规则 1: unassigned 项目不晋升 (M35-L2-02)
    if project_key.trim().is_empty() || project_key == "unassigned" {
        return Ok(Vec::new());
    }

    // 查询该项目所有 current 状态的 L1 条目及其当前 revision
    let rows = sqlx::query(
        "SELECT mi.id as item_id, mi.first_seen_at, mi.last_seen_at, \
                mir.id as revision_id, mir.revision_number, mir.category, mir.status, \
                mir.title, mir.summary, mir.rationale, mir.promotion_nomination, \
                mir.occurred_at, mir.evidence_fingerprint \
         FROM memory_items mi \
         JOIN memory_item_revisions mir ON mi.tenant_id = mir.tenant_id AND mi.current_revision_id = mir.id \
         WHERE mi.tenant_id = ?1 AND mi.project_key = ?2 AND mi.layer = 'l1' AND mi.lifecycle = 'current'",
    )
    .bind(tenant_id)
    .bind(project_key)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    // 加载当前已有 active L2 标题，用于避免重复创建 (M35-L2-02)
    let existing_l2_titles: HashSet<String> = sqlx::query_scalar(
        "SELECT LOWER(TRIM(mir.title)) \
         FROM memory_items mi \
         JOIN memory_item_revisions mir ON mi.tenant_id = mir.tenant_id AND mi.current_revision_id = mir.id \
         WHERE mi.tenant_id = ?1 AND mi.project_key = ?2 AND mi.layer = 'l2' AND mi.lifecycle = 'current'",
    )
    .bind(tenant_id)
    .bind(project_key)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?
    .into_iter()
    .collect();

    let mut candidates = Vec::new();

    for row in rows {
        let item_id: String = row.get("item_id");
        let revision_id: String = row.get("revision_id");
        let category_raw: String = row.get("category");
        let status_raw: String = row.get("status");
        let nomination_raw: String = row.get("promotion_nomination");
        let title: String = row.get("title");
        let summary: String = row.get("summary");
        let rationale: String = row.get("rationale");
        let occurred_at: String = row.get("occurred_at");
        let evidence_fingerprint: String = row.get("evidence_fingerprint");

        let category = match category_raw.as_str() {
            "decision" => MemoryItemCategory::Decision,
            "research" => MemoryItemCategory::Research,
            "verification" => MemoryItemCategory::Verification,
            "blocker" => MemoryItemCategory::Blocker,
            "follow_up" => MemoryItemCategory::FollowUp,
            _ => MemoryItemCategory::Progress,
        };

        let status = match status_raw.as_str() {
            "blocked" => MemoryItemStatus::Blocked,
            "waiting" => MemoryItemStatus::Waiting,
            "completed" => MemoryItemStatus::Completed,
            "verified" => MemoryItemStatus::Verified,
            "abandoned" => MemoryItemStatus::Abandoned,
            "superseded" => MemoryItemStatus::Superseded,
            _ => MemoryItemStatus::Active,
        };

        let nomination = match nomination_raw.as_str() {
            "project_decision" => MemoryPromotionNomination::ProjectDecision,
            "project_constraint" => MemoryPromotionNomination::ProjectConstraint,
            "recurring_blocker" => MemoryPromotionNomination::RecurringBlocker,
            "recurring_todo" => MemoryPromotionNomination::RecurringTodo,
            "research_conclusion" => MemoryPromotionNomination::ResearchConclusion,
            "global_rule" => MemoryPromotionNomination::GlobalRule,
            "cross_project_pattern" => MemoryPromotionNomination::CrossProjectPattern,
            _ => MemoryPromotionNomination::None,
        };

        // 规则 2: progress 与 completion 不晋升，无 nomination 绝对不晋升 (M35-L2-01, M35-L2-05)
        if nomination == MemoryPromotionNomination::None {
            continue;
        }
        if status == MemoryItemStatus::Completed
            && nomination != MemoryPromotionNomination::ProjectDecision
            && nomination != MemoryPromotionNomination::ProjectConstraint
            && nomination != MemoryPromotionNomination::ResearchConclusion
        {
            continue;
        }

        // 查询该 item 在 memory_promotion_observations 中的有效观察记录
        let obs_rows = sqlx::query(
            "SELECT mpo.snapshot_id, mpo.evidence_fingerprint, mpo.observed_at, \
                    rms.publication_kind \
             FROM memory_promotion_observations mpo \
             JOIN recent_memory_snapshots rms ON mpo.tenant_id = rms.tenant_id AND mpo.snapshot_id = rms.id \
             WHERE mpo.tenant_id = ?1 AND mpo.item_id = ?2 \
             ORDER BY mpo.observed_at ASC",
        )
        .bind(tenant_id)
        .bind(&item_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;

        // 仅保留由 generated 快照产生的观察记录 (M35-L1-06 / M35-L2-04: reused 不计入观察)
        let generated_obs: Vec<_> = obs_rows
            .into_iter()
            .filter(|r| {
                let pub_kind: String = r.get("publication_kind");
                pub_kind == "generated"
            })
            .collect();

        if generated_obs.is_empty() {
            continue;
        }

        // 加载当前 revision 的所有引用
        let ref_rows = sqlx::query(
            "SELECT source_id, session_id, reference_key, question_id, turn_id, part_id, node_id, source_revision, availability \
             FROM memory_item_source_references \
             WHERE tenant_id = ?1 AND item_revision_id = ?2",
        )
        .bind(tenant_id)
        .bind(&revision_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;

        let mut candidate_refs = Vec::new();
        let mut has_available_user_ref = false;

        for r in ref_rows {
            let source_id: String = r.get("source_id");
            let session_id: String = r.get("session_id");
            let reference_key: String = r.get("reference_key");
            let question_id: Option<String> = r.get("question_id");
            let turn_id: Option<String> = r.get("turn_id");
            let availability: String = r.get("availability");
            let is_available = availability == "available";

            // 判断是否为用户内容引用 (question_id 非空，或者 reference_key 包含 user 标记)
            let is_user_content = question_id.is_some()
                || turn_id.is_some()
                || reference_key.to_ascii_lowercase().contains("user");
            if is_available && is_user_content {
                has_available_user_ref = true;
            }

            candidate_refs.push(L2CandidateReferenceView {
                source_id,
                session_id,
                reference_key,
                role: if is_user_content {
                    Some("user".to_string())
                } else {
                    None
                },
                question_id,
                turn_id: r.get("turn_id"),
                part_id: r.get("part_id"),
                node_id: r.get("node_id"),
                source_revision: r.get("source_revision"),
                available: is_available,
            });
        }

        // 如果没有可用引用，不能晋升
        if candidate_refs.iter().all(|r| !r.available) {
            continue;
        }

        // 避免与已有 L2 产生重名冲突
        if existing_l2_titles.contains(&title.trim().to_lowercase()) {
            continue;
        }

        let observed_snapshot_ids: Vec<String> = generated_obs
            .iter()
            .map(|r| r.get::<String, _>("snapshot_id"))
            .collect();
        let obs_count = generated_obs.len();

        let qualifies = match nomination {
            // M35-L2-03: project_decision 或 project_constraint 1 次 generated 观察即可晋升，但必须有用户有效引用
            MemoryPromotionNomination::ProjectDecision
            | MemoryPromotionNomination::ProjectConstraint => {
                obs_count >= 1 && has_available_user_ref
            }
            // M35-L2-04: blocker/todo/research 必须连续 2 次 generated 观察，且指纹变化 + 至少 1 个新可用引用
            MemoryPromotionNomination::RecurringBlocker
            | MemoryPromotionNomination::RecurringTodo
            | MemoryPromotionNomination::ResearchConclusion => {
                if obs_count < 2 {
                    false
                } else {
                    let first_fp: String = generated_obs[0].get("evidence_fingerprint");
                    let last_fp: String = generated_obs[obs_count - 1].get("evidence_fingerprint");
                    // 必须存在指纹变化且至少有可用引用
                    first_fp != last_fp && candidate_refs.iter().any(|r| r.available)
                }
            }
            _ => false,
        };

        if qualifies {
            candidates.push(L2PromotionCandidate {
                item_id,
                item_revision_id: revision_id,
                project_key: project_key.to_string(),
                nomination,
                category,
                status,
                title,
                summary,
                rationale,
                occurred_at,
                evidence_fingerprint,
                observation_count: obs_count,
                observed_snapshot_ids,
                session_references: candidate_refs,
            });
        }
    }

    Ok(candidates)
}
