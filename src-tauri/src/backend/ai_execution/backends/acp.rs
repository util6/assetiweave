use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use agent_client_protocol::schema::v1::{SessionId, StopReason};
use uuid::Uuid;

use crate::backend::{
    agents::{
        process::{ManagedAgentProcess, ManagedAgentProcessError},
        protocol::acp::{AcpConnectConfig, AcpError, AcpProtocol, AcpProtocolChannels},
        types::{AgentDefinition, AgentProtocol},
    },
    ai_execution::{AiExecutionError, AiExecutionPhase, AiExecutionRequest, AiExecutionResult},
    operation_log::{log_info, log_warn},
};

use super::acp_aggregator::{AggregatorAction, TranslationTextAggregator};

const PROTOCOL_EVENT_CAPACITY: usize = 128;

#[derive(Clone, Debug)]
pub(crate) struct AcpExecutionBackend {
    workspace_root: PathBuf,
}

impl AcpExecutionBackend {
    pub(crate) fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    pub(crate) async fn execute(
        &self,
        definition: &AgentDefinition,
        request: AiExecutionRequest,
    ) -> Result<AiExecutionResult, AiExecutionError> {
        request.validate()?;
        if request.cancellation.is_cancelled() {
            request.report_phase(AiExecutionPhase::Cancelling);
            return Err(cancelled_error(definition));
        }
        let started = Instant::now();
        let workspace = create_workspace(&self.workspace_root)?;
        let mut guard = AcpExecutionGuard::new(workspace);

        let outcome = {
            let execution = run_execution(&mut guard, definition, &request, started);
            tokio::pin!(execution);
            let cancellation = request.cancellation.cancelled();
            tokio::pin!(cancellation);
            let timeout = tokio::time::sleep(request.limits.total_timeout);
            tokio::pin!(timeout);
            tokio::select! {
                outcome = &mut execution => outcome,
                _ = &mut cancellation => Err(cancelled_error(definition)),
                _ = &mut timeout => {
                    request.cancellation.cancel();
                    Err(timeout_error(definition, request.limits.total_timeout))
                }
            }
        };
        if outcome.is_err() {
            request.report_phase(AiExecutionPhase::Cancelling);
        }
        request.report_phase(AiExecutionPhase::Closing);
        let cleanup = guard.cleanup(outcome.is_err(), &request).await;
        request.report_phase(AiExecutionPhase::CleaningUp);
        let mut cleanup_fields = vec![
            ("execution_id", request.execution_id.clone()),
            ("agent_id", definition.id.to_string()),
            ("protocol", "acp".to_string()),
            ("phase", "cleaning_up".to_string()),
            ("process_reaped", cleanup.process_reaped.to_string()),
            ("workspace_removed", cleanup.workspace_removed.to_string()),
            ("stderr_bytes", cleanup.stderr_bytes.to_string()),
            ("stderr_truncated", cleanup.stderr_truncated.to_string()),
            ("failure_count", cleanup.failures.len().to_string()),
        ];
        if let Some(process_id) = cleanup.process_id {
            cleanup_fields.push(("pid", process_id.to_string()));
        }
        if let Some(exit_code) = cleanup.exit_code {
            cleanup_fields.push(("exit_code", exit_code.to_string()));
        }
        if cleanup.failures.is_empty() {
            log_info(
                "ai_execution.cleanup",
                "AI execution cleanup completed",
                &cleanup_fields,
            );
        } else {
            log_warn(
                "ai_execution.cleanup",
                "AI execution cleanup reported failures",
                &cleanup_fields,
            );
        }

        if cleanup.failures.is_empty() {
            return outcome;
        }
        if outcome.is_ok() || !cleanup.process_reaped || !cleanup.workspace_removed {
            return Err(AiExecutionError::CleanupFailed {
                failures: cleanup.failures,
            });
        }
        outcome
    }
}

