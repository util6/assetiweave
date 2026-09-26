use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::{
    L3CandidateReferenceView, L3PromotionCandidate, MemoryItemCategory, MemoryItemStatus,
    MemoryPromotionNomination,
};
use sqlx::{Row, SqlitePool};
use std::collections::{HashMap, HashSet};

pub(crate) fn resolve_global_reference(
    references: &HashMap<String, L3CandidateReferenceView>,
    reference_key: &str,
    strict: bool,
) -> AppResult<L3CandidateReferenceView> {
    if let Some(reference) = references.get(reference_key) {
        return Ok(reference.clone());
    }
    if strict {
        return Err(AppError::Validation(format!(
            "MEMORY_OUTPUT_INVALID: global reference is outside the Work Order: {reference_key}"
        )));
    }

    // 旧的显式 custom_operations 入口没有 Work Order，只能保留其历史
    // reference_key；生产 Agent 路径严格要求引用当前 Work Order 的真实 locator。
    Ok(L3CandidateReferenceView {
        project_key: "unassigned".to_string(),
        source_id: "global".to_string(),
        session_id: "global_evidence".to_string(),
        reference_key: reference_key.to_string(),
        role: None,
        question_id: None,
        turn_id: None,
        part_id: None,
        node_id: None,
        source_revision: 1,
        available: true,
        unavailable_reason: None,
    })
}

pub(crate) fn promotion_nomination_for_source_refs(
    candidates: &[L3PromotionCandidate],
    source_refs: &[String],
) -> MemoryPromotionNomination {
    // L3 publication is allowed only when the operation preserves the scope
    // declared by an admitted candidate. A cross-project candidate must keep
    // evidence from at least two real projects; a global rule must retain a
    // canonical user reference.
    for candidate in candidates {
        let matching = candidate
            .session_references
            .iter()
            .filter(|reference| {
                reference.available && source_refs.contains(&reference.reference_key)
            })
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        match candidate.nomination {
            MemoryPromotionNomination::GlobalRule
                if matching
                    .iter()
                    .any(|reference| reference.role.as_deref() == Some("user")) =>
            {
                return MemoryPromotionNomination::GlobalRule;
            }
            MemoryPromotionNomination::CrossProjectPattern => {
                let projects = matching
                    .iter()
                    .map(|reference| reference.project_key.as_str())
                    .filter(|project_key| !project_key.is_empty() && *project_key != "unassigned")
                    .collect::<HashSet<_>>();
                if projects.len() >= 2 {
                    return MemoryPromotionNomination::CrossProjectPattern;
                }
            }
            _ => {}
        }
    }
    MemoryPromotionNomination::None
}

