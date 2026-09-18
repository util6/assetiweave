use std::{
    collections::HashMap,
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tokio::sync::Semaphore;

use crate::backend::agents::{
    registry::{AgentAvailability, AgentProbeError, AgentRegistry, AgentRegistryHandle},
    types::{
        AgentCatalogEntry, AgentConnectionResult, AgentDefinition, AgentId, AgentModelOption,
        AgentModelsResult, AgentProtocol, DeclaredAgentCapabilities,
    },
};

use super::{
    backends::{acp::AcpExecutionBackend, native::NativeExecutionBackend},
    AiExecutionCleanupReport, AiExecutionError, AiExecutionPhase, AiExecutionProgressSink,
    AiExecutionPurpose, AiExecutionRequest, AiExecutionResult, PersistentBindingStore,
    SessionCleanupStatus,
};

pub(crate) type BackendFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AiExecutionResult, AiExecutionError>> + Send + 'a>>;
pub(crate) type AgentConnectionFuture<'a> =
    Pin<Box<dyn Future<Output = AgentConnectionResult> + Send + 'a>>;
pub(crate) type BackendConnectionFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), AiExecutionError>> + Send + 'a>>;
pub(crate) type BackendModelsFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<(Vec<AgentModelOption>, Option<String>), AiExecutionError>>
            + Send
            + 'a,
    >,
>;
pub(crate) type AgentModelsFuture<'a> =
    Pin<Box<dyn Future<Output = AgentModelsResult> + Send + 'a>>;

pub(crate) trait AgentExecutionBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        definition: AgentDefinition,
        request: AiExecutionRequest,
    ) -> BackendFuture<'a>;

    fn check_connection<'a>(&'a self, _definition: AgentDefinition) -> BackendConnectionFuture<'a> {
        Box::pin(async {
            Err(AiExecutionError::Protocol {
                operation: "agent_connection_probe",
            })
        })
    }

    fn discover_models<'a>(&'a self, _definition: AgentDefinition) -> BackendModelsFuture<'a> {
        Box::pin(async {
            Err(AiExecutionError::Protocol {
                operation: "agent_model_discovery",
            })
        })
    }
}

pub(crate) trait AgentExecutionRuntime: Send + Sync {
    fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a>;

    fn list_agent_catalog(&self) -> Vec<AgentCatalogEntry> {
        Vec::new()
    }

    fn agent_capabilities(&self, _agent_id: &AgentId) -> Option<DeclaredAgentCapabilities> {
        None
    }

    fn check_agent_installation(&self, agent_id: &AgentId) -> AgentConnectionResult {
        unavailable_connection_result(
            agent_id,
            "agent_not_found",
            "The selected AI agent is not registered.",
        )
    }

    fn check_agent_connection<'a>(&'a self, agent_id: &'a AgentId) -> AgentConnectionFuture<'a> {
        let result = self.check_agent_installation(agent_id);
        Box::pin(async move { result })
    }

    fn discover_agent_models<'a>(&'a self, agent_id: &'a AgentId) -> AgentModelsFuture<'a> {
        let result = unavailable_models_result(
            agent_id,
            "agent_not_found",
            "The selected AI agent is not registered.",
        );
        Box::pin(async move { result })
    }

    fn check_availability(&self, agent_id: &AgentId) -> AgentAvailability {
        AgentAvailability {
            available: false,
            installed: false,
            version: None,
            error: Some(AgentProbeError::ProbeNotConfigured {
                agent_id: agent_id.clone(),
                kind: "availability",
            }),
        }
    }

    fn discover_models(
        &self,
        agent_id: &AgentId,
        _timeout: Duration,
    ) -> Result<Vec<u8>, AgentProbeError> {
        Err(AgentProbeError::ProbeNotConfigured {
            agent_id: agent_id.clone(),
            kind: "model_discovery",
        })
    }

    fn cancel_all(&self) {}
}

