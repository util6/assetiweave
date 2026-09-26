use super::prelude::*;
use super::runtime_requirements::*;

pub(crate) const ADAPTER_RUNTIME_PROBE_TIMEOUT_MS: u64 = 3_000;
pub(crate) const ADAPTER_RUNTIME_PROBE_OUTPUT_CAP: usize = 16 * 1024;

pub(crate) enum RuntimeProbeError {
    Spawn(String),
    Output(String),
    Timeout { stdout: Vec<u8>, stderr: Vec<u8> },
}

pub(crate) async fn ensure_adapter_runtime_available(
    runtime: &ConversationAdapterRuntime,
    invocation: &AdapterCommandInvocation,
) -> InfraResult<()> {
    if matches!(runtime.kind, ConversationAdapterRuntimeKind::Executable) {
        return Ok(());
    }

    let status = probe_adapter_runtime_status_with_requirement(
        &runtime.kind,
        invocation.program.clone(),
        runtime.version.as_deref(),
    )
    .await;
    if status.available {
        Ok(())
    } else {
        Err(InfraError::external(status.error.unwrap_or_else(|| {
            adapter_runtime_missing_message(runtime, &invocation.program)
        })))
    }
}

pub(crate) async fn list_adapter_runtime_statuses_with_settings(
    requirements: &[(ConversationAdapterRuntimeKind, String)],
    settings: &Value,
) -> Vec<ConversationAdapterRuntimeStatus> {
    let mut statuses = Vec::new();
    for kind in [
        ConversationAdapterRuntimeKind::Node,
        ConversationAdapterRuntimeKind::Python,
        ConversationAdapterRuntimeKind::Bash,
    ] {
        let program = configured_runtime_program(&kind, settings);
        let required_version = requirements
            .iter()
            .find(|(requirement_kind, _)| *requirement_kind == kind)
            .map(|(_, version)| version.as_str());
        statuses.push(
            probe_adapter_runtime_status_with_requirement(&kind, program, required_version).await,
        );
    }
    statuses
}

#[cfg(test)]
pub(crate) async fn probe_adapter_runtime_status(
    kind: &ConversationAdapterRuntimeKind,
    program: PathBuf,
) -> ConversationAdapterRuntimeStatus {
    probe_adapter_runtime_status_with_requirement(kind, program, None).await
}

pub(crate) async fn probe_adapter_runtime_status_with_requirement(
    kind: &ConversationAdapterRuntimeKind,
    program: PathBuf,
    required_version: Option<&str>,
) -> ConversationAdapterRuntimeStatus {
    let required_version = required_version.map(str::to_string);
    match run_runtime_probe(
        program.clone(),
        runtime_version_args(kind),
        Duration::from_millis(ADAPTER_RUNTIME_PROBE_TIMEOUT_MS),
    )
    .await
    {
        Ok((status, stdout, stderr)) if status.success() => {
            runtime_status_from_success(kind, &program, required_version, &stdout, &stderr)
        }
        Ok((status, stdout, stderr)) => ConversationAdapterRuntimeStatus {
            kind: kind.clone(),
            program: program.to_string_lossy().to_string(),
            available: false,
            version: runtime_version_from_output(&stdout, &stderr),
            required_version,
            error: Some(format!(
                "adapter runtime {} at {} failed version probe with status {}: {}",
                runtime_display_name(kind),
                program.display(),
                status,
                String::from_utf8_lossy(&stderr)
            )),
            hint: Some(runtime_remediation_hint(kind, &program)),
        },
        Err(RuntimeProbeError::Spawn(error))
            if error.to_ascii_lowercase().contains("not found")
                || error.to_ascii_lowercase().contains("no such file") =>
        {
            let requirement = required_version
                .as_deref()
                .map(|version| format!(" {version}"))
                .unwrap_or_default();
            ConversationAdapterRuntimeStatus {
                kind: kind.clone(),
                program: program.to_string_lossy().to_string(),
                available: false,
                version: None,
                required_version,
                error: Some(format!(
                    "adapter runtime {}{} was not found{}: {}",
                    runtime_display_name(kind),
                    requirement,
                    runtime_program_location_suffix(&program),
                    program.display()
                )),
                hint: Some(runtime_remediation_hint(kind, &program)),
            }
        }
        Err(RuntimeProbeError::Spawn(error)) => ConversationAdapterRuntimeStatus {
            kind: kind.clone(),
            program: program.to_string_lossy().to_string(),
            available: false,
            version: None,
            required_version,
            error: Some(format!(
                "failed to probe adapter runtime {} at {}: {error}",
                runtime_display_name(kind),
                program.display()
            )),
            hint: Some(runtime_remediation_hint(kind, &program)),
        },
        Err(RuntimeProbeError::Output(error)) => ConversationAdapterRuntimeStatus {
            kind: kind.clone(),
            program: program.to_string_lossy().to_string(),
            available: false,
            version: None,
            required_version,
            error: Some(format!(
                "failed to read adapter runtime {} probe output at {}: {error}",
                runtime_display_name(kind),
                program.display()
            )),
            hint: Some(runtime_remediation_hint(kind, &program)),
        },
        Err(RuntimeProbeError::Timeout { stdout, stderr }) => ConversationAdapterRuntimeStatus {
            kind: kind.clone(),
            program: program.to_string_lossy().to_string(),
            available: false,
            version: runtime_version_from_output(&stdout, &stderr),
            required_version,
            error: Some(format!(
                "adapter runtime {} at {} timed out after {} ms",
                runtime_display_name(kind),
                program.display(),
                ADAPTER_RUNTIME_PROBE_TIMEOUT_MS
            )),
            hint: Some(runtime_remediation_hint(kind, &program)),
        },
    }
}

