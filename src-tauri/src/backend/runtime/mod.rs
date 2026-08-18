//! 进程级运行时资源宿主。
//!
//! `AppRuntime` 将数据库连接池、请求上下文快照、Agent 注册表和并发控制
//! 绑定到进程生命周期。请求级 `AppService` 只从这里创建，不再重复执行
//! 数据库 migration、默认数据 seed 或 Agent 恢复。

mod app_runtime;
mod error;
mod locks;
pub(crate) mod tasks;

#[allow(unused_imports)]
pub(crate) use app_runtime::{
    current_process_runtime, install_process_runtime, AppRuntime, RequestContextSnapshot,
    RuntimeRole, ShutdownReport, ShutdownState,
};
#[allow(unused_imports)]
pub(crate) use error::{AppError, AppErrorView, AppResult};
#[allow(unused_imports)]
pub(crate) use locks::{PlanScopeGuard, RuntimeLocks};

#[cfg(test)]
mod tests;
