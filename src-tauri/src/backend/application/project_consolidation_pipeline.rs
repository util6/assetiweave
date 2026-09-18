use crate::backend::dto::recent_snapshot::SourceAvailability;
use crate::backend::models::{
    compute_project_consolidation_fingerprint, L2CandidateReferenceView, L2MemoryItemView,
    L2ProjectMemoryView, L2PromotionCandidate, L2SourceAvailabilityItem, L2SourceReferenceView,
    L2SupersededIndexItem, MemoryItemCategory, MemoryItemStatus, MemoryPromotionNomination,
    ProjectConsolidationInput, ProjectConsolidationOperation, ProjectConsolidationResult,
};
use crate::backend::runtime::{AppError, AppResult};
use crate::backend::store;
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

fn resolve_project_reference(
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

fn parse_promotion_nomination(value: &str) -> MemoryPromotionNomination {
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

fn promotion_nomination_for_source_refs(
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

/// Load the complete, structured L2 input that belongs to one maintenance
/// Work Order. This is deliberately separate from the commit path so the
/// coordinator can freeze exactly the evidence that the Agent will see.
pub(crate) async fn load_project_consolidation_input(
    pool: &SqlitePool,
    tenant_id: &str,
    project_key: &str,
    project_path: Option<&str>,
) -> AppResult<ProjectConsolidationInput> {
    Ok(ProjectConsolidationInput {
        project_key: project_key.to_string(),
        project_title: project_key.to_string(),
        project_path: project_path.map(str::to_string),
        current_l2_items: load_l2_items(pool, tenant_id, project_key).await?,
        candidates: evaluate_l2_candidates(pool, tenant_id, project_key).await?,
        source_availability_summary: load_source_availability_summary(pool, tenant_id, project_key)
            .await?,
        superseded_index: load_superseded_index(pool, tenant_id, project_key).await?,
    })
}

/// 判断项目是否真的有 L2 consolidation 输入变化。
///
/// Recent 水位本身不是 Project Agent 的触发条件；只有通过准入的候选或
/// 来源可用性变化才应该创建 durable maintenance job。
pub(crate) async fn should_schedule_project_consolidation(
    pool: &SqlitePool,
    tenant_id: &str,
    project_key: &str,
) -> AppResult<bool> {
    if project_key.trim().is_empty() || project_key == "unassigned" {
        return Ok(false);
    }
    let candidates = evaluate_l2_candidates(pool, tenant_id, project_key).await?;
    if !candidates.is_empty() {
        return Ok(true);
    }
    Ok(
        !load_source_availability_summary(pool, tenant_id, project_key)
            .await?
            .is_empty(),
    )
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
        None::<
            fn(
                ProjectConsolidationInput,
            ) -> std::future::Ready<AppResult<ProjectConsolidationResult>>,
        >,
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
    reconcile_project_consolidation_with_lease(
        pool,
        tenant_id,
        project_key,
        project_path,
        lock_map,
        agent_runner,
        None,
        None,
    )
    .await
}

pub(crate) async fn reconcile_project_consolidation_with_lease<F, Fut>(
    pool: &SqlitePool,
    tenant_id: &str,
    project_key: &str,
    project_path: Option<&str>,
    lock_map: &ProjectConsolidationLockMap,
    agent_runner: Option<F>,
    job_lease: Option<(&str, &str)>,
    frozen_input: Option<ProjectConsolidationInput>,
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

    // 2. Freeze the Agent input at enqueue time. A worker may only use a
    // newer read to detect staleness; it must never silently replace the
    // immutable Work Order evidence with that newer read.
    let frozen_input_fingerprint = frozen_input
        .as_ref()
        .map(compute_project_consolidation_fingerprint);
    let live_input =
        load_project_consolidation_input(pool, tenant_id, project_key, project_path).await?;
    let input = if let Some(frozen_input) = frozen_input {
        if frozen_input.project_key != project_key
            || compute_project_consolidation_fingerprint(&frozen_input)
                != compute_project_consolidation_fingerprint(&live_input)
        {
            return Err(AppError::Domain {
                code: "MEMORY_WORK_ORDER_STALE".to_string(),
                message: "project maintenance evidence changed after enqueue".to_string(),
                retryable: false,
                details: None,
            });
        }
        frozen_input
    } else {
        live_input
    };

    let candidates = input.candidates.clone();
    let availability_changes = input.source_availability_summary.clone();

    // 3. 触发门禁判断 (M35-L2-06):
    // 仅在出现合格候选、来源可用性变更或后续证据修订时运行；无候选时不空转
    if candidates.is_empty() && availability_changes.is_empty() {
        // 无候选、无变更时：Agent 调用严格为 0 次！直接返回现有视图
        let view = load_l2_project_memory_view(pool, tenant_id, project_key).await?;
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

    // 4. 计算不可变输入事实包指纹
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
            let view = load_l2_project_memory_view(pool, tenant_id, project_key).await?;
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

    // The Agent call can be long-running. Re-check immediately before opening
    // the commit transaction so a newer SQLite revision can never be published
    // from a stale Work Order.
    if let Some(expected_fingerprint) = frozen_input_fingerprint {
        let current_input =
            load_project_consolidation_input(pool, tenant_id, project_key, project_path).await?;
        if compute_project_consolidation_fingerprint(&current_input) != expected_fingerprint {
            return Err(AppError::Domain {
                code: "MEMORY_WORK_ORDER_STALE".to_string(),
                message: "project maintenance evidence changed before commit".to_string(),
                retryable: false,
                details: None,
            });
        }
    }

    // Agent 只能引用本次 Work Order 通过准入的、仍可用的候选引用。
    // 这张映射同时保留真实 source/session locator，避免把 Agent 返回的
    // reference_key 当成可写入的来源身份。
    let candidate_reference_map: HashMap<_, _> = candidates
        .iter()
        .flat_map(|candidate| candidate.session_references.iter())
        .filter(|reference| reference.available)
        .map(|reference| (reference.reference_key.clone(), reference.clone()))
        .collect();

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
                let promotion_nomination = promotion_nomination_for_source_refs(
                    &candidates,
                    &source_refs,
                    MemoryPromotionNomination::None,
                );
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
                     ) VALUES (?1, ?2, ?3, 1, ?4, 'active', ?5, ?6, ?7, NULL, ?8, ?9, ?10, NULL, NULL, ?9)",
                )
                .bind(tenant_id)
                .bind(&new_rev_id)
                .bind(&new_item_id)
                .bind(category.as_str())
                .bind(&title)
                .bind(&statement)
                .bind(&rationale)
                .bind(promotion_nomination.as_str())
                .bind(&now)
                .bind(&evidence_fp)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                // 关联来源引用
                for ref_key in source_refs {
                    let source_ref = resolve_project_reference(
                        &candidate_reference_map,
                        &ref_key,
                        strict_reference_validation,
                    )?;
                    let ref_id = format!("ref-l2-{}", uuid::Uuid::new_v4());
                    sqlx::query(
                        "INSERT INTO memory_item_source_references (\
                            tenant_id, id, item_revision_id, record_kind, source_id, \
                            session_id, question_id, turn_id, part_id, node_id, \
                            node_order, reference_key, source_revision, availability, \
                            unavailable_reason, unavailable_at, created_at\
                         ) VALUES (?1, ?2, ?3, 'session', ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11, 'available', NULL, NULL, ?12)",
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
                    "SELECT mir.revision_number, mir.category, mir.title, mir.promotion_nomination \
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
                    let previous_nomination = parse_promotion_nomination(
                        r.get::<String, _>("promotion_nomination").as_str(),
                    );
                    let promotion_nomination = promotion_nomination_for_source_refs(
                        &candidates,
                        &source_refs,
                        previous_nomination,
                    );
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
                         ) VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?7, ?8, NULL, ?9, ?10, ?11, NULL, NULL, ?10)",
                    )
                    .bind(tenant_id)
                    .bind(&new_rev_id)
                    .bind(&item_id)
                    .bind(new_rev_num)
                    .bind(&category)
                    .bind(&title)
                    .bind(&statement)
                    .bind(&rationale)
                    .bind(promotion_nomination.as_str())
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

                    for ref_key in source_refs {
                        let source_ref = resolve_project_reference(
                            &candidate_reference_map,
                            &ref_key,
                            strict_reference_validation,
                        )?;
                        let ref_id = format!("ref-l2-{}", uuid::Uuid::new_v4());
                        sqlx::query(
                            "INSERT INTO memory_item_source_references (\
                                tenant_id, id, item_revision_id, record_kind, source_id, \
                                session_id, question_id, turn_id, part_id, node_id, \
                                node_order, reference_key, source_revision, availability, \
                                unavailable_reason, unavailable_at, created_at\
                             ) VALUES (?1, ?2, ?3, 'session', ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11, 'available', NULL, NULL, ?12)",
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
                        .bind(&now)
                        .execute(&mut *tx)
                        .await
                        .map_err(AppError::external)?;
                    }
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
                let promotion_nomination = promotion_nomination_for_source_refs(
                    &candidates,
                    &source_refs,
                    MemoryPromotionNomination::None,
                );
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
                     ) VALUES (?1, ?2, ?3, 1, ?4, 'active', ?5, ?6, ?7, NULL, ?8, ?9, ?10, NULL, NULL, ?9)",
                )
                .bind(tenant_id)
                .bind(&new_rev_id)
                .bind(&new_item_id)
                .bind(category.as_str())
                .bind(&replacement_title)
                .bind(&replacement_statement)
                .bind(&rationale)
                .bind(promotion_nomination.as_str())
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
                for ref_key in source_refs {
                    let source_ref = resolve_project_reference(
                        &candidate_reference_map,
                        &ref_key,
                        strict_reference_validation,
                    )?;
                    let ref_id = format!("ref-l2-{}", uuid::Uuid::new_v4());
                    sqlx::query(
                        "INSERT INTO memory_item_source_references (\
                            tenant_id, id, item_revision_id, record_kind, source_id, \
                            session_id, question_id, turn_id, part_id, node_id, \
                            node_order, reference_key, source_revision, availability, \
                            unavailable_reason, unavailable_at, created_at\
                         ) VALUES (?1, ?2, ?3, 'session', ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11, 'available', NULL, NULL, ?12)",
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
                    .bind(&now)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::external)?;
                }
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

    if let Some((job_id, ownership_token)) = job_lease {
        let completed = store::complete_memory_v2_maintenance_job_tx(
            &mut tx,
            tenant_id,
            job_id,
            ownership_token,
            &now,
        )
        .await?;
        if !completed {
            return Err(AppError::Conflict(
                "memory v2 maintenance lease is no longer owned".to_string(),
            ));
        }
    }

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
#[path = "project_consolidation_pipeline_tests.rs"]
mod tests;
