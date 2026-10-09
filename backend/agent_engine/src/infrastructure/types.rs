use std::{fmt, sync::Arc, time::Duration};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::{AgentId, AgentProtocol};

use super::{error::AiExecutionError, session_events::SessionEvent};

pub const MAX_PROMPT_BYTES: usize = 1_000_000;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AgentConnectionResult {
    pub agent_id: String,
    pub available: bool,
    pub installed: bool,
    pub connected: bool,
    pub version: Option<String>,
    pub connection_method: Option<String>,
    pub error_code: Option<String>,
    pub error: Option<String>,
    pub installation_status: Option<String>,
    pub runtime_status: Option<String>,
    pub protocol_status: Option<String>,
    pub execution_ready: bool,
    pub health_stale: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AgentModelOption {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AgentModelsResult {
    pub agent_id: String,
    pub available: bool,
    pub models: Vec<AgentModelOption>,
    pub current_model_id: Option<String>,
    pub error_code: Option<String>,
    pub error: Option<String>,
}

pub trait AiExecutionProgressSink: Send + Sync {
    fn set_phase(&self, phase: AiExecutionPhase);

    fn emit_session_event(&self, _event: SessionEvent) {}

    fn failure_phase(&self) -> Option<AiExecutionPhase> {
        None
    }

    fn set_cleanup_report(&self, _report: AiExecutionCleanupReport) {}
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AiExecutionCleanupReport {
    pub process_reaped: bool,
    pub workspace_removed: bool,
    pub failure_count: usize,
    pub session_closed: Option<bool>,
    pub session_deleted: Option<bool>,
    pub session_delete_method: Option<AiExecutionSessionDeleteMethod>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiExecutionSessionDeleteMethod {
    Acp,
    ProviderFallback,
}

#[derive(Clone)]
pub struct AiExecutionCancellation {
    token: tokio_util::sync::CancellationToken,
}

impl AiExecutionCancellation {
    pub fn from_token(token: tokio_util::sync::CancellationToken) -> Self {
        Self { token }
    }

    pub fn cancel(&self) {
        self.token.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    pub async fn cancelled(&self) {
        self.token.cancelled().await;
    }
}

impl Default for AiExecutionCancellation {
    fn default() -> Self {
        Self {
            token: tokio_util::sync::CancellationToken::new(),
        }
    }
}

impl fmt::Debug for AiExecutionCancellation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AiExecutionCancellation")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionMode {
    OneShot,
    Persistent,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiExecutionPurpose {
    Translation,
    SessionMemory,
    PromptOptimization,
    ConnectionTest,
    ModelDiscovery,
    MemoryGeneration,
    ProjectMemory,
    GlobalMemory,
    Recall,
}

impl AiExecutionPurpose {
    pub fn category_folder(&self) -> &'static str {
        match self {
            Self::Translation => "translations",
            Self::SessionMemory
            | Self::MemoryGeneration
            | Self::ProjectMemory
            | Self::GlobalMemory
            | Self::Recall => "memory",
            Self::PromptOptimization | Self::ConnectionTest | Self::ModelDiscovery => "general",
        }
    }
}

#[derive(Clone)]
pub struct AiRecallTools {
    pub tenant_id: String,
    pub recall_session_id: String,
    pub database_path: String,
}

#[derive(Clone)]
pub struct AiMemoryGenerationTools {
    pub tenant_id: String,
    pub job_id: String,
    pub ownership_token: String,
    pub database_path: String,
}

impl fmt::Debug for AiMemoryGenerationTools {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AiMemoryGenerationTools")
            .field("tenant_id", &self.tenant_id)
            .field("job_id", &self.job_id)
            .field("ownership_token", &"<redacted>")
            .field("database_path", &"<redacted>")
            .finish()
    }
}

impl fmt::Debug for AiRecallTools {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AiRecallTools")
            .field("tenant_id", &self.tenant_id)
            .field("recall_session_id", &self.recall_session_id)
            .field("database_path", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiExecutionLimits {
    pub total_timeout: Duration,
    pub spawn_timeout: Duration,
    pub initialize_timeout: Duration,
    pub config_rpc_timeout: Duration,
    pub cancel_grace: Duration,
    pub close_timeout: Duration,
    pub cleanup_timeout: Duration,
    pub text_bytes: usize,
    pub stderr_bytes: usize,
}

impl Default for AiExecutionLimits {
    fn default() -> Self {
        Self {
            total_timeout: Duration::from_secs(180),
            spawn_timeout: Duration::from_secs(30),
            initialize_timeout: Duration::from_secs(10),
            config_rpc_timeout: Duration::from_secs(5),
            cancel_grace: Duration::from_secs(2),
            close_timeout: Duration::from_secs(2),
            cleanup_timeout: Duration::from_secs(10),
            text_bytes: 1024 * 1024,
            stderr_bytes: 256 * 1024,
        }
    }
}

#[derive(Clone)]
pub struct AiExecutionRequest {
    pub execution_id: String,
    pub agent_id: AgentId,
    pub purpose: AiExecutionPurpose,
    pub session_mode: AgentSessionMode,
    pub prompt: String,
    pub model: Option<String>,
    pub limits: AiExecutionLimits,
    pub cancellation: AiExecutionCancellation,
    pub progress: Option<Arc<dyn AiExecutionProgressSink>>,
    pub tenant_id: Option<String>,
    pub execution_context_key: Option<String>,
    pub binding: Option<crate::domain::PersistentExecutionBinding>,
    pub replay: bool,
    pub restore_only: bool,
    pub recall_tools: Option<AiRecallTools>,
    pub memory_generation_tools: Option<AiMemoryGenerationTools>,
    pub account: Option<crate::domain::account::AgentAccount>,
}

impl AiExecutionRequest {
    pub fn validate(&self) -> Result<(), AiExecutionError> {
        if matches!(self.session_mode, AgentSessionMode::Persistent)
            && self
                .execution_context_key
                .as_deref()
                .is_none_or(|key| key.trim().is_empty())
        {
            return Err(AiExecutionError::InvalidContextKey);
        }
        if self.replay && !matches!(self.session_mode, AgentSessionMode::Persistent) {
            return Err(AiExecutionError::InvalidReplayMode);
        }
        if self.restore_only
            && (!matches!(self.session_mode, AgentSessionMode::Persistent) || self.replay)
        {
            return Err(AiExecutionError::InvalidReplayMode);
        }
        if matches!(self.purpose, AiExecutionPurpose::Recall) && self.recall_tools.is_none() {
            return Err(AiExecutionError::RecallToolsUnavailable);
        }
        if !matches!(self.purpose, AiExecutionPurpose::Recall) && self.recall_tools.is_some() {
            return Err(AiExecutionError::Protocol {
                operation: "recall_tools_scope",
            });
        }
        if !matches!(self.purpose, AiExecutionPurpose::MemoryGeneration)
            && self.memory_generation_tools.is_some()
        {
            return Err(AiExecutionError::Protocol {
                operation: "memory_generation_tools_scope",
            });
        }
        if !self.restore_only {
            normalize_prompt(&self.prompt)?;
        }
        normalize_model(self.model.as_deref())?;
        Ok(())
    }

    pub fn report_phase(&self, phase: AiExecutionPhase) {
        if let Some(progress) = self.progress.as_ref() {
            progress.set_phase(phase);
        }
    }

    pub fn report_cleanup(&self, report: AiExecutionCleanupReport) {
        if let Some(progress) = self.progress.as_ref() {
            progress.set_cleanup_report(report);
        }
    }

    pub fn report_session_event(&self, event: SessionEvent) {
        if let Some(progress) = self.progress.as_ref() {
            progress.emit_session_event(event);
        }
    }
}

impl fmt::Debug for AiExecutionRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AiExecutionRequest")
            .field("execution_id", &self.execution_id)
            .field("agent_id", &self.agent_id)
            .field("purpose", &self.purpose)
            .field("session_mode", &self.session_mode)
            .field("prompt", &"<redacted>")
            .field("model", &self.model.as_ref().map(|_| "<redacted>"))
            .field("limits", &self.limits)
            .field("cancellation", &self.cancellation)
            .field("tenant_id", &self.tenant_id)
            .field("execution_context_key", &self.execution_context_key)
            .field("binding", &self.binding)
            .field("replay", &self.replay)
            .field("restore_only", &self.restore_only)
            .field("recall_tools", &self.recall_tools)
            .field("memory_generation_tools", &self.memory_generation_tools)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionCleanupStatus {
    Deleted,
    Unsupported,
    Failed(String),
    Skipped,
}

#[derive(Clone, Eq, PartialEq)]
pub struct AiExecutionResult {
    pub text: String,
    pub agent_id: AgentId,
    pub protocol: AgentProtocol,
    pub requested_model: Option<String>,
    pub elapsed_ms: u64,
    pub persistent_binding: Option<crate::domain::PersistentExecutionBinding>,
    pub replay_text: Option<String>,
    pub session_cleanup: SessionCleanupStatus,
}

impl fmt::Debug for AiExecutionResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AiExecutionResult")
            .field("text", &"<redacted>")
            .field("agent_id", &self.agent_id)
            .field("protocol", &self.protocol)
            .field(
                "requested_model",
                &self.requested_model.as_ref().map(|_| "<redacted>"),
            )
            .field("elapsed_ms", &self.elapsed_ms)
            .field("persistent_binding", &self.persistent_binding)
            .field(
                "replay_text",
                &self.replay_text.as_ref().map(|_| "<redacted>"),
            )
            .field("session_cleanup", &self.session_cleanup)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiExecutionPhase {
    Queued,
    Resolving,
    Spawning,
    Initializing,
    CreatingSession,
    Configuring,
    Prompting,
    Cancelling,
    Closing,
    CleaningUp,
}

pub fn normalize_prompt(prompt: &str) -> Result<String, AiExecutionError> {
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

pub fn normalize_model(model: Option<&str>) -> Result<Option<String>, AiExecutionError> {
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

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
