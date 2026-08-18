use super::prelude::*;

pub(crate) struct AppService {
    /// 请求绑定的进程级运行时。旧的测试构造器在迁移期间允许为空；生产
    /// Tauri/Engine surface 一律通过 `from_runtime` 创建。
    pub(super) runtime: Option<std::sync::Arc<crate::backend::runtime::AppRuntime>>,
    pub(super) db: crate::backend::store::Database,
    pub(super) db_path: PathBuf,
    pub(super) context: RequestContext,
    pub(super) agent_runtime_manager:
        std::sync::Arc<crate::backend::agent_market::AgentRuntimeManager>,
    pub(super) agent_runtime:
        std::sync::Arc<dyn crate::backend::ai_execution::AgentExecutionRuntime>,
}

pub(crate) struct AgentAppService {
    pub(super) _service: AppService,
    pub(super) agent_runtime:
        std::sync::Arc<dyn crate::backend::ai_execution::AgentExecutionRuntime>,
}
