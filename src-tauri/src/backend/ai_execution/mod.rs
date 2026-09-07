pub(crate) mod backends;
pub(crate) mod bindings;
pub(crate) mod composition;
mod error;
pub(crate) mod executor;
pub(crate) mod session_events;
mod types;

pub(crate) use bindings::{PersistentBindingStore, PersistentExecutionBinding};
pub(crate) use error::{AiExecutionError, AiExecutionErrorView};
pub(crate) use executor::AgentExecutionRuntime;
#[allow(unused_imports)]
pub(crate) use session_events::{
    SessionEvent, SessionEventApplyResult, SessionEventDelivery, SessionEventIdentity,
    SessionEventKind, SessionEventProjection, SessionEventProjectionLimits, SessionEventSink,
    SessionItemIdentity, SessionItemKind, SessionItemSnapshot, SessionItemState,
    SessionProcessingState, SessionSnapshot, SessionTaskStatus, SessionToolState,
};
pub(crate) use types::{
    AgentSessionMode, AiExecutionCancellation, AiExecutionCleanupReport, AiExecutionLimits,
    AiExecutionPhase, AiExecutionProgressSink, AiExecutionPurpose, AiExecutionRequest,
    AiExecutionResult, AiExecutionSessionDeleteMethod, AiRecallTools, AiTeamTools,
};

use crate::backend::agents::types::{
    AgentConnectionCheckMode, AgentConnectionResult, AgentId, AgentModelsResult,
};
use std::path::Path;
use std::sync::Arc;

const MAX_PROMPT_BYTES: usize = 1_000_000;

pub(crate) fn agent_execution_workspace_root(db_path: &Path) -> String {
    db_path
        .parent()
        .map(|parent| parent.join("agent-executions"))
        // Conversation adapters persist their native cwd/project path. Keep the
        // exclusion prefix native too; converting it to an @config/@data token
        // would no longer match imported Agent execution sessions.
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default()
}

pub(crate) async fn execute_agent(
    runtime: Arc<dyn AgentExecutionRuntime>,
    request: AiExecutionRequest,
) -> Result<AiExecutionResult, AiExecutionError> {
    runtime.execute(request).await
}

pub(crate) async fn check_agent_connection(
    runtime: Arc<dyn AgentExecutionRuntime>,
    agent_id: AgentId,
    mode: AgentConnectionCheckMode,
) -> AgentConnectionResult {
    if matches!(mode, AgentConnectionCheckMode::Installation) {
        return runtime.check_agent_installation(&agent_id);
    }
    runtime.check_agent_connection(&agent_id).await
}

pub(crate) async fn discover_agent_models(
    runtime: Arc<dyn AgentExecutionRuntime>,
    agent_id: AgentId,
) -> AgentModelsResult {
    runtime.discover_agent_models(&agent_id).await
}

pub(crate) fn normalize_prompt(prompt: &str) -> Result<String, AiExecutionError> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err(AiExecutionError::InvalidPrompt(
            "AI prompt is empty".to_string(),
        ));
    }
    if prompt.len() > MAX_PROMPT_BYTES {
        return Err(AiExecutionError::InvalidPrompt(format!(
            "AI prompt exceeds the {MAX_PROMPT_BYTES}-byte limit"
        )));
    }
    Ok(prompt.to_string())
}

pub(crate) fn normalize_model(model: Option<&str>) -> Result<Option<String>, AiExecutionError> {
    let Some(model) = model.map(str::trim).filter(|model| !model.is_empty()) else {
        return Ok(None);
    };
    if model.len() > 120 || model.contains(['\n', '\r', '\0']) {
        return Err(AiExecutionError::InvalidModel(
            "AI model is invalid".to_string(),
        ));
    }
    Ok(Some(model.to_string()))
}