/// L3 晋升候选准入评估 (M35-L3-01, M35-L3-02)
///
/// 评估条目是否满足晋升为 L3 全局长期记忆的资格：
/// - M35-L3-01: L3 只保存明确全局规则、长期偏好、跨项目稳定工作方式与通用约束。禁止项目进展、待办与单次命令结果。
/// - M35-L3-02:
///   1. 明确全局规则 (global_rule): 必须有 available 用户引用且具全局范围；
///   2. 跨项目模式 (cross_project_pattern): 必须由至少两个不同真实 project_key (非 unassigned)
///      的当前有效 L2 revision 独立支持，且每个项目至少有一个 available 引用。
///   3. unavailable 引用不计入支持数；单项目或 unassigned 不晋升。
pub(crate) async fn evaluate_l3_candidates(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<L3PromotionCandidate>> {
    // 1. 加载当前已有 active L3 标题，避免重复创建已存在的知识
    let existing_l3_titles: HashSet<String> = sqlx::query_scalar(
        "SELECT LOWER(TRIM(mir.title)) \
         FROM memory_items mi \
         JOIN memory_item_revisions mir ON mi.tenant_id = mir.tenant_id AND mi.current_revision_id = mir.id \
         WHERE mi.tenant_id = ?1 AND mi.layer = 'l3' AND mi.lifecycle = 'current'",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?
    .into_iter()
    .collect();

    // 2. 加载所有当前有效的 L2 条目（必须有 project_key 且非 unassigned）
    let l2_rows = sqlx::query(
        "SELECT mi.id as item_id, mi.project_key, \
                mir.id as revision_id, mir.category, mir.status, \
                mir.title, mir.summary, mir.rationale, mir.promotion_nomination \
         FROM memory_items mi \
         JOIN memory_item_revisions mir ON mi.tenant_id = mir.tenant_id AND mi.current_revision_id = mir.id \
         WHERE mi.tenant_id = ?1 AND mi.layer = 'l2' AND mi.lifecycle = 'current' \
           AND mi.project_key IS NOT NULL AND mi.project_key != 'unassigned'",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    // 3. 加载所有与这些 L2 revisions 关联的引用及其可用性
    let ref_rows = sqlx::query(
        "SELECT sr.item_revision_id, sr.source_id, sr.session_id, sr.reference_key, \
                sr.question_id, sr.turn_id, sr.part_id, sr.node_id, sr.source_revision, sr.availability, \
                sr.unavailable_reason, mi.project_key \
         FROM memory_item_source_references sr \
         JOIN memory_item_revisions mir ON sr.tenant_id = mir.tenant_id AND sr.item_revision_id = mir.id \
         JOIN memory_items mi ON mir.tenant_id = mi.tenant_id AND mir.item_id = mi.id \
         WHERE sr.tenant_id = ?1 AND mi.layer = 'l2' AND mi.lifecycle = 'current'",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    let mut refs_by_revision: HashMap<String, Vec<L3CandidateReferenceView>> = HashMap::new();
    for r in ref_rows {
        let rev_id: String = r.get("item_revision_id");
        let project_key: String = r.get("project_key");
        let source_id: String = r.get("source_id");
        let session_id: String = r.get("session_id");
        let reference_key: String = r.get("reference_key");
        let question_id: Option<String> = r.get("question_id");
        let turn_id: Option<String> = r.get("turn_id");
        let part_id: Option<String> = r.get("part_id");
        let node_id: Option<String> = r.get("node_id");
        let availability: String = r.get("availability");
        let unavailable_reason: Option<String> = r.get("unavailable_reason");
        let is_user_reference = question_id.is_some()
            || turn_id.is_some()
            || reference_key.to_ascii_lowercase().contains("user");

        refs_by_revision
            .entry(rev_id)
            .or_default()
            .push(L3CandidateReferenceView {
                project_key,
                source_id,
                session_id,
                reference_key,
                role: is_user_reference.then(|| "user".to_string()),
                question_id,
                turn_id,
                part_id,
                node_id,
                source_revision: r.get("source_revision"),
                available: availability == "available",
                unavailable_reason,
            });
    }

    let mut candidates = Vec::new();

    // 对条目按规范化标题或主题进行聚类，用于发现 cross_project_pattern
    struct GroupedCandidate {
        title: String,
        summary: String,
        rationale: String,
        category: MemoryItemCategory,
        status: MemoryItemStatus,
        nomination: MemoryPromotionNomination,
        primary_item_id: String,
        primary_revision_id: String,
        supporting_projects: HashSet<String>,
        supporting_sessions: HashSet<String>,
        references: Vec<L3CandidateReferenceView>,
    }

    let mut clusters: HashMap<String, GroupedCandidate> = HashMap::new();

    for row in l2_rows {
        let item_id: String = row.get("item_id");
        let project_key: String = row.get("project_key");
        let revision_id: String = row.get("revision_id");
        let category_raw: String = row.get("category");
        let status_raw: String = row.get("status");
        let nomination_raw: String = row.get("promotion_nomination");
        let title: String = row.get("title");
        let summary: String = row.get("summary");
        let rationale: String = row.get("rationale");

        // 已经存在于 active L3 中的知识不再作为新候选
        if existing_l3_titles.contains(&title.trim().to_lowercase()) {
            continue;
        }

        let category = match category_raw.as_str() {
            "decision" => MemoryItemCategory::Decision,
            "research" => MemoryItemCategory::Research,
            "verification" => MemoryItemCategory::Verification,
            "blocker" => MemoryItemCategory::Blocker,
            "follow_up" => MemoryItemCategory::FollowUp,
            _ => MemoryItemCategory::Progress,
        };

        // M35-L3-01: 普通进度绝不作为 L3 候选
        if category == MemoryItemCategory::Progress {
            continue;
        }

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
            "global_rule" => MemoryPromotionNomination::GlobalRule,
            "cross_project_pattern" => MemoryPromotionNomination::CrossProjectPattern,
            "project_decision" => MemoryPromotionNomination::ProjectDecision,
            "project_constraint" => MemoryPromotionNomination::ProjectConstraint,
            "recurring_blocker" => MemoryPromotionNomination::RecurringBlocker,
            "recurring_todo" => MemoryPromotionNomination::RecurringTodo,
            "research_conclusion" => MemoryPromotionNomination::ResearchConclusion,
            _ => MemoryPromotionNomination::None,
        };

        let item_refs = refs_by_revision.remove(&revision_id).unwrap_or_default();

        // 仅计入 available 引用 (M35-L3-04)
        let has_available_refs = item_refs.iter().any(|r| r.available);
        if !has_available_refs {
            continue;
        }

        let cluster_key = title.trim().to_lowercase();
        let entry = clusters
            .entry(cluster_key)
            .or_insert_with(|| GroupedCandidate {
                title: title.clone(),
                summary: summary.clone(),
                rationale: rationale.clone(),
                category,
                status,
                nomination,
                primary_item_id: item_id.clone(),
                primary_revision_id: revision_id.clone(),
                supporting_projects: HashSet::new(),
                supporting_sessions: HashSet::new(),
                references: Vec::new(),
            });

        // 检查该项目是否有 available 引用
        let project_has_available = item_refs
            .iter()
            .any(|r| r.project_key == project_key && r.available);
        if project_has_available {
            entry.supporting_projects.insert(project_key);
            for r in &item_refs {
                if r.available {
                    entry.supporting_sessions.insert(r.session_id.clone());
                }
            }
        }
        entry.references.extend(item_refs);

        // 若其中有明确声明为 global_rule 或 cross_project_pattern 则升级 nomination
        if nomination == MemoryPromotionNomination::GlobalRule {
            entry.nomination = MemoryPromotionNomination::GlobalRule;
        } else if nomination == MemoryPromotionNomination::CrossProjectPattern
            && entry.nomination != MemoryPromotionNomination::GlobalRule
        {
            entry.nomination = MemoryPromotionNomination::CrossProjectPattern;
        }
    }

    // 4. 准入漏斗判定
    for (_, grouped) in clusters {
        let is_global_rule = grouped.nomination == MemoryPromotionNomination::GlobalRule;
        let is_cross_project =
            grouped.supporting_projects.len() >= 2 && grouped.supporting_sessions.len() >= 2;

        if is_global_rule {
            // M35-L3-02 规则 1: 明确全局规则必须有 available 的用户引用。
            // L2 的 global_rule nomination 是范围声明；引用 locator 的
            // question/turn 或 user 标记是用户证据声明。
            let has_available_user_reference = grouped
                .references
                .iter()
                .any(|r| r.available && r.role.as_deref() == Some("user"));
            if has_available_user_reference {
                candidates.push(L3PromotionCandidate {
                    item_id: grouped.primary_item_id,
                    item_revision_id: grouped.primary_revision_id,
                    nomination: MemoryPromotionNomination::GlobalRule,
                    category: grouped.category,
                    status: grouped.status,
                    title: grouped.title,
                    summary: grouped.summary,
                    rationale: grouped.rationale,
                    supporting_project_keys: grouped.supporting_projects.into_iter().collect(),
                    session_references: grouped.references,
                });
            }
        } else if is_cross_project {
            // M35-L3-02 规则 2: 跨项目模式必须由至少两个不同真实 project_key 独立支持
            // 且 session 也不能是同一 session 重复映射
            let mut sorted_projects: Vec<String> =
                grouped.supporting_projects.into_iter().collect();
            sorted_projects.sort();

            candidates.push(L3PromotionCandidate {
                item_id: grouped.primary_item_id,
                item_revision_id: grouped.primary_revision_id,
                nomination: MemoryPromotionNomination::CrossProjectPattern,
                category: grouped.category,
                status: grouped.status,
                title: grouped.title,
                summary: grouped.summary,
                rationale: grouped.rationale,
                supporting_project_keys: sorted_projects,
                session_references: grouped.references,
            });
        }
    }

    // 按标题稳定排序
    candidates.sort_by(|a, b| a.title.cmp(&b.title));
    Ok(candidates)
}
