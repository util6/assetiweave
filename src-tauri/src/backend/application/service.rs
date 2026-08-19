use super::prelude::*;

pub(crate) struct AppService {
    /// 请求绑定的进程级运行时。生产与测试都必须通过同一运行时边界创建。
    pub(super) runtime: std::sync::Arc<crate::backend::runtime::AppRuntime>,
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
