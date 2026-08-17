use super::prelude::*;

pub(crate) struct AppService {
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
