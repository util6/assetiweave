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
    registry::{AgentAvailability, AgentProbeError, AgentRegistry},
    types::{AgentDefinition, AgentId, AgentProtocol},
};
use crate::backend::operation_log::{log_info, log_warn, LogField};

use super::{
    backends::acp::AcpExecutionBackend, AiExecutionError, AiExecutionPhase,
    AiExecutionProgressSink, AiExecutionPurpose, AiExecutionRequest, AiExecutionResult,
};

const DEFAULT_MAX_CONCURRENCY: usize = 2;

pub(crate) type BackendFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AiExecutionResult, AiExecutionError>> + Send + 'a>>;

pub(crate) trait AgentExecutionBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        definition: AgentDefinition,
        request: AiExecutionRequest,
    ) -> BackendFuture<'a>;
}

pub(crate) trait AgentExecutionRuntime: Send + Sync {
    fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a>;

    fn check_availability(&self, agent_id: &AgentId) -> AgentAvailability {
        AgentAvailability {
            available: false,
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
}

pub(crate) struct AgentExecutor {
    registry: Arc<AgentRegistry>,
    acp: Arc<dyn AgentExecutionBackend>,
    permits: Arc<Semaphore>,
    active: Arc<Mutex<HashMap<uuid::Uuid, super::AiExecutionCancellation>>>,
}

impl AgentExecutor {
    pub(crate) fn builtin(workspace_root: PathBuf) -> Result<Self, AiExecutionError> {
        let registry = AgentRegistry::builtin().map_err(|_| AiExecutionError::Protocol {
            operation: "registry_initialize",
        })?;
        Ok(Self::new(
            Arc::new(registry),
            Arc::new(AcpExecutionBackend::new(workspace_root)),
            DEFAULT_MAX_CONCURRENCY,
        ))
    }

    pub(crate) fn new(
        registry: Arc<AgentRegistry>,
        acp: Arc<dyn AgentExecutionBackend>,
        max_concurrency: usize,
    ) -> Self {
        Self {
            registry,
            acp,
            permits: Arc::new(Semaphore::new(max_concurrency.max(1))),
            active: Arc::new(Mutex::new(HashMap::new())),
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
            let active_id = uuid::Uuid::new_v4();
            self.active
                .lock()
                .map_err(|_| AiExecutionError::Protocol {
                    operation: "active_execution_registry",
                })?
                .insert(active_id, request.cancellation.clone());
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
            let Some(definition) = self.registry.get(&request.agent_id).cloned() else {
                return Err(AiExecutionError::AgentNotFound {
                    agent_id: request.agent_id.clone(),
                });
            };
            let backend = match definition.protocol {
                AgentProtocol::Acp => Arc::clone(&self.acp),
                protocol => return Err(AiExecutionError::UnsupportedProtocol { protocol }),
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

    #[cfg(test)]
    fn available_permits(&self) -> usize {
        self.permits.available_permits()
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
            .map(|active| active.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for cancellation in cancellations {
            cancellation.cancel();
        }
    }
}

struct ActiveExecutionGuard {
    id: uuid::Uuid,
    active: Arc<Mutex<HashMap<uuid::Uuid, super::AiExecutionCancellation>>>,
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
        stdout: Vec::new(),
        stderr: Vec::new(),
        stdout_truncated: false,
        stderr_truncated: false,
    }
}

fn timeout_before_spawn(request: &AiExecutionRequest, timeout: Duration) -> AiExecutionError {
    AiExecutionError::Timeout {
        program: PathBuf::from(request.agent_id.as_str()),
        timeout,
        stdout: Vec::new(),
        stderr: Vec::new(),
        stdout_truncated: false,
        stderr_truncated: false,
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
            types::{AgentId, DeclaredAgentCapabilities},
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
        assert!(matches!(
            native_executor.execute(request("fake-agent")).await,
            Err(AiExecutionError::UnsupportedProtocol {
                protocol: AgentProtocol::Native
            })
        ));
        assert_eq!(native_backend.calls.load(Ordering::SeqCst), 0);
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
