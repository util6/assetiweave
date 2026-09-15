use crate::backend::dto::recent_snapshot::SourceAvailability;
use crate::backend::models::{
    compute_project_consolidation_fingerprint, L2CandidateReferenceView, L2MemoryItemView,
    L2ProjectMemoryView, L2PromotionCandidate, L2SourceAvailabilityItem, L2SourceReferenceView,
    L2SupersededIndexItem, MemoryItemCategory, MemoryItemStatus, MemoryPromotionNomination,
    ProjectConsolidationInput, ProjectConsolidationOperation, ProjectConsolidationResult,
};
use crate::backend::runtime::{AppError, AppResult};
use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex as TokioMutex;

/// 全局同项目串行锁单例
static PROJECT_CONSOLIDATION_LOCKS: OnceLock<ProjectConsolidationLockMap> = OnceLock::new();

pub(crate) fn global_project_consolidation_lock_map() -> &'static ProjectConsolidationLockMap {
    PROJECT_CONSOLIDATION_LOCKS.get_or_init(ProjectConsolidationLockMap::new)
}

/// 同项目串行锁管理器（按 tenant_id + project_key 分片互斥）
#[derive(Clone, Default)]
pub(crate) struct ProjectConsolidationLockMap {
    locks: Arc<TokioMutex<HashMap<String, Arc<TokioMutex<()>>>>>,
}

impl ProjectConsolidationLockMap {
    pub(crate) fn new() -> Self {
        Self {
            locks: Arc::new(TokioMutex::new(HashMap::new())),
        }
    }

    pub(crate) async fn lock_for(&self, tenant_id: &str, project_key: &str) -> Arc<TokioMutex<()>> {
        let key = format!("{}:{}", tenant_id, project_key);
        let mut map = self.locks.lock().await;
        map.entry(key)
            .or_insert_with(|| Arc::new(TokioMutex::new(())))
            .clone()
    }
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
            "SELECT source_id, session_id, reference_key, question_id, turn_id, part_id, node_id, availability \
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
            let is_user_content = question_id.is_some() || turn_id.is_some() || reference_key.contains("user");
            if is_available && is_user_content {
                has_available_user_ref = true;
            }

