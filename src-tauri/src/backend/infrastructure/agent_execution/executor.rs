use std::{
    collections::HashMap,
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tokio::sync::Semaphore;

use crate::backend::{
    domain::agents::{
        AgentCatalogEntry, AgentDefinition, AgentId, AgentProtocol, DeclaredAgentCapabilities,
    },
    store::system::PersistentBindingStore,
};

use super::{
    backends::{acp::AcpExecutionBackend, native::NativeExecutionBackend},
    registry::{AgentAvailability, AgentProbeError, AgentRegistry, AgentRegistryHandle},
    AgentConnectionResult, AgentModelOption, AgentModelsResult, AiExecutionCleanupReport,
    AiExecutionError, AiExecutionPhase, AiExecutionProgressSink, AiExecutionPurpose,
    AiExecutionRequest, AiExecutionResult, SessionCleanupStatus,
};

pub(crate) use super::executor_support::*;
pub(crate) use super::executor_traits::*;

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

impl AgentExecutionRuntime for AgentExecutor {
    fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
        Box::pin(async move { AgentExecutor::execute(self, request).await })
    }

    fn check_availability(&self, agent_id: &AgentId) -> AgentAvailability {
        self.registry.cached_availability(agent_id)
    }

    fn list_agent_catalog(&self) -> Vec<AgentCatalogEntry> {
        self.registry
            .definitions()
            .iter()
            .map(AgentCatalogEntry::from_definition)
            .collect()
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

#[cfg(test)]
#[path = "executor_tests.rs"]
mod tests;