impl AgentExecutionBackend for AcpExecutionBackend {
    fn execute<'a>(
        &'a self,
        definition: AgentDefinition,
        request: AiExecutionRequest,
    ) -> BackendFuture<'a> {
        Box::pin(async move { AcpExecutionBackend::execute(self, &definition, request).await })
    }

    fn check_connection<'a>(&'a self, definition: AgentDefinition) -> BackendConnectionFuture<'a> {
        Box::pin(async move { AcpExecutionBackend::check_connection(self, &definition).await })
    }

    fn discover_models<'a>(&'a self, definition: AgentDefinition) -> BackendModelsFuture<'a> {
        Box::pin(async move { AcpExecutionBackend::discover_models(self, &definition).await })
    }
}

impl AgentExecutionBackend for NativeExecutionBackend {
    fn execute<'a>(
        &'a self,
        definition: AgentDefinition,
        request: AiExecutionRequest,
    ) -> BackendFuture<'a> {
        Box::pin(async move { NativeExecutionBackend::execute(self, &definition, request).await })
    }

    fn check_connection<'a>(&'a self, definition: AgentDefinition) -> BackendConnectionFuture<'a> {
        Box::pin(async move { NativeExecutionBackend::check_connection(self, &definition).await })
    }

    fn discover_models<'a>(&'a self, definition: AgentDefinition) -> BackendModelsFuture<'a> {
        Box::pin(async move { NativeExecutionBackend::discover_models(self, &definition).await })
    }
}

pub(crate) struct AgentExecutor {
    registry: AgentRegistryHandle,
    acp: Arc<dyn AgentExecutionBackend>,
    native: Arc<dyn AgentExecutionBackend>,
    permits: Arc<Semaphore>,
    active:
        Arc<Mutex<HashMap<uuid::Uuid, (AgentId, Option<String>, super::AiExecutionCancellation)>>>,
    mutation_gates: Arc<Mutex<HashMap<String, Arc<tokio::sync::RwLock<()>>>>>,
    persistent_bindings: Option<Arc<PersistentBindingStore>>,
}

impl AgentExecutor {
    pub(crate) fn with_backends(
        registry: Arc<AgentRegistry>,
        acp: Arc<dyn AgentExecutionBackend>,
        native: Arc<dyn AgentExecutionBackend>,
        max_concurrency: usize,
    ) -> Self {
        Self {
            registry: AgentRegistryHandle::from_registry(registry),
            acp,
            native,
            permits: Arc::new(Semaphore::new(max_concurrency.max(1))),
            active: Arc::new(Mutex::new(HashMap::new())),
            mutation_gates: Arc::new(Mutex::new(HashMap::new())),
            persistent_bindings: None,
        }
    }

    pub(crate) fn with_registry_handle_and_bindings(
        registry: AgentRegistryHandle,
        acp: Arc<dyn AgentExecutionBackend>,
        native: Arc<dyn AgentExecutionBackend>,
        max_concurrency: usize,
        persistent_bindings: Arc<PersistentBindingStore>,
    ) -> Self {
        Self {
            registry,
            acp,
            native,
            permits: Arc::new(Semaphore::new(max_concurrency.max(1))),
            active: Arc::new(Mutex::new(HashMap::new())),
            mutation_gates: Arc::new(Mutex::new(HashMap::new())),
            persistent_bindings: Some(persistent_bindings),
        }
    }

