use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use tokio::io::AsyncReadExt;
use uuid::Uuid;

use crate::backend::{
    agents::{
        process::{ManagedAgentProcess, ManagedAgentProcessError},
        types::{AgentDefinition, AgentModelOption, AgentProtocol, SESSION_ID_PLACEHOLDER},
    },
    ai_execution::{
        AgentSessionMode, AiExecutionCleanupReport, AiExecutionError, AiExecutionPhase,
        AiExecutionRequest, AiExecutionResult,
    },
    extension_kernel::{
        EnvEntry, ExtensionLauncher, InvocationLimits, ProbeKind, ProbeSpec, ProcessInvocation,
        RuntimeProgramKind,
    },
    host_process::resolve_host_executable,
    operation_log::log_info,
};

pub(crate) struct NativeExecutionBackend {
    workspace_root: PathBuf,
}

impl NativeExecutionBackend {
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
        let workspace = if let Some(binding) = request.binding.as_ref() {
            let path = PathBuf::from(&binding.workspace_path);
            if !path.is_absolute() || !path.is_dir() {
                return Err(AiExecutionError::ResumeUnavailable);
            }
            path
        } else {
            create_workspace(&self.workspace_root)?
        };
        let mut guard = NativeExecutionGuard::new(workspace);
        guard.preserve_workspace = matches!(request.session_mode, AgentSessionMode::Persistent);

        let outcome = {
            let execution = run_native_execution(&mut guard, definition, &request, started);
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
        let cleanup = guard.cleanup().await;
        request.report_cleanup(AiExecutionCleanupReport {
            process_reaped: cleanup.process_reaped,
            workspace_removed: cleanup.workspace_removed,
            failure_count: usize::from(
                !cleanup.process_reaped
                    || (!cleanup.workspace_removed && !guard.preserve_workspace),
            ),
            session_closed: None,
            session_deleted: None,
            session_delete_method: None,
        });
        request.report_phase(AiExecutionPhase::CleaningUp);

        let cleanup_fields = vec![
            ("execution_id", request.execution_id.clone()),
            ("agent_id", definition.id.to_string()),
            ("protocol", "native".to_string()),
            ("phase", "cleaning_up".to_string()),
            ("process_reaped", cleanup.process_reaped.to_string()),
            ("workspace_removed", cleanup.workspace_removed.to_string()),
        ];
        log_info(
            "ai_execution.cleanup",
            "Native execution cleanup completed",
            &cleanup_fields,
        );

        if (!cleanup.process_reaped || (!cleanup.workspace_removed && !guard.preserve_workspace))
            && outcome.is_ok()
        {
            return Err(AiExecutionError::CleanupFailed {
                failures: vec!["process or workspace cleanup failed".to_string()],
            });
        }

        outcome
    }

    pub(crate) async fn check_connection(
        &self,
        definition: &AgentDefinition,
    ) -> Result<(), AiExecutionError> {
        let command_name = definition
            .availability_probe
            .as_ref()
            .and_then(|p| p.command.as_deref())
            .unwrap_or(&definition.command);
        let program = resolve_host_executable(command_name).ok_or_else(|| {
            AiExecutionError::RuntimeUnavailable {
                command_name: command_name.to_string(),
            }
        })?;

        let probe_args = definition
            .availability_probe
            .as_ref()
            .map(|p| p.args.clone())
            .unwrap_or_else(|| vec!["--version".to_string()]);

        let env = definition
            .env
            .iter()
            .map(|entry| EnvEntry {
                key: entry.name.clone(),
                value: entry.value.clone(),
            })
            .collect::<Vec<_>>();
        let invocation = ProcessInvocation {
            kind: RuntimeProgramKind::Executable,
            entry: program.to_string_lossy().to_string(),
            args: Vec::new(),
            env: env.clone(),
            working_dir: None,
            version_req: None,
            immutable_install_dir: PathBuf::from("."),
        };
        let probe = ProbeSpec {
            program: Some(program.to_string_lossy().to_string()),
            args: probe_args,
            env,
            timeout: std::time::Duration::from_secs(5),
            output_limit: 64 * 1024,
            kind: ProbeKind::Availability,
        };
        let result = ExtensionLauncher::default()
            .probe(
                &invocation,
                &probe,
                tokio_util::sync::CancellationToken::new(),
            )
            .await
            .map_err(|error| AiExecutionError::Output {
                message: error.to_string(),
            })?;

        if result.available {
            Ok(())
        } else {
            Err(AiExecutionError::Output {
                message: result
                    .error
                    .unwrap_or_else(|| "probe command exited with non-zero status".to_string()),
            })
        }
    }

