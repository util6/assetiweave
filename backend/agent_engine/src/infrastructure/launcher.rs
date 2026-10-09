use std::{path::PathBuf, time::Duration};

use tokio_util::sync::CancellationToken;

use crate::infrastructure::host_process::{
    run_host_command, HostCommandOutput, HostCommandSpec, HostInput, HostProcessError,
};

use super::launcher_error::ExtensionError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeProgramKind {
    Node,
    Python,
    Bash,
    Executable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvEntry {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInvocation {
    pub kind: RuntimeProgramKind,
    pub entry: String,
    pub args: Vec<String>,
    pub env: Vec<EnvEntry>,
    pub working_dir: Option<PathBuf>,
    pub version_req: Option<String>,
    pub immutable_install_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeKind {
    Availability,
    ModelDiscovery,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeSpec {
    pub program: Option<String>,
    pub args: Vec<String>,
    pub env: Vec<EnvEntry>,
    pub timeout: Duration,
    pub output_limit: usize,
    pub kind: ProbeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeResult {
    pub program: String,
    pub available: bool,
    pub version: Option<String>,
    pub required_version: Option<String>,
    pub error: Option<String>,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvocationLimits {
    pub timeout: Duration,
    pub stdout_limit: usize,
    pub stderr_limit: usize,
}

#[derive(Debug)]
pub struct InvocationResult {
    pub status: std::process::ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub elapsed: Duration,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ExtensionLauncher;

impl ExtensionLauncher {
    pub async fn invoke(
        &self,
        invocation: &ProcessInvocation,
        input: HostInput,
        limits: InvocationLimits,
        cancellation: CancellationToken,
    ) -> Result<InvocationResult, ExtensionError> {
        let output = run_host_command(
            HostCommandSpec {
                program: PathBuf::from(&invocation.entry),
                args: invocation.args.clone(),
                env: invocation
                    .env
                    .iter()
                    .map(|entry| (entry.key.clone(), entry.value.clone()))
                    .collect(),
                working_dir: invocation.working_dir.clone(),
                stdin: input,
                timeout: limits.timeout,
                stdout_limit: limits.stdout_limit,
                stderr_limit: limits.stderr_limit,
            },
            cancellation,
        )
        .await
        .map_err(|error| launch_error(invocation, error))?;
        if let Some(error) = output.output_limit_error() {
            return Err(launch_error(invocation, error));
        }
        if !output.status.success() {
            return Err(ExtensionError::NonZeroExit {
                package_id: package_label(invocation),
                status: output.status.code(),
            });
        }
        Ok(InvocationResult::from(output))
    }

    pub async fn probe(
        &self,
        invocation: &ProcessInvocation,
        probe: &ProbeSpec,
        cancellation: CancellationToken,
    ) -> Result<ProbeResult, ExtensionError> {
        let program = probe
            .program
            .clone()
            .unwrap_or_else(|| invocation.entry.clone());
        let output = run_host_command(
            HostCommandSpec {
                program: PathBuf::from(&program),
                args: probe.args.clone(),
                env: probe
                    .env
                    .iter()
                    .map(|entry| (entry.key.clone(), entry.value.clone()))
                    .collect(),
                working_dir: invocation.working_dir.clone(),
                stdin: HostInput::Null,
                timeout: probe.timeout,
                stdout_limit: probe.output_limit,
                stderr_limit: probe.output_limit,
            },
            cancellation,
        )
        .await
        .map_err(|error| probe_error(&program, error))?;
        if let Some(error) = output.output_limit_error() {
            return Err(probe_error(&program, error));
        }
        if !output.status.success() {
            return Err(ExtensionError::NonZeroExit {
                package_id: safe_program_label(&program),
                status: output.status.code(),
            });
        }
        let version =
            first_nonempty_line(&output.stdout).or_else(|| first_nonempty_line(&output.stderr));
        let available = true;
        Ok(ProbeResult {
            program,
            available,
            version,
            required_version: invocation.version_req.clone(),
            error: None,
            hint: None,
        })
    }
}

impl From<HostCommandOutput> for InvocationResult {
    fn from(output: HostCommandOutput) -> Self {
        Self {
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
            stdout_truncated: output.stdout_truncated,
            stderr_truncated: output.stderr_truncated,
            elapsed: output.elapsed,
        }
    }
}

fn launch_error(invocation: &ProcessInvocation, error: HostProcessError) -> ExtensionError {
    let package_id = package_label(invocation);
    match error {
        HostProcessError::MissingProgram { .. } => ExtensionError::ProgramNotFound { package_id },
        HostProcessError::Spawn(_) | HostProcessError::Output(_) => ExtensionError::LaunchFailed {
            package_id,
            reason: "host process could not be launched".to_string(),
        },
        HostProcessError::Timeout { .. } => ExtensionError::Timeout { package_id },
        HostProcessError::Cancelled => ExtensionError::Cancelled { package_id },
        HostProcessError::OutputLimitExceeded { stdout, stderr } => {
            ExtensionError::OutputLimitExceeded {
                package_id,
                stdout,
                stderr,
            }
        }
        HostProcessError::Cleanup(_) => ExtensionError::CleanupFailed {
            package_id,
            reason: "host process cleanup failed".to_string(),
        },
    }
}

fn probe_error(program: &str, error: HostProcessError) -> ExtensionError {
    let package_id = safe_program_label(program);
    match error {
        HostProcessError::MissingProgram { .. } => ExtensionError::ProgramNotFound { package_id },
        HostProcessError::Spawn(_) | HostProcessError::Output(_) => ExtensionError::ProbeFailed {
            package_id,
            reason: "host process probe could not be launched".to_string(),
        },
        HostProcessError::Timeout { .. } => ExtensionError::Timeout { package_id },
        HostProcessError::Cancelled => ExtensionError::Cancelled { package_id },
        HostProcessError::OutputLimitExceeded { stdout, stderr } => {
            ExtensionError::OutputLimitExceeded {
                package_id,
                stdout,
                stderr,
            }
        }
        HostProcessError::Cleanup(_) => ExtensionError::CleanupFailed {
            package_id,
            reason: "host process cleanup failed".to_string(),
        },
    }
}

fn package_label(invocation: &ProcessInvocation) -> String {
    safe_program_label(&invocation.entry)
}

fn safe_program_label(program: &str) -> String {
    std::path::Path::new(program)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("extension-program")
        .to_string()
}

fn first_nonempty_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
#[path = "launcher_tests.rs"]
mod tests;
