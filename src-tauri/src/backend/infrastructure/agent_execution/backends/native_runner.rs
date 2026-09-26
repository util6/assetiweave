use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use tokio::io::AsyncReadExt;
use uuid::Uuid;

use crate::backend::infrastructure::agent_execution::managed_process as agent_process;
use crate::backend::{
    domain::agents::{
        AgentDefinition, AgentProtocol, PersistentExecutionBinding, SESSION_ID_PLACEHOLDER,
    },
    infrastructure::agent_execution::{
        AgentSessionMode, AiExecutionError, AiExecutionPhase, AiExecutionRequest,
        AiExecutionResult, SessionCleanupStatus,
    },
};

pub(crate) struct NativeExecutionGuard {
    pub(super) workspace: PathBuf,
    pub(super) process: Option<agent_process::ManagedAgentProcess>,
    pub(super) preserve_workspace: bool,
}

impl NativeExecutionGuard {
    pub(crate) fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            process: None,
            preserve_workspace: false,
        }
    }

    pub(crate) async fn cleanup(&mut self) -> NativeCleanupOutcome {
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

pub(crate) struct NativeCleanupOutcome {
    pub(crate) process_reaped: bool,
    pub(crate) workspace_removed: bool,
}

pub(crate) async fn run_native_execution(
    guard: &mut NativeExecutionGuard,
    definition: &AgentDefinition,
    request: &AiExecutionRequest,
    started: Instant,
) -> Result<AiExecutionResult, AiExecutionError> {
    request.report_phase(AiExecutionPhase::Spawning);

    let persistent_session_id = if matches!(request.session_mode, AgentSessionMode::Persistent) {
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

    if matches!(request.session_mode, AgentSessionMode::Persistent) && !request.restore_only {
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

    let process = agent_process::ManagedAgentProcess::spawn(
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
                PersistentExecutionBinding {
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
            session_cleanup: SessionCleanupStatus::Skipped,
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
            PersistentExecutionBinding {
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
        session_cleanup: SessionCleanupStatus::Skipped,
    })
}

pub(crate) fn process_native_line(
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

pub(crate) fn create_workspace(root: &Path) -> Result<PathBuf, AiExecutionError> {
    let workspace = root.join(format!("exec-{}", Uuid::new_v4()));
    fs::create_dir_all(&workspace).map_err(|error| AiExecutionError::Output {
        message: format!("could not create execution workspace: {error}"),
    })?;
    Ok(workspace)
}

pub(crate) fn cancelled_error(definition: &AgentDefinition) -> AiExecutionError {
    AiExecutionError::Cancelled {
        program: PathBuf::from(&definition.command),
    }
}

pub(crate) fn timeout_error(
    definition: &AgentDefinition,
    timeout: std::time::Duration,
) -> AiExecutionError {
    AiExecutionError::Timeout {
        program: PathBuf::from(&definition.command),
        timeout,
    }
}

pub(crate) fn map_process_error(
    definition: &AgentDefinition,
    error: agent_process::ManagedAgentProcessError,
) -> AiExecutionError {
    match error {
        agent_process::ManagedAgentProcessError::ExecutableNotFound { command_name } => {
            AiExecutionError::RuntimeUnavailable { command_name }
        }
        other => AiExecutionError::Spawn {
            program: PathBuf::from(&definition.command),
            message: other.to_string(),
        },
    }
}
