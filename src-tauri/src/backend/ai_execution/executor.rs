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
        AgentModelsResult, AgentProtocol,
    },
};
use crate::backend::operation_log::{log_info, log_warn, LogField};

use super::{
    backends::{acp::AcpExecutionBackend, native::NativeExecutionBackend},
    AiExecutionCleanupReport, AiExecutionError, AiExecutionPhase, AiExecutionProgressSink,
    AiExecutionPurpose, AiExecutionRequest, AiExecutionResult,
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
}

impl AgentExecutor {
    #[cfg(test)]
    pub(crate) fn new(
        registry: Arc<AgentRegistry>,
        acp: Arc<dyn AgentExecutionBackend>,
        max_concurrency: usize,
    ) -> Self {
        Self::with_backends(registry, acp.clone(), acp, max_concurrency)
    }

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
        }
    }

    pub(crate) fn with_registry_handle(
        registry: AgentRegistryHandle,
        acp: Arc<dyn AgentExecutionBackend>,
        native: Arc<dyn AgentExecutionBackend>,
        max_concurrency: usize,
    ) -> Self {
        Self {
            registry,
            acp,
            native,
            permits: Arc::new(Semaphore::new(max_concurrency.max(1))),
            active: Arc::new(Mutex::new(HashMap::new())),
            mutation_gates: Arc::new(Mutex::new(HashMap::new())),
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
        log_info(
            "ai_execution.lifecycle",
            "AI execution started",
            &execution_log_fields(&execution_id, &agent_id, purpose, 0),
        );
        let downstream_progress = request.progress.take();
        request.progress = Some(Arc::new(ObservedProgressSink {
            execution_id: execution_id.clone(),
            agent_id: agent_id.clone(),
            purpose,
            started,
            downstream: downstream_progress,
        }));

        let outcome = async {
            request.validate()?;
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
            drop(permit);
            outcome
        }
        .await;

        let mut fields = execution_log_fields(
            &execution_id,
            &agent_id,
            purpose,
            started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        );
        match &outcome {
            Ok(result) => {
                fields.extend([
                    (
                        "protocol",
                        format!("{:?}", result.protocol).to_ascii_lowercase(),
                    ),
                    ("text_bytes", result.text.len().to_string()),
                ]);
                log_info("ai_execution.lifecycle", "AI execution completed", &fields);
            }
            Err(error) => {
                fields.push(("error_code", error.to_view().code));
                log_warn("ai_execution.lifecycle", "AI execution failed", &fields);
            }
        }
        outcome
    }

    async fn check_connection(&self, agent_id: &AgentId) -> AgentConnectionResult {
        let installation = self.registry.check_availability(agent_id);
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
        let installation = self.registry.check_availability(agent_id);
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

    #[cfg(test)]
    fn available_permits(&self) -> usize {
        self.permits.available_permits()
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
    downstream: Option<Arc<dyn AiExecutionProgressSink>>,
}

impl AiExecutionProgressSink for ObservedProgressSink {
    fn set_phase(&self, phase: AiExecutionPhase) {
        if let Some(downstream) = self.downstream.as_ref() {
            downstream.set_phase(phase);
        }
        let mut fields = execution_log_fields(
            &self.execution_id,
            &self.agent_id,
            self.purpose,
            self.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        );
        fields.push(("phase", format!("{phase:?}").to_ascii_lowercase()));
        log_info("ai_execution.phase", "AI execution phase changed", &fields);
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

fn execution_log_fields(
    execution_id: &str,
    agent_id: &str,
    purpose: AiExecutionPurpose,
    elapsed_ms: u64,
) -> Vec<LogField> {
    vec![
        ("execution_id", execution_id.to_string()),
        ("agent_id", agent_id.to_string()),
        ("purpose", format!("{purpose:?}").to_ascii_lowercase()),
        ("elapsed_ms", elapsed_ms.to_string()),
    ]
}

impl AgentExecutionRuntime for AgentExecutor {
    fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
        Box::pin(async move { AgentExecutor::execute(self, request).await })
    }

    fn check_availability(&self, agent_id: &AgentId) -> AgentAvailability {
        self.registry.check_availability(agent_id)
    }

    fn list_agent_catalog(&self) -> Vec<AgentCatalogEntry> {
        self.registry.catalog()
    }

    fn check_agent_installation(&self, agent_id: &AgentId) -> AgentConnectionResult {
        connection_result_from_availability(agent_id, &self.registry.check_availability(agent_id))
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
        timeout: Duration,
    ) -> Result<Vec<u8>, AgentProbeError> {
        self.registry.discover_models(agent_id, timeout)
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
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::sync::{mpsc, Semaphore};

    use crate::backend::{
        agents::{
            registry::AgentRegistry,
            types::{AgentCommandDefinition, AgentId, DeclaredAgentCapabilities},
        },
        ai_execution::{AiExecutionCancellation, AiExecutionLimits, AiExecutionPurpose},
    };

    #[derive(Clone, Copy)]
    enum FakeMode {
        Immediate,
        Hold,
        WaitCancellation,
    }

    struct FakeBackend {
        mode: FakeMode,
        calls: AtomicUsize,
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
        cleaned: Arc<AtomicBool>,
        started: mpsc::UnboundedSender<()>,
        gate: Arc<Semaphore>,
    }

    impl FakeBackend {
        fn new(mode: FakeMode) -> (Arc<Self>, mpsc::UnboundedReceiver<()>) {
            let (started, receiver) = mpsc::unbounded_channel();
            (
                Arc::new(Self {
                    mode,
                    calls: AtomicUsize::new(0),
                    active: Arc::new(AtomicUsize::new(0)),
                    max_active: Arc::new(AtomicUsize::new(0)),
                    cleaned: Arc::new(AtomicBool::new(false)),
                    started,
                    gate: Arc::new(Semaphore::new(0)),
                }),
                receiver,
            )
        }

        fn record_active(&self) -> ActiveCallGuard {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(active, Ordering::SeqCst);
            ActiveCallGuard(Arc::clone(&self.active))
        }
    }

    struct ActiveCallGuard(Arc<AtomicUsize>);

    impl Drop for ActiveCallGuard {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    impl AgentExecutionBackend for FakeBackend {
        fn execute<'a>(
            &'a self,
            definition: AgentDefinition,
            request: AiExecutionRequest,
        ) -> BackendFuture<'a> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                let _active = self.record_active();
                let _ = self.started.send(());
                match self.mode {
                    FakeMode::Immediate => {}
                    FakeMode::Hold => {
                        self.gate.acquire().await.unwrap().forget();
                    }
                    FakeMode::WaitCancellation => {
                        request.cancellation.cancelled().await;
                        self.cleaned.store(true, Ordering::SeqCst);
                        return Err(cancelled_before_spawn(&request));
                    }
                }
                Ok(AiExecutionResult {
                    text: "fake result".to_owned(),
                    agent_id: definition.id,
                    protocol: definition.protocol,
                    requested_model: request.model,
                    elapsed_ms: 1,
                })
            })
        }

        fn check_connection<'a>(
            &'a self,
            _definition: AgentDefinition,
        ) -> BackendConnectionFuture<'a> {
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn exe_01_invalid_request_never_calls_backend() {
        let (backend, _started) = FakeBackend::new(FakeMode::Immediate);
        let executor = executor(AgentProtocol::Acp, backend.clone(), 2);
        let mut invalid = request("fake-agent");
        invalid.prompt = "   ".to_owned();

        assert!(matches!(
            executor.execute(invalid).await,
            Err(AiExecutionError::InvalidPrompt(_))
        ));
        assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn exe_02_unknown_agent_has_stable_error_and_never_calls_backend() {
        let (backend, _started) = FakeBackend::new(FakeMode::Immediate);
        let executor = executor(AgentProtocol::Acp, backend.clone(), 2);

        let result = executor.execute(request("unknown-agent")).await;

        assert!(matches!(
            result,
            Err(AiExecutionError::AgentNotFound { .. })
        ));
        assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn exe_03_and_04_route_only_by_protocol() {
        let (acp_backend, _started) = FakeBackend::new(FakeMode::Immediate);
        let acp_executor = executor(AgentProtocol::Acp, acp_backend.clone(), 2);
        assert!(acp_executor.execute(request("fake-agent")).await.is_ok());
        assert_eq!(acp_backend.calls.load(Ordering::SeqCst), 1);

        let (native_backend, _started) = FakeBackend::new(FakeMode::Immediate);
        let native_executor = executor(AgentProtocol::Native, native_backend.clone(), 2);
        assert!(native_executor.execute(request("fake-agent")).await.is_ok());
        assert_eq!(native_backend.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn exe_05_shared_semaphore_limits_concurrency_to_two() {
        let (backend, mut started) = FakeBackend::new(FakeMode::Hold);
        let executor = Arc::new(executor(AgentProtocol::Acp, backend.clone(), 2));
        let first = spawn_execution(Arc::clone(&executor), request("fake-agent"));
        let second = spawn_execution(Arc::clone(&executor), request("fake-agent"));
        let third = spawn_execution(Arc::clone(&executor), request("fake-agent"));

        started.recv().await.unwrap();
        started.recv().await.unwrap();
        assert_eq!(backend.calls.load(Ordering::SeqCst), 2);
        assert_eq!(backend.max_active.load(Ordering::SeqCst), 2);
        assert_eq!(executor.available_permits(), 0);

        backend.gate.add_permits(2);
        started.recv().await.unwrap();
        backend.gate.add_permits(1);
        for handle in [first, second, third] {
            handle.await.unwrap().unwrap();
        }
        assert_eq!(backend.max_active.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn exe_06_queued_cancel_never_spawns_backend() {
        let (backend, mut started) = FakeBackend::new(FakeMode::Hold);
        let executor = Arc::new(executor(AgentProtocol::Acp, backend.clone(), 1));
        let first = spawn_execution(Arc::clone(&executor), request("fake-agent"));
        started.recv().await.unwrap();

        let queued_request = request("fake-agent");
        let cancellation = queued_request.cancellation.clone();
        let queued = spawn_execution(Arc::clone(&executor), queued_request);
        tokio::task::yield_now().await;
        cancellation.cancel();
        let result = queued.await.unwrap();

        assert!(matches!(result, Err(AiExecutionError::Cancelled { .. })));
        assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
        backend.gate.add_permits(1);
        first.await.unwrap().unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn exe_07_queue_wait_is_bounded_by_the_total_deadline() {
        let (backend, mut started) = FakeBackend::new(FakeMode::Hold);
        let executor = Arc::new(executor(AgentProtocol::Acp, backend.clone(), 1));
        let first = spawn_execution(Arc::clone(&executor), request("fake-agent"));
        started.recv().await.unwrap();
        let mut queued_request = request("fake-agent");
        queued_request.limits.total_timeout = Duration::from_millis(30);

        let result = executor.execute(queued_request).await;

        assert!(matches!(result, Err(AiExecutionError::Timeout { .. })));
        assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
        backend.gate.add_permits(1);
        first.await.unwrap().unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn exe_08_total_timeout_cancels_backend_and_waits_for_cleanup() {
        let (backend, mut started) = FakeBackend::new(FakeMode::WaitCancellation);
        let executor = executor(AgentProtocol::Acp, backend.clone(), 1);
        let mut execution_request = request("fake-agent");
        execution_request.limits.total_timeout = Duration::from_millis(30);

        let execution = spawn_execution(Arc::new(executor), execution_request);
        started.recv().await.unwrap();
        let result = execution.await.unwrap();

        assert!(matches!(result, Err(AiExecutionError::Timeout { .. })));
        assert!(backend.cleaned.load(Ordering::SeqCst));
        assert_eq!(backend.active.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn exe_09_and_10_result_metadata_does_not_claim_confirmed_model_use() {
        let (backend, _started) = FakeBackend::new(FakeMode::Immediate);
        let executor = executor(AgentProtocol::Acp, backend, 2);
        let mut execution_request = request("fake-agent");
        execution_request.model = Some("requested/model".to_owned());

        let result = executor.execute(execution_request).await.unwrap();

        assert_eq!(result.agent_id, AgentId::parse("fake-agent").unwrap());
        assert_eq!(result.protocol, AgentProtocol::Acp);
        assert_eq!(result.requested_model.as_deref(), Some("requested/model"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn exe_11_connection_check_runs_install_probe_then_acp_handshake() {
        let (backend, _started) = FakeBackend::new(FakeMode::Immediate);
        let mut agent_definition = definition(AgentProtocol::Acp);
        agent_definition.command = "sh".to_owned();
        agent_definition.availability_probe = Some(AgentCommandDefinition::with_command(
            "sh",
            ["-c", "printf 'fake-agent 1.2.3\\n'"],
        ));
        let registry = AgentRegistry::from_definitions([agent_definition]).unwrap();
        let executor = AgentExecutor::new(registry.into(), backend, 1);

        let result = executor
            .check_connection(&AgentId::parse("fake-agent").unwrap())
            .await;

        assert!(result.available);
        assert!(result.installed);
        assert!(result.connected);
        assert_eq!(result.version.as_deref(), Some("fake-agent 1.2.3"));
        assert_eq!(result.connection_method.as_deref(), Some("acp"));
        assert_eq!(result.error_code, None);
    }

    #[test]
    fn sec_execution_log_fields_exclude_prompt_model_and_payload() {
        let mut execution_request = request("fake-agent");
        execution_request.prompt = "SECRET_PROMPT".to_owned();
        execution_request.model = Some("SECRET_MODEL".to_owned());

        let fields = execution_log_fields(
            &execution_request.execution_id,
            execution_request.agent_id.as_str(),
            execution_request.purpose,
            7,
        );
        let rendered = format!("{fields:?}");

        assert!(!rendered.contains("SECRET_PROMPT"));
        assert!(!rendered.contains("SECRET_MODEL"));
        assert!(!rendered.contains("fake result"));
        assert!(rendered.contains("execution_id"));
        assert!(rendered.contains("elapsed_ms"));
    }

    fn spawn_execution(
        executor: Arc<AgentExecutor>,
        request: AiExecutionRequest,
    ) -> tokio::task::JoinHandle<Result<AiExecutionResult, AiExecutionError>> {
        tokio::spawn(async move { executor.execute(request).await })
    }

    fn executor(
        protocol: AgentProtocol,
        backend: Arc<FakeBackend>,
        concurrency: usize,
    ) -> AgentExecutor {
        let registry = AgentRegistry::from_definitions([definition(protocol)]).unwrap();
        AgentExecutor::new(registry.into(), backend, concurrency)
    }

    fn definition(protocol: AgentProtocol) -> AgentDefinition {
        AgentDefinition {
            id: AgentId::parse("fake-agent").unwrap(),
            installation_id: None,
            display_name: "Fake Agent".to_owned(),
            protocol,
            command: "fake-agent".to_owned(),
            args: Vec::new(),
            env: Vec::new(),
            declared_capabilities: DeclaredAgentCapabilities::acp_text(),
            availability_probe: None,
            model_discovery: None,
        }
    }

    fn request(agent_id: &str) -> AiExecutionRequest {
        AiExecutionRequest {
            execution_id: uuid::Uuid::new_v4().to_string(),
            agent_id: AgentId::parse(agent_id).unwrap(),
            purpose: AiExecutionPurpose::Translation,
            session_mode: crate::backend::ai_execution::AgentSessionMode::OneShot,
            prompt: "translate".to_owned(),
            model: None,
            limits: AiExecutionLimits {
                total_timeout: Duration::from_secs(2),
                ..AiExecutionLimits::default()
            },
            cancellation: AiExecutionCancellation::default(),
            progress: None,
        }
    }
}