    pub(crate) async fn discover_models(
        &self,
        definition: &AgentDefinition,
    ) -> Result<(Vec<AgentModelOption>, Option<String>), AiExecutionError> {
        let command_name = definition
            .model_discovery
            .as_ref()
            .and_then(|p| p.command.as_deref())
            .unwrap_or(&definition.command);
        let program = resolve_host_executable(command_name).ok_or_else(|| {
            AiExecutionError::RuntimeUnavailable {
                command_name: command_name.to_string(),
            }
        })?;

        let model_args = definition
            .model_discovery
            .as_ref()
            .map(|p| p.args.clone())
            .unwrap_or_else(|| vec!["models".to_string()]);

        let invocation = ProcessInvocation {
            kind: RuntimeProgramKind::Executable,
            entry: program.to_string_lossy().to_string(),
            args: model_args,
            env: definition
                .env
                .iter()
                .map(|entry| EnvEntry {
                    key: entry.name.clone(),
                    value: entry.value.clone(),
                })
                .collect(),
            working_dir: None,
            version_req: None,
            immutable_install_dir: PathBuf::from("."),
        };
        let output = ExtensionLauncher::default()
            .invoke(
                &invocation,
                crate::backend::host_process::HostInput::Null,
                InvocationLimits {
                    timeout: std::time::Duration::from_secs(5),
                    stdout_limit: 1024 * 1024,
                    stderr_limit: 64 * 1024,
                },
                tokio_util::sync::CancellationToken::new(),
            )
            .await
            .map_err(|error| AiExecutionError::Output {
                message: error.to_string(),
            })?;

        if output.stdout_truncated || output.stderr_truncated {
            return Err(AiExecutionError::Output {
                message: "model discovery output exceeded the configured limit".to_string(),
            });
        }
        if !output.status.success() {
            return Err(AiExecutionError::Output {
                message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        let _discovery_elapsed = output.elapsed;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let models = parse_agy_models(&stdout);
        let current_model_id = models.first().map(|m| m.id.clone());
        Ok((models, current_model_id))
    }
}

pub(crate) fn parse_agy_models(stdout: &str) -> Vec<AgentModelOption> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.eq_ignore_ascii_case("Fetching available models..."))
        .filter_map(|line| {
            let mut fields = line.splitn(2, '\t');
            let id = fields.next().unwrap_or("").trim();
            if !looks_like_model_id(id) {
                return None;
            }
            let display = fields
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(id);
            Some(AgentModelOption {
                id: id.to_owned(),
                label: display.to_owned(),
                description: None,
            })
        })
        .collect()
}

fn looks_like_model_id(token: &str) -> bool {
    !token.is_empty()
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
}

struct NativeExecutionGuard {
    workspace: PathBuf,
    process: Option<ManagedAgentProcess>,
    preserve_workspace: bool,
}

impl NativeExecutionGuard {
    fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            process: None,
            preserve_workspace: false,
        }
    }

    async fn cleanup(&mut self) -> NativeCleanupOutcome {
        let mut process_reaped = true;
        if let Some(process) = self.process.as_ref() {
            let termination = process.terminate(std::time::Duration::from_secs(2)).await;
            if termination.exit.is_none() {
                process_reaped = false;
            }
        }
        self.process.take();
        let workspace_removed = if self.preserve_workspace {
            false
        } else {
            fs::remove_dir_all(&self.workspace).is_ok() || !self.workspace.exists()
        };
        NativeCleanupOutcome {
            process_reaped,
            workspace_removed,
        }
    }
}

struct NativeCleanupOutcome {
    process_reaped: bool,
    workspace_removed: bool,
}