async fn run_execution(
    guard: &mut AcpExecutionGuard,
    definition: &AgentDefinition,
    request: &AiExecutionRequest,
    started: Instant,
) -> Result<AiExecutionResult, AiExecutionError> {
    request.report_phase(AiExecutionPhase::Spawning);
    let process = ManagedAgentProcess::spawn(
        definition,
        Some(&guard.workspace),
        request.limits.stderr_bytes,
    )
    .await
    .map_err(|error| map_process_error(definition, error))?;
    log_info(
        "ai_execution.process",
        "AI agent process started",
        &[
            ("execution_id", request.execution_id.clone()),
            ("agent_id", definition.id.to_string()),
            ("protocol", "acp".to_string()),
            ("phase", "spawning".to_string()),
            ("pid", process.process_id().to_string()),
            ("arg_count", definition.args.len().to_string()),
            ("env_key_count", definition.env.len().to_string()),
            ("cwd_kind", "ephemeral".to_string()),
        ],
    );
    guard.process = Some(process);

    let (stdin, stdout) = guard
        .process
        .as_ref()
        .expect("process stored before stdio")
        .take_stdio()
        .await
        .map_err(|_| AiExecutionError::Protocol {
            operation: "take_stdio",
        })?;
    let mut config = AcpConnectConfig::new(request.limits.initialize_timeout);
    config.event_channel_capacity = PROTOCOL_EVENT_CAPACITY;
    request.report_phase(AiExecutionPhase::Initializing);
    let (protocol, channels) = AcpProtocol::connect(stdin, stdout, config)
        .await
        .map_err(|error| map_acp_error("initialize", error))?;
    guard.protocol = Some(protocol);

    request.report_phase(AiExecutionPhase::CreatingSession);
    let session = guard
        .protocol
        .as_ref()
        .expect("protocol stored before session")
        .new_session(guard.workspace.clone())
        .await
        .map_err(|error| map_acp_error("session_new", error))?
        .session_id;
    guard.session_id = Some(session.clone());

    if let Some(model) = request.model.as_deref() {
        request.report_phase(AiExecutionPhase::Configuring);
        guard
            .protocol
            .as_ref()
            .expect("protocol stored before model")
            .set_model(
                session.clone(),
                model.trim(),
                request.limits.config_rpc_timeout,
            )
            .await
            .map_err(|_| AiExecutionError::ModelSelectionFailed)?;
    }

    request.report_phase(AiExecutionPhase::Prompting);
    let text = run_prompt_and_aggregate(
        guard
            .protocol
            .as_ref()
            .expect("protocol stored before prompt"),
        guard
            .process
            .as_ref()
            .expect("process stored before prompt"),
        channels,
        session,
        request,
        definition,
    )
    .await?;

    Ok(AiExecutionResult {
        text,
        agent_id: definition.id.clone(),
        protocol: AgentProtocol::Acp,
        requested_model: request.model.clone(),
        elapsed_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    })
}

