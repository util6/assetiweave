pub(crate) mod backends;
pub(crate) mod composition;
mod error;
pub(crate) mod executor;
mod types;

pub(crate) use error::{AiExecutionError, AiExecutionErrorView};
pub(crate) use executor::AgentExecutionRuntime;
pub(crate) use types::{
    AiExecutionCancellation, AiExecutionLimits, AiExecutionPhase, AiExecutionProgressSink,
    AiExecutionPurpose, AiExecutionRequest, AiExecutionResult,
};

use crate::backend::agents::types::{
    AgentConnectionCheckMode, AgentConnectionResult, AgentId, AgentModelsResult,
};
#[cfg(test)]
use std::path::Path;
use std::sync::Arc;

const MAX_PROMPT_BYTES: usize = 1_000_000;

#[cfg(test)]
pub(crate) fn agent_runtime_manager(
    db_path: &Path,
) -> Result<Arc<crate::backend::agent_market::AgentRuntimeManager>, AiExecutionError> {
    let db = crate::backend::store::Database::open_initialized(db_path).map_err(|_| {
        AiExecutionError::Protocol {
            operation: "runtime_database_initialize",
        }
    })?;
    let pool = db.pool().clone();
    let context = db
        .block_on(
            async move { crate::backend::store::load_local_request_context_sqlx(&pool).await },
        )
        .map_err(|_| AiExecutionError::Protocol {
            operation: "runtime_context_initialize",
        })?;
    let workspace_root = db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("agent-executions");
    let manager = Arc::new(crate::backend::agent_market::AgentRuntimeManager::new(
        db.pool().clone(),
        workspace_root,
    ));
    let runtime_root = crate::backend::agent_market::default_runtime_root().map_err(|_| {
        AiExecutionError::Protocol {
            operation: "runtime_root_initialize",
        }
    })?;
    db.block_on(manager.recover_startup(&context.tenant.id, &runtime_root))
        .map_err(|_| AiExecutionError::Protocol {
            operation: "runtime_registry_reload",
        })?;
    Ok(manager)
}

/// Runs the async Agent runtime from synchronous application/Engine seams.
///
/// The dedicated thread is intentional: callers may already be inside a Tokio
/// runtime, where nesting `Runtime::block_on` would panic. Desktop background
/// tasks should call `AgentExecutionRuntime::execute` directly instead.
pub(crate) fn execute_agent_blocking(
    runtime: Arc<dyn AgentExecutionRuntime>,
    request: AiExecutionRequest,
) -> Result<AiExecutionResult, AiExecutionError> {
    std::thread::spawn(move || {
        let runtime_driver = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| AiExecutionError::Protocol {
                operation: "blocking_runtime_initialize",
            })?;
        runtime_driver.block_on(runtime.execute(request))
    })
    .join()
    .map_err(|_| AiExecutionError::Protocol {
        operation: "blocking_runtime_join",
    })?
}

pub(crate) fn check_agent_connection_blocking(
    runtime: Arc<dyn AgentExecutionRuntime>,
    agent_id: AgentId,
    mode: AgentConnectionCheckMode,
) -> AgentConnectionResult {
    if matches!(mode, AgentConnectionCheckMode::Installation) {
        return runtime.check_agent_installation(&agent_id);
    }

    let thread_runtime = runtime.clone();
    let fallback_agent_id = agent_id.clone();
    std::thread::spawn(move || {
        let runtime_driver = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime_driver) => runtime_driver,
            Err(_) => {
                return connection_probe_failure(
                    &agent_id,
                    "connection_runtime_initialize",
                    "The Agent connection probe runtime could not be initialized.",
                )
            }
        };
        runtime_driver.block_on(thread_runtime.check_agent_connection(&agent_id))
    })
    .join()
    .unwrap_or_else(|_| {
        connection_probe_failure(
            &fallback_agent_id,
            "connection_runtime_join",
            "The Agent connection probe did not complete.",
        )
    })
}

pub(crate) fn discover_agent_models_blocking(
    runtime: Arc<dyn AgentExecutionRuntime>,
    agent_id: AgentId,
) -> AgentModelsResult {
    let fallback_agent_id = agent_id.clone();
    std::thread::spawn(move || {
        let runtime_driver = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime_driver) => runtime_driver,
            Err(_) => {
                return unavailable_models_result(
                    &agent_id,
                    "model_discovery_runtime_initialize",
                    "The Agent model discovery runtime could not be initialized.",
                )
            }
        };
        runtime_driver.block_on(runtime.discover_agent_models(&agent_id))
    })
    .join()
    .unwrap_or_else(|_| {
        unavailable_models_result(
            &fallback_agent_id,
            "model_discovery_runtime_join",
            "The Agent model discovery did not complete.",
        )
    })
}

fn connection_probe_failure(
    agent_id: &AgentId,
    error_code: &str,
    error: &str,
) -> AgentConnectionResult {
    AgentConnectionResult {
        agent_id: agent_id.to_string(),
        available: false,
        installed: false,
        connected: false,
        version: None,
        connection_method: None,
        error_code: Some(error_code.to_string()),
        error: Some(error.to_string()),
        installation_status: None,
        runtime_status: None,
        protocol_status: None,
        execution_ready: false,
        health_stale: false,
    }
}

fn unavailable_models_result(
    agent_id: &AgentId,
    error_code: &str,
    error: &str,
) -> AgentModelsResult {
    AgentModelsResult {
        agent_id: agent_id.to_string(),
        available: false,
        models: Vec::new(),
        current_model_id: None,
        error_code: Some(error_code.to_string()),
        error: Some(error.to_string()),
    }
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
