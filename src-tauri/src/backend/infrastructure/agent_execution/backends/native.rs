use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use tokio::io::AsyncReadExt;
use uuid::Uuid;

pub(crate) use super::native_runner::*;

use crate::backend::infrastructure::agent_execution::managed_process as agent_process;
use crate::backend::infrastructure::host_process::resolve_host_executable;
use crate::backend::{
    domain::agents::{
        AgentDefinition, AgentProtocol, PersistentExecutionBinding, SESSION_ID_PLACEHOLDER,
    },
    infrastructure::agent_execution::{
        AgentModelOption, AgentSessionMode, AiExecutionCleanupReport, AiExecutionError,
        AiExecutionPhase, AiExecutionRequest, AiExecutionResult, SessionCleanupStatus,
    },
    infrastructure::extensions::{
        EnvEntry, ExtensionLauncher, InvocationLimits, ProbeKind, ProbeSpec, ProcessInvocation,
        RuntimeProgramKind,
    },
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

        tracing::info!(
            action = "ai_execution.cleanup",
            execution_id = %request.execution_id,
            agent_id = %definition.id,
            protocol = "native",
            phase = "cleaning_up",
            process_reaped = %cleanup.process_reaped,
            workspace_removed = %cleanup.workspace_removed,
            "Native execution cleanup completed"
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
                crate::backend::infrastructure::host_process::HostInput::Null,
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
        let models = parse_native_models(&stdout);
        let current_model_id = models.first().map(|m| m.id.clone());
        Ok((models, current_model_id))
    }
}

pub(crate) fn parse_native_models(stdout: &str) -> Vec<AgentModelOption> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
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

pub(crate) use super::native_runner::*;

#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;