async fn run_prompt_and_aggregate(
    protocol: &AcpProtocol,
    process: &ManagedAgentProcess,
    channels: AcpProtocolChannels,
    session_id: SessionId,
    request: &AiExecutionRequest,
    definition: &AgentDefinition,
) -> Result<String, AiExecutionError> {
    let AcpProtocolChannels {
        mut events,
        mut disconnects,
    } = channels;
    let mut aggregator =
        TranslationTextAggregator::new(session_id.clone(), request.limits.text_bytes);
    let mut prompt =
        Box::pin(protocol.prompt(session_id.clone(), request.prompt.trim().to_owned()));
    let mut process_exit = Box::pin(process.wait_for_exit());
    let cancellation = request.cancellation.cancelled();
    tokio::pin!(cancellation);
    let mut prompt_response = None;

    loop {
        tokio::select! {
            response = &mut prompt, if prompt_response.is_none() => {
                prompt_response = Some(response.map_err(|error| map_acp_error("prompt", error))?);
            }
            event = events.recv() => {
                let Some(event) = event else {
                    return Err(AiExecutionError::Protocol { operation: "event_stream" });
                };
                match aggregator.apply(event) {
                    AggregatorAction::Continue => {}
                    AggregatorAction::CancelAndFail(error) => {
                        let _ = protocol.cancel(session_id.clone());
                        return Err(error);
                    }
                    AggregatorAction::Complete { stop_reason } => {
                        let response = match prompt_response.take() {
                            Some(response) => response,
                            None => prompt.await.map_err(|error| map_acp_error("prompt", error))?,
                        };
                        if response.stop_reason != stop_reason {
                            return Err(AiExecutionError::Protocol { operation: "prompt_completion" });
                        }
                        let diagnostics = aggregator.diagnostics();
                        let outcome = match stop_reason {
                            StopReason::EndTurn | StopReason::MaxTokens | StopReason::MaxTurnRequests => {
                                aggregator.finish()
                            }
                            StopReason::Cancelled => Err(cancelled_error(definition)),
                            StopReason::Refusal => Err(AiExecutionError::Protocol { operation: "prompt_refused" }),
                            _ => Err(AiExecutionError::Protocol { operation: "prompt_stopped" }),
                        };
                        let text_bytes = outcome.as_ref().map(|text| text.len()).unwrap_or_default();
                        log_info(
                            "ai_execution.output",
                            "AI execution output aggregated",
                            &[
                                ("execution_id", request.execution_id.clone()),
                                ("agent_id", definition.id.to_string()),
                                ("protocol", "acp".to_string()),
                                ("phase", "prompting".to_string()),
                                ("text_bytes", text_bytes.to_string()),
                                ("chunk_count", diagnostics.0.to_string()),
                                ("thinking_chunk_count", diagnostics.1.to_string()),
                                ("ignored_session_event_count", diagnostics.2.to_string()),
                                ("stop_reason", format!("{stop_reason:?}").to_ascii_lowercase()),
                            ],
                        );
                        return outcome;
                    }
                }
            }
            _ = &mut cancellation => {
                let _ = protocol.cancel(session_id.clone());
                return Err(cancelled_error(definition));
            }
            exit = &mut process_exit => {
                return Err(AiExecutionError::AgentExited {
                    code: exit.and_then(|exit| exit.code),
                });
            }
            changed = disconnects.changed() => {
                if changed.is_err() || disconnects.borrow().is_some() {
                    return Err(AiExecutionError::Protocol { operation: "disconnect" });
                }
            }
        }
    }
}

fn create_workspace(root: &Path) -> Result<PathBuf, AiExecutionError> {
    fs::create_dir_all(root).map_err(|_| AiExecutionError::Workspace {
        operation: "create_root",
    })?;
    let workspace = root.join(format!("execution-{}", Uuid::new_v4()));
    fs::create_dir(&workspace).map_err(|_| AiExecutionError::Workspace {
        operation: "create",
    })?;
    if !workspace.is_absolute() {
        let _ = fs::remove_dir_all(&workspace);
        return Err(AiExecutionError::Workspace {
            operation: "require_absolute_path",
        });
    }
    Ok(workspace)
}

fn map_process_error(
    definition: &AgentDefinition,
    error: ManagedAgentProcessError,
) -> AiExecutionError {
    match error {
        ManagedAgentProcessError::ExecutableNotFound { command_name } => {
            AiExecutionError::RuntimeUnavailable { command_name }
        }
        other => AiExecutionError::Spawn {
            program: PathBuf::from(&definition.command),
            message: other.to_string(),
        },
    }
}

fn map_acp_error(operation: &'static str, _error: AcpError) -> AiExecutionError {
    AiExecutionError::Protocol { operation }
}

fn cancelled_error(definition: &AgentDefinition) -> AiExecutionError {
    AiExecutionError::Cancelled {
        program: PathBuf::from(&definition.command),
        stdout: Vec::new(),
        stderr: Vec::new(),
        stdout_truncated: false,
        stderr_truncated: false,
    }
}

fn timeout_error(definition: &AgentDefinition, timeout: std::time::Duration) -> AiExecutionError {
    AiExecutionError::Timeout {
        program: PathBuf::from(&definition.command),
        timeout,
        stdout: Vec::new(),
        stderr: Vec::new(),
        stdout_truncated: false,
        stderr_truncated: false,
    }
}

