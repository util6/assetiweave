use super::external_manifest::run_external_adapter_with_settings;
use super::external_parser::parse_external_adapter_output_impl;
use super::external_reader::ExternalAdapterProgressListener;
use super::external_sanitize::sanitize_adapter_progress;
use super::prelude::*;

pub(crate) async fn prepare_adapter_invocation(
    validation: &ExternalAdapterValidationResult,
    settings: &Value,
) -> InfraResult<AdapterCommandInvocation> {
    let manifest = &validation.manifest;
    let manifest_dir = Path::new(&validation.manifest_path)
        .parent()
        .ok_or_else(|| InfraError::external("adapter manifest path has no parent directory"))?;
    let execution_runtime = adapter_execution_runtime(manifest);
    let mut invocation = match execution_runtime.as_ref() {
        Some(runtime) => {
            build_adapter_runtime_invocation_with_settings(manifest_dir, runtime, &[], settings)
        }
        None => build_adapter_invocation_with_settings(manifest_dir, manifest, settings)?,
    };
    if invocation.program.components().count() == 1 {
        invocation.program = crate::backend::infrastructure::host_process::resolve_host_executable(
            &invocation.program.to_string_lossy(),
        )
        .ok_or_else(|| InfraError::external("adapter runtime program was not found"))?;
    }
    if let Some(runtime) = execution_runtime.as_ref() {
        ensure_adapter_runtime_available(runtime, &invocation).await?;
    }
    Ok(invocation)
}

pub(crate) async fn run_prepared_adapter(
    validation: &ExternalAdapterValidationResult,
    invocation: &AdapterCommandInvocation,
    method: &str,
    request: Value,
    timeout: Duration,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
    progress_listener: Option<ExternalAdapterProgressListener>,
) -> InfraResult<ExternalAdapterRunResult> {
    let manifest = &validation.manifest;
    let request_text = serde_json::to_vec(&request).map_err(InfraError::external)?;
    let line_listener: Option<
        crate::backend::infrastructure::host_process::HostStdoutLineListener,
    > = progress_listener.map(|cb| {
        std::sync::Arc::new(move |line: &str| {
            let trimmed = line.trim();
            if trimmed.starts_with('{') {
                if let Ok(parsed) = serde_json::from_str::<ExternalAdapterLine>(trimmed) {
                    if parsed.kind == "progress" {
                        let prog = sanitize_adapter_progress(&parsed);
                        cb(&prog);
                    }
                }
            }
        }) as crate::backend::infrastructure::host_process::HostStdoutLineListener
    });
    let output = crate::backend::infrastructure::host_process::run_host_command_async_streaming(
        crate::backend::infrastructure::host_process::HostCommandSpec {
            program: invocation.program.clone(),
            args: invocation.args.clone(),
            env: Vec::new(),
            working_dir: None,
            stdin: crate::backend::infrastructure::host_process::HostInput::Bytes(
                request_text.into_iter().chain([b'\n']).collect(),
            ),
            timeout,
            stdout_limit: DEFAULT_MAX_TOTAL_BYTES,
            stderr_limit: 1024 * 1024,
        },
        cancellation,
        line_listener,
    )
    .await
    .map_err(|error| match error {
        crate::backend::infrastructure::host_process::HostProcessError::MissingProgram {
            program,
        } => {
            format!("adapter program was not found: {}", program.display())
        }
        crate::backend::infrastructure::host_process::HostProcessError::Spawn(reason) => {
            format!(
                "failed to start adapter {}: {reason}",
                invocation.display_path.display()
            )
        }
        crate::backend::infrastructure::host_process::HostProcessError::Output(reason) => {
            format!(
                "failed to capture adapter {} output: {reason}",
                invocation.display_path.display()
            )
        }
        crate::backend::infrastructure::host_process::HostProcessError::Timeout { .. } => {
            format!("adapter timed out after {} ms", timeout.as_millis())
        }
        crate::backend::infrastructure::host_process::HostProcessError::Cancelled => {
            "adapter process was cancelled".to_string()
        }
        crate::backend::infrastructure::host_process::HostProcessError::Cleanup(reason) => {
            format!("adapter process cleanup failed: {reason}")
        }
        crate::backend::infrastructure::host_process::HostProcessError::OutputLimitExceeded {
            ..
        } => "adapter output exceeded configured limit".to_string(),
    })
    .map_err(|error| {
        if cancellation.is_some_and(tokio_util::sync::CancellationToken::is_cancelled) {
            InfraError::Cancelled("conversation sync cancelled".to_string())
        } else {
            InfraError::external(error)
        }
    })?;
    if output.stdout_truncated || output.stderr_truncated {
        return Err(InfraError::external(format!(
            "adapter output exceeded configured cap (stdout={} bytes, stderr={} bytes)",
            DEFAULT_MAX_TOTAL_BYTES,
            1024 * 1024
        )));
    }
    if !output.status.success() {
        return Err(InfraError::external(format!(
            "adapter exited with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    parse_external_adapter_output_with_manifest(method, output.stdout, output.stderr, manifest)
}

#[cfg(test)]
pub(super) async fn run_external_adapter(
    validation: &ExternalAdapterValidationResult,
    method: &str,
    request: Value,
    timeout: Duration,
) -> InfraResult<ExternalAdapterRunResult> {
    run_external_adapter_with_settings(validation, method, request, timeout, &serde_json::json!({}))
        .await
}

#[cfg(test)]
pub(super) fn parse_external_adapter_output(
    method: &str,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
) -> InfraResult<ExternalAdapterRunResult> {
    parse_external_adapter_output_impl(method, stdout, stderr, None)
}

pub(super) fn parse_external_adapter_output_with_manifest(
    method: &str,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    manifest: &ConversationAdapterManifest,
) -> InfraResult<ExternalAdapterRunResult> {
    parse_external_adapter_output_impl(method, stdout, stderr, Some(manifest))
}
