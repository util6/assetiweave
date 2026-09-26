pub(crate) use crate::backend::application::memory::global_consolidation_candidates::{
    evaluate_l3_candidates, promotion_nomination_for_source_refs,
};
pub(crate) use crate::backend::application::memory::global_consolidation_persist::apply_global_consolidation_patch;
pub(crate) use crate::backend::application::memory::global_consolidation_view::{
    get_global_memory_l3_view, reconcile_source_invalidation,
};
pub(crate) use crate::backend::domain::{
    compute_global_consolidation_fingerprint, GlobalConsolidationInput,
    GlobalConsolidationOperation, GlobalConsolidationResult, L3CandidateReferenceView,
    L3GlobalMemoryView, L3MemoryItemView, L3PromotionCandidate, L3SourceReferenceView,
    L3SupersededIndexItem, MemoryItemCategory, MemoryItemStatus, MemoryPromotionNomination,
};
pub(crate) use sha2::Sha256;

use crate::backend::application::{AppError, AppResult};
use crate::backend::store;
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
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
            let completed = store::finish_memory_maintenance_job_sqlx(
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
                    "memory maintenance lease is no longer owned".to_string(),
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
                let completed = store::finish_memory_maintenance_job_sqlx(
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
                        "memory maintenance lease is no longer owned".to_string(),
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

    apply_global_consolidation_patch(
        pool,
        tenant_id,
        now,
        operations,
        &candidates,
        strict_reference_validation,
        &candidate_reference_map,
        &input_fingerprint,
        job_lease,
    )
    .await?;

    get_global_memory_l3_view(pool, tenant_id).await
}

#[cfg(test)]
#[path = "global_consolidation_pipeline_tests.rs"]
pub(crate) mod tests;
