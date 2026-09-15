use crate::backend::dto::recent_snapshot::SourceAvailability;
use crate::backend::models::{
    compute_global_consolidation_fingerprint, GlobalConsolidationInput,
    GlobalConsolidationOperation, L3CandidateReferenceView,
    L3GlobalMemoryView, L3MemoryItemView, L3PromotionCandidate, L3SourceReferenceView,
    L3SupersededIndexItem, MemoryItemCategory, MemoryItemStatus, MemoryPromotionNomination,
};
use crate::backend::runtime::{AppError, AppResult};
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
                sr.availability, sr.unavailable_reason, mi.project_key \
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
        let availability: String = r.get("availability");
        let unavailable_reason: Option<String> = r.get("unavailable_reason");

        refs_by_revision
            .entry(rev_id)
            .or_default()
            .push(L3CandidateReferenceView {
                project_key,
                source_id,
                session_id,
                reference_key,
                role: None,
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
        let entry = clusters.entry(cluster_key).or_insert_with(|| GroupedCandidate {
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
        let project_has_available = item_refs.iter().any(|r| r.project_key == project_key && r.available);
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
        let is_cross_project = grouped.supporting_projects.len() >= 2
            && grouped.supporting_sessions.len() >= 2;

        if is_global_rule {
            // M35-L3-02 规则 1: 明确全局规则必须有至少一个 available 引用
            let has_available = grouped.references.iter().any(|r| r.available);
            if has_available {
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
            let mut sorted_projects: Vec<String> = grouped.supporting_projects.into_iter().collect();
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
            "SELECT source_id, session_id, reference_key, availability, unavailable_reason \
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
    // 步骤 1: 评估候选
    let candidates = evaluate_l3_candidates(pool, tenant_id).await?;

    // 步骤 2: 检查触发门禁 (M35-L3-03: 0 候选绝不触发，custom_operations 除外)
    let should_trigger = if custom_operations.is_some() {
        true
    } else {
        should_trigger_global_consolidation(
            pool,
            tenant_id,
            candidates.len(),
            now,
            is_manual_rebuild,
        )
        .await?
    };

    if !should_trigger {
        return get_global_memory_l3_view(pool, tenant_id).await;
    }

    // 步骤 3: 锁定该 tenant 互斥锁
    let lock_map = global_consolidation_lock_map();
    let tenant_lock = lock_map.lock_for(tenant_id).await;
    let _guard = tenant_lock.lock().await;

    // 步骤 4: 组装输入事实首包
    let current_view = get_global_memory_l3_view(pool, tenant_id).await?;
    let current_l3_items = current_view
        .as_ref()
        .map(|v| v.items.clone())
        .unwrap_or_default();

    // 加载被取代条目轻量索引 (M35-L3-05)
    let superseded_rows = sqlx::query(
        "SELECT superseded_item_id, superseding_item_id, reason \
         FROM memory_item_supersessions WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    let mut superseded_index = Vec::new();
    for row in superseded_rows {
        superseded_index.push(L3SupersededIndexItem {
            old_item_id: row.get("superseded_item_id"),
            superseding_item_id: row.get("superseding_item_id"),
            reason: row.get("reason"),
        });
    }

    let input = GlobalConsolidationInput {
        tenant_id: tenant_id.to_string(),
        current_l3_items,
        candidates: candidates.clone(),
        superseded_index,
    };

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
            return get_global_memory_l3_view(pool, tenant_id).await;
        }
    }

    // 步骤 5: 确定 operations (来自 Agent 输出或测试注入)
    let operations = if let Some(ops) = custom_operations {
        ops
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
                     VALUES (?1, ?2, ?3, 1, ?4, 'active', ?5, ?6, ?7, NULL, 'global_rule', ?8, ?9, NULL, NULL, ?8)",
                )
                .bind(tenant_id)
                .bind(&new_rev_id)
                .bind(&new_item_id)
                .bind(category_str)
                .bind(&title)
                .bind(&statement)
                .bind(&rationale)
                .bind(&now_str)
                .bind(&evidence_fingerprint)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                for ref_key in source_refs {
                    let ref_id = format!("ref-l3-{}", uuid::Uuid::new_v4());
                    sqlx::query(
                        "INSERT INTO memory_item_source_references \
                         (tenant_id, id, item_revision_id, record_kind, source_id, session_id, \
                          reference_key, source_revision, availability, created_at) \
                         VALUES (?1, ?2, ?3, 'session', 'global', 'global_evidence', ?4, 1, 'available', ?5)",
                    )
                    .bind(tenant_id)
                    .bind(&ref_id)
                    .bind(&new_rev_id)
                    .bind(&ref_key)
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
                     VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?7, ?8, NULL, 'global_rule', ?9, ?10, NULL, ?11, ?9)",
                )
                .bind(tenant_id)
                .bind(&new_rev_id)
                .bind(&item_id)
                .bind(new_rev_number)
                .bind(&category_str)
                .bind(&title)
                .bind(&statement)
                .bind(&rationale)
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
                    let ref_id = format!("ref-l3-{}", uuid::Uuid::new_v4());
                    sqlx::query(
                        "INSERT INTO memory_item_source_references \
                         (tenant_id, id, item_revision_id, record_kind, source_id, session_id, \
                          reference_key, source_revision, availability, created_at) \
                         VALUES (?1, ?2, ?3, 'session', 'global', 'global_evidence', ?4, 1, 'available', ?5)",
                    )
                    .bind(tenant_id)
                    .bind(&ref_id)
                    .bind(&new_rev_id)
                    .bind(&ref_key)
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
                     VALUES (?1, ?2, ?3, 1, ?4, 'active', ?5, ?6, ?7, NULL, 'global_rule', ?8, ?9, NULL, NULL, ?8)",
                )
                .bind(tenant_id)
                .bind(&new_rev_id)
                .bind(&new_item_id)
                .bind(category_str)
                .bind(&replacement_title)
                .bind(&replacement_statement)
                .bind(&rationale)
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
                    let ref_id = format!("ref-l3-{}", uuid::Uuid::new_v4());
                    sqlx::query(
                        "INSERT INTO memory_item_source_references \
                         (tenant_id, id, item_revision_id, record_kind, source_id, session_id, \
                          reference_key, source_revision, availability, created_at) \
                         VALUES (?1, ?2, ?3, 'session', 'global', 'global_evidence', ?4, 1, 'available', ?5)",
                    )
                    .bind(tenant_id)
                    .bind(&ref_id)
                    .bind(&new_rev_id)
                    .bind(&ref_key)
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
pub(crate) mod tests {
    use super::*;
    use crate::backend::application::AppService;
    use chrono::TimeZone;

    async fn setup_test_db() -> (AppService, SqlitePool, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("test-global-consolidation-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let db_path = root.join("app.db");
        let service = AppService::open_with_db_path(db_path)
            .await
            .expect("open service");
        let pool = service.db.pool().clone();
        (service, pool, root)
    }

    /// 辅助插入 L2 条目及其 revision 与引用
    async fn insert_l2_fixture(
        pool: &SqlitePool,
        tenant_id: &str,
        project_key: &str,
        title: &str,
        summary: &str,
        category: &str,
        nomination: &str,
        session_id: &str,
        reference_key: &str,
        available: bool,
    ) -> (String, String) {
        let now = "2026-09-15T00:00:00Z";
        let item_id = format!("l2-item-{}", uuid::Uuid::new_v4());
        let rev_id = format!("l2-rev-{}", uuid::Uuid::new_v4());

        sqlx::query(
            "INSERT INTO memory_items \
             (tenant_id, id, layer, project_key, current_revision_id, lifecycle, \
              first_seen_at, last_seen_at, created_at, updated_at) \
             VALUES (?1, ?2, 'l2', ?3, ?4, 'current', ?5, ?5, ?5, ?5)",
        )
        .bind(tenant_id)
        .bind(&item_id)
        .bind(project_key)
        .bind(&rev_id)
        .bind(now)
        .execute(pool)
        .await
        .expect("insert l2 item");

        let mut hasher = Sha256::new();
        hasher.update(title.as_bytes());
        let fp = format!("{:x}", hasher.finalize());

        sqlx::query(
            "INSERT INTO memory_item_revisions \
             (tenant_id, id, item_id, revision_number, category, status, title, summary, \
              rationale, recommendation_rank, promotion_nomination, occurred_at, \
              evidence_fingerprint, created_at) \
             VALUES (?1, ?2, ?3, 1, ?4, 'active', ?5, ?6, 'Test rationale', NULL, ?7, ?8, ?9, ?8)",
        )
        .bind(tenant_id)
        .bind(&rev_id)
        .bind(&item_id)
        .bind(category)
        .bind(title)
        .bind(summary)
        .bind(nomination)
        .bind(now)
        .bind(&fp)
        .execute(pool)
        .await
        .expect("insert l2 revision");

        let ref_id = format!("ref-{}", uuid::Uuid::new_v4());
        let avail_str = if available { "available" } else { "unavailable" };
        let reason = if available { None } else { Some("deleted") };

        sqlx::query(
            "INSERT INTO memory_item_source_references \
             (tenant_id, id, item_revision_id, record_kind, source_id, session_id, \
              reference_key, source_revision, availability, unavailable_reason, created_at) \
             VALUES (?1, ?2, ?3, 'session', 'src-1', ?4, ?5, 1, ?6, ?7, ?8)",
        )
        .bind(tenant_id)
        .bind(&ref_id)
        .bind(&rev_id)
        .bind(session_id)
        .bind(reference_key)
        .bind(avail_str)
        .bind(reason)
        .bind(now)
        .execute(pool)
        .await
        .expect("insert l2 ref");

        (item_id, rev_id)
    }

    /// 测试 1: M35-L3-02 跨项目候选必须由至少两个不同真实 project_key 独立支持
    #[tokio::test]
    async fn test_m35_l3_02_cross_project_candidate_requires_two_distinct_real_projects() {
        let (_service, pool, root) = setup_test_db().await;

        // 项目 A 引入知识 "Shared Error Handling Pattern"
        insert_l2_fixture(
            &pool,
            "default",
            "project-alpha",
            "Shared Error Handling Pattern",
            "Always wrap external calls in AppError::external",
            "decision",
            "cross_project_pattern",
            "session-alpha-1",
            "ref-alpha-1",
            true,
        )
        .await;

        // 单项目时评估候选: 不足以成为 cross_project_pattern 候选
        let candidates_one = evaluate_l3_candidates(&pool, "default").await.unwrap();
        assert!(
            candidates_one.is_empty(),
            "单项目支持不能形成 cross_project_pattern 候选"
        );

        // 如果第二个项目来自 unassigned，仍然不能成为合格候选 (M35-L3-02)
        insert_l2_fixture(
            &pool,
            "default",
            "unassigned",
            "Shared Error Handling Pattern",
            "Always wrap external calls in AppError::external",
            "decision",
            "cross_project_pattern",
            "session-unassigned-1",
            "ref-unassigned-1",
            true,
        )
        .await;
        let candidates_unassigned = evaluate_l3_candidates(&pool, "default").await.unwrap();
        assert!(
            candidates_unassigned.is_empty(),
            "unassigned 项目不能作为独立支持项目"
        );

        // 项目 B (第二个真实项目) 独立支持该知识
        insert_l2_fixture(
            &pool,
            "default",
            "project-beta",
            "Shared Error Handling Pattern",
            "Always wrap external calls in AppError::external",
            "decision",
            "cross_project_pattern",
            "session-beta-1",
            "ref-beta-1",
            true,
        )
        .await;

        // 两个真实独立项目均支持: 形成合格候选
        let candidates_two = evaluate_l3_candidates(&pool, "default").await.unwrap();
        assert_eq!(candidates_two.len(), 1);
        assert_eq!(candidates_two[0].title, "Shared Error Handling Pattern");
        assert_eq!(candidates_two[0].nomination, MemoryPromotionNomination::CrossProjectPattern);
        assert_eq!(candidates_two[0].supporting_project_keys.len(), 2);
        assert!(candidates_two[0].supporting_project_keys.contains(&"project-alpha".to_string()));
        assert!(candidates_two[0].supporting_project_keys.contains(&"project-beta".to_string()));

        let _ = std::fs::remove_dir_all(root);
    }

    /// 测试 2: M35-L3-02 & M35-L3-04 unavailable 引用不计入独立支持数
    #[tokio::test]
    async fn test_m35_l3_02_unavailable_references_do_not_count_towards_cross_project() {
        let (_service, pool, root) = setup_test_db().await;

        // 项目 A: available 引用
        insert_l2_fixture(
            &pool,
            "default",
            "project-alpha",
            "Universal Metric Format",
            "Use ISO timestamps for all metrics",
            "decision",
            "cross_project_pattern",
            "session-alpha-1",
            "ref-alpha-1",
            true,
        )
        .await;

        // 项目 B: unavailable 引用 (来源已失效)
        insert_l2_fixture(
            &pool,
            "default",
            "project-beta",
            "Universal Metric Format",
            "Use ISO timestamps for all metrics",
            "decision",
            "cross_project_pattern",
            "session-beta-1",
            "ref-beta-1",
            false, // unavailable!
        )
        .await;

        // 因为项目 B 仅有 unavailable 引用，不计入独立支持
        let candidates = evaluate_l3_candidates(&pool, "default").await.unwrap();
        assert!(
            candidates.is_empty(),
            "包含 unavailable 引用的项目不能计入独立支持数"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    /// 测试 3: M35-L3-02 明确全局规则 (global_rule) 需 available 用户引用
    #[tokio::test]
    async fn test_m35_l3_02_global_rule_requires_available_reference() {
        let (_service, pool, root) = setup_test_db().await;

        // 仅在单项目中声明，但被提名为 global_rule
        insert_l2_fixture(
            &pool,
            "default",
            "project-alpha",
            "Global License Policy",
            "All internal libraries must use Apache-2.0",
            "decision",
            "global_rule",
            "session-alpha-1",
            "ref-alpha-1",
            true,
        )
        .await;

        let candidates = evaluate_l3_candidates(&pool, "default").await.unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].nomination, MemoryPromotionNomination::GlobalRule);
        assert_eq!(candidates[0].title, "Global License Policy");

        let _ = std::fs::remove_dir_all(root);
    }

    /// 测试 4: M35-L3-03 0 候选绝不触发 Agent
    #[tokio::test]
    async fn test_m35_l3_03_zero_candidates_triggers_zero_agent_calls() {
        let (_service, pool, root) = setup_test_db().await;

        let now = Utc.with_ymd_and_hms(2026, 9, 15, 0, 0, 0).unwrap();
        let should_trigger = should_trigger_global_consolidation(&pool, "default", 0, now, false).await.unwrap();
        assert!(!should_trigger, "0 候选时 should_trigger 必须为 false");

        // 即使是 manual_rebuild，0 候选也不能触发
        let should_trigger_manual = should_trigger_global_consolidation(&pool, "default", 0, now, true).await.unwrap();
        assert!(!should_trigger_manual, "0 候选时 manual_rebuild 依然必须为 false");

        let _ = std::fs::remove_dir_all(root);
    }

    /// 测试 5: M35-L3-03 低频触发（周级/8候选）与指纹跳过
    #[tokio::test]
    async fn test_m35_l3_03_low_frequency_trigger_and_fingerprint_skip() {
        let (_service, pool, root) = setup_test_db().await;

        let now = Utc.with_ymd_and_hms(2026, 9, 15, 0, 0, 0).unwrap();

        // 少量候选 (3 个)
        let trigger_few = should_trigger_global_consolidation(&pool, "default", 3, now, false).await.unwrap();
        // 因为没有上次成功记录，首次有候选允许触发
        assert!(trigger_few);

        // 插入上次成功记录为 3 天前
        let three_days_ago = Utc.with_ymd_and_hms(2026, 9, 12, 0, 0, 0).unwrap().to_rfc3339();
        sqlx::query(
            "INSERT INTO global_memory_state (tenant_id, last_successful_consolidation_at, last_input_fingerprint, revision_hash, created_at, updated_at) \
             VALUES ('default', ?1, 'fp-old', 'rev-hash', ?1, ?1)",
        )
        .bind(&three_days_ago)
        .execute(&pool)
        .await
        .unwrap();

        // 3 天前且只有 3 个候选 -> 不足 7 天且未达 8 候选，不触发
        let trigger_blocked = should_trigger_global_consolidation(&pool, "default", 3, now, false).await.unwrap();
        assert!(!trigger_blocked, "未达 7 天且候选少于 8 时不触发低频 Consolidation");

        // 候选达到 8 个 -> 立即触发
        let trigger_threshold = should_trigger_global_consolidation(&pool, "default", 8, now, false).await.unwrap();
        assert!(trigger_threshold, "候选达 8 个时立即触发");

        // 超过 7 天 (如 8 天前) -> 触发周级巩固
        let eight_days_ago = Utc.with_ymd_and_hms(2026, 9, 7, 0, 0, 0).unwrap().to_rfc3339();
        sqlx::query("UPDATE global_memory_state SET last_successful_consolidation_at = ?1 WHERE tenant_id = 'default'")
            .bind(&eight_days_ago)
            .execute(&pool)
            .await
            .unwrap();

        let trigger_weekly = should_trigger_global_consolidation(&pool, "default", 1, now, false).await.unwrap();
        assert!(trigger_weekly, "距离上次成功超过 7 天且有候选时触发周级维护");

        let _ = std::fs::remove_dir_all(root);
    }

    /// 测试 6: M35-L3-04 来源失效更新引用可用性，保留 L2/L3 长期条目
    #[tokio::test]
    async fn test_m35_l3_04_source_invalidation_updates_reference_keeps_l2_l3() {
        let (_service, pool, root) = setup_test_db().await;

        // 创建 source 和 session 实体
        sqlx::query("INSERT INTO conversation_sources (tenant_id, id, adapter_id, name, kind, location, enabled, created_at, updated_at) VALUES ('default', 'src-1', 'adp-1', 'Source 1', 'fs', '/path', 1, '2026-09-15T00:00:00Z', '2026-09-15T00:00:00Z')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, title, missing, created_at, imported_at) VALUES ('default', 'sess-1', 'src-1', 'adp-1', 'ext-1', 'Session 1', 0, '2026-09-15T00:00:00Z', '2026-09-15T00:00:00Z')")
            .execute(&pool).await.unwrap();

        // 插入 L2 条目
        let (l2_item_id, _) = insert_l2_fixture(
            &pool,
            "default",
            "proj-a",
            "Immutable Architecture Principle",
            "Always keep domain models decoupled from persistence",
            "decision",
            "project_decision",
            "sess-1",
            "ref-1",
            true,
        )
        .await;

        // 插入 L3 条目
        let now = Utc.with_ymd_and_hms(2026, 9, 15, 0, 0, 0).unwrap();
        let ops = vec![GlobalConsolidationOperation::Create {
            category: MemoryItemCategory::Decision,
            title: "Global Resilience Rule".to_string(),
            statement: "Never drop long term knowledge on source deletion".to_string(),
            rationale: "Contract M35-L3-04".to_string(),
            source_refs: vec!["ref-l3-sess-1".to_string()],
        }];
        let l3_view = reconcile_global_consolidation(&pool, "default", now, true, Some(ops))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(l3_view.items.len(), 1);
        let l3_item_id = l3_view.items[0].item_id.clone();

        // 将该 session 标记为 missing = 1
        sqlx::query("UPDATE conversation_sessions SET missing = 1 WHERE tenant_id = 'default' AND id = 'sess-1'")
            .execute(&pool).await.unwrap();

        // 执行来源失效协调
        reconcile_source_invalidation(&pool, "default", now, &[], &[]).await.unwrap();

        // 验证引用的状态变为 unavailable, reason = missing
        let l2_ref_avail: String = sqlx::query_scalar(
            "SELECT availability FROM memory_item_source_references WHERE session_id = 'sess-1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(l2_ref_avail, "unavailable");

        // 关键验证 (M35-L3-04): L2 和 L3 条目的 lifecycle 仍然是 'current'！
        let l2_lifecycle: String = sqlx::query_scalar(
            "SELECT lifecycle FROM memory_items WHERE tenant_id = 'default' AND id = ?1",
        )
        .bind(&l2_item_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(l2_lifecycle, "current", "已晋升的 L2 条目不能因来源失效而被删除或 retired");

        let l3_lifecycle: String = sqlx::query_scalar(
            "SELECT lifecycle FROM memory_items WHERE tenant_id = 'default' AND id = ?1",
        )
        .bind(&l3_item_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(l3_lifecycle, "current", "已晋升的 L3 条目不能因来源失效而被删除或 retired");

        let _ = std::fs::remove_dir_all(root);
    }

    /// 测试 7: M35-L3-05 Revise 生成新 revision 并完整保留历史
    #[tokio::test]
    async fn test_m35_l3_05_revise_creates_new_revision_preserving_history() {
        let (_service, pool, root) = setup_test_db().await;
        let now = Utc.with_ymd_and_hms(2026, 9, 15, 0, 0, 0).unwrap();

        // 1. Create L3 条目
        let create_op = vec![GlobalConsolidationOperation::Create {
            category: MemoryItemCategory::Decision,
            title: "Database Lock Standard".to_string(),
            statement: "Use 500ms lock timeout".to_string(),
            rationale: "Initial policy".to_string(),
            source_refs: vec!["ref-1".to_string()],
        }];
        let view1 = reconcile_global_consolidation(&pool, "default", now, true, Some(create_op))
            .await
            .unwrap()
            .unwrap();
        let item_id = view1.items[0].item_id.clone();
        let rev1_id = view1.items[0].revision_id.clone();
        assert_eq!(view1.items[0].revision_number, 1);
        assert_eq!(view1.items[0].summary, "Use 500ms lock timeout");

        // 2. Revise 文本更新
        let revise_now = Utc.with_ymd_and_hms(2026, 9, 16, 0, 0, 0).unwrap();
        let revise_op = vec![GlobalConsolidationOperation::Revise {
            item_id: item_id.clone(),
            statement: "Use 1000ms lock timeout for large batches".to_string(),
            rationale: "Observed batch timeout under high concurrency".to_string(),
            source_refs: vec!["ref-2".to_string()],
        }];
        let view2 = reconcile_global_consolidation(&pool, "default", revise_now, true, Some(revise_op))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(view2.items.len(), 1);
        assert_eq!(view2.items[0].item_id, item_id);
        assert_eq!(view2.items[0].revision_number, 2);
        assert_eq!(view2.items[0].summary, "Use 1000ms lock timeout for large batches");
        let rev2_id = view2.items[0].revision_id.clone();
        assert_ne!(rev1_id, rev2_id);

        // 3. 验证历史完整保留 (M35-L3-05)
        let total_revs: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM memory_item_revisions WHERE tenant_id = 'default' AND item_id = ?1",
        )
        .bind(&item_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(total_revs, 2, "旧 revision 必须保留在数据库中供审计与回忆");

        let supersedes_check: Option<String> = sqlx::query_scalar(
            "SELECT supersedes_revision_id FROM memory_item_revisions WHERE id = ?1",
        )
        .bind(&rev2_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(supersedes_check, Some(rev1_id));

        let _ = std::fs::remove_dir_all(root);
    }

    /// 测试 8: M35-L3-05 Supersede 标记旧条目为 superseded 并新建取代条目
    #[tokio::test]
    async fn test_m35_l3_05_supersede_marks_old_and_creates_new_l3() {
        let (_service, pool, root) = setup_test_db().await;
        let now = Utc.with_ymd_and_hms(2026, 9, 15, 0, 0, 0).unwrap();

        // 1. 创建旧条目
        let create_op = vec![GlobalConsolidationOperation::Create {
            category: MemoryItemCategory::Decision,
            title: "Legacy State Machine V1".to_string(),
            statement: "Use monolithic state".to_string(),
            rationale: "Early MVP".to_string(),
            source_refs: vec!["ref-1".to_string()],
        }];
        let view1 = reconcile_global_consolidation(&pool, "default", now, true, Some(create_op))
            .await
            .unwrap()
            .unwrap();
        let old_item_id = view1.items[0].item_id.clone();

        // 2. 执行 Supersede 操作
        let supersede_now = Utc.with_ymd_and_hms(2026, 9, 16, 0, 0, 0).unwrap();
        let supersede_op = vec![GlobalConsolidationOperation::Supersede {
            old_item_id: old_item_id.clone(),
            replacement_title: "Event-Driven State Architecture V2".to_string(),
            replacement_statement: "Transition to distributed event-driven state machine".to_string(),
            rationale: "ADR-0015 full replacement".to_string(),
            category: MemoryItemCategory::Decision,
            source_refs: vec!["ref-2".to_string()],
        }];
        let view2 = reconcile_global_consolidation(&pool, "default", supersede_now, true, Some(supersede_op))
            .await
            .unwrap()
            .unwrap();

        // 活跃列表中旧条目已被过滤，只包含新条目 (M35-L3-06)
        assert_eq!(view2.items.len(), 1);
        assert_ne!(view2.items[0].item_id, old_item_id);
        assert_eq!(view2.items[0].title, "Event-Driven State Architecture V2");

        // 验证数据库中旧条目 lifecycle 为 superseded
        let old_lifecycle: String = sqlx::query_scalar(
            "SELECT lifecycle FROM memory_items WHERE id = ?1",
        )
        .bind(&old_item_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(old_lifecycle, "superseded");

        // 验证 memory_item_supersessions 关系表记录
        let super_row = sqlx::query(
            "SELECT superseded_item_id, superseding_item_id, reason \
             FROM memory_item_supersessions WHERE superseded_item_id = ?1",
        )
        .bind(&old_item_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        let target_item_id: String = super_row.get("superseding_item_id");
        assert_eq!(target_item_id, view2.items[0].item_id);
        let reason: String = super_row.get("reason");
        assert_eq!(reason, "ADR-0015 full replacement");

        let _ = std::fs::remove_dir_all(root);
    }

    /// 测试 9: M35-L3-06 Context Resolver 读取当前 L3 且忽略 superseded
    #[tokio::test]
    async fn test_m35_l3_06_context_resolver_reads_current_l3_and_ignores_superseded() {
        let (service, pool, root) = setup_test_db().await;
        let now = Utc.with_ymd_and_hms(2026, 9, 15, 0, 0, 0).unwrap();

        // 创建条目 A 并随后 supersede 它
        let ops1 = vec![GlobalConsolidationOperation::Create {
            category: MemoryItemCategory::Decision,
            title: "Old Global Rule".to_string(),
            statement: "Old deprecated statement".to_string(),
            rationale: "To be superseded".to_string(),
            source_refs: vec!["ref-old".to_string()],
        }];
        let view1 = reconcile_global_consolidation(&pool, "default", now, true, Some(ops1))
            .await
            .unwrap()
            .unwrap();
        let old_item_id = view1.items[0].item_id.clone();

        let ops2 = vec![GlobalConsolidationOperation::Supersede {
            old_item_id: old_item_id.clone(),
            replacement_title: "Active Global Rule".to_string(),
            replacement_statement: "New active long-term principle".to_string(),
            rationale: "Replaces old rule".to_string(),
            category: MemoryItemCategory::Decision,
            source_refs: vec!["ref-new".to_string()],
        }];
        reconcile_global_consolidation(&pool, "default", now, true, Some(ops2))
            .await
            .unwrap();

        // Context 解析
        let ctx = service
            .resolve_memory_context(crate::backend::application::MemoryContextResolveParams {
                project_path: None,
                query: None,
                token_budget: Some(2000),
            })
            .await
            .unwrap();

        assert!(ctx.text.contains("Active Global Rule"));
        assert!(ctx.text.contains("New active long-term principle"));
        assert!(!ctx.text.contains("Old Global Rule"));
        assert!(!ctx.text.contains("Old deprecated statement"));

        // 验证 references 包含 global_memory_l3
        let l3_refs: Vec<_> = ctx.references.iter().filter(|r| r.kind == "global_memory_l3").collect();
        assert_eq!(l3_refs.len(), 1);

        let _ = std::fs::remove_dir_all(root);
    }
}
