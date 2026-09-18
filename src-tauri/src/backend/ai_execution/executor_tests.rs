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

type LogField = (&'static str, String);

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

impl AgentExecutor {
    pub(crate) fn new(
        registry: Arc<AgentRegistry>,
        acp: Arc<dyn AgentExecutionBackend>,
        max_concurrency: usize,
    ) -> Self {
        Self::with_backends(registry, acp.clone(), acp, max_concurrency)
    }

    fn available_permits(&self) -> usize {
        self.permits.available_permits()
    }
}

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
                persistent_binding: None,
                replay_text: None,
                session_cleanup: crate::backend::ai_execution::types::SessionCleanupStatus::Deleted,
            })
        })
    }

    fn check_connection<'a>(&'a self, _definition: AgentDefinition) -> BackendConnectionFuture<'a> {
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
async fn memory_generation_tools_require_an_acp_agent() {
    let (backend, _started) = FakeBackend::new(FakeMode::Immediate);
    let executor = executor(AgentProtocol::Native, backend.clone(), 2);
    let mut execution_request = request("fake-agent");
    execution_request.purpose = AiExecutionPurpose::MemoryGeneration;
    execution_request.memory_generation_tools =
        Some(crate::backend::ai_execution::AiMemoryGenerationTools {
            tenant_id: "tenant-fixture".to_string(),
            job_id: "job-fixture".to_string(),
            ownership_token: "lease-fixture".to_string(),
            database_path: ":memory:".to_string(),
        });

    assert!(matches!(
        executor.execute(execution_request).await,
        Err(AiExecutionError::MemoryGenerationToolsUnavailable)
    ));
    assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
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

#[tokio::test(flavor = "current_thread")]
async fn antigravity_routes_only_via_acp_and_rejects_missing_capabilities() {
    let (acp_backend, _started_acp) = FakeBackend::new(FakeMode::Immediate);
    let (native_backend, _started_native) = FakeBackend::new(FakeMode::Immediate);
    let antigravity_def = AgentDefinition {
        id: AgentId::parse("antigravity").unwrap(),
        installation_id: None,
        display_name: "Antigravity".to_owned(),
        protocol: AgentProtocol::Acp,
        command: "antigravity-acp".to_owned(),
        args: Vec::new(),
        env: Vec::new(),
        declared_capabilities: DeclaredAgentCapabilities::acp_text(),
        availability_probe: None,
        model_discovery: None,
        session_cleanup: None,
        session_cleanup_not_found_markers: Vec::new(),
    };
    let registry = AgentRegistry::from_definitions([antigravity_def.clone()]).unwrap();
    let executor = AgentExecutor::with_backends(
        registry.into(),
        acp_backend.clone(),
        native_backend.clone(),
        2,
    );

    // 1. Normal translation routed strictly to ACP backend, Native backend is not touched
    let normal_req = request("antigravity");
    assert!(executor.execute(normal_req).await.is_ok());
    assert_eq!(acp_backend.calls.load(Ordering::SeqCst), 1);
    assert_eq!(native_backend.calls.load(Ordering::SeqCst), 0);

    // 2. Team tools rejected with TeamToolsUnavailable because capability is false
    let mut team_req = request("antigravity");
    team_req.team_tools = Some(crate::backend::ai_execution::AiTeamTools {
        tenant_id: "test-tenant".to_string(),
        team_id: "test-team".to_string(),
        run_id: "test-run".to_string(),
        member_id: "test-member".to_string(),
        credential: "test-cred".to_string(),
        database_path: ":memory:".to_string(),
    });
    let team_res = executor.execute(team_req).await;
    assert!(matches!(
        team_res,
        Err(AiExecutionError::TeamToolsUnavailable)
    ));
    assert_eq!(acp_backend.calls.load(Ordering::SeqCst), 1);
    assert_eq!(native_backend.calls.load(Ordering::SeqCst), 0);

    // 3. Replay rejected with ResumeUnavailable when history_replay capability is false
    let mut no_replay_def = antigravity_def.clone();
    no_replay_def.id = AgentId::parse("antigravity-no-replay").unwrap();
    no_replay_def.declared_capabilities.history_replay = false;
    let registry_no_replay = AgentRegistry::from_definitions([no_replay_def]).unwrap();
    let executor_no_replay = AgentExecutor::with_backends(
        registry_no_replay.into(),
        acp_backend.clone(),
        native_backend.clone(),
        2,
    );
    let mut replay_req = request("antigravity-no-replay");
    replay_req.session_mode = crate::backend::ai_execution::AgentSessionMode::Persistent;
    replay_req.execution_context_key = Some("ctx-key".to_string());
    replay_req.replay = true;
    let replay_res = executor_no_replay.execute(replay_req).await;
    assert!(matches!(
        replay_res,
        Err(AiExecutionError::ResumeUnavailable)
    ));
    assert_eq!(acp_backend.calls.load(Ordering::SeqCst), 1);
    assert_eq!(native_backend.calls.load(Ordering::SeqCst), 0);
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
        session_cleanup: None,
        session_cleanup_not_found_markers: Vec::new(),
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
        tenant_id: None,
        execution_context_key: None,
        binding: None,
        replay: false,
        restore_only: false,
        team_tools: None,
        recall_tools: None,
        memory_generation_tools: None,
    }
}
