use std::{path::PathBuf, time::Duration};

use tokio_util::sync::CancellationToken;

use crate::backend::host_process::{
    run_host_command, HostCommandOutput, HostCommandSpec, HostInput, HostProcessError,
};

use super::ExtensionError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeProgramKind {
    Node,
    Python,
    Bash,
    Executable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EnvEntry {
    pub(crate) key: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessInvocation {
    pub(crate) kind: RuntimeProgramKind,
    pub(crate) entry: String,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<EnvEntry>,
    pub(crate) working_dir: Option<PathBuf>,
    pub(crate) version_req: Option<String>,
    pub(crate) immutable_install_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProbeKind {
    Availability,
    ModelDiscovery,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProbeSpec {
    pub(crate) program: Option<String>,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<EnvEntry>,
    pub(crate) timeout: Duration,
    pub(crate) output_limit: usize,
    pub(crate) kind: ProbeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProbeResult {
    pub(crate) program: String,
    pub(crate) available: bool,
    pub(crate) version: Option<String>,
    pub(crate) required_version: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) hint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InvocationLimits {
    pub(crate) timeout: Duration,
    pub(crate) stdout_limit: usize,
    pub(crate) stderr_limit: usize,
}

#[derive(Debug)]
pub(crate) struct InvocationResult {
    pub(crate) status: std::process::ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) stdout_truncated: bool,
    pub(crate) stderr_truncated: bool,
    pub(crate) elapsed: Duration,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ExtensionLauncher;

impl ExtensionLauncher {
    pub(crate) async fn invoke(
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
        Ok(InvocationResult::from(output))
    }

    pub(crate) async fn probe(
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
        let version =
            first_nonempty_line(&output.stdout).or_else(|| first_nonempty_line(&output.stderr));
        let available =
            output.status.success() && !output.stdout_truncated && !output.stderr_truncated;
        Ok(ProbeResult {
            program,
            available,
            version,
            required_version: invocation.version_req.clone(),
            error: (!available).then(|| {
                if output.stdout_truncated || output.stderr_truncated {
                    "probe output exceeded the configured limit".to_string()
                } else {
                    format!("probe exited with status {}", output.status)
                }
            }),
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
    ExtensionError::LaunchFailed {
        package_id: invocation.entry.clone(),
        reason: host_process_error_message(error),
    }
}

fn probe_error(program: &str, error: HostProcessError) -> ExtensionError {
    ExtensionError::ProbeFailed {
        package_id: program.to_string(),
        reason: host_process_error_message(error),
    }
}

fn host_process_error_message(error: HostProcessError) -> String {
    match error {
        HostProcessError::Spawn(reason) | HostProcessError::Output(reason) => reason,
        HostProcessError::Timeout { .. } => "process deadline exceeded".to_string(),
        HostProcessError::Cancelled { .. } => "process cancelled".to_string(),
    }
}

fn first_nonempty_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}