    pub(crate) async fn execute(
        &self,
        mut request: AiExecutionRequest,
    ) -> Result<AiExecutionResult, AiExecutionError> {
        let started = Instant::now();
        let execution_id = request.execution_id.clone();
        let agent_id = request.agent_id.to_string();
        let purpose = request.purpose;
        let suppress_diagnostics = request.replay;
        let session_mode = request.session_mode;
        if !suppress_diagnostics {
            tracing::info!(
                action = "ai_execution.lifecycle",
                execution_id = %execution_id,
                agent_id = %agent_id,
                purpose = ?purpose,
                elapsed_ms = 0,
                "AI execution started"
            );
        }
        let downstream_progress = request.progress.take();
        request.progress = Some(Arc::new(ObservedProgressSink {
            execution_id: execution_id.clone(),
            agent_id: agent_id.clone(),
            purpose,
            started,
            suppress_diagnostics,
            downstream: downstream_progress,
        }));

        let outcome = async {
            request.validate()?;
            if matches!(request.session_mode, super::AgentSessionMode::Persistent)
                && request.binding.is_none()
            {
                if let (Some(store), Some(tenant_id), Some(context_key)) = (
                    self.persistent_bindings.as_ref(),
                    request.tenant_id.as_deref(),
                    request.execution_context_key.as_deref(),
                ) {
                    request.binding = store.load(tenant_id, context_key).await.map_err(|_| {
                        AiExecutionError::Protocol {
                            operation: "persistent_binding_load",
                        }
                    })?;
                }
            }
            let mutation_gate = self.mutation_gate(request.agent_id.as_str());
            let _execution_lease = mutation_gate.read().await;
            let active_id = uuid::Uuid::new_v4();
            self.active
                .lock()
                .map_err(|_| AiExecutionError::Protocol {
                    operation: "active_execution_registry",
                })?
                .insert(
                    active_id,
                    (request.agent_id.clone(), None, request.cancellation.clone()),
                );
            let _active_guard = ActiveExecutionGuard {
                id: active_id,
                active: self.active.clone(),
            };
            let original_timeout = request.limits.total_timeout;
            let deadline = tokio::time::Instant::now() + original_timeout;
            let queue_cancellation_token = request.cancellation.clone();
            let queue_cancellation = queue_cancellation_token.cancelled();
            tokio::pin!(queue_cancellation);
            let permit = tokio::select! {
                permit = Arc::clone(&self.permits).acquire_owned() => {
                    permit.map_err(|_| AiExecutionError::Protocol { operation: "execution_queue" })?
                }
                _ = &mut queue_cancellation => {
                    request.report_phase(AiExecutionPhase::Cancelling);
                    return Err(cancelled_before_spawn(&request));
                }
                _ = tokio::time::sleep_until(deadline) => {
                    return Err(timeout_before_spawn(&request, original_timeout));
                }
            };

            if request.cancellation.is_cancelled() {
                request.report_phase(AiExecutionPhase::Cancelling);
                return Err(cancelled_before_spawn(&request));
            }
            request.report_phase(AiExecutionPhase::Resolving);
            let Some(definition) = self.registry.get(&request.agent_id) else {
                return Err(AiExecutionError::AgentNotFound {
                    agent_id: request.agent_id.clone(),
                });
            };
            if request.binding.as_ref().is_some_and(|binding| {
                binding.agent_id != definition.id.as_str()
                    || binding.installation_id != definition.installation_id
            }) {
                return Err(AiExecutionError::ResumeUnavailable);
            }
            if request.restore_only && request.binding.is_none() {
                return Err(AiExecutionError::ResumeUnavailable);
            }
            if matches!(request.session_mode, super::AgentSessionMode::Persistent)
                && !definition.declared_capabilities.resume
            {
                return Err(AiExecutionError::ResumeUnavailable);
            }
            if request.replay && !definition.declared_capabilities.history_replay {
                return Err(AiExecutionError::ResumeUnavailable);
            }
            if matches!(request.purpose, AiExecutionPurpose::Recall)
                && !matches!(definition.protocol, AgentProtocol::Acp)
            {
                return Err(AiExecutionError::RecallToolsUnavailable);
            }
            if request.memory_generation_tools.is_some()
                && !matches!(definition.protocol, AgentProtocol::Acp)
            {
                return Err(AiExecutionError::MemoryGenerationToolsUnavailable);
            }
            if request.team_tools.is_some() && !definition.declared_capabilities.team_tools {
                return Err(AiExecutionError::TeamToolsUnavailable);
            }
            if let Ok(mut active) = self.active.lock() {
                if let Some((_, installation_id, _)) = active.get_mut(&active_id) {
                    *installation_id = definition.installation_id.clone();
                }
            }
            let backend = match definition.protocol {
                AgentProtocol::Acp => Arc::clone(&self.acp),
                AgentProtocol::Native => Arc::clone(&self.native),
            };

            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(timeout_before_spawn(&request, original_timeout));
            }
            request.limits.total_timeout = remaining;
            let cancellation = request.cancellation.clone();
            let timeout_request = request.clone();
            let execution = backend.execute(definition, request);
            tokio::pin!(execution);
            let outcome = tokio::select! {
                result = &mut execution => result,
                _ = tokio::time::sleep_until(deadline) => {
                    timeout_request.report_phase(AiExecutionPhase::Cancelling);
                    cancellation.cancel();
                    match execution.await {
                        Err(error @ AiExecutionError::CleanupFailed { .. }) => Err(error),
                        _ => Err(timeout_before_spawn(&timeout_request, original_timeout)),
                    }
                }
            };
            let outcome = enforce_cleanup_contract(purpose, outcome);
            drop(permit);
            outcome
        }
        .await;

