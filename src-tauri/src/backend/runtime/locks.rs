use super::AppError;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex, MutexGuard},
};

/// 按部署计划资源键排序获取的互斥范围。
///
/// 该锁只用于同步的部署临界区；数据库事务、扫描去重和注册表重建不应
/// 通过它串行化。
pub(crate) struct PlanScopeGuard {
    _guards: Vec<MutexGuard<'static, ()>>,
    _locks: Vec<Arc<Mutex<()>>>,
}

#[derive(Default)]
pub(crate) struct RuntimeLocks {
    plan_locks: Mutex<BTreeMap<String, Arc<Mutex<()>>>>,
    tenant: Mutex<()>,
}

impl RuntimeLocks {
    pub(crate) fn acquire_plan_scope(
        &self,
        keys: BTreeSet<String>,
    ) -> Result<PlanScopeGuard, AppError> {
        let mut lock_map = self
            .plan_locks
            .lock()
            .map_err(|_| AppError::Conflict("部署锁表不可用".to_string()))?;
        let lock_refs = keys
            .into_iter()
            .map(|key| lock_map.entry(key).or_default().clone())
            .collect::<Vec<_>>();
        drop(lock_map);

        let guards = lock_refs
            .iter()
            .map(|lock| {
                // The lock map owns each Arc for the lifetime of RuntimeLocks. Extending
                // the guard lifetime is therefore safe and keeps the guard non-async.
                let guard = lock
                    .lock()
                    .map_err(|_| AppError::Conflict("部署资源锁不可用".to_string()))?;
                Ok(unsafe {
                    std::mem::transmute::<MutexGuard<'_, ()>, MutexGuard<'static, ()>>(guard)
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        Ok(PlanScopeGuard {
            _locks: lock_refs,
            _guards: guards,
        })
    }

    pub(crate) fn with_tenant_scope<T>(&self, f: impl FnOnce() -> T) -> Result<T, AppError> {
        let _guard = self
            .tenant
            .lock()
            .map_err(|_| AppError::Conflict("租户状态锁不可用".to_string()))?;
        Ok(f())
    }
}