pub(crate) fn runtime_status_from_success(
    kind: &ConversationAdapterRuntimeKind,
    program: &Path,
    required_version: Option<String>,
    stdout: &[u8],
    stderr: &[u8],
) -> ConversationAdapterRuntimeStatus {
    let version = runtime_version_from_output(stdout, stderr);
    if let Some(requirement) = required_version.as_deref() {
        let error = match version.as_deref() {
            Some(detected_version) => {
                let satisfied = runtime_version_satisfies_constraint(detected_version, requirement);
                match satisfied {
                    Ok(true) => None,
                    Ok(false) => Some(runtime_version_mismatch_error(
                        kind,
                        program,
                        requirement,
                        detected_version,
                    )),
                    Err(error) => Some(error.to_string()),
                }
            }
            None => Some(format!(
                "adapter runtime {} requires {requirement}, but {} did not report a version",
                runtime_display_name(kind),
                program.display()
            )),
        };
        if let Some(error) = error {
            return ConversationAdapterRuntimeStatus {
                kind: kind.clone(),
                program: program.to_string_lossy().to_string(),
                available: false,
                version,
                required_version,
                error: Some(error),
                hint: Some(runtime_remediation_hint(kind, program)),
            };
        }
    }
    ConversationAdapterRuntimeStatus {
        kind: kind.clone(),
        program: program.to_string_lossy().to_string(),
        available: true,
        version,
        required_version,
        error: None,
        hint: None,
    }
}

pub(crate) fn runtime_version_mismatch_error(
    kind: &ConversationAdapterRuntimeKind,
    program: &Path,
    requirement: &str,
    detected_version: &str,
) -> String {
    format!(
        "adapter runtime {} requires {requirement}, but {} reported {detected_version}",
        runtime_display_name(kind),
        program.display()
    )
}

async fn run_runtime_probe(
    program: PathBuf,
    args: Vec<&str>,
    timeout: Duration,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), RuntimeProbeError> {
    let args = args.into_iter().map(str::to_string).collect::<Vec<_>>();
    let spec = crate::backend::infrastructure::host_process::HostCommandSpec {
        program,
        args,
        env: Vec::new(),
        working_dir: None,
        stdin: crate::backend::infrastructure::host_process::HostInput::Null,
        timeout,
        stdout_limit: ADAPTER_RUNTIME_PROBE_OUTPUT_CAP,
        stderr_limit: ADAPTER_RUNTIME_PROBE_OUTPUT_CAP,
    };
    let output = crate::backend::infrastructure::host_process::run_host_command_async(spec, None)
        .await
        .map_err(|error| match error {
            crate::backend::infrastructure::host_process::HostProcessError::MissingProgram { program } => {
                RuntimeProbeError::Spawn(format!("program not found: {}", program.display()))
            }
            crate::backend::infrastructure::host_process::HostProcessError::Spawn(reason) => {
                RuntimeProbeError::Spawn(reason)
            }
            crate::backend::infrastructure::host_process::HostProcessError::Output(reason) => {
                RuntimeProbeError::Output(reason)
            }
            crate::backend::infrastructure::host_process::HostProcessError::Timeout { stdout, stderr, .. } => {
                RuntimeProbeError::Timeout { stdout, stderr }
            }
            crate::backend::infrastructure::host_process::HostProcessError::Cancelled => {
                RuntimeProbeError::Output("runtime probe was cancelled".to_string())
            }
            crate::backend::infrastructure::host_process::HostProcessError::Cleanup(reason) => {
                RuntimeProbeError::Output(reason)
            }
            crate::backend::infrastructure::host_process::HostProcessError::OutputLimitExceeded { .. } => {
                RuntimeProbeError::Output(
                    "runtime probe output exceeded configured limit".to_string(),
                )
            }
        })?;
    if output.stdout_truncated || output.stderr_truncated {
        return Err(RuntimeProbeError::Output(format!(
            "runtime probe output exceeded cap of {ADAPTER_RUNTIME_PROBE_OUTPUT_CAP} bytes"
        )));
    }
    Ok((output.status, output.stdout, output.stderr))
}

pub(crate) fn runtime_version_from_output(stdout: &[u8], stderr: &[u8]) -> Option<String> {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
pub(crate) async fn list_adapter_runtime_statuses(
    requirements: &[(ConversationAdapterRuntimeKind, String)],
) -> Vec<ConversationAdapterRuntimeStatus> {
    list_adapter_runtime_statuses_with_settings(requirements, &serde_json::json!({})).await
}
