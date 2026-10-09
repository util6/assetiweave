pub use agent_engine::infrastructure::*;

pub fn resolve_agent_execution_workspace_root() -> std::path::PathBuf {
    if let Ok(override_dir) = std::env::var("ASSETIWEAVE_AGENT_RUNS_DIR") {
        let trimmed = override_dir.trim();
        if !trimmed.is_empty() {
            return std::path::PathBuf::from(trimmed);
        }
    }
    if let Ok(assetiweave_home) = std::env::var("ASSETIWEAVE_HOME") {
        let trimmed = assetiweave_home.trim();
        if !trimmed.is_empty() {
            return std::path::PathBuf::from(trimmed).join("agent-runs");
        }
    }
    dirs::home_dir()
        .map(|home| home.join(".assetiweave").join("agent-runs"))
        .unwrap_or_else(|| std::env::temp_dir().join(".assetiweave").join("agent-runs"))
}

pub fn agent_execution_workspace_root(_db_path: &std::path::Path) -> String {
    resolve_agent_execution_workspace_root()
        .to_string_lossy()
        .to_string()
}

pub async fn execute_agent(
    runtime: std::sync::Arc<dyn AgentExecutionRuntime>,
    request: AiExecutionRequest,
) -> Result<AiExecutionResult, AiExecutionError> {
    runtime.execute(request).await
}

pub async fn check_agent_connection(
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

pub async fn discover_agent_models(
    runtime: std::sync::Arc<dyn AgentExecutionRuntime>,
    agent_id: crate::backend::domain::agents::AgentId,
) -> AgentModelsResult {
    runtime.discover_agent_models(&agent_id).await
}

#[cfg(test)]
#[path = "agent_execution_tests.rs"]
mod tests;