        let outcome = match outcome {
            Ok(result) if matches!(session_mode, super::AgentSessionMode::Persistent) => {
                if let (Some(store), Some(binding)) = (
                    self.persistent_bindings.as_ref(),
                    result.persistent_binding.as_ref(),
                ) {
                    store
                        .save(binding)
                        .await
                        .map_err(|_| AiExecutionError::Protocol {
                            operation: "persistent_binding_save",
                        })?;
                }
                Ok(result)
            }
            other => other,
        };
        if !suppress_diagnostics {
            let elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            match &outcome {
                Ok(result) => {
                    tracing::info!(
                        action = "ai_execution.lifecycle",
                        execution_id = %execution_id,
                        agent_id = %agent_id,
                        purpose = ?purpose,
                        elapsed_ms,
                        protocol = ?result.protocol,
                        text_bytes = result.text.len(),
                        "AI execution completed"
                    );
                }
                Err(error) => {
                    tracing::warn!(
                        action = "ai_execution.lifecycle",
                        execution_id = %execution_id,
                        agent_id = %agent_id,
                        purpose = ?purpose,
                        elapsed_ms,
                        error_code = error.to_view().code,
                        "AI execution failed"
                    );
                }
            }
        }
        outcome
    }

    async fn check_connection(&self, agent_id: &AgentId) -> AgentConnectionResult {
        let installation = self.registry.check_availability(agent_id).await;
        let mut result = connection_result_from_availability(agent_id, &installation);
        if !installation.available {
            return result;
        }

        let Some(definition) = self.registry.get(agent_id) else {
            return unavailable_connection_result(
                agent_id,
                "agent_not_found",
                "The selected AI agent is not registered.",
            );
        };

        let backend = match definition.protocol {
            AgentProtocol::Acp => Arc::clone(&self.acp),
            AgentProtocol::Native => Arc::clone(&self.native),
        };

        let protocol = definition.protocol;
        match backend.check_connection(definition).await {
            Ok(()) => {
                result.available = true;
                result.connected = true;
                result.connection_method = Some(if protocol == AgentProtocol::Native {
                    "native".to_string()
                } else {
                    "acp".to_string()
                });
                result.error_code = None;
                result.error = None;
            }
            Err(error) => {
                result.available = false;
                result.connected = false;
                result.connection_method = Some(if protocol == AgentProtocol::Native {
                    "native".to_string()
                } else {
                    "acp".to_string()
                });
                result.error_code = Some(if protocol == AgentProtocol::Native {
                    "native_connection_failed".to_string()
                } else {
                    "acp_connection_failed".to_string()
                });
                result.error = Some(error.to_view().message);
            }
        }
        result
    }

    async fn discover_agent_models(&self, agent_id: &AgentId) -> AgentModelsResult {
        let installation = self.registry.check_availability(agent_id).await;
        if !installation.available {
            return models_result_from_availability(agent_id, &installation);
        }

        let Some(definition) = self.registry.get(agent_id) else {
            return unavailable_models_result(
                agent_id,
                "agent_not_found",
                "The selected AI agent is not registered.",
            );
        };

        let backend = match definition.protocol {
            AgentProtocol::Acp => Arc::clone(&self.acp),
            AgentProtocol::Native => Arc::clone(&self.native),
        };

        match backend.discover_models(definition).await {
            Ok((models, current_model_id)) => AgentModelsResult {
                agent_id: agent_id.to_string(),
                available: true,
                current_model_id: current_model_id
                    .or_else(|| models.first().map(|model| model.id.clone())),
                models,
                error_code: None,
                error: None,
            },
            Err(error) => {
                unavailable_models_result(agent_id, "model_discovery_failed", &error.to_string())
            }
        }
    }

    pub(crate) fn active_count(&self, agent_id: &AgentId) -> usize {
        self.active
            .lock()
            .map(|active| active.values().filter(|(id, _, _)| id == agent_id).count())
            .unwrap_or(0)
    }

    pub(crate) fn agent_in_use(&self, agent_id: &str) -> bool {
        AgentId::parse(agent_id)
            .map(|id| self.active_count(&id) > 0)
            .unwrap_or(false)
    }

    pub(crate) fn mutation_gate(&self, agent_id: &str) -> Arc<tokio::sync::RwLock<()>> {
        let mut gates = self
            .mutation_gates
            .lock()
            .expect("agent mutation gate lock poisoned");
        gates
            .entry(agent_id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::RwLock::new(())))
            .clone()
    }
}