struct AcpExecutionGuard {
    workspace: PathBuf,
    process: Option<ManagedAgentProcess>,
    protocol: Option<AcpProtocol>,
    session_id: Option<SessionId>,
    cleaned: bool,
}

impl AcpExecutionGuard {
    fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            process: None,
            protocol: None,
            session_id: None,
            cleaned: false,
        }
    }

    async fn cleanup(
        &mut self,
        cancel_before_close: bool,
        request: &AiExecutionRequest,
    ) -> CleanupReport {
        if self.cleaned {
            return CleanupReport::already_cleaned();
        }
        self.cleaned = true;
        let mut report = CleanupReport::default();

        if let Some(protocol) = self.protocol.as_ref() {
            if cancel_before_close {
                if let Some(session_id) = self.session_id.clone() {
                    if protocol.cancel(session_id).is_err() {
                        report.failures.push("cancel".to_owned());
                    }
                }
            }
            if let Some(session_id) = self.session_id.clone() {
                match tokio::time::timeout(
                    request.limits.close_timeout,
                    protocol.close_session(session_id),
                )
                .await
                {
                    Ok(Ok(_)) => {}
                    Ok(Err(_)) => report.failures.push("close".to_owned()),
                    Err(_) => report.failures.push("close_timeout".to_owned()),
                }
            }
            if protocol
                .shutdown(request.limits.close_timeout)
                .await
                .is_err()
            {
                report.failures.push("protocol_shutdown".to_owned());
            }
        }
        self.protocol.take();

        if let Some(process) = self.process.as_ref() {
            report.process_id = Some(process.process_id());
            let termination = process.terminate(request.limits.cancel_grace).await;
            report.process_reaped = termination.exit.is_some();
            report.exit_code = termination.exit.as_ref().and_then(|exit| exit.code);
            if !termination.signal_errors.is_empty() {
                report.failures.push("process_signal".to_owned());
            }
            if !report.process_reaped {
                report.failures.push("process_reap".to_owned());
            }
            if !process
                .wait_for_stderr_eof(request.limits.close_timeout)
                .await
            {
                report.failures.push("stderr_join".to_owned());
            }
            match process.stderr_tail() {
                Ok(stderr) => {
                    report.stderr_bytes = stderr.bytes.len();
                    report.stderr_truncated = stderr.truncated;
                    if stderr.read_error {
                        report.failures.push("stderr_read".to_owned());
                    }
                }
                Err(_) => report.failures.push("stderr_state".to_owned()),
            }
        } else {
            report.process_reaped = true;
        }
        self.process.take();

        report.workspace_removed = match fs::remove_dir_all(&self.workspace) {
            Ok(()) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(_) => {
                report.failures.push("workspace_remove".to_owned());
                false
            }
        };
        report
    }
}

#[derive(Default, Debug)]
struct CleanupReport {
    failures: Vec<String>,
    process_id: Option<u32>,
    process_reaped: bool,
    workspace_removed: bool,
    stderr_bytes: usize,
    stderr_truncated: bool,
    exit_code: Option<i32>,
}

