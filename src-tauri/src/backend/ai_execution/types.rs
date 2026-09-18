use std::{fmt, sync::Arc, time::Duration};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::backend::agents::types::{AgentId, AgentProtocol};

use super::{
    bindings::PersistentExecutionBinding, normalize_model, normalize_prompt,
    session_events::SessionEvent, AiExecutionError,
};

pub(crate) trait AiExecutionProgressSink: Send + Sync {
    fn set_phase(&self, phase: AiExecutionPhase);

    fn emit_session_event(&self, _event: SessionEvent) {}

    fn failure_phase(&self) -> Option<AiExecutionPhase> {
        None
    }

    fn set_cleanup_report(&self, _report: AiExecutionCleanupReport) {}
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct AiExecutionCleanupReport {
    pub(crate) process_reaped: bool,
    pub(crate) workspace_removed: bool,
    pub(crate) failure_count: usize,
    pub(crate) session_closed: Option<bool>,
    pub(crate) session_deleted: Option<bool>,
    pub(crate) session_delete_method: Option<AiExecutionSessionDeleteMethod>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AiExecutionSessionDeleteMethod {
    Acp,
    ProviderFallback,
}

#[derive(Clone)]
pub(crate) struct AiExecutionCancellation {
    token: tokio_util::sync::CancellationToken,
}

impl AiExecutionCancellation {
    pub(crate) fn from_token(token: tokio_util::sync::CancellationToken) -> Self {
        Self { token }
    }

    pub(crate) fn cancel(&self) {
        self.token.cancel();
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    pub(crate) async fn cancelled(&self) {
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
pub(crate) enum AgentSessionMode {
    OneShot,
    Persistent,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AiExecutionPurpose {
    Translation,
    SessionMemory,
    PromptOptimization,
    ConnectionTest,
    ModelDiscovery,
    MemoryGeneration,
    ProjectMemory,
    GlobalMemory,
    Recall,
    TeamLeaderChat,
    TeamMemberTurn,
    TeamDraft,
    TeamTask,
    TeamSummary,
}

#[derive(Clone)]
pub(crate) struct AiTeamTools {
    pub(crate) tenant_id: String,
    pub(crate) team_id: String,
    pub(crate) run_id: String,
    pub(crate) member_id: String,
    pub(crate) credential: String,
    pub(crate) database_path: String,
}

#[derive(Clone)]
pub(crate) struct AiRecallTools {
    pub(crate) tenant_id: String,
    pub(crate) recall_session_id: String,
    pub(crate) database_path: String,
}

#[derive(Clone)]
pub(crate) struct AiMemoryGenerationTools {
    pub(crate) tenant_id: String,
    pub(crate) job_id: String,
    pub(crate) ownership_token: String,
    pub(crate) database_path: String,
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

impl fmt::Debug for AiTeamTools {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AiTeamTools")
            .field("tenant_id", &self.tenant_id)
            .field("team_id", &self.team_id)
            .field("run_id", &self.run_id)
            .field("member_id", &self.member_id)
            .field("credential", &"<redacted>")
            .field("database_path", &self.database_path)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AiExecutionLimits {
    pub(crate) total_timeout: Duration,
    pub(crate) spawn_timeout: Duration,
    pub(crate) initialize_timeout: Duration,
    pub(crate) config_rpc_timeout: Duration,
    pub(crate) cancel_grace: Duration,
    pub(crate) close_timeout: Duration,
    pub(crate) cleanup_timeout: Duration,
    pub(crate) text_bytes: usize,
    pub(crate) stderr_bytes: usize,
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
pub(crate) struct AiExecutionRequest {
    pub(crate) execution_id: String,
    pub(crate) agent_id: AgentId,
    pub(crate) purpose: AiExecutionPurpose,
    pub(crate) session_mode: AgentSessionMode,
    pub(crate) prompt: String,
    pub(crate) model: Option<String>,
    pub(crate) limits: AiExecutionLimits,
    pub(crate) cancellation: AiExecutionCancellation,
    pub(crate) progress: Option<Arc<dyn AiExecutionProgressSink>>,
    pub(crate) tenant_id: Option<String>,
    pub(crate) execution_context_key: Option<String>,
    pub(crate) binding: Option<PersistentExecutionBinding>,
    pub(crate) replay: bool,
    pub(crate) restore_only: bool,
    pub(crate) team_tools: Option<AiTeamTools>,
    pub(crate) recall_tools: Option<AiRecallTools>,
    pub(crate) memory_generation_tools: Option<AiMemoryGenerationTools>,
}

impl AiExecutionRequest {
    pub(crate) fn validate(&self) -> Result<(), AiExecutionError> {
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

    pub(crate) fn report_phase(&self, phase: AiExecutionPhase) {
        if let Some(progress) = self.progress.as_ref() {
            progress.set_phase(phase);
        }
    }

    pub(crate) fn report_cleanup(&self, report: AiExecutionCleanupReport) {
        if let Some(progress) = self.progress.as_ref() {
            progress.set_cleanup_report(report);
        }
    }

    pub(crate) fn report_session_event(&self, event: SessionEvent) {
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
            .field("progress", &self.progress.as_ref().map(|_| "<sink>"))
            .field("tenant_id", &self.tenant_id)
            .field("execution_context_key", &self.execution_context_key)
            .field("binding", &self.binding)
            .field("replay", &self.replay)
            .field("restore_only", &self.restore_only)
            .field("team_tools", &self.team_tools)
            .field("recall_tools", &self.recall_tools)
            .field("memory_generation_tools", &self.memory_generation_tools)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionCleanupStatus {
    Deleted,
    Unsupported,
    Failed(String),
    Skipped,
}

#[derive(Clone)]
pub(crate) struct AiExecutionResult {
    pub(crate) text: String,
    pub(crate) agent_id: AgentId,
    pub(crate) protocol: AgentProtocol,
    pub(crate) requested_model: Option<String>,
    pub(crate) elapsed_ms: u64,
    pub(crate) persistent_binding: Option<PersistentExecutionBinding>,
    pub(crate) replay_text: Option<String>,
    pub(crate) session_cleanup: SessionCleanupStatus,
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
pub(crate) enum AiExecutionPhase {
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

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
