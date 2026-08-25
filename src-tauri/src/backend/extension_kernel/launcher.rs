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
mod tests {
    use super::*;
    use crate::backend::runtime::AppError;

    fn invocation(entry: &str) -> ProcessInvocation {
        ProcessInvocation {
            kind: RuntimeProgramKind::Executable,
            entry: entry.to_string(),
            args: Vec::new(),
            env: Vec::new(),
            working_dir: None,
            version_req: None,
            immutable_install_dir: std::env::temp_dir(),
        }
    }

    async fn invoke_code(
        invocation: &ProcessInvocation,
        input_args: Vec<String>,
        limits: InvocationLimits,
        cancellation: CancellationToken,
    ) -> String {
        let mut invocation = invocation.clone();
        invocation.args = input_args;
        let error = ExtensionLauncher
            .invoke(&invocation, HostInput::Null, limits, cancellation)
            .await
            .expect_err("fixture must fail");
        AppError::from(error).view().code
    }

    #[tokio::test(flavor = "current_thread")]
    async fn host_process_failures_keep_distinct_extension_codes() {
        let missing = invoke_code(
            &invocation("/tmp/assetiweave-missing-program"),
            Vec::new(),
            InvocationLimits {
                timeout: Duration::from_secs(1),
                stdout_limit: 64,
                stderr_limit: 64,
            },
            CancellationToken::new(),
        )
        .await;
        assert_eq!(missing, "program_not_found");

        let timeout = invoke_code(
            &invocation("/bin/sh"),
            vec!["-c".into(), "sleep 2".into()],
            InvocationLimits {
                timeout: Duration::from_millis(50),
                stdout_limit: 64,
                stderr_limit: 64,
            },
            CancellationToken::new(),
        )
        .await;
        assert_eq!(timeout, "timeout");

        let cancelled = invoke_code(
            &invocation("/bin/sh"),
            vec!["-c".into(), "sleep 2".into()],
            InvocationLimits {
                timeout: Duration::from_secs(1),
                stdout_limit: 64,
                stderr_limit: 64,
            },
            {
                let token = CancellationToken::new();
                token.cancel();
                token
            },
        )
        .await;
        assert_eq!(cancelled, "cancelled");

        let output_limit = invoke_code(
            &invocation("/bin/sh"),
            vec!["-c".into(), "printf 1234567890".into()],
            InvocationLimits {
                timeout: Duration::from_secs(1),
                stdout_limit: 4,
                stderr_limit: 64,
            },
            CancellationToken::new(),
        )
        .await;
        assert_eq!(output_limit, "output_limit_exceeded");

        let nonzero = invoke_code(
            &invocation("/bin/sh"),
            vec!["-c".into(), "exit 7".into()],
            InvocationLimits {
                timeout: Duration::from_secs(1),
                stdout_limit: 64,
                stderr_limit: 64,
            },
            CancellationToken::new(),
        )
        .await;
        assert_eq!(nonzero, "nonzero_exit");
    }

    #[test]
    fn launch_error_maps_spawn_and_cleanup_without_reclassifying_them() {
        let invocation = invocation("/tmp/private-agent");
        assert_eq!(
            AppError::from(launch_error(
                &invocation,
                HostProcessError::Spawn("permission denied".to_string()),
            ))
            .view()
            .code,
            "launch_failed"
        );
        assert_eq!(
            AppError::from(launch_error(
                &invocation,
                HostProcessError::Cleanup("cleanup failed".to_string()),
            ))
            .view()
            .code,
            "cleanup_failed"
        );
    }
}