impl CleanupReport {
    fn already_cleaned() -> Self {
        Self {
            process_reaped: true,
            workspace_removed: true,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{
        agents::types::{AgentEnvEntry, AgentId, DeclaredAgentCapabilities},
        ai_execution::{AiExecutionCancellation, AiExecutionLimits, AiExecutionPurpose},
    };
    use std::time::Duration;

    fn definition(mode: &str, record_path: &Path) -> AgentDefinition {
        AgentDefinition {
            id: AgentId::parse("fake-acp").unwrap(),
            display_name: "Fake ACP".to_owned(),
            protocol: AgentProtocol::Acp,
            command: "node".to_owned(),
            args: vec![Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("test-fixtures/fake-acp-agent.mjs")
                .to_string_lossy()
                .into_owned()],
            env: vec![
                AgentEnvEntry::new("ASSETIWEAVE_FAKE_ACP_MODE", mode),
                AgentEnvEntry::new(
                    "ASSETIWEAVE_FAKE_ACP_RECORD_PATH",
                    record_path.to_string_lossy(),
                ),
            ],
            declared_capabilities: DeclaredAgentCapabilities::acp_text(),
            availability_probe: None,
            model_discovery: None,
        }
    }

    fn request(model: Option<&str>) -> AiExecutionRequest {
        AiExecutionRequest {
            execution_id: Uuid::new_v4().to_string(),
            agent_id: AgentId::parse("fake-acp").unwrap(),
            purpose: AiExecutionPurpose::Translation,
            prompt: "translate fixture".to_owned(),
            model: model.map(str::to_owned),
            limits: AiExecutionLimits {
                initialize_timeout: Duration::from_millis(300),
                config_rpc_timeout: Duration::from_millis(100),
                cancel_grace: Duration::from_millis(100),
                close_timeout: Duration::from_millis(100),
                text_bytes: 1024,
                stderr_bytes: 1024,
                ..AiExecutionLimits::default()
            },
            cancellation: AiExecutionCancellation::default(),
            progress: None,
        }
    }

    fn test_paths(name: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("assetiweave-acp-{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let record = root.join("record.ndjson");
        (root, record)
    }

    fn records(path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_default()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn life_01_happy_stdio_flow_closes_reaps_and_removes_workspace() {
        let (root, record) = test_paths("happy");
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());

        let result = backend
            .execute(&definition("happy", &record), request(Some("vendor/model")))
            .await
            .expect("happy execution");

        assert_eq!(result.text, "translated");
        assert_eq!(result.requested_model.as_deref(), Some("vendor/model"));
        assert_eq!(result.protocol, AgentProtocol::Acp);
        let record = records(&record);
        assert!(record.contains("\"event\":\"initialize\""));
        assert!(record.contains("\"event\":\"new\""));
        assert!(record.contains("\"mcpCount\":0"));
        assert!(record.contains("\"event\":\"model\""));
        assert!(record.contains("\"event\":\"prompt\""));
        assert!(record.contains("\"event\":\"close\""));
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn chunked_wrong_session_thinking_and_late_chunk_modes_are_aggregated() {
        for (mode, expected) in [
            ("chunked", "你好🌍"),
            ("wrong_session", "right"),
            ("thinking", "visible"),
            ("late_chunk", "before late"),
        ] {
            let (root, record) = test_paths(mode);
            let backend = AcpExecutionBackend::new(root.join("workspaces"));
            let result = backend
                .execute(&definition(mode, &record), request(None))
                .await
                .unwrap();
            assert_eq!(result.text, expected, "mode {mode}");
            let _ = fs::remove_dir_all(root);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn init_new_and_model_failures_still_cleanup_and_model_never_prompts() {
        for mode in [
            "initialize_error",
            "initialize_timeout",
            "new_error",
            "model_reject",
            "model_timeout",
        ] {
            let (root, record) = test_paths(mode);
            let workspace_root = root.join("workspaces");
            let backend = AcpExecutionBackend::new(workspace_root.clone());
            let result = backend
                .execute(&definition(mode, &record), request(Some("vendor/model")))
                .await;
            assert!(result.is_err(), "mode {mode}");
            if mode.starts_with("model_") {
                assert!(!records(&record).contains("\"event\":\"prompt\""));
            }
            assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
            let _ = fs::remove_dir_all(root);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn permission_tool_empty_and_output_limit_fail_closed_and_cleanup() {
        for mode in ["permission", "tool_call", "empty", "oversized"] {
            let (root, record) = test_paths(mode);
            let workspace_root = root.join("workspaces");
            let backend = AcpExecutionBackend::new(workspace_root.clone());
            let mut execution_request = request(None);
            execution_request.limits.text_bytes = 64;
            let result = backend
                .execute(&definition(mode, &record), execution_request)
                .await;
            match mode {
                "permission" => assert!(matches!(result, Err(AiExecutionError::PermissionDenied))),
                "tool_call" => assert!(matches!(result, Err(AiExecutionError::ToolUseDenied))),
                "empty" => assert!(matches!(result, Err(AiExecutionError::EmptyOutput { .. }))),
                "oversized" => assert!(matches!(result, Err(AiExecutionError::OutputLimit { .. }))),
                _ => unreachable!(),
            }
            if matches!(mode, "permission" | "tool_call" | "oversized") {
                assert!(records(&record).contains("\"event\":\"cancel\""));
            }
            assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
            let _ = fs::remove_dir_all(root);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn user_cancel_reaches_in_flight_agent_before_terminal_cleanup() {
        let (root, record) = test_paths("cancel");
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());
        let execution_request = request(None);
        let cancellation = execution_request.cancellation.clone();
        let definition = definition("cancel_wait", &record);

        let execution = backend.execute(&definition, execution_request);
        tokio::pin!(execution);
        loop {
            tokio::select! {
                result = &mut execution => panic!("execution completed before cancellation: {result:?}"),
                _ = async {
                    if records(&record).contains("\"event\":\"prompt\"") {
                        return;
                    }
                    tokio::task::yield_now().await;
                } => {
                    if records(&record).contains("\"event\":\"prompt\"") {
                        break;
                    }
                }
            }
        }
        cancellation.cancel();
        let result = tokio::time::timeout(Duration::from_secs(2), &mut execution)
            .await
            .expect("cancel cleanup timeout");

        assert!(matches!(result, Err(AiExecutionError::Cancelled { .. })));
        assert!(records(&record).contains("\"event\":\"cancel\""));
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn total_timeout_cancels_in_flight_prompt_and_waits_for_cleanup() {
        let (root, record) = test_paths("total-timeout");
        let workspace_root = root.join("workspaces");
        let backend = AcpExecutionBackend::new(workspace_root.clone());
        let mut execution_request = request(None);
        execution_request.limits.total_timeout = Duration::from_millis(500);

        let result = backend
            .execute(&definition("cancel_wait", &record), execution_request)
            .await;

        assert!(matches!(result, Err(AiExecutionError::Timeout { .. })));
        assert!(records(&record).contains("\"event\":\"cancel\""));
        assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn close_failure_never_returns_success_and_process_is_reaped() {
        for mode in ["close_error", "close_hang"] {
            let (root, record) = test_paths(mode);
            let workspace_root = root.join("workspaces");
            let backend = AcpExecutionBackend::new(workspace_root.clone());
            let result = backend
                .execute(&definition(mode, &record), request(None))
                .await;

            assert!(matches!(
                result,
                Err(AiExecutionError::CleanupFailed { .. })
            ));
            assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
            let _ = fs::remove_dir_all(root);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn disconnect_and_process_exit_are_classified_and_cleaned() {
        for mode in ["disconnect", "exit_during_prompt"] {
            let (root, record) = test_paths(mode);
            let workspace_root = root.join("workspaces");
            let backend = AcpExecutionBackend::new(workspace_root.clone());
            let result = backend
                .execute(&definition(mode, &record), request(None))
                .await;

            assert!(matches!(
                result,
                Err(AiExecutionError::Protocol { .. } | AiExecutionError::AgentExited { .. })
            ));
            assert_eq!(fs::read_dir(&workspace_root).unwrap().count(), 0);
            let _ = fs::remove_dir_all(root);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cleanup_is_idempotent() {
        let (root, _record) = test_paths("cleanup-repeat");
        let workspace = create_workspace(&root.join("workspaces")).unwrap();
        let mut guard = AcpExecutionGuard::new(workspace);
        let execution_request = request(None);

        let first = guard.cleanup(false, &execution_request).await;
        let second = guard.cleanup(false, &execution_request).await;

        assert!(first.workspace_removed);
        assert!(second.workspace_removed);
        assert!(second.process_reaped);
        assert!(second.failures.is_empty());
        let _ = fs::remove_dir_all(root);
    }
}
