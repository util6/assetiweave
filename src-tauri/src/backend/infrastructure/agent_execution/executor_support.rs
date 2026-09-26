use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::backend::domain::agents::AgentId;

use super::{
    registry::AgentAvailability, AgentConnectionResult, AgentModelsResult, AiExecutionCancellation,
    AiExecutionCleanupReport, AiExecutionError, AiExecutionPhase, AiExecutionProgressSink,
    AiExecutionPurpose, AiExecutionRequest, AiExecutionResult, SessionCleanupStatus,
};

pub(crate) struct ObservedProgressSink {
    pub(crate) execution_id: String,
    pub(crate) agent_id: String,
    pub(crate) purpose: AiExecutionPurpose,
    pub(crate) started: Instant,
    pub(crate) suppress_diagnostics: bool,
    pub(crate) downstream: Option<Arc<dyn AiExecutionProgressSink>>,
}

impl AiExecutionProgressSink for ObservedProgressSink {
    fn set_phase(&self, phase: AiExecutionPhase) {
        if let Some(downstream) = self.downstream.as_ref() {
            downstream.set_phase(phase);
        }
        if !self.suppress_diagnostics {
            let elapsed_ms = self.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            tracing::info!(
                action = "ai_execution.phase",
                execution_id = %self.execution_id,
                agent_id = %self.agent_id,
                purpose = ?self.purpose,
                elapsed_ms,
                phase = ?phase,
                "AI execution phase changed"
            );
        }
    }

    fn emit_session_event(&self, event: super::session_events::SessionEvent) {
        if let Some(downstream) = self.downstream.as_ref() {
            downstream.emit_session_event(event);
        }
    }

    fn failure_phase(&self) -> Option<AiExecutionPhase> {
        self.downstream
            .as_ref()
            .and_then(|progress| progress.failure_phase())
    }

    fn set_cleanup_report(&self, report: AiExecutionCleanupReport) {
        if let Some(downstream) = self.downstream.as_ref() {
            downstream.set_cleanup_report(report);
        }
    }
}

pub(crate) fn models_result_from_availability(
    agent_id: &AgentId,
    availability: &AgentAvailability,
) -> AgentModelsResult {
    AgentModelsResult {
        agent_id: agent_id.to_string(),
        available: availability.available,
        models: Vec::new(),
        current_model_id: None,
        error_code: availability
            .error
            .as_ref()
            .map(|error| error.code().to_string()),
        error: availability.error.as_ref().map(ToString::to_string),
    }
}

pub(crate) fn connection_result_from_availability(
    agent_id: &AgentId,
    availability: &AgentAvailability,
) -> AgentConnectionResult {
    AgentConnectionResult {
        agent_id: agent_id.to_string(),
        available: availability.available,
        installed: availability.installed,
        connected: false,
        version: availability.version.clone(),
        connection_method: availability.available.then(|| "cli_version".to_string()),
        error_code: availability
            .error
            .as_ref()
            .map(|error| error.code().to_string()),
        error: availability.error.as_ref().map(ToString::to_string),
        installation_status: None,
        runtime_status: None,
        protocol_status: None,
        execution_ready: false,
        health_stale: false,
    }
}

pub(crate) struct ActiveExecutionGuard {
    pub(crate) id: uuid::Uuid,
    pub(crate) active:
        Arc<Mutex<HashMap<uuid::Uuid, (AgentId, Option<String>, AiExecutionCancellation)>>>,
}

impl Drop for ActiveExecutionGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active.lock() {
            active.remove(&self.id);
        }
    }
}

pub(crate) fn enforce_cleanup_contract(
    purpose: AiExecutionPurpose,
    outcome: Result<AiExecutionResult, AiExecutionError>,
) -> Result<AiExecutionResult, AiExecutionError> {
    if matches!(purpose, AiExecutionPurpose::SessionMemory) {
        return outcome;
    }

    match outcome {
        Ok(result) => match result.session_cleanup {
            SessionCleanupStatus::Deleted | SessionCleanupStatus::Skipped => Ok(result),
            SessionCleanupStatus::Unsupported => Err(AiExecutionError::CleanupFailed {
                failures: vec!["delete_unsupported".to_string()],
            }),
            SessionCleanupStatus::Failed(reason) => Err(AiExecutionError::CleanupFailed {
                failures: vec![reason],
            }),
        },
        Err(error) => Err(error),
    }
}

pub(crate) fn cancelled_before_spawn(request: &AiExecutionRequest) -> AiExecutionError {
    AiExecutionError::Cancelled {
        program: PathBuf::from(request.agent_id.as_str()),
    }
}

pub(crate) fn timeout_before_spawn(
    request: &AiExecutionRequest,
    timeout: Duration,
) -> AiExecutionError {
    AiExecutionError::Timeout {
        program: PathBuf::from(request.agent_id.as_str()),
        timeout,
    }
}
