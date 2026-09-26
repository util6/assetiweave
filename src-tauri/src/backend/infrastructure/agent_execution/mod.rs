pub(crate) mod backends;
mod error;
pub(crate) mod executor;
pub(crate) mod executor_support;
pub(crate) mod executor_traits;
pub(crate) mod managed_process;
mod managed_process_support;
pub(crate) mod protocol;
pub(crate) mod registry;
pub(crate) mod session_events;
pub(crate) mod types;

pub(crate) use error::{AiExecutionError, AiExecutionErrorView};
pub(crate) use executor::AgentExecutionRuntime;
pub(crate) use session_events::{
    SessionEvent, SessionEventApplyResult, SessionEventDelivery, SessionEventIdentity,
    SessionEventKind, SessionEventProjection, SessionEventProjectionLimits, SessionEventSink,
    SessionItemIdentity, SessionItemKind, SessionItemSnapshot, SessionItemState,
    SessionProcessingState, SessionSnapshot, SessionTaskStatus, SessionToolState,
};
pub(crate) use types::{
    normalize_model, normalize_prompt, AgentConnectionResult, AgentModelOption, AgentModelsResult,
    AgentSessionMode, AiExecutionCancellation, AiExecutionCleanupReport, AiExecutionLimits,
    AiExecutionPhase, AiExecutionProgressSink, AiExecutionPurpose, AiExecutionRequest,
    AiExecutionResult, AiExecutionSessionDeleteMethod, AiMemoryGenerationTools, AiRecallTools,
    SessionCleanupStatus, MAX_PROMPT_BYTES,
};

pub(crate) fn agent_execution_workspace_root(db_path: &std::path::Path) -> String {
    db_path
        .parent()
        .map(|parent| parent.join("agent-executions"))
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default()
}

pub(crate) async fn execute_agent(
    runtime: std::sync::Arc<dyn AgentExecutionRuntime>,
    request: AiExecutionRequest,
) -> Result<AiExecutionResult, AiExecutionError> {
    runtime.execute(request).await
}

pub(crate) async fn check_agent_connection(
    runtime: std::sync::Arc<dyn AgentExecutionRuntime>,
    agent_id: crate::backend::domain::agents::AgentId,
    mode: crate::backend::domain::agents::AgentConnectionCheckMode,
) -> AgentConnectionResult {
    if matches!(
        mode,
        crate::backend::domain::agents::AgentConnectionCheckMode::Installation
    ) {
        return runtime.check_agent_installation(&agent_id);
    }
    runtime.check_agent_connection(&agent_id).await
}

pub(crate) async fn discover_agent_models(
    runtime: std::sync::Arc<dyn AgentExecutionRuntime>,
    agent_id: crate::backend::domain::agents::AgentId,
) -> AgentModelsResult {
    runtime.discover_agent_models(&agent_id).await
}