struct ObservedProgressSink {
    execution_id: String,
    agent_id: String,
    purpose: AiExecutionPurpose,
    started: Instant,
    suppress_diagnostics: bool,
    downstream: Option<Arc<dyn AiExecutionProgressSink>>,
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

impl AgentExecutionRuntime for AgentExecutor {
    fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
        Box::pin(async move { AgentExecutor::execute(self, request).await })
    }

    fn check_availability(&self, agent_id: &AgentId) -> AgentAvailability {
        self.registry.cached_availability(agent_id)
    }

    fn list_agent_catalog(&self) -> Vec<AgentCatalogEntry> {
        self.registry.catalog()
    }

    fn agent_capabilities(&self, agent_id: &AgentId) -> Option<DeclaredAgentCapabilities> {
        self.registry
            .get(agent_id)
            .map(|definition| definition.declared_capabilities.clone())
    }

    fn check_agent_installation(&self, agent_id: &AgentId) -> AgentConnectionResult {
        connection_result_from_availability(agent_id, &self.registry.cached_availability(agent_id))
    }

    fn check_agent_connection<'a>(&'a self, agent_id: &'a AgentId) -> AgentConnectionFuture<'a> {
        Box::pin(async move { AgentExecutor::check_connection(self, agent_id).await })
    }

    fn discover_agent_models<'a>(&'a self, agent_id: &'a AgentId) -> AgentModelsFuture<'a> {
        Box::pin(async move { AgentExecutor::discover_agent_models(self, agent_id).await })
    }

    fn discover_models(
        &self,
        agent_id: &AgentId,
        _timeout: Duration,
    ) -> Result<Vec<u8>, AgentProbeError> {
        Err(AgentProbeError::ProbeNotConfigured {
            agent_id: agent_id.clone(),
            kind: "model_discovery",
        })
    }

    fn cancel_all(&self) {
        let cancellations = self
            .active
            .lock()
            .map(|active| {
                active
                    .values()
                    .map(|(_, _, cancellation)| cancellation.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for cancellation in cancellations {
            cancellation.cancel();
        }
    }
}

fn unavailable_connection_result(
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

fn models_result_from_availability(
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

fn connection_result_from_availability(
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

struct ActiveExecutionGuard {
    id: uuid::Uuid,
    active:
        Arc<Mutex<HashMap<uuid::Uuid, (AgentId, Option<String>, super::AiExecutionCancellation)>>>,
}

impl Drop for ActiveExecutionGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active.lock() {
            active.remove(&self.id);
        }
    }
}

fn enforce_cleanup_contract(
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

fn cancelled_before_spawn(request: &AiExecutionRequest) -> AiExecutionError {
    AiExecutionError::Cancelled {
        program: PathBuf::from(request.agent_id.as_str()),
    }
}

fn timeout_before_spawn(request: &AiExecutionRequest, timeout: Duration) -> AiExecutionError {
    AiExecutionError::Timeout {
        program: PathBuf::from(request.agent_id.as_str()),
        timeout,
    }
}

#[cfg(test)]
#[path = "executor_tests.rs"]
mod tests;
