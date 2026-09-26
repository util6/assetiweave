pub(crate) use crate::backend::application::memory::project_consolidation_candidates::evaluate_l2_candidates;
pub(crate) use crate::backend::application::memory::project_consolidation_persist::apply_project_consolidation_patch;
pub(crate) use crate::backend::application::memory::project_consolidation_view::{
    load_l2_items, load_l2_project_memory_view, load_source_availability_summary,
    load_superseded_index,
};

use crate::backend::application::{AppError, AppResult};
pub(crate) use crate::backend::domain::{
    compute_project_consolidation_fingerprint, L2ProjectMemoryView, ProjectConsolidationInput,
    ProjectConsolidationOperation, ProjectConsolidationResult,
};
use crate::backend::store;
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
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

    apply_project_consolidation_patch(
        pool,
        tenant_id,
        project_key,
        project_path,
        &candidates,
        consolidation_result,
        &input_fingerprint,
        job_lease,
        strict_reference_validation,
    )
    .await?;

    load_l2_project_memory_view(pool, tenant_id, project_key).await
}

#[cfg(test)]
#[path = "project_consolidation_pipeline_tests.rs"]
mod tests;
