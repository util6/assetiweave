use crate::backend::dto::recent_snapshot::SourceAvailability;
use crate::backend::models::{
    compute_global_consolidation_fingerprint, GlobalConsolidationInput,
    GlobalConsolidationOperation, GlobalConsolidationResult, L3CandidateReferenceView,
    L3GlobalMemoryView, L3MemoryItemView, L3PromotionCandidate, L3SourceReferenceView,
    L3SupersededIndexItem, MemoryItemCategory, MemoryItemStatus, MemoryPromotionNomination,
};
use crate::backend::runtime::{AppError, AppResult};
use crate::backend::store;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex as TokioMutex;

/// 全局同 Tenant 串行锁单例
static GLOBAL_CONSOLIDATION_LOCKS: OnceLock<GlobalConsolidationLockMap> = OnceLock::new();

pub(crate) fn global_consolidation_lock_map() -> &'static GlobalConsolidationLockMap {
    GLOBAL_CONSOLIDATION_LOCKS.get_or_init(GlobalConsolidationLockMap::new)
}

/// 同 Tenant 串行锁管理器（按 tenant_id 互斥）
#[derive(Clone, Default)]
pub(crate) struct GlobalConsolidationLockMap {
    locks: Arc<TokioMutex<HashMap<String, Arc<TokioMutex<()>>>>>,
}

impl GlobalConsolidationLockMap {
    pub(crate) fn new() -> Self {
        Self {
            locks: Arc::new(TokioMutex::new(HashMap::new())),
        }
    }

    pub(crate) async fn lock_for(&self, tenant_id: &str) -> Arc<TokioMutex<()>> {
        let mut map = self.locks.lock().await;
        map.entry(tenant_id.to_string())
            .or_insert_with(|| Arc::new(TokioMutex::new(())))
            .clone()
    }
}

