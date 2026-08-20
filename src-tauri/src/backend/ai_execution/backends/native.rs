use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use tokio::io::{AsyncBufReadExt, BufReader};
use uuid::Uuid;

use crate::backend::{
    agents::{
        process::{ManagedAgentProcess, ManagedAgentProcessError},
        types::{AgentDefinition, AgentModelOption, AgentProtocol},
    },
    ai_execution::{AiExecutionError, AiExecutionPhase, AiExecutionRequest, AiExecutionResult},
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
        let workspace = create_workspace(&self.workspace_root)?;
        let mut guard = NativeExecutionGuard::new(workspace);

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

        if !cleanup.process_reaped || !cleanup.workspace_removed {
            if outcome.is_ok() {
                return Err(AiExecutionError::CleanupFailed {
                    failures: vec!["process or workspace cleanup failed".to_string()],
                });
            }
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
                program: PathBuf::from(command_name),
                message: error.to_string(),
            })?;

        if result.available {
            Ok(())
        } else {
            Err(AiExecutionError::Output {
                program: PathBuf::from(command_name),
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
                program: PathBuf::from(command_name),
                message: error.to_string(),
            })?;

        if output.stdout_truncated || output.stderr_truncated {
            return Err(AiExecutionError::Output {
                program: PathBuf::from(command_name),
                message: "model discovery output exceeded the configured limit".to_string(),
            });
        }
        if !output.status.success() {
            return Err(AiExecutionError::Output {
                program: PathBuf::from(command_name),
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
}

impl NativeExecutionGuard {
    fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            process: None,
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
        let workspace_removed =
            fs::remove_dir_all(&self.workspace).is_ok() || !self.workspace.exists();
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

    let mut args = vec![
        "-p".to_string(),
        request.prompt.clone(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--dangerously-skip-permissions".to_string(),
        "--print-timeout".to_string(),
        "10m".to_string(),
    ];

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

    let mut lines = BufReader::new(stdout).lines();
    let mut accumulated_text = String::new();
    let mut result_response = String::new();
    let mut result_error: Option<String> = None;

    while let Ok(Some(line)) = lines.next_line().await {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let event = v.get("event").and_then(|e| e.as_str()).unwrap_or("");
            match event {
                "step_update" => {
                    if let Some(step) = v.get("step_update") {
                        let step_type =
                            step.get("step_type").and_then(|s| s.as_str()).unwrap_or("");
                        if step_type == "agent_response" {
                            if let Some(delta) = step.get("text_delta").and_then(|d| d.as_str()) {
                                accumulated_text.push_str(delta);
                            }
                        }
                    }
                }
                "result" => {
                    if let Some(res) = v.get("result") {
                        let status = res.get("status").and_then(|s| s.as_str()).unwrap_or("");
                        if status == "SUCCESS" {
                            if let Some(resp) = res.get("response").and_then(|r| r.as_str()) {
                                result_response = resp.to_string();
                            }
                        } else if status == "ERROR" {
                            let err = res
                                .get("error")
                                .and_then(|e| e.as_str())
                                .unwrap_or("agy execution returned an error");
                            result_error = Some(err.to_string());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if let Some(err) = result_error {
        return Err(AiExecutionError::Output {
            program: PathBuf::from(&definition.command),
            message: err,
        });
    }

    let text = if !accumulated_text.is_empty() {
        accumulated_text
    } else {
        result_response
    };

    if text.is_empty() {
        return Err(AiExecutionError::Output {
            program: PathBuf::from(&definition.command),
            message: "agent produced empty response text".to_string(),
        });
    }

    Ok(AiExecutionResult {
        text,
        agent_id: definition.id.clone(),
        protocol: AgentProtocol::Native,
        requested_model: request.model.clone(),
        elapsed_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    })
}

fn create_workspace(root: &Path) -> Result<PathBuf, AiExecutionError> {
    let workspace = root.join(format!("exec-{}", Uuid::new_v4()));
    fs::create_dir_all(&workspace).map_err(|error| AiExecutionError::Output {
        program: workspace.clone(),
        message: format!("could not create execution workspace: {error}"),
    })?;
    Ok(workspace)
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
}