async fn run_native_execution(
    guard: &mut NativeExecutionGuard,
    definition: &AgentDefinition,
    request: &AiExecutionRequest,
    started: Instant,
) -> Result<AiExecutionResult, AiExecutionError> {
    request.report_phase(AiExecutionPhase::Spawning);

    let persistent_session_id = if matches!(
        request.session_mode,
        crate::backend::ai_execution::AgentSessionMode::Persistent
    ) {
        let Some(resume_args) = definition.declared_capabilities.resume_args.as_ref() else {
            return Err(AiExecutionError::ResumeUnavailable);
        };
        if resume_args.is_empty() || !definition.declared_capabilities.resume {
            return Err(AiExecutionError::ResumeUnavailable);
        }
        Some(
            request
                .binding
                .as_ref()
                .map(|binding| binding.provider_session_id.clone())
                .unwrap_or_else(|| format!("native-session-{}", Uuid::new_v4().simple())),
        )
    } else {
        None
    };
    let mut args = if let Some(session_id) = persistent_session_id.as_deref() {
        definition
            .declared_capabilities
            .resume_args
            .as_ref()
            .expect("persistent session id requires resume args")
            .iter()
            .map(|arg| {
                if arg == SESSION_ID_PLACEHOLDER {
                    session_id.to_string()
                } else {
                    arg.clone()
                }
            })
            .collect::<Vec<_>>()
    } else {
        vec![
            "-p".to_string(),
            request.prompt.clone(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--print-timeout".to_string(),
            "10m".to_string(),
        ]
    };

    if matches!(
        request.session_mode,
        crate::backend::ai_execution::AgentSessionMode::Persistent
    ) && !request.restore_only
    {
        args.push("-p".to_string());
        args.push(request.prompt.clone());
    }

    if let Some(model) = &request.model {
        if !model.trim().is_empty() {
            args.push("--model".to_string());
            args.push(model.trim().to_string());
        }
    }

    args.push("--add-dir".to_string());
    args.push(guard.workspace.to_string_lossy().into_owned());

    let mut run_definition = definition.clone();
    run_definition.args = args;

    let process = ManagedAgentProcess::spawn(
        &run_definition,
        Some(&guard.workspace),
        request.limits.stderr_bytes,
    )
    .await
    .map_err(|e| map_process_error(definition, e))?;

    guard.process = Some(process);

    let (stdin, stdout) = guard
        .process
        .as_ref()
        .expect("process exists")
        .take_stdio()
        .await
        .map_err(|_| AiExecutionError::Protocol {
            operation: "take_stdio",
        })?;

    // Close stdin so agy finishes when prompt turn finishes.
    drop(stdin);

    request.report_phase(AiExecutionPhase::Prompting);

    let mut stdout = stdout;
    let mut buffer = [0_u8; 16 * 1024];
    let mut pending_line = Vec::new();
    let mut output_bytes = 0_usize;
    let mut accumulated_text = String::new();
    let mut result_response = String::new();
    let mut result_error: Option<String> = None;

    loop {
        let read = stdout
            .read(&mut buffer)
            .await
            .map_err(|error| AiExecutionError::Output {
                message: format!("failed to read native agent output: {error}"),
            })?;
        if read == 0 {
            break;
        }
        output_bytes = output_bytes.saturating_add(read);
        if output_bytes > request.limits.text_bytes {
            return Err(AiExecutionError::OutputLimit {
                limit: request.limits.text_bytes,
            });
        }
        pending_line.extend_from_slice(&buffer[..read]);

        while let Some(newline) = pending_line.iter().position(|byte| *byte == b'\n') {
            let line = pending_line.drain(..=newline).collect::<Vec<_>>();
            process_native_line(
                &line[..line.len().saturating_sub(1)],
                &mut accumulated_text,
                &mut result_response,
                &mut result_error,
            )?;
        }
    }

    if !pending_line.is_empty() {
        process_native_line(
            &pending_line,
            &mut accumulated_text,
            &mut result_response,
            &mut result_error,
        )?;
    }

    let exit = guard
        .process
        .as_ref()
        .expect("process exists")
        .wait_for_exit()
        .await
        .ok_or(AiExecutionError::AgentExited { code: None })?;
    if !exit.success {
        if let Some(wait_error) = exit.wait_error {
            return Err(AiExecutionError::Output {
                message: format!("failed to wait for native agent: {wait_error}"),
            });
        }
        return Err(AiExecutionError::AgentExited { code: exit.code });
    }

    if let Some(err) = result_error {
        return Err(AiExecutionError::Output { message: err });
    }

    if request.restore_only {
        return Ok(AiExecutionResult {
            text: String::new(),
            agent_id: definition.id.clone(),
            protocol: AgentProtocol::Native,
            requested_model: request.model.clone(),
            elapsed_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            persistent_binding: persistent_session_id.map(|provider_session_id| {
                crate::backend::ai_execution::PersistentExecutionBinding {
                    tenant_id: request.tenant_id.clone().unwrap_or_default(),
                    execution_context_key: request
                        .execution_context_key
                        .clone()
                        .unwrap_or_default(),
                    provider_session_id,
                    agent_id: definition.id.to_string(),
                    installation_id: definition.installation_id.clone(),
                    model: request.model.clone(),
                    workspace_path: guard.workspace.to_string_lossy().into_owned(),
                    binding_version: 1,
                    provider_metadata_json: "{\"protocol\":\"native\"}".to_string(),
                }
            }),
            replay_text: None,
        });
    }

    let text = if !accumulated_text.is_empty() {
        accumulated_text
    } else {
        result_response
    };

    if text.is_empty() {
        return Err(AiExecutionError::Output {
            message: "agent produced empty response text".to_string(),
        });
    }

    Ok(AiExecutionResult {
        text,
        agent_id: definition.id.clone(),
        protocol: AgentProtocol::Native,
        requested_model: request.model.clone(),
        elapsed_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        persistent_binding: persistent_session_id.map(|provider_session_id| {
            crate::backend::ai_execution::PersistentExecutionBinding {
                tenant_id: request.tenant_id.clone().unwrap_or_default(),
                execution_context_key: request.execution_context_key.clone().unwrap_or_default(),
                provider_session_id,
                agent_id: definition.id.to_string(),
                installation_id: definition.installation_id.clone(),
                model: request.model.clone(),
                workspace_path: guard.workspace.to_string_lossy().into_owned(),
                binding_version: 1,
                provider_metadata_json: "{\"protocol\":\"native\"}".to_string(),
            }
        }),
        replay_text: None,
    })
}

fn process_native_line(
    line: &[u8],
    accumulated_text: &mut String,
    result_response: &mut String,
    result_error: &mut Option<String>,
) -> Result<(), AiExecutionError> {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let line = std::str::from_utf8(line).map_err(|error| AiExecutionError::Output {
        message: format!("native agent output was not valid UTF-8: {error}"),
    })?;
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let value = serde_json::from_str::<serde_json::Value>(trimmed).map_err(|error| {
        AiExecutionError::Output {
            message: format!("native agent emitted invalid JSON: {error}"),
        }
    })?;
    let event = value
        .get("event")
        .and_then(|event| event.as_str())
        .unwrap_or("");
    match event {
        "step_update" => {
            if let Some(step) = value.get("step_update") {
                let step_type = step
                    .get("step_type")
                    .and_then(|step_type| step_type.as_str())
                    .unwrap_or("");
                if is_native_tool_activity(step_type) {
                    return Err(if step_type.to_ascii_lowercase().contains("permission") {
                        AiExecutionError::PermissionDenied
                    } else {
                        AiExecutionError::ToolUseDenied
                    });
                }
                if step_type == "agent_response" {
                    if let Some(delta) = step.get("text_delta").and_then(|delta| delta.as_str()) {
                        accumulated_text.push_str(delta);
                    }
                }
            }
        }
        "permission" | "permission_request" | "permission_requested" => {
            return Err(AiExecutionError::PermissionDenied);
        }
        "tool_call" | "tool_use" | "tool_activity" => {
            return Err(AiExecutionError::ToolUseDenied);
        }
        "result" => {
            if let Some(result) = value.get("result") {
                let status = result
                    .get("status")
                    .and_then(|status| status.as_str())
                    .unwrap_or("");
                if status == "SUCCESS" {
                    if let Some(response) = result
                        .get("response")
                        .and_then(|response| response.as_str())
                    {
                        *result_response = response.to_string();
                    }
                } else if status == "ERROR" {
                    let error = result
                        .get("error")
                        .and_then(|error| error.as_str())
                        .unwrap_or("native agent execution returned an error");
                    *result_error = Some(error.to_string());
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn is_native_tool_activity(step_type: &str) -> bool {
    let step_type = step_type.to_ascii_lowercase();
    step_type.contains("tool") || step_type.contains("permission")
}

fn create_workspace(root: &Path) -> Result<PathBuf, AiExecutionError> {
    let workspace = root.join(format!("exec-{}", Uuid::new_v4()));
    fs::create_dir_all(&workspace).map_err(|error| AiExecutionError::Output {
        message: format!("could not create execution workspace: {error}"),
    })?;
    Ok(workspace)
}

fn cancelled_error(definition: &AgentDefinition) -> AiExecutionError {
    AiExecutionError::Cancelled {
        program: PathBuf::from(&definition.command),
    }
}

fn timeout_error(definition: &AgentDefinition, timeout: std::time::Duration) -> AiExecutionError {
    AiExecutionError::Timeout {
        program: PathBuf::from(&definition.command),
        timeout,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{
        agents::types::{AgentEnvEntry, AgentId, DeclaredAgentCapabilities},
        ai_execution::{AiExecutionCancellation, AiExecutionLimits, AiExecutionPurpose},
    };

    #[test]
    fn test_parse_agy_models_tsv() {
        let sample = "Fetching available models...\ngemini-3.7-flash-high\tGemini 3.7 Flash (High)\ngemini-3.7-flash-medium\tGemini 3.7 Flash (Medium)\nclaude-sonnet-4-6\tClaude Sonnet 4.6 (Thinking)\n";
        let models = parse_agy_models(sample);
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].id, "gemini-3.7-flash-high");
        assert_eq!(models[0].label, "Gemini 3.7 Flash (High)");
        assert_eq!(models[1].id, "gemini-3.7-flash-medium");
        assert_eq!(models[2].id, "claude-sonnet-4-6");
    }

    #[test]
    fn test_parse_agy_models_bare() {
        let sample = "gemini-3.7-flash-high\ngemini-3.7-flash-medium\n";
        let models = parse_agy_models(sample);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "gemini-3.7-flash-high");
        assert_eq!(models[0].label, "gemini-3.7-flash-high");
    }

    #[test]
    fn native_text_execution_rejects_tool_and_permission_events() {
        let mut text = String::new();
        let mut response = String::new();
        let mut error = None;

        assert!(matches!(
            process_native_line(
                br#"{"event":"tool_call"}"#,
                &mut text,
                &mut response,
                &mut error,
            ),
            Err(AiExecutionError::ToolUseDenied)
        ));
        assert!(matches!(
            process_native_line(
                br#"{"event":"permission_request"}"#,
                &mut text,
                &mut response,
                &mut error,
            ),
            Err(AiExecutionError::PermissionDenied)
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn persistent_native_resume_uses_declared_session_argument() {
        let root = std::env::temp_dir().join(format!("assetiweave-native-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let record = root.join("argv.ndjson");
        let definition = AgentDefinition {
            id: AgentId::parse("fake-native").unwrap(),
            installation_id: Some("fixture-installation".to_string()),
            display_name: "Fake Native".to_string(),
            protocol: AgentProtocol::Native,
            command: Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("test-fixtures/fake-native-agent")
                .to_string_lossy()
                .into_owned(),
            args: vec!["unused-launch-arg-is-replaced-by-the-native-invocation".to_string()],
            env: vec![AgentEnvEntry::new(
                "ASSETIWEAVE_FAKE_NATIVE_RECORD_PATH",
                record.to_string_lossy(),
            )],
            declared_capabilities: DeclaredAgentCapabilities::native_text_with_resume(vec![
                "--session".to_string(),
                "{session_id}".to_string(),
            ]),
            availability_probe: None,
            model_discovery: None,
            session_cleanup: None,
            session_cleanup_not_found_markers: Vec::new(),
        };
        let backend = NativeExecutionBackend::new(root.join("workspaces"));
        let request = |execution_id: &str| AiExecutionRequest {
            execution_id: execution_id.to_string(),
            agent_id: definition.id.clone(),
            purpose: AiExecutionPurpose::TeamTask,
            session_mode: AgentSessionMode::Persistent,
            prompt: "fixture prompt".to_string(),
            model: Some("fixture-model".to_string()),
            limits: AiExecutionLimits::default(),
            cancellation: AiExecutionCancellation::default(),
            progress: None,
            tenant_id: Some("tenant-fixture".to_string()),
            execution_context_key: Some("member-context".to_string()),
            binding: None,
            replay: false,
            restore_only: false,
            team_tools: None,
        };

        let first = backend
            .execute(&definition, request("native-first"))
            .await
            .expect("first native persistent execution");
        let binding = first
            .persistent_binding
            .clone()
            .expect("first native execution returns a binding");
        assert_eq!(first.text, "native fixture response");

        let mut resumed = request("native-second");
        resumed.binding = Some(binding.clone());
        let second = backend
            .execute(&definition, resumed)
            .await
            .expect("resumed native persistent execution");
        assert_eq!(second.text, "native fixture response");
        assert_eq!(
            second.persistent_binding.unwrap().provider_session_id,
            binding.provider_session_id
        );

        let mut restored = request("native-restore");
        restored.binding = Some(binding);
        restored.restore_only = true;
        let restored = backend
            .execute(&definition, restored)
            .await
            .expect("native restore probe");
        assert!(restored.text.is_empty());

        let records = fs::read_to_string(&record).unwrap();
        let records = records
            .split("--END--\n")
            .filter(|record| !record.is_empty())
            .map(|record| record.lines().map(str::to_string).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 3);
        let session = records[0][1].as_str();
        assert_eq!(records[1][1].as_str(), session);
        assert_eq!(records[2][1].as_str(), session);
        assert!(records[0].iter().any(|arg| arg == "fixture prompt"));
        assert!(!records[2].iter().any(|arg| arg == "fixture prompt"));

        let _ = fs::remove_dir_all(root);
    }
}