fn resolve_global_reference(
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

fn promotion_nomination_for_source_refs(
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

/// Load the complete, structured L3 input that belongs to one maintenance
/// Work Order. The coordinator serializes this value into the durable job so
/// the Agent sees an immutable evidence set.
pub(crate) async fn load_global_consolidation_input(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<GlobalConsolidationInput> {
    let current_l3_items = get_global_memory_l3_view(pool, tenant_id)
        .await?
        .map(|view| view.items)
        .unwrap_or_default();

    let superseded_rows = sqlx::query(
        "SELECT superseded_item_id, superseding_item_id, reason \
         FROM memory_item_supersessions WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    let superseded_index = superseded_rows
        .into_iter()
        .map(|row| L3SupersededIndexItem {
            old_item_id: row.get("superseded_item_id"),
            superseding_item_id: row.get("superseding_item_id"),
            reason: row.get("reason"),
        })
        .collect();

    Ok(GlobalConsolidationInput {
        tenant_id: tenant_id.to_string(),
        current_l3_items,
        candidates: evaluate_l3_candidates(pool, tenant_id).await?,
        superseded_index,
    })
}

/// 检查是否满足 Global Consolidation 触发条件 (M35-L3-03)
///
/// 触发条件：
/// - 必须至少有 1 个合格候选（0 候选永远返回 false，绝不空转 Agent）
/// - 若 is_manual_rebuild 为 true，则绕过低频周期直接触发
/// - 否则需满足：
///   1. 候选数量达到阈值 (>= 8 个待处理候选)；或
///   2. 距离上次成功 Consolidation 已满 7 天（周级维护水位）。
pub(crate) async fn should_trigger_global_consolidation(
    pool: &SqlitePool,
    tenant_id: &str,
    candidates_count: usize,
    now: DateTime<Utc>,
    is_manual_rebuild: bool,
) -> AppResult<bool> {
    // 0 候选绝不触发 (M35-L3-03)
    if candidates_count == 0 {
        return Ok(false);
    }

    // 手动维护可绕过低频周期门禁
    if is_manual_rebuild {
        return Ok(true);
    }

    // 候选阈值: 达到 8 个直接触发
    if candidates_count >= 8 {
        return Ok(true);
    }

    // 检查上次成功时间
    let last_success: Option<String> = sqlx::query_scalar(
        "SELECT last_successful_consolidation_at FROM global_memory_state WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;

    let Some(last_str) = last_success else {
        // 从未成功巩固过且有候选，触发首次
        return Ok(true);
    };

    let Ok(last_dt) = DateTime::parse_from_rfc3339(&last_str) else {
        return Ok(true);
    };

    let elapsed = now.signed_duration_since(last_dt.with_timezone(&Utc));
    Ok(elapsed.num_days() >= 7)
}

/// Recent 水位完成后只在 Global Consolidation 的低频触发门禁通过时创建
/// durable job。没有合格 L3 候选时不产生空转任务。
pub(crate) async fn should_schedule_global_consolidation(
    pool: &SqlitePool,
    tenant_id: &str,
    now: DateTime<Utc>,
) -> AppResult<bool> {
    let candidates = evaluate_l3_candidates(pool, tenant_id).await?;
    should_trigger_global_consolidation(pool, tenant_id, candidates.len(), now, false).await
}

/// 读取当前 Tenant 的 L3 全局记忆视图
pub(crate) async fn get_global_memory_l3_view(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Option<L3GlobalMemoryView>> {
    let state_row = sqlx::query(
        "SELECT last_successful_consolidation_at, revision_hash \
         FROM global_memory_state WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;

    // 加载所有 current 状态的 L3 条目 (M35-L3-06)
    let rows = sqlx::query(
        "SELECT mi.id as item_id, mi.updated_at, \
                mir.id as revision_id, mir.revision_number, mir.category, mir.status, \
                mir.title, mir.summary, mir.rationale \
         FROM memory_items mi \
         JOIN memory_item_revisions mir ON mi.tenant_id = mir.tenant_id AND mi.current_revision_id = mir.id \
         WHERE mi.tenant_id = ?1 AND mi.layer = 'l3' AND mi.lifecycle = 'current' \
         ORDER BY mir.occurred_at DESC, mi.id ASC",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    if rows.is_empty() && state_row.is_none() {
        return Ok(None);
    }

    let mut items = Vec::new();
    for r in rows {
        let item_id: String = r.get("item_id");
        let revision_id: String = r.get("revision_id");
        let revision_number: i64 = r.get("revision_number");
        let category_raw: String = r.get("category");
        let status_raw: String = r.get("status");
        let title: String = r.get("title");
        let summary: String = r.get("summary");
        let rationale: String = r.get("rationale");
        let updated_at: String = r.get("updated_at");

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

        // 加载关联引用
        let ref_rows = sqlx::query(
            "SELECT source_id, session_id, reference_key, question_id, turn_id, part_id, node_id, availability, unavailable_reason \
             FROM memory_item_source_references \
             WHERE tenant_id = ?1 AND item_revision_id = ?2",
        )
        .bind(tenant_id)
        .bind(&revision_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;

        let mut source_refs = Vec::new();
        let mut has_available = false;
        let mut has_unavailable = false;

        for ref_r in ref_rows {
            let availability: String = ref_r.get("availability");
            let is_avail = availability == "available";
            if is_avail {
                has_available = true;
            } else {
                has_unavailable = true;
            }
            source_refs.push(L3SourceReferenceView {
                source_id: ref_r.get("source_id"),
                session_id: ref_r.get("session_id"),
                reference_key: ref_r.get("reference_key"),
                project_key: None,
                question_id: ref_r.get("question_id"),
                turn_id: ref_r.get("turn_id"),
                part_id: ref_r.get("part_id"),
                node_id: ref_r.get("node_id"),
                available: is_avail,
                unavailable_reason: ref_r.get("unavailable_reason"),
            });
        }

        let source_availability = if !source_refs.is_empty() && !has_available {
            SourceAvailability::Unavailable
        } else if has_available && has_unavailable {
            SourceAvailability::PartiallyUnavailable
        } else {
            SourceAvailability::Available
        };

        items.push(L3MemoryItemView {
            item_id,
            revision_id,
            revision_number,
            category,
            status,
            title,
            summary,
            rationale,
            lifecycle: "current".to_string(),
            source_availability,
            source_references: source_refs,
            updated_at,
        });
    }

    let (last_successful_consolidation_at, revision_hash) = match state_row {
        Some(r) => (
            r.get("last_successful_consolidation_at"),
            r.get::<Option<String>, _>("revision_hash")
                .unwrap_or_default(),
        ),
        None => (None, String::new()),
    };

    Ok(Some(L3GlobalMemoryView {
        tenant_id: tenant_id.to_string(),
        items,
        last_successful_consolidation_at,
        revision_hash,
    }))
}

/// 执行 Global Consolidation 协调流程 (M35-L3-01 ~ M35-L3-06)
///
/// 1. 评估 L3 候选；若 0 候选则立即退出（0 Agent 调用）
/// 2. 检查触发门禁（周级维护或阈值达到，或显式手动）
/// 3. 单 Tenant 互斥锁加锁
/// 4. 组装事实输入并计算指纹；若指纹未变则跳过（0 Agent 调用）
/// 5. 执行操作：Create, Revise, Supersede, Keep
/// 6. 单一原子事务提交更新；失败回滚保留当前 L3
pub(crate) async fn reconcile_global_consolidation(
    pool: &SqlitePool,
    tenant_id: &str,
    now: DateTime<Utc>,
    is_manual_rebuild: bool,
    custom_operations: Option<Vec<GlobalConsolidationOperation>>,
) -> AppResult<Option<L3GlobalMemoryView>> {
    reconcile_global_consolidation_with_runner(
        pool,
        tenant_id,
        now,
        is_manual_rebuild,
        custom_operations,
        None::<
            fn(
                GlobalConsolidationInput,
            ) -> std::future::Ready<AppResult<GlobalConsolidationResult>>,
        >,
    )
    .await
}

pub(crate) async fn reconcile_global_consolidation_with_runner<F, Fut>(
    pool: &SqlitePool,
    tenant_id: &str,
    now: DateTime<Utc>,
    is_manual_rebuild: bool,
    custom_operations: Option<Vec<GlobalConsolidationOperation>>,
    agent_runner: Option<F>,
) -> AppResult<Option<L3GlobalMemoryView>>
where
    F: FnOnce(GlobalConsolidationInput) -> Fut,
    Fut: std::future::Future<Output = AppResult<GlobalConsolidationResult>>,
{
    reconcile_global_consolidation_with_runner_and_lease(
        pool,
        tenant_id,
        now,
        is_manual_rebuild,
        custom_operations,
        agent_runner,
        None,
        None,
    )
    .await
}

pub(crate) async fn reconcile_global_consolidation_with_runner_and_lease<F, Fut>(
    pool: &SqlitePool,
    tenant_id: &str,
    now: DateTime<Utc>,
    is_manual_rebuild: bool,
    custom_operations: Option<Vec<GlobalConsolidationOperation>>,
    agent_runner: Option<F>,
    job_lease: Option<(&str, &str)>,
    frozen_input: Option<GlobalConsolidationInput>,
) -> AppResult<Option<L3GlobalMemoryView>>
where
    F: FnOnce(GlobalConsolidationInput) -> Fut,
    Fut: std::future::Future<Output = AppResult<GlobalConsolidationResult>>,
{
    // 步骤 1: 读取并验证不可变输入首包
    let initial_live_input = load_global_consolidation_input(pool, tenant_id).await?;
    let frozen_input_fingerprint = frozen_input
        .as_ref()
        .map(compute_global_consolidation_fingerprint);
    if let Some(frozen_input) = frozen_input.as_ref() {
        if frozen_input.tenant_id != tenant_id
            || compute_global_consolidation_fingerprint(frozen_input)
                != compute_global_consolidation_fingerprint(&initial_live_input)
        {
            return Err(AppError::Domain {
                code: "MEMORY_WORK_ORDER_STALE".to_string(),
                message: "global maintenance evidence changed after enqueue".to_string(),
                retryable: false,
                details: None,
            });
        }
    }
    let initial_candidate_count = frozen_input
        .as_ref()
        .map(|input| input.candidates.len())
        .unwrap_or(initial_live_input.candidates.len());

    // 步骤 2: 检查触发门禁 (M35-L3-03: 0 候选绝不触发，custom_operations 除外)
    let should_trigger = if custom_operations.is_some() {
        true
    } else {
        should_trigger_global_consolidation(
            pool,
            tenant_id,
            initial_candidate_count,
            now,
            is_manual_rebuild,
        )
        .await?
    };

    if !should_trigger {
        let view = get_global_memory_l3_view(pool, tenant_id).await?;
        if let Some((job_id, ownership_token)) = job_lease {
            let completed = store::finish_memory_v2_maintenance_job_sqlx(
                pool,
                tenant_id,
                job_id,
                ownership_token,
                "succeeded",
                None,
                None,
                false,
                &Utc::now().to_rfc3339(),
            )
            .await?;
            if !completed {
                return Err(AppError::Conflict(
                    "memory v2 maintenance lease is no longer owned".to_string(),
                ));
            }
        }
        return Ok(view);
    }

    // 步骤 3: 锁定该 tenant 互斥锁
    let lock_map = global_consolidation_lock_map();
    let tenant_lock = lock_map.lock_for(tenant_id).await;
    let _guard = tenant_lock.lock().await;

    // 步骤 4: 锁内再次确认输入；Agent 只消费冻结的 Work Order 首包
    let input = if let Some(frozen_input) = frozen_input {
        let current_live_input = load_global_consolidation_input(pool, tenant_id).await?;
        if compute_global_consolidation_fingerprint(&frozen_input)
            != compute_global_consolidation_fingerprint(&current_live_input)
        {
            return Err(AppError::Domain {
                code: "MEMORY_WORK_ORDER_STALE".to_string(),
                message: "global maintenance evidence changed before execution".to_string(),
                retryable: false,
                details: None,
            });
        }
        frozen_input
    } else {
        load_global_consolidation_input(pool, tenant_id).await?
    };
    let candidates = input.candidates.clone();

    let input_fingerprint = compute_global_consolidation_fingerprint(&input);

    // 检查上次输入指纹；若完全相同则跳过 Agent 调用
    let last_fingerprint: Option<String> = sqlx::query_scalar(
        "SELECT last_input_fingerprint FROM global_memory_state WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;

    if let Some(ref prev) = last_fingerprint {
        if prev == &input_fingerprint && !is_manual_rebuild {
            let view = get_global_memory_l3_view(pool, tenant_id).await?;
            if let Some((job_id, ownership_token)) = job_lease {
                let completed = store::finish_memory_v2_maintenance_job_sqlx(
                    pool,
                    tenant_id,
                    job_id,
                    ownership_token,
                    "succeeded",
                    None,
                    None,
                    false,
                    &Utc::now().to_rfc3339(),
                )
                .await?;
                if !completed {
                    return Err(AppError::Conflict(
                        "memory v2 maintenance lease is no longer owned".to_string(),
                    ));
                }
            }
            return Ok(view);
        }
    }

    let strict_reference_validation = agent_runner.is_some();

    // 步骤 5: 确定 operations (来自 Agent 输出或测试注入)
    let operations = if let Some(ops) = custom_operations {
        ops
    } else if let Some(runner) = agent_runner {
        runner(input.clone()).await?.operations
    } else {
        // 默认将通过准入的候选转换为 Create 操作
        let mut default_ops = Vec::new();
        for candidate in &candidates {
            default_ops.push(GlobalConsolidationOperation::Create {
                category: candidate.category,
                title: candidate.title.clone(),
                statement: candidate.summary.clone(),
                rationale: candidate.rationale.clone(),
                source_refs: candidate
                    .session_references
                    .iter()
                    .filter(|r| r.available)
                    .map(|r| r.reference_key.clone())
                    .collect(),
            });
        }
        default_ops
    };

    // Agent 运行期间 SQLite 可能发生新的 L2/L3 变更。再次读取只用于
    // stale 检测，绝不替换 Agent 已消费的 frozen input。
    if let Some(expected_fingerprint) = frozen_input_fingerprint {
        let current_input = load_global_consolidation_input(pool, tenant_id).await?;
        if compute_global_consolidation_fingerprint(&current_input) != expected_fingerprint {
            return Err(AppError::Domain {
                code: "MEMORY_WORK_ORDER_STALE".to_string(),
                message: "global maintenance evidence changed before commit".to_string(),
                retryable: false,
                details: None,
            });
        }
    }

    // Preserve the real source/session locator attached to each candidate.
    // Agent-provided reference keys are only valid when they resolve to an
    // available reference from this Work Order.
    let candidate_reference_map: HashMap<_, _> = candidates
        .iter()
        .flat_map(|candidate| candidate.session_references.iter())
        .filter(|reference| reference.available)
        .map(|reference| (reference.reference_key.clone(), reference.clone()))
        .collect();

    if strict_reference_validation {
        for operation in &operations {
            let source_refs = match operation {
                GlobalConsolidationOperation::Create { source_refs, .. }
                | GlobalConsolidationOperation::Revise { source_refs, .. }
                | GlobalConsolidationOperation::Supersede { source_refs, .. } => source_refs,
                GlobalConsolidationOperation::Keep { .. } => continue,
            };
            if source_refs.is_empty()
                || promotion_nomination_for_source_refs(&candidates, source_refs)
                    == MemoryPromotionNomination::None
            {
                return Err(AppError::Validation(
                    "MEMORY_OUTPUT_INVALID: global operation has no valid global or cross-project scope"
                        .to_string(),
                ));
            }
        }
    }

    // 步骤 6: 单一原子事务提交 (M35-L3-05)
    let now_str = now.to_rfc3339();
    let mut tx = pool.begin().await.map_err(AppError::external)?;

    for op in operations {
        match op {
            GlobalConsolidationOperation::Create {
                category,
                title,
                statement,
                rationale,
                source_refs,
            } => {
                let promotion_nomination = if strict_reference_validation {
                    promotion_nomination_for_source_refs(&candidates, &source_refs)
                } else {
                    MemoryPromotionNomination::GlobalRule
                };
                let new_item_id = format!("mem-l3-{}", uuid::Uuid::new_v4());
                let new_rev_id = format!("rev-l3-{}", uuid::Uuid::new_v4());

                sqlx::query(
                    "INSERT INTO memory_items \
                     (tenant_id, id, layer, project_key, current_revision_id, lifecycle, \
                      first_seen_at, last_seen_at, created_at, updated_at) \
                     VALUES (?1, ?2, 'l3', NULL, ?3, 'current', ?4, ?4, ?4, ?4)",
                )
                .bind(tenant_id)
                .bind(&new_item_id)
                .bind(&new_rev_id)
                .bind(&now_str)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                let category_str = match category {
                    MemoryItemCategory::Decision => "decision",
                    MemoryItemCategory::Research => "research",
                    MemoryItemCategory::Verification => "verification",
                    MemoryItemCategory::Blocker => "blocker",
                    MemoryItemCategory::FollowUp => "follow_up",
                    MemoryItemCategory::Progress => "progress",
                };

                let mut hasher = Sha256::new();
                hasher.update(title.as_bytes());
                hasher.update(statement.as_bytes());
                let evidence_fingerprint = format!("{:x}", hasher.finalize());

                sqlx::query(
                    "INSERT INTO memory_item_revisions \
                     (tenant_id, id, item_id, revision_number, category, status, title, summary, \
                      rationale, recommendation_rank, promotion_nomination, occurred_at, \
                      evidence_fingerprint, generated_by_snapshot_id, supersedes_revision_id, created_at) \
                     VALUES (?1, ?2, ?3, 1, ?4, 'active', ?5, ?6, ?7, NULL, ?8, ?9, ?10, NULL, NULL, ?9)",
                )
                .bind(tenant_id)
                .bind(&new_rev_id)
                .bind(&new_item_id)
                .bind(category_str)
                .bind(&title)
                .bind(&statement)
                .bind(&rationale)
                .bind(promotion_nomination.as_str())
                .bind(&now_str)
                .bind(&evidence_fingerprint)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                for ref_key in source_refs {
                    let source_ref = resolve_global_reference(
                        &candidate_reference_map,
                        &ref_key,
                        strict_reference_validation,
                    )?;
                    let ref_id = format!("ref-l3-{}", uuid::Uuid::new_v4());
                    sqlx::query(
                        "INSERT INTO memory_item_source_references \
                         (tenant_id, id, item_revision_id, record_kind, source_id, session_id, \
                          question_id, turn_id, part_id, node_id, reference_key, source_revision, availability, created_at) \
                         VALUES (?1, ?2, ?3, 'session', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'available', ?12)",
                    )
                    .bind(tenant_id)
                    .bind(&ref_id)
                    .bind(&new_rev_id)
                    .bind(&source_ref.source_id)
                    .bind(&source_ref.session_id)
                    .bind(&source_ref.question_id)
                    .bind(&source_ref.turn_id)
                    .bind(&source_ref.part_id)
                    .bind(&source_ref.node_id)
                    .bind(&source_ref.reference_key)
                    .bind(source_ref.source_revision)
                    .bind(&now_str)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::external)?;
                }
            }
            GlobalConsolidationOperation::Revise {
                item_id,
                statement,
                rationale,
                source_refs,
            } => {
                let promotion_nomination = if strict_reference_validation {
                    promotion_nomination_for_source_refs(&candidates, &source_refs)
                } else {
                    MemoryPromotionNomination::GlobalRule
                };
                let item_row = sqlx::query(
                    "SELECT mi.current_revision_id, mir.revision_number, mir.category, mir.title \
                     FROM memory_items mi \
                     JOIN memory_item_revisions mir ON mi.tenant_id = mir.tenant_id AND mi.current_revision_id = mir.id \
                     WHERE mi.tenant_id = ?1 AND mi.id = ?2 AND mi.layer = 'l3' AND mi.lifecycle = 'current'",
                )
                .bind(tenant_id)
                .bind(&item_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(AppError::external)?;

                let prev_rev_id: String = item_row.get("current_revision_id");
                let prev_rev_number: i64 = item_row.get("revision_number");
                let category_str: String = item_row.get("category");
                let title: String = item_row.get("title");

                let new_rev_id = format!("rev-l3-{}", uuid::Uuid::new_v4());
                let new_rev_number = prev_rev_number + 1;

                let mut hasher = Sha256::new();
                hasher.update(title.as_bytes());
                hasher.update(statement.as_bytes());
                let evidence_fingerprint = format!("{:x}", hasher.finalize());

                sqlx::query(
                    "INSERT INTO memory_item_revisions \
                     (tenant_id, id, item_id, revision_number, category, status, title, summary, \
                      rationale, recommendation_rank, promotion_nomination, occurred_at, \
                      evidence_fingerprint, generated_by_snapshot_id, supersedes_revision_id, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?7, ?8, NULL, ?9, ?10, ?11, NULL, ?12, ?10)",
                )
                .bind(tenant_id)
                .bind(&new_rev_id)
                .bind(&item_id)
                .bind(new_rev_number)
                .bind(&category_str)
                .bind(&title)
                .bind(&statement)
                .bind(&rationale)
                .bind(promotion_nomination.as_str())
                .bind(&now_str)
                .bind(&evidence_fingerprint)
                .bind(&prev_rev_id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                sqlx::query(
                    "UPDATE memory_items \
                     SET current_revision_id = ?3, last_seen_at = ?4, updated_at = ?4 \
                     WHERE tenant_id = ?1 AND id = ?2",
                )
                .bind(tenant_id)
                .bind(&item_id)
                .bind(&new_rev_id)
                .bind(&now_str)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                for ref_key in source_refs {
                    let source_ref = resolve_global_reference(
                        &candidate_reference_map,
                        &ref_key,
                        strict_reference_validation,
                    )?;
                    let ref_id = format!("ref-l3-{}", uuid::Uuid::new_v4());
                    sqlx::query(
                        "INSERT INTO memory_item_source_references \
                         (tenant_id, id, item_revision_id, record_kind, source_id, session_id, \
                          question_id, turn_id, part_id, node_id, reference_key, source_revision, availability, created_at) \
                         VALUES (?1, ?2, ?3, 'session', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'available', ?12)",
                    )
                    .bind(tenant_id)
                    .bind(&ref_id)
                    .bind(&new_rev_id)
                    .bind(&source_ref.source_id)
                    .bind(&source_ref.session_id)
                    .bind(&source_ref.question_id)
                    .bind(&source_ref.turn_id)
                    .bind(&source_ref.part_id)
                    .bind(&source_ref.node_id)
                    .bind(&source_ref.reference_key)
                    .bind(source_ref.source_revision)
                    .bind(&now_str)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::external)?;
                }
            }
            GlobalConsolidationOperation::Supersede {
                old_item_id,
                replacement_title,
                replacement_statement,
                rationale,
                category,
                source_refs,
            } => {
                let promotion_nomination = if strict_reference_validation {
                    promotion_nomination_for_source_refs(&candidates, &source_refs)
                } else {
                    MemoryPromotionNomination::GlobalRule
                };
                // 标记旧条目为 superseded
                sqlx::query(
                    "UPDATE memory_items \
                     SET lifecycle = 'superseded', updated_at = ?3 \
                     WHERE tenant_id = ?1 AND id = ?2",
                )
                .bind(tenant_id)
                .bind(&old_item_id)
                .bind(&now_str)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                // 创建新条目
                let new_item_id = format!("mem-l3-{}", uuid::Uuid::new_v4());
                let new_rev_id = format!("rev-l3-{}", uuid::Uuid::new_v4());

                sqlx::query(
                    "INSERT INTO memory_items \
                     (tenant_id, id, layer, project_key, current_revision_id, lifecycle, \
                      first_seen_at, last_seen_at, created_at, updated_at) \
                     VALUES (?1, ?2, 'l3', NULL, ?3, 'current', ?4, ?4, ?4, ?4)",
                )
                .bind(tenant_id)
                .bind(&new_item_id)
                .bind(&new_rev_id)
                .bind(&now_str)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                let category_str = match category {
                    MemoryItemCategory::Decision => "decision",
                    MemoryItemCategory::Research => "research",
                    MemoryItemCategory::Verification => "verification",
                    MemoryItemCategory::Blocker => "blocker",
                    MemoryItemCategory::FollowUp => "follow_up",
                    MemoryItemCategory::Progress => "progress",
                };

                let mut hasher = Sha256::new();
                hasher.update(replacement_title.as_bytes());
                hasher.update(replacement_statement.as_bytes());
                let evidence_fingerprint = format!("{:x}", hasher.finalize());

                sqlx::query(
                    "INSERT INTO memory_item_revisions \
                     (tenant_id, id, item_id, revision_number, category, status, title, summary, \
                      rationale, recommendation_rank, promotion_nomination, occurred_at, \
                      evidence_fingerprint, generated_by_snapshot_id, supersedes_revision_id, created_at) \
                     VALUES (?1, ?2, ?3, 1, ?4, 'active', ?5, ?6, ?7, NULL, ?8, ?9, ?10, NULL, NULL, ?9)",
                )
                .bind(tenant_id)
                .bind(&new_rev_id)
                .bind(&new_item_id)
                .bind(category_str)
                .bind(&replacement_title)
                .bind(&replacement_statement)
                .bind(&rationale)
                .bind(promotion_nomination.as_str())
                .bind(&now_str)
                .bind(&evidence_fingerprint)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                // 记录 supersession 关系 (M35-L3-05)
                let super_id = format!("super-{}", uuid::Uuid::new_v4());
                sqlx::query(
                    "INSERT INTO memory_item_supersessions \
                     (tenant_id, id, superseded_item_id, superseding_item_id, reason, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .bind(tenant_id)
                .bind(&super_id)
                .bind(&old_item_id)
                .bind(&new_item_id)
                .bind(&rationale)
                .bind(&now_str)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                for ref_key in source_refs {
                    let source_ref = resolve_global_reference(
                        &candidate_reference_map,
                        &ref_key,
                        strict_reference_validation,
                    )?;
                    let ref_id = format!("ref-l3-{}", uuid::Uuid::new_v4());
                    sqlx::query(
                        "INSERT INTO memory_item_source_references \
                         (tenant_id, id, item_revision_id, record_kind, source_id, session_id, \
                          question_id, turn_id, part_id, node_id, reference_key, source_revision, availability, created_at) \
                         VALUES (?1, ?2, ?3, 'session', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'available', ?12)",
                    )
                    .bind(tenant_id)
                    .bind(&ref_id)
                    .bind(&new_rev_id)
                    .bind(&source_ref.source_id)
                    .bind(&source_ref.session_id)
                    .bind(&source_ref.question_id)
                    .bind(&source_ref.turn_id)
                    .bind(&source_ref.part_id)
                    .bind(&source_ref.node_id)
                    .bind(&source_ref.reference_key)
                    .bind(source_ref.source_revision)
                    .bind(&now_str)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::external)?;
                }
            }
            GlobalConsolidationOperation::Keep { .. } => {}
        }
    }

    // 计算新的 revision_hash
    let active_l3_revs: Vec<String> = sqlx::query_scalar(
        "SELECT mir.id \
         FROM memory_items mi \
         JOIN memory_item_revisions mir ON mi.current_revision_id = mir.id \
         WHERE mi.tenant_id = ?1 AND mi.layer = 'l3' AND mi.lifecycle = 'current' \
         ORDER BY mir.occurred_at DESC, mi.id ASC",
    )
    .bind(tenant_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(AppError::external)?;

    let mut hash_input = active_l3_revs.join(":");
    if hash_input.is_empty() {
        hash_input = format!("l3-empty-{}", tenant_id);
    }
    let mut hasher = Sha256::new();
    hasher.update(hash_input.as_bytes());
    let revision_hash = format!("{:x}", hasher.finalize());

    // 更新 global_memory_state 指针
    sqlx::query(
        "INSERT INTO global_memory_state \
         (tenant_id, last_successful_consolidation_at, last_input_fingerprint, revision_hash, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?2, ?2) \
         ON CONFLICT(tenant_id) DO UPDATE SET \
            last_successful_consolidation_at = excluded.last_successful_consolidation_at, \
            last_input_fingerprint = excluded.last_input_fingerprint, \
            revision_hash = excluded.revision_hash, \
            updated_at = excluded.updated_at",
    )
    .bind(tenant_id)
    .bind(&now_str)
    .bind(&input_fingerprint)
    .bind(&revision_hash)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;

    if let Some((job_id, ownership_token)) = job_lease {
        let completed = store::complete_memory_v2_maintenance_job_tx(
            &mut tx,
            tenant_id,
            job_id,
            ownership_token,
            &now_str,
        )
        .await?;
        if !completed {
            return Err(AppError::Conflict(
                "memory v2 maintenance lease is no longer owned".to_string(),
            ));
        }
    }

    tx.commit().await.map_err(AppError::external)?;

    get_global_memory_l3_view(pool, tenant_id).await
}

/// 长期来源失效协调器 (M35-L3-04)
///
/// 当外部 session 删除、缺失、排除或 source 禁用时：
/// 1. 仅更新 `memory_item_source_references` 的 availability 为 'unavailable'，并记录 unavailable_reason。
/// 2. 已晋升的 L2/L3 长期知识条目继续存在（lifecycle 保持 'current'），绝不自动删除或退出。
/// 3. 未晋升的 L1 条目若所有引用失效，退出为 'retired'。
pub(crate) async fn reconcile_source_invalidation(
    pool: &SqlitePool,
    tenant_id: &str,
    now: DateTime<Utc>,
    excluded_source_ids: &[String],
    excluded_session_ids: &[String],
) -> AppResult<usize> {
    let now_str = now.to_rfc3339();
    let mut affected_total = 0usize;

    // 1. source 被禁用 (enabled = 0)
    let res = sqlx::query(
        "UPDATE memory_item_source_references \
         SET availability = 'unavailable', \
             unavailable_reason = 'source_disabled', \
             unavailable_at = COALESCE(unavailable_at, ?2) \
         WHERE tenant_id = ?1 \
           AND availability = 'available' \
           AND source_id IN (SELECT id FROM conversation_sources WHERE tenant_id = ?1 AND enabled = 0)",
    )
    .bind(tenant_id)
    .bind(&now_str)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    affected_total += res.rows_affected() as usize;

    // 2. session 缺失 (missing = 1)
    let res = sqlx::query(
        "UPDATE memory_item_source_references \
         SET availability = 'unavailable', \
             unavailable_reason = 'missing', \
             unavailable_at = COALESCE(unavailable_at, ?2) \
         WHERE tenant_id = ?1 \
           AND availability = 'available' \
           AND session_id IN (SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND missing = 1)",
    )
    .bind(tenant_id)
    .bind(&now_str)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    affected_total += res.rows_affected() as usize;

    // 3. source 或 session 已被删除
    let res = sqlx::query(
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
    .bind(&now_str)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    affected_total += res.rows_affected() as usize;

    // 4. 设置中排除的 source
    for excluded_src in excluded_source_ids {
        let res = sqlx::query(
            "UPDATE memory_item_source_references \
             SET availability = 'unavailable', \
                 unavailable_reason = 'excluded', \
                 unavailable_at = COALESCE(unavailable_at, ?2) \
             WHERE tenant_id = ?1 \
               AND availability = 'available' \
               AND source_id = ?3",
        )
        .bind(tenant_id)
        .bind(&now_str)
        .bind(excluded_src)
        .execute(pool)
        .await
        .map_err(AppError::external)?;
        affected_total += res.rows_affected() as usize;
    }

    // 5. 设置中排除的 session
    for excluded_sess in excluded_session_ids {
        let res = sqlx::query(
            "UPDATE memory_item_source_references \
             SET availability = 'unavailable', \
                 unavailable_reason = 'excluded', \
                 unavailable_at = COALESCE(unavailable_at, ?2) \
             WHERE tenant_id = ?1 \
               AND availability = 'available' \
               AND session_id = ?3",
        )
        .bind(tenant_id)
        .bind(&now_str)
        .bind(excluded_sess)
        .execute(pool)
        .await
        .map_err(AppError::external)?;
        affected_total += res.rows_affected() as usize;
    }

    // 6. M35-L3-04: 来源失效使未晋升 L1 退出 (layer = 'l1' AND lifecycle = 'current')
    // 注意：L2 / L3 条目绝不因来源不可用自动 retired 或删除！
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
    .bind(&now_str)
    .execute(pool)
    .await
    .map_err(AppError::external)?;

    Ok(affected_total)
}

#[cfg(test)]
#[path = "global_consolidation_pipeline_tests.rs"]
pub(crate) mod tests;
