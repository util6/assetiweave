use std::{fmt, sync::Arc, time::Duration};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::backend::agents::types::{AgentId, AgentProtocol};

use super::{
    bindings::PersistentExecutionBinding, normalize_model, normalize_prompt, AiExecutionError,
};

pub(crate) trait AiExecutionProgressSink: Send + Sync {
    fn set_phase(&self, phase: AiExecutionPhase);

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
    PromptOptimization,
    ConnectionTest,
    ModelDiscovery,
    TeamLeaderChat,
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
    pub(crate) initialize_timeout: Duration,
    pub(crate) config_rpc_timeout: Duration,
    pub(crate) cancel_grace: Duration,
    pub(crate) close_timeout: Duration,
    pub(crate) text_bytes: usize,
    pub(crate) stderr_bytes: usize,
}

impl Default for AiExecutionLimits {
    fn default() -> Self {
        Self {
            total_timeout: Duration::from_secs(180),
            initialize_timeout: Duration::from_secs(10),
            config_rpc_timeout: Duration::from_secs(5),
            cancel_grace: Duration::from_secs(2),
            close_timeout: Duration::from_secs(2),
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
            .finish()
    }
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
mod tests {
    use super::*;
    use crate::backend::agents::types::AgentId;

    #[test]
    fn default_limits_match_the_phase_one_contract() {
        let limits = AiExecutionLimits::default();

        assert_eq!(limits.total_timeout.as_secs(), 180);
        assert_eq!(limits.initialize_timeout.as_secs(), 10);
        assert_eq!(limits.config_rpc_timeout.as_secs(), 5);
        assert_eq!(limits.cancel_grace.as_secs(), 2);
        assert_eq!(limits.close_timeout.as_secs(), 2);
        assert_eq!(limits.text_bytes, 1024 * 1024);
        assert_eq!(limits.stderr_bytes, 256 * 1024);
    }

    #[test]
    fn request_validation_rejects_invalid_prompt_and_model_before_execution() {
        let empty_prompt = request("   ", None);
        assert!(empty_prompt.validate().is_err());

        let oversized_prompt = request(&"x".repeat(1_000_001), None);
        assert!(oversized_prompt.validate().is_err());

        let invalid_model = request("translate", Some("model\nwith-newline"));
        assert!(invalid_model.validate().is_err());
    }

    #[test]
    fn request_debug_output_redacts_the_prompt() {
        let request = request("SECRET_PROMPT", Some("SECRET_MODEL"));

        let debug = format!("{request:?}");

        assert!(!debug.contains("SECRET_PROMPT"));
        assert!(!debug.contains("SECRET_MODEL"));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn result_debug_output_redacts_text_and_requested_model() {
        let result = AiExecutionResult {
            text: "SECRET_RESULT".to_string(),
            agent_id: AgentId::parse("opencode").unwrap(),
            protocol: AgentProtocol::Acp,
            requested_model: Some("SECRET_MODEL".to_string()),
            elapsed_ms: 1,
            persistent_binding: None,
            replay_text: None,
        };

        let debug = format!("{result:?}");

        assert!(!debug.contains("SECRET_RESULT"));
        assert!(!debug.contains("SECRET_MODEL"));
        assert!(debug.contains("<redacted>"));
    }

    fn request(prompt: &str, model: Option<&str>) -> AiExecutionRequest {
        AiExecutionRequest {
            execution_id: uuid::Uuid::new_v4().to_string(),
            agent_id: AgentId::parse("opencode").unwrap(),
            purpose: AiExecutionPurpose::Translation,
            session_mode: AgentSessionMode::OneShot,
            prompt: prompt.to_string(),
            model: model.map(str::to_string),
            limits: AiExecutionLimits::default(),
            cancellation: AiExecutionCancellation::default(),
            progress: None,
            tenant_id: None,
            execution_context_key: None,
            binding: None,
            replay: false,
            restore_only: false,
            team_tools: None,
        }
    }
}