            candidate_refs.push(L2CandidateReferenceView {
                source_id,
                session_id,
                reference_key,
                role: if is_user_content { Some("user".to_string()) } else { None },
                question_id,
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

/// 默认 Project Consolidation 调度器（无自定义 runner）
pub(crate) async fn reconcile_project_consolidation_default(
    pool: &SqlitePool,
    tenant_id: &str,
    project_key: &str,
    project_path: Option<&str>,
    lock_map: &ProjectConsolidationLockMap,
) -> AppResult<Option<L2ProjectMemoryView>> {
    reconcile_project_consolidation(
        pool,
        tenant_id,
        project_key,
        project_path,
        lock_map,
        None::<fn(ProjectConsolidationInput) -> std::future::Ready<AppResult<ProjectConsolidationResult>>>,
    )
    .await
}

/// Project Consolidation 协调器 (M35-L2-06)
///
/// 核心准则：
/// 1. 同项目串行 (通过 LockMap 锁)
/// 2. 无候选且无输入变化时，Agent 调用严格为 0 次 (M35-L2-06 / M35-V18)
/// 3. 输入指纹相同则跳过
/// 4. 事务内原子提交；任何失败保留 current L2
pub(crate) async fn reconcile_project_consolidation<F, Fut>(
    pool: &SqlitePool,
    tenant_id: &str,
    project_key: &str,
    project_path: Option<&str>,
    lock_map: &ProjectConsolidationLockMap,
    agent_runner: Option<F>,
) -> AppResult<Option<L2ProjectMemoryView>>
where
    F: FnOnce(ProjectConsolidationInput) -> Fut,
    Fut: std::future::Future<Output = AppResult<ProjectConsolidationResult>>,
{
    if project_key.trim().is_empty() || project_key == "unassigned" {
        return Ok(None);
    }

    // 1. 获取同项目互斥锁 (同项目串行，不同项目并行)
    let project_lock = lock_map.lock_for(tenant_id, project_key).await;
    let _guard = project_lock.lock().await;

    // 2. 评估合格晋升候选 (M35-L2-01 ~ M35-L2-05)
    let candidates = evaluate_l2_candidates(pool, tenant_id, project_key).await?;

    // 加载当前 L2 条目
    let current_l2 = load_l2_items(pool, tenant_id, project_key).await?;

    // 加载已被取代条目索引
    let superseded_index = load_superseded_index(pool, tenant_id, project_key).await?;

    // 检查来源失效变更
    let availability_changes = load_source_availability_summary(pool, tenant_id, project_key).await?;

    // 3. 触发门禁判断 (M35-L2-06):
    // 仅在出现合格候选、来源可用性变更或后续证据修订时运行；无候选时不空转
    if candidates.is_empty() && availability_changes.is_empty() {
        // 无候选、无变更时：Agent 调用严格为 0 次！直接返回现有视图
        return load_l2_project_memory_view(pool, tenant_id, project_key).await;
    }

    // 4. 构建输入事实包与指纹
    let input = ProjectConsolidationInput {
        project_key: project_key.to_string(),
        project_title: project_key.to_string(),
        project_path: project_path.map(|p| p.to_string()),
        current_l2_items: current_l2.clone(),
        candidates: candidates.clone(),
        source_availability_summary: availability_changes,
        superseded_index,
    };

    let input_fingerprint = compute_project_consolidation_fingerprint(&input);

    // 查询上一次成功的 consolidation 记录
    let last_state = sqlx::query(
        "SELECT last_input_fingerprint, revision_hash \
         FROM project_memory_state \
         WHERE tenant_id = ?1 AND project_key = ?2",
    )
    .bind(tenant_id)
    .bind(project_key)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;

    if let Some(row) = last_state {
        let last_fp: Option<String> = row.get("last_input_fingerprint");
        if last_fp.as_deref() == Some(&input_fingerprint) {
            // 输入指纹完全相同：跳过 Agent 调用 (0 Agent calls)
            return load_l2_project_memory_view(pool, tenant_id, project_key).await;
        }
    }

    // 5. 调用 Project Agent 获得结构化 operations (若未提供 runner，则采用确定性直通)
    let consolidation_result = if let Some(runner) = agent_runner {
        runner(input).await?
    } else {
        // 默认确定性直通逻辑：为候选创建 Create 操作
        let mut default_ops = Vec::new();
        for candidate in &candidates {
            default_ops.push(ProjectConsolidationOperation::Create {
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
        ProjectConsolidationResult {
            operations: default_ops,
        }
    };

    // 6. 应用准入与单事务原子提交 (M35-L2-06: 失败保留 current L2)
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let now = Utc::now().to_rfc3339();

    for op in consolidation_result.operations {
        match op {
            ProjectConsolidationOperation::Create {
                category,
                title,
                statement,
                rationale,
                source_refs,
            } => {
                let new_item_id = format!("item-l2-{}", uuid::Uuid::new_v4());
                let new_rev_id = format!("rev-l2-{}", uuid::Uuid::new_v4());

                // 写入 memory_items (layer = 'l2')
                sqlx::query(
                    "INSERT INTO memory_items (\
                        tenant_id, id, layer, project_key, current_revision_id, \
                        lifecycle, first_seen_at, last_seen_at, created_at, updated_at\
                     ) VALUES (?1, ?2, 'l2', ?3, ?4, 'current', ?5, ?5, ?5, ?5)",
                )
                .bind(tenant_id)
                .bind(&new_item_id)
                .bind(project_key)
                .bind(&new_rev_id)
                .bind(&now)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                // 计算证据指纹
                let mut fp_hasher = Sha256::new();
                for r in &source_refs {
                    fp_hasher.update(r.as_bytes());
                }
                let evidence_fp = format!("{:x}", fp_hasher.finalize());

                // 写入 memory_item_revisions
                sqlx::query(
                    "INSERT INTO memory_item_revisions (\
                        tenant_id, id, item_id, revision_number, category, status, \
                        title, summary, rationale, recommendation_rank, promotion_nomination, \
                        occurred_at, evidence_fingerprint, generated_by_snapshot_id, \
                        supersedes_revision_id, created_at\
                     ) VALUES (?1, ?2, ?3, 1, ?4, 'active', ?5, ?6, ?7, NULL, 'none', ?8, ?9, NULL, NULL, ?8)",
                )
                .bind(tenant_id)
                .bind(&new_rev_id)
                .bind(&new_item_id)
                .bind(category.as_str())
                .bind(&title)
                .bind(&statement)
                .bind(&rationale)
                .bind(&now)
                .bind(&evidence_fp)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                // 关联来源引用
                for ref_key in source_refs {
                    let ref_id = format!("ref-l2-{}", uuid::Uuid::new_v4());
                    sqlx::query(
                        "INSERT INTO memory_item_source_references (\
                            tenant_id, id, item_revision_id, record_kind, source_id, \
                            session_id, question_id, turn_id, part_id, node_id, \
                            node_order, reference_key, source_revision, availability, \
                            unavailable_reason, unavailable_at, created_at\
                         ) VALUES (?1, ?2, ?3, 'session', 'system', 'consolidation', NULL, NULL, NULL, NULL, NULL, ?4, 0, 'available', NULL, NULL, ?5)",
                    )
                    .bind(tenant_id)
                    .bind(&ref_id)
                    .bind(&new_rev_id)
                    .bind(&ref_key)
                    .bind(&now)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::external)?;
                }
            }
            ProjectConsolidationOperation::Revise {
                item_id,
                statement,
                rationale,
                source_refs,
            } => {
                // 查询当前版本号
                let cur_row = sqlx::query(
                    "SELECT mir.revision_number, mir.category, mir.title \
                     FROM memory_items mi \
                     JOIN memory_item_revisions mir ON mi.tenant_id = mir.tenant_id AND mi.current_revision_id = mir.id \
                     WHERE mi.tenant_id = ?1 AND mi.id = ?2 AND mi.layer = 'l2' AND mi.lifecycle = 'current'",
                )
                .bind(tenant_id)
                .bind(&item_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(AppError::external)?;

                if let Some(r) = cur_row {
                    let cur_rev_num: i64 = r.get("revision_number");
                    let category: String = r.get("category");
                    let title: String = r.get("title");
                    let new_rev_num = cur_rev_num + 1;
                    let new_rev_id = format!("rev-l2-{}", uuid::Uuid::new_v4());

                    let mut fp_hasher = Sha256::new();
                    for r in &source_refs {
                        fp_hasher.update(r.as_bytes());
                    }
                    let evidence_fp = format!("{:x}", fp_hasher.finalize());

                    // 写入新 revision
                    sqlx::query(
                        "INSERT INTO memory_item_revisions (\
                            tenant_id, id, item_id, revision_number, category, status, \
                            title, summary, rationale, recommendation_rank, promotion_nomination, \
                            occurred_at, evidence_fingerprint, generated_by_snapshot_id, \
                            supersedes_revision_id, created_at\
                         ) VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?7, ?8, NULL, 'none', ?9, ?10, NULL, NULL, ?9)",
                    )
                    .bind(tenant_id)
                    .bind(&new_rev_id)
                    .bind(&item_id)
                    .bind(new_rev_num)
                    .bind(&category)
                    .bind(&title)
                    .bind(&statement)
                    .bind(&rationale)
                    .bind(&now)
                    .bind(&evidence_fp)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::external)?;

                    // 更新 memory_items 指向新 revision
                    sqlx::query(
                        "UPDATE memory_items \
                         SET current_revision_id = ?1, last_seen_at = ?2, updated_at = ?2 \
                         WHERE tenant_id = ?3 AND id = ?4",
                    )
                    .bind(&new_rev_id)
                    .bind(&now)
                    .bind(tenant_id)
                    .bind(&item_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::external)?;
                }
            }
            ProjectConsolidationOperation::Supersede {
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
                     SET lifecycle = 'superseded', updated_at = ?1 \
                     WHERE tenant_id = ?2 AND id = ?3 AND layer = 'l2' AND lifecycle = 'current'",
                )
                .bind(&now)
                .bind(tenant_id)
                .bind(&old_item_id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                // 创建替代新条目
                let new_item_id = format!("item-l2-{}", uuid::Uuid::new_v4());
                let new_rev_id = format!("rev-l2-{}", uuid::Uuid::new_v4());

                sqlx::query(
                    "INSERT INTO memory_items (\
                        tenant_id, id, layer, project_key, current_revision_id, \
                        lifecycle, first_seen_at, last_seen_at, created_at, updated_at\
                     ) VALUES (?1, ?2, 'l2', ?3, ?4, 'current', ?5, ?5, ?5, ?5)",
                )
                .bind(tenant_id)
                .bind(&new_item_id)
                .bind(project_key)
                .bind(&new_rev_id)
                .bind(&now)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                let mut fp_hasher = Sha256::new();
                for r in &source_refs {
                    fp_hasher.update(r.as_bytes());
                }
                let evidence_fp = format!("{:x}", fp_hasher.finalize());

                sqlx::query(
                    "INSERT INTO memory_item_revisions (\
                        tenant_id, id, item_id, revision_number, category, status, \
                        title, summary, rationale, recommendation_rank, promotion_nomination, \
                        occurred_at, evidence_fingerprint, generated_by_snapshot_id, \
                        supersedes_revision_id, created_at\
                     ) VALUES (?1, ?2, ?3, 1, ?4, 'active', ?5, ?6, ?7, NULL, 'none', ?8, ?9, NULL, NULL, ?8)",
                )
                .bind(tenant_id)
                .bind(&new_rev_id)
                .bind(&new_item_id)
                .bind(category.as_str())
                .bind(&replacement_title)
                .bind(&replacement_statement)
                .bind(&rationale)
                .bind(&now)
                .bind(&evidence_fp)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                // 记录 supersession 关系
                let sup_id = format!("sup-{}", uuid::Uuid::new_v4());
                sqlx::query(
                    "INSERT INTO memory_item_supersessions (\
                        tenant_id, id, superseded_item_id, superseding_item_id, reason, created_at\
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .bind(tenant_id)
                .bind(&sup_id)
                .bind(&old_item_id)
                .bind(&new_item_id)
                .bind(&rationale)
                .bind(&now)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;
            }
            ProjectConsolidationOperation::Keep { item_id } => {
                // 保持原样，仅刷新 last_seen_at
                sqlx::query(
                    "UPDATE memory_items SET last_seen_at = ?1, updated_at = ?1 \
                     WHERE tenant_id = ?2 AND id = ?3",
                )
                .bind(&now)
                .bind(tenant_id)
                .bind(&item_id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;
            }
        }
    }

    // 计算新的 revision_hash (所有 active L2 revisions 的确定性指纹)
    let rev_ids: Vec<String> = sqlx::query_scalar(
        "SELECT current_revision_id FROM memory_items \
         WHERE tenant_id = ?1 AND project_key = ?2 AND layer = 'l2' AND lifecycle = 'current' \
         ORDER BY id ASC",
    )
    .bind(tenant_id)
    .bind(project_key)
    .fetch_all(&mut *tx)
    .await
    .map_err(AppError::external)?;

    let mut rev_hasher = Sha256::new();
    rev_hasher.update(project_key.as_bytes());
    for r in rev_ids {
        rev_hasher.update(r.as_bytes());
    }
    let revision_hash = format!("{:x}", rev_hasher.finalize());

    // 更新 project_memory_state 指针
    sqlx::query(
        "INSERT INTO project_memory_state (\
            tenant_id, project_key, project_path, last_successful_consolidation_at, \
            last_input_fingerprint, revision_hash, created_at, updated_at\
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?4, ?4) \
         ON CONFLICT (tenant_id, project_key) DO UPDATE SET \
            project_path = excluded.project_path, \
            last_successful_consolidation_at = excluded.last_successful_consolidation_at, \
            last_input_fingerprint = excluded.last_input_fingerprint, \
            revision_hash = excluded.revision_hash, \
            updated_at = excluded.updated_at",
    )
    .bind(tenant_id)
    .bind(project_key)
    .bind(project_path)
    .bind(&now)
    .bind(&input_fingerprint)
    .bind(&revision_hash)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;

    // 事务提交
    tx.commit().await.map_err(AppError::external)?;

    load_l2_project_memory_view(pool, tenant_id, project_key).await
}

/// 读取指定项目当前有效的 L2 Memory Item 列表
pub(crate) async fn load_l2_items(
    pool: &SqlitePool,
    tenant_id: &str,
    project_key: &str,
) -> AppResult<Vec<L2MemoryItemView>> {
    let rows = sqlx::query(
        "SELECT mi.id as item_id, mi.lifecycle, mi.updated_at, \
                mir.id as revision_id, mir.revision_number, mir.category, mir.status, \
                mir.title, mir.summary, mir.rationale \
         FROM memory_items mi \
         JOIN memory_item_revisions mir ON mi.tenant_id = mir.tenant_id AND mi.current_revision_id = mir.id \
         WHERE mi.tenant_id = ?1 AND mi.project_key = ?2 AND mi.layer = 'l2' AND mi.lifecycle = 'current' \
         ORDER BY mi.created_at ASC",
    )
    .bind(tenant_id)
    .bind(project_key)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    let mut items = Vec::new();
    for row in rows {
        let item_id: String = row.get("item_id");
        let revision_id: String = row.get("revision_id");
        let revision_number: i64 = row.get("revision_number");
        let category_raw: String = row.get("category");
        let status_raw: String = row.get("status");
        let title: String = row.get("title");
        let summary: String = row.get("summary");
        let rationale: String = row.get("rationale");
        let lifecycle: String = row.get("lifecycle");
        let updated_at: String = row.get("updated_at");

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

        // 查询引用
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

        let mut source_references = Vec::new();
        let mut available_count = 0;
        let total_count = ref_rows.len();

        for r in ref_rows {
            let availability_str: String = r.get("availability");
            let is_avail = availability_str == "available";
            if is_avail {
                available_count += 1;
            }
            source_references.push(L2SourceReferenceView {
                source_id: r.get("source_id"),
                session_id: r.get("session_id"),
                reference_key: r.get("reference_key"),
                available: is_avail,
                unavailable_reason: r.get("unavailable_reason"),
            });
        }

        let source_availability = if total_count == 0 || available_count == total_count {
            SourceAvailability::Available
        } else if available_count == 0 {
            SourceAvailability::Unavailable
        } else {
            SourceAvailability::PartiallyUnavailable
        };

        items.push(L2MemoryItemView {
            item_id,
            revision_id,
            revision_number,
            category,
            status,
            title,
            summary,
            rationale,
            lifecycle,
            source_availability,
            source_references,
            updated_at,
        });
    }

    Ok(items)
}

/// 加载 L2 项目完整视图
pub(crate) async fn load_l2_project_memory_view(
    pool: &SqlitePool,
    tenant_id: &str,
    project_key: &str,
) -> AppResult<Option<L2ProjectMemoryView>> {
    let items = load_l2_items(pool, tenant_id, project_key).await?;

    let state_row = sqlx::query(
        "SELECT project_path, last_successful_consolidation_at, revision_hash \
         FROM project_memory_state \
         WHERE tenant_id = ?1 AND project_key = ?2",
    )
    .bind(tenant_id)
    .bind(project_key)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;

    if items.is_empty() && state_row.is_none() {
        return Ok(None);
    }

    let (project_path, last_successful_consolidation_at, revision_hash) = match state_row {
        Some(r) => (
            r.get("project_path"),
            r.get("last_successful_consolidation_at"),
            r.get::<Option<String>, _>("revision_hash")
                .unwrap_or_default(),
        ),
        None => (None, None, String::new()),
    };

    Ok(Some(L2ProjectMemoryView {
        project_key: project_key.to_string(),
        project_path,
        items,
        last_successful_consolidation_at,
        revision_hash,
    }))
}

async fn load_superseded_index(
    pool: &SqlitePool,
    tenant_id: &str,
    project_key: &str,
) -> AppResult<Vec<L2SupersededIndexItem>> {
    let rows = sqlx::query(
        "SELECT mis.superseded_item_id, mis.superseding_item_id, mis.reason \
         FROM memory_item_supersessions mis \
         JOIN memory_items mi ON mis.tenant_id = mi.tenant_id AND mis.superseded_item_id = mi.id \
         WHERE mis.tenant_id = ?1 AND mi.project_key = ?2",
    )
    .bind(tenant_id)
    .bind(project_key)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    Ok(rows
        .into_iter()
        .map(|r| L2SupersededIndexItem {
            old_item_id: r.get("superseded_item_id"),
            superseding_item_id: r.get("superseding_item_id"),
            reason: r.get("reason"),
        })
        .collect())
}

async fn load_source_availability_summary(
    pool: &SqlitePool,
    tenant_id: &str,
    project_key: &str,
) -> AppResult<Vec<L2SourceAvailabilityItem>> {
    let rows = sqlx::query(
        "SELECT misr.reference_key, misr.session_id, misr.availability, misr.unavailable_reason \
         FROM memory_item_source_references misr \
         JOIN memory_item_revisions mir ON misr.tenant_id = mir.tenant_id AND misr.item_revision_id = mir.id \
         JOIN memory_items mi ON mir.tenant_id = mi.tenant_id AND mir.item_id = mi.id \
         WHERE misr.tenant_id = ?1 AND mi.project_key = ?2 AND mi.layer = 'l2' AND misr.availability = 'unavailable'",
    )
    .bind(tenant_id)
    .bind(project_key)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let avail_str: String = r.get("availability");
            L2SourceAvailabilityItem {
                reference_key: r.get("reference_key"),
                session_id: r.get("session_id"),
                available: avail_str == "available",
                reason: r.get("unavailable_reason"),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::application::AppService;
    use std::fs;

    async fn setup_test_service() -> (AppService, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-project-consolidation-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create test root");
        let db_path = root.join("app.db");

        let service = AppService::open_with_db_path(db_path)
            .await
            .expect("open service");

        (service, root)
    }

    async fn seed_snapshot(
        pool: &SqlitePool,
        snapshot_id: &str,
        sequence: i64,
        watermark_utc: &str,
        pub_kind: &str,
    ) {
        sqlx::query(
            "INSERT INTO recent_memory_snapshots (\
                tenant_id, id, sequence, target_watermark_utc, local_watermark_date, \
                local_watermark_time, timezone_offset_minutes, window_hours, window_start_utc, \
                window_end_utc, publication_kind, reused_from_snapshot_id, target_fingerprint, \
                content_fingerprint, generation_skill_asset_id, generation_skill_revision, \
                generation_skill_content_hash, contract_version, budget_policy_version, \
                projection_policy_version, content_generated_at, published_at\
             ) VALUES (\
                'default', ?1, ?2, ?3, '2026-09-15', '14:00', 0, 48, '2026-09-13T14:00:00Z', \
                ?3, ?4, NULL, 'tfp', 'cfp', NULL, NULL, NULL, 'v2', 'budget.v1', 'proj.v1', ?3, ?3\
             )",
        )
        .bind(snapshot_id)
        .bind(sequence)
        .bind(watermark_utc)
        .bind(pub_kind)
        .execute(pool)
        .await
        .expect("seed snapshot");
    }

    async fn seed_l1_item(
        pool: &SqlitePool,
        item_id: &str,
        rev_id: &str,
        project_key: &str,
        category: &str,
        status: &str,
        nomination: &str,
        title: &str,
        evidence_fp: &str,
    ) {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO memory_items (\
                tenant_id, id, layer, project_key, current_revision_id, lifecycle, \
                first_seen_at, last_seen_at, created_at, updated_at\
             ) VALUES ('default', ?1, 'l1', ?2, ?3, 'current', ?4, ?4, ?4, ?4)",
        )
        .bind(item_id)
        .bind(project_key)
        .bind(rev_id)
        .bind(&now)
        .execute(pool)
        .await
        .expect("seed l1 item");

        sqlx::query(
            "INSERT INTO memory_item_revisions (\
                tenant_id, id, item_id, revision_number, category, status, title, \
                summary, rationale, recommendation_rank, promotion_nomination, \
                occurred_at, evidence_fingerprint, generated_by_snapshot_id, \
                supersedes_revision_id, created_at\
             ) VALUES ('default', ?1, ?2, 1, ?3, ?4, ?5, 'summary', 'rationale', NULL, ?6, ?7, ?8, NULL, NULL, ?7)",
        )
        .bind(rev_id)
        .bind(item_id)
        .bind(category)
        .bind(status)
        .bind(title)
        .bind(nomination)
        .bind(&now)
        .bind(evidence_fp)
        .execute(pool)
        .await
        .expect("seed l1 revision");
    }

    async fn seed_source_reference(
        pool: &SqlitePool,
        rev_id: &str,
        ref_key: &str,
        is_user_content: bool,
        is_available: bool,
    ) {
        let ref_id = format!("ref-{}", uuid::Uuid::new_v4());
        let now = Utc::now().to_rfc3339();
        let q_id = if is_user_content { Some("q-1".to_string()) } else { None };
        let avail = if is_available { "available" } else { "unavailable" };

        sqlx::query(
            "INSERT INTO memory_item_source_references (\
                tenant_id, id, item_revision_id, record_kind, source_id, session_id, \
                question_id, turn_id, part_id, node_id, node_order, reference_key, \
                source_revision, availability, unavailable_reason, unavailable_at, created_at\
             ) VALUES ('default', ?1, ?2, 'session', 'src-1', 'sess-1', ?3, NULL, NULL, NULL, NULL, ?4, 1, ?5, NULL, NULL, ?6)",
        )
        .bind(&ref_id)
        .bind(rev_id)
        .bind(q_id)
        .bind(ref_key)
        .bind(avail)
        .bind(&now)
        .execute(pool)
        .await
        .expect("seed source reference");
    }

    async fn seed_observation(
        pool: &SqlitePool,
        item_id: &str,
        rev_id: &str,
        snapshot_id: &str,
        nomination: &str,
        evidence_fp: &str,
        project_key: &str,
        observed_at: &str,
    ) {
        let obs_id = format!("obs-{}", uuid::Uuid::new_v4());
        sqlx::query(
            "INSERT INTO memory_promotion_observations (\
                tenant_id, id, item_id, item_revision_id, snapshot_id, nomination, \
                evidence_fingerprint, project_key, observed_at\
             ) VALUES ('default', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(&obs_id)
        .bind(item_id)
        .bind(rev_id)
        .bind(snapshot_id)
        .bind(nomination)
        .bind(evidence_fp)
        .bind(project_key)
        .bind(observed_at)
        .execute(pool)
        .await
        .expect("seed observation");
    }

    #[tokio::test]
    async fn test_m35_l2_03_decision_promotes_with_one_observation() {
        let (service, _root) = setup_test_service().await;
        let pool = service.db.pool();

        seed_snapshot(pool, "snap-1", 1, "2026-09-15T02:00:00Z", "generated").await;
        seed_l1_item(
            pool,
            "item-dec-1",
            "rev-dec-1",
            "proj-alpha",
            "decision",
            "active",
            "project_decision",
            "Decide SQLite Engine",
            "fp-1",
        )
        .await;
        seed_source_reference(pool, "rev-dec-1", "ref-user-1", true, true).await;
        seed_observation(
            pool,
            "item-dec-1",
            "rev-dec-1",
            "snap-1",
            "project_decision",
            "fp-1",
            "proj-alpha",
            "2026-09-15T02:00:00Z",
        )
        .await;

        let candidates = evaluate_l2_candidates(pool, "default", "proj-alpha")
            .await
            .expect("evaluate candidates");

        assert_eq!(candidates.len(), 1, "M35-L2-03: project_decision with 1 generated observation must be a candidate");
        assert_eq!(candidates[0].nomination, MemoryPromotionNomination::ProjectDecision);
        assert_eq!(candidates[0].observation_count, 1);
    }

    #[tokio::test]
    async fn test_m35_l2_03_decision_without_user_content_rejected() {
        let (service, _root) = setup_test_service().await;
        let pool = service.db.pool();

        seed_snapshot(pool, "snap-1", 1, "2026-09-15T02:00:00Z", "generated").await;
        seed_l1_item(
            pool,
            "item-dec-2",
            "rev-dec-2",
            "proj-alpha",
            "decision",
            "active",
            "project_decision",
            "Decide Without User Content",
            "fp-1",
        )
        .await;
        // Reference is available but NOT user content
        seed_source_reference(pool, "rev-dec-2", "ref-agent-1", false, true).await;
        seed_observation(
            pool,
            "item-dec-2",
            "rev-dec-2",
            "snap-1",
            "project_decision",
            "fp-1",
            "proj-alpha",
            "2026-09-15T02:00:00Z",
        )
        .await;

        let candidates = evaluate_l2_candidates(pool, "default", "proj-alpha")
            .await
            .expect("evaluate candidates");

        assert_eq!(candidates.len(), 0, "Decision without available user content reference must be rejected");
    }

    #[tokio::test]
    async fn test_m35_l2_04_blocker_requires_two_generated_snapshots_with_new_evidence() {
        let (service, _root) = setup_test_service().await;
        let pool = service.db.pool();

        seed_snapshot(pool, "snap-1", 1, "2026-09-15T02:00:00Z", "generated").await;
        seed_l1_item(
            pool,
            "item-blk-1",
            "rev-blk-1",
            "proj-alpha",
            "blocker",
            "blocked",
            "recurring_blocker",
            "Docker Network Flake",
            "fp-blk-1",
        )
        .await;
        seed_source_reference(pool, "rev-blk-1", "ref-user-1", true, true).await;
        seed_observation(
            pool,
            "item-blk-1",
            "rev-blk-1",
            "snap-1",
            "recurring_blocker",
            "fp-blk-1",
            "proj-alpha",
            "2026-09-15T02:00:00Z",
        )
        .await;

        // 仅 1 次观察：不满足
        let candidates1 = evaluate_l2_candidates(pool, "default", "proj-alpha")
            .await
            .expect("evaluate candidates");
        assert_eq!(candidates1.len(), 0, "Single observation of blocker must not qualify");

        // 中间插入 reused snapshot: reused snapshot 不增加观察
        seed_snapshot(pool, "snap-2", 2, "2026-09-15T08:00:00Z", "reused").await;
        let candidates2 = evaluate_l2_candidates(pool, "default", "proj-alpha")
            .await
            .expect("evaluate candidates");
        assert_eq!(candidates2.len(), 0, "Reused snapshot does not qualify blocker");

        // 第 2 次 generated snapshot，但指纹没有变化: 依然不满足
        seed_snapshot(pool, "snap-3", 3, "2026-09-15T14:00:00Z", "generated").await;
        seed_observation(
            pool,
            "item-blk-1",
            "rev-blk-1",
            "snap-3",
            "recurring_blocker",
            "fp-blk-1", // 相同指纹！
            "proj-alpha",
            "2026-09-15T14:00:00Z",
        )
        .await;
        let candidates3 = evaluate_l2_candidates(pool, "default", "proj-alpha")
            .await
            .expect("evaluate candidates");
        assert_eq!(candidates3.len(), 0, "Identical fingerprint without new evidence must not qualify");

        // 第 3 次 generated snapshot，指纹发生变化且有新证据: 满足晋升！
        seed_snapshot(pool, "snap-4", 4, "2026-09-16T02:00:00Z", "generated").await;
        seed_observation(
            pool,
            "item-blk-1",
            "rev-blk-1",
            "snap-4",
            "recurring_blocker",
            "fp-blk-2", // 新指纹！
            "proj-alpha",
            "2026-09-16T02:00:00Z",
        )
        .await;

        let candidates4 = evaluate_l2_candidates(pool, "default", "proj-alpha")
            .await
            .expect("evaluate candidates");
        assert_eq!(candidates4.len(), 1, "Blocker with 2 distinct generated observations and changed fingerprint must qualify");
        assert_eq!(candidates4[0].nomination, MemoryPromotionNomination::RecurringBlocker);
        assert_eq!(candidates4[0].observation_count, 3);
    }

    #[tokio::test]
    async fn test_m35_l2_05_progress_and_completion_do_not_promote() {
        let (service, _root) = setup_test_service().await;
        let pool = service.db.pool();

        seed_snapshot(pool, "snap-1", 1, "2026-09-15T02:00:00Z", "generated").await;
        seed_snapshot(pool, "snap-2", 2, "2026-09-15T14:00:00Z", "generated").await;

        // 1. Progress 项 (nomination = none): 绝不作为合格候选
        seed_l1_item(
            pool,
            "item-prog-1",
            "rev-prog-1",
            "proj-alpha",
            "progress",
            "active",
            "none",
            "Refactored CSS styles",
            "fp-prog-1",
        )
        .await;
        seed_source_reference(pool, "rev-prog-1", "ref-1", true, true).await;

        // 2. Completed 项: 即使 nomination 为 recurring_blocker 且有 2 次历史观察，一旦 completed 也不再晋升
        seed_l1_item(
            pool,
            "item-comp-1",
            "rev-comp-1",
            "proj-alpha",
            "blocker",
            "completed",
            "recurring_blocker",
            "Resolved Flaky Test",
            "fp-comp-2",
        )
        .await;
        seed_source_reference(pool, "rev-comp-1", "ref-2", true, true).await;
        seed_observation(pool, "item-comp-1", "rev-comp-1", "snap-1", "recurring_blocker", "fp-comp-1", "proj-alpha", "2026-09-15T02:00:00Z").await;
        seed_observation(pool, "item-comp-1", "rev-comp-1", "snap-2", "recurring_blocker", "fp-comp-2", "proj-alpha", "2026-09-15T14:00:00Z").await;

        let candidates = evaluate_l2_candidates(pool, "default", "proj-alpha")
            .await
            .expect("evaluate candidates");
        assert_eq!(candidates.len(), 0, "Progress items and completed blockers must NEVER promote to L2 (M35-L2-05)");
    }

    #[tokio::test]
    async fn test_m35_l2_02_unassigned_does_not_promote() {
        let (service, _root) = setup_test_service().await;
        let pool = service.db.pool();

        seed_snapshot(pool, "snap-1", 1, "2026-09-15T02:00:00Z", "generated").await;
        seed_l1_item(
            pool,
            "item-unassigned",
            "rev-unassigned",
            "unassigned",
            "decision",
            "active",
            "project_decision",
            "Unassigned Decision",
            "fp-u",
        )
        .await;
        seed_source_reference(pool, "rev-unassigned", "ref-u", true, true).await;
        seed_observation(pool, "item-unassigned", "rev-unassigned", "snap-1", "project_decision", "fp-u", "unassigned", "2026-09-15T02:00:00Z").await;

        let candidates = evaluate_l2_candidates(pool, "default", "unassigned")
            .await
            .expect("evaluate candidates");
        assert_eq!(candidates.len(), 0, "unassigned items must NEVER promote to L2 (M35-L2-02)");
    }

    #[tokio::test]
    async fn test_m35_l2_06_zero_candidates_triggers_zero_agent_calls() {
        let (service, _root) = setup_test_service().await;
        let pool = service.db.pool();
        let lock_map = ProjectConsolidationLockMap::new();

        // 当没有候选时，传入的 Agent runner 若被调用则直接 panic
        let result = reconcile_project_consolidation(
            pool,
            "default",
            "proj-empty",
            Some("/path/empty"),
            &lock_map,
            Some(|_input: ProjectConsolidationInput| async move {
                panic!("Agent runner MUST NOT be called when there are 0 candidates! (M35-L2-06)");
                #[allow(unreachable_code)]
                Ok::<_, AppError>(ProjectConsolidationResult { operations: vec![] })
            }),
        )
        .await
        .expect("consolidation with zero candidates");

        assert!(result.is_none(), "Zero candidates should result in None view");
    }

    #[tokio::test]
    async fn test_m35_l2_06_serial_per_project_and_failure_preserves_current_l2() {
        let (service, _root) = setup_test_service().await;
        let pool = service.db.pool();
        let lock_map = ProjectConsolidationLockMap::new();

        // 1. 预先在数据库中插入一个已存在的 current L2 item
        let existing_l2_item_id = "item-l2-existing";
        let existing_l2_rev_id = "rev-l2-existing";
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO memory_items (\
                tenant_id, id, layer, project_key, current_revision_id, lifecycle, \
                first_seen_at, last_seen_at, created_at, updated_at\
             ) VALUES ('default', ?1, 'l2', 'proj-alpha', ?2, 'current', ?3, ?3, ?3, ?3)",
        )
        .bind(existing_l2_item_id)
        .bind(existing_l2_rev_id)
        .bind(&now)
        .execute(pool)
        .await
        .expect("insert existing l2 item");

        sqlx::query(
            "INSERT INTO memory_item_revisions (\
                tenant_id, id, item_id, revision_number, category, status, title, \
                summary, rationale, recommendation_rank, promotion_nomination, \
                occurred_at, evidence_fingerprint, generated_by_snapshot_id, \
                supersedes_revision_id, created_at\
             ) VALUES ('default', ?1, ?2, 1, 'decision', 'active', 'Pre-existing Decision', \
                       'Summary existing', 'Rationale', NULL, 'none', ?3, 'fp-ex', NULL, NULL, ?3)",
        )
        .bind(existing_l2_rev_id)
        .bind(existing_l2_item_id)
        .bind(&now)
        .execute(pool)
        .await
        .expect("insert existing l2 revision");

        // 2. 插入一个新合格候选
        seed_snapshot(pool, "snap-1", 1, "2026-09-15T02:00:00Z", "generated").await;
        seed_l1_item(
            pool,
            "item-dec-new",
            "rev-dec-new",
            "proj-alpha",
            "decision",
            "active",
            "project_decision",
            "New Alpha Decision",
            "fp-new",
        )
        .await;
        seed_source_reference(pool, "rev-dec-new", "ref-user-new", true, true).await;
        seed_observation(
            pool,
            "item-dec-new",
            "rev-dec-new",
            "snap-1",
            "project_decision",
            "fp-new",
            "proj-alpha",
            "2026-09-15T02:00:00Z",
        )
        .await;

        // 3. 运行 consolidation，模拟 Agent 或准入校验失败
        let consolidation_res = reconcile_project_consolidation(
            pool,
            "default",
            "proj-alpha",
            Some("/path/alpha"),
            &lock_map,
            Some(|_input: ProjectConsolidationInput| async move {
                Err::<ProjectConsolidationResult, _>(AppError::external("Simulated Agent Failure"))
            }),
        )
        .await;

        assert!(consolidation_res.is_err(), "Consolidation should fail when Agent fails");

        // 4. 验证原有的 current L2 条目保持完好无损 (M35-L2-06: 失败保留 current L2)
        let loaded = load_l2_items(pool, "default", "proj-alpha")
            .await
            .expect("load l2 items after failure");

        assert_eq!(loaded.len(), 1, "Original L2 item must be preserved on failure");
        assert_eq!(loaded[0].item_id, existing_l2_item_id);
        assert_eq!(loaded[0].title, "Pre-existing Decision");
    }

    #[tokio::test]
    async fn test_m35_l2_successful_consolidation_and_fingerprint_reuse() {
        let (service, _root) = setup_test_service().await;
        let pool = service.db.pool();
        let lock_map = ProjectConsolidationLockMap::new();

        // 1. 插入一个合格候选
        seed_snapshot(pool, "snap-1", 1, "2026-09-15T02:00:00Z", "generated").await;
        seed_l1_item(
            pool,
            "item-dec-success",
            "rev-dec-success",
            "proj-beta",
            "decision",
            "active",
            "project_decision",
            "Beta Decision Adopted",
            "fp-success",
        )
        .await;
        seed_source_reference(pool, "rev-dec-success", "ref-user-success", true, true).await;
        seed_observation(
            pool,
            "item-dec-success",
            "rev-dec-success",
            "snap-1",
            "project_decision",
            "fp-success",
            "proj-beta",
            "2026-09-15T02:00:00Z",
        )
        .await;

        // 2. 首次运行 Consolidation: 成功提交
        let view1 = reconcile_project_consolidation(
            pool,
            "default",
            "proj-beta",
            Some("/path/beta"),
            &lock_map,
            Some(|input: ProjectConsolidationInput| async move {
                assert_eq!(input.candidates.len(), 1);
                Ok::<_, AppError>(ProjectConsolidationResult {
                    operations: vec![ProjectConsolidationOperation::Create {
                        category: MemoryItemCategory::Decision,
                        title: "Beta Decision Adopted".to_string(),
                        statement: "Adopted Beta Strategy".to_string(),
                        rationale: "Validated by benchmark".to_string(),
                        source_refs: vec!["ref-user-success".to_string()],
                    }],
                })
            }),
        )
        .await
        .expect("consolidation succeeds")
        .expect("view exists");

        assert_eq!(view1.items.len(), 1);
        assert_eq!(view1.items[0].title, "Beta Decision Adopted");
        assert!(!view1.revision_hash.is_empty(), "Revision hash must be generated");

        // 3. 再次运行 Consolidation: 由于输入指纹未变，Agent runner 被跳过 (0 Agent calls)
        let view2 = reconcile_project_consolidation(
            pool,
            "default",
            "proj-beta",
            Some("/path/beta"),
            &lock_map,
            Some(|_input: ProjectConsolidationInput| async move {
                panic!("Agent should not be called when input fingerprint is identical!");
                #[allow(unreachable_code)]
                Ok::<_, AppError>(ProjectConsolidationResult { operations: vec![] })
            }),
        )
        .await
        .expect("consolidation with cached fingerprint")
        .expect("view exists");

        assert_eq!(view2.items.len(), 1);
        assert_eq!(view2.revision_hash, view1.revision_hash);
    }

    #[tokio::test]
    async fn test_context_resolver_reads_current_l2_project_memory() {
        let (service, _root) = setup_test_service().await;
        let pool = service.db.pool();

        let now = Utc::now().to_rfc3339();
        let item_id = "item-l2-ctx";
        let rev_id = "rev-l2-ctx";

        sqlx::query(
            "INSERT INTO memory_items (\
                tenant_id, id, layer, project_key, current_revision_id, lifecycle, \
                first_seen_at, last_seen_at, created_at, updated_at\
             ) VALUES ('default', ?1, 'l2', '/workspace/app', ?2, 'current', ?3, ?3, ?3, ?3)",
        )
        .bind(item_id)
        .bind(rev_id)
        .bind(&now)
        .execute(pool)
        .await
        .expect("insert l2 item");

        sqlx::query(
            "INSERT INTO memory_item_revisions (\
                tenant_id, id, item_id, revision_number, category, status, title, \
                summary, rationale, recommendation_rank, promotion_nomination, \
                occurred_at, evidence_fingerprint, generated_by_snapshot_id, \
                supersedes_revision_id, created_at\
             ) VALUES ('default', ?1, ?2, 1, 'decision', 'active', 'Use SQLite WAL Mode', \
                       'Enable WAL mode for performance', 'Benchmark proof', NULL, 'none', ?3, 'fp-wal', NULL, NULL, ?3)",
        )
        .bind(rev_id)
        .bind(item_id)
        .bind(&now)
        .execute(pool)
        .await
        .expect("insert l2 revision");

        let res = service
            .resolve_memory_context(crate::backend::application::MemoryContextResolveParams {
                project_path: Some("/workspace/app".to_string()),
                query: None,
                token_budget: Some(2000),
            })
            .await
            .expect("resolve memory context");

        assert!(res.text.contains("Use SQLite WAL Mode"), "Context text must contain L2 title");
        assert!(res.text.contains("Enable WAL mode for performance"), "Context text must contain L2 summary");
        assert!(
            res.references.iter().any(|r| r.kind == "project_memory_l2" && r.id == rev_id),
            "Context references must include project_memory_l2"
        );
    }
}
