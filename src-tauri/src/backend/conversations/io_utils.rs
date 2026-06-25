use super::prelude::*;
use std::io::ErrorKind;

const CONVERSATION_RUNTIME_OVERRIDES_KEY: &str = "conversationRuntimeOverrides";
const ADAPTER_RUNTIME_PROBE_TIMEOUT_MS: u64 = 3_000;
const ADAPTER_RUNTIME_PROBE_OUTPUT_CAP: usize = 16 * 1024;

enum RuntimeProbeError {
    Spawn(std::io::Error),
    Output(String),
    Timeout { stdout: Vec<u8>, stderr: Vec<u8> },
}

pub(super) fn resolve_command_path(manifest_dir: &Path, command: &str) -> PathBuf {
    let path = PathBuf::from(command);
    if path.is_absolute() {
        path
    } else {
        manifest_dir.join(path)
    }
}

pub(super) fn resolve_adapter_entry_path(
    manifest_dir: &Path,
    manifest: &ConversationAdapterManifest,
) -> AppResult<PathBuf> {
    if let Some(runtime) = manifest.runtime.as_ref() {
        return Ok(resolve_command_path(manifest_dir, &runtime.entry));
    }
    let command = manifest
        .command
        .first()
        .ok_or_else(|| "adapter command must include an executable".to_string())?;
    Ok(resolve_command_path(manifest_dir, command))
}

pub(super) struct AdapterCommandInvocation {
    pub(super) program: PathBuf,
    pub(super) args: Vec<String>,
    pub(super) display_path: PathBuf,
}

pub(super) fn build_adapter_invocation(
    manifest_dir: &Path,
    manifest: &ConversationAdapterManifest,
) -> AppResult<AdapterCommandInvocation> {
    if let Some(runtime) = manifest.runtime.as_ref() {
        return Ok(build_adapter_runtime_invocation(manifest_dir, runtime, &[]));
    }
    let (command, args) = manifest
        .command
        .split_first()
        .ok_or_else(|| "adapter command must include an executable".to_string())?;
    Ok(build_adapter_command_invocation(
        manifest_dir,
        command,
        args,
    ))
}

pub(super) fn build_adapter_command_invocation(
    manifest_dir: &Path,
    command: &str,
    args: &[String],
) -> AdapterCommandInvocation {
    let executable = resolve_command_path(manifest_dir, command);
    if is_javascript_adapter_command(&executable) {
        let mut node_args = Vec::with_capacity(args.len() + 1);
        node_args.push(executable.to_string_lossy().to_string());
        node_args.extend_from_slice(args);
        return AdapterCommandInvocation {
            program: PathBuf::from("node"),
            args: node_args,
            display_path: executable,
        };
    }
    AdapterCommandInvocation {
        program: executable.clone(),
        args: args.to_vec(),
        display_path: executable,
    }
}

pub(super) fn build_adapter_runtime_invocation(
    manifest_dir: &Path,
    runtime: &ConversationAdapterRuntime,
    call_args: &[String],
) -> AdapterCommandInvocation {
    let entry_path = resolve_command_path(manifest_dir, &runtime.entry);
    if matches!(runtime.kind, ConversationAdapterRuntimeKind::Executable) {
        let mut args = runtime.args.clone();
        args.extend_from_slice(call_args);
        return AdapterCommandInvocation {
            program: entry_path.clone(),
            args,
            display_path: entry_path,
        };
    }
    let mut args = runtime_args(&runtime.kind);
    args.push(entry_path.to_string_lossy().to_string());
    args.extend_from_slice(&runtime.args);
    args.extend_from_slice(call_args);

    AdapterCommandInvocation {
        program: configured_runtime_program(&runtime.kind),
        args,
        display_path: entry_path,
    }
}

pub(super) fn ensure_adapter_runtime_available(
    runtime: &ConversationAdapterRuntime,
    invocation: &AdapterCommandInvocation,
) -> AppResult<()> {
    if matches!(runtime.kind, ConversationAdapterRuntimeKind::Executable) {
        return Ok(());
    }

    let status = probe_adapter_runtime_status(&runtime.kind, invocation.program.clone());
    if status.available {
        Ok(())
    } else {
        let message = adapter_runtime_missing_message(runtime, &invocation.program);
        match status.error {
            Some(error) if error.contains("was not found") && error != message => {
                Err(format!("{message}; {error}"))
            }
            Some(error) if error.contains("was not found") => Err(message),
            Some(error) => Err(error),
            _ => Err(message),
        }
    }
}

pub(super) fn list_adapter_runtime_statuses() -> Vec<ConversationAdapterRuntimeStatus> {
    [
        ConversationAdapterRuntimeKind::Node,
        ConversationAdapterRuntimeKind::Python,
        ConversationAdapterRuntimeKind::Bash,
    ]
    .into_iter()
    .map(|kind| {
        let program = configured_runtime_program(&kind);
        probe_adapter_runtime_status(&kind, program)
    })
    .collect()
}

pub(super) fn probe_adapter_runtime_status(
    kind: &ConversationAdapterRuntimeKind,
    program: PathBuf,
) -> ConversationAdapterRuntimeStatus {
    let mut command = Command::new(&program);
    command.args(runtime_version_args(kind));
    match run_runtime_probe(
        command,
        Duration::from_millis(ADAPTER_RUNTIME_PROBE_TIMEOUT_MS),
    ) {
        Ok((status, stdout, stderr)) if status.success() => ConversationAdapterRuntimeStatus {
            kind: kind.clone(),
            program: program.to_string_lossy().to_string(),
            available: true,
            version: runtime_version_from_output(&stdout, &stderr),
            error: None,
        },
        Ok((status, stdout, stderr)) => ConversationAdapterRuntimeStatus {
            kind: kind.clone(),
            program: program.to_string_lossy().to_string(),
            available: false,
            version: runtime_version_from_output(&stdout, &stderr),
            error: Some(format!(
                "adapter runtime {} at {} failed version probe with status {}: {}",
                runtime_display_name(kind),
                program.display(),
                status,
                String::from_utf8_lossy(&stderr)
            )),
        },
        Err(RuntimeProbeError::Spawn(error)) if error.kind() == ErrorKind::NotFound => {
            ConversationAdapterRuntimeStatus {
                kind: kind.clone(),
                program: program.to_string_lossy().to_string(),
                available: false,
                version: None,
                error: Some(format!(
                    "adapter runtime {} was not found{}: {}",
                    runtime_display_name(kind),
                    runtime_program_location_suffix(&program),
                    program.display()
                )),
            }
        }
        Err(RuntimeProbeError::Spawn(error)) => ConversationAdapterRuntimeStatus {
            kind: kind.clone(),
            program: program.to_string_lossy().to_string(),
            available: false,
            version: None,
            error: Some(format!(
                "failed to probe adapter runtime {} at {}: {error}",
                runtime_display_name(kind),
                program.display()
            )),
        },
        Err(RuntimeProbeError::Output(error)) => ConversationAdapterRuntimeStatus {
            kind: kind.clone(),
            program: program.to_string_lossy().to_string(),
            available: false,
            version: None,
            error: Some(format!(
                "failed to read adapter runtime {} probe output at {}: {error}",
                runtime_display_name(kind),
                program.display()
            )),
        },
        Err(RuntimeProbeError::Timeout { stdout, stderr }) => ConversationAdapterRuntimeStatus {
            kind: kind.clone(),
            program: program.to_string_lossy().to_string(),
            available: false,
            version: runtime_version_from_output(&stdout, &stderr),
            error: Some(format!(
                "adapter runtime {} at {} timed out after {} ms",
                runtime_display_name(kind),
                program.display(),
                ADAPTER_RUNTIME_PROBE_TIMEOUT_MS
            )),
        },
    }
}

fn run_runtime_probe(
    mut command: Command,
    timeout: Duration,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), RuntimeProbeError> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(RuntimeProbeError::Spawn)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| RuntimeProbeError::Output("runtime stdout was not available".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| RuntimeProbeError::Output("runtime stderr was not available".to_string()))?;
    let stdout_reader =
        thread::spawn(move || read_capped(stdout, ADAPTER_RUNTIME_PROBE_OUTPUT_CAP));
    let stderr_reader =
        thread::spawn(move || read_capped(stderr, ADAPTER_RUNTIME_PROBE_OUTPUT_CAP));

    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| RuntimeProbeError::Output(error.to_string()))?
        {
            let stdout = stdout_reader
                .join()
                .map_err(|_| {
                    RuntimeProbeError::Output("runtime stdout reader panicked".to_string())
                })?
                .map_err(RuntimeProbeError::Output)?;
            let stderr = stderr_reader
                .join()
                .map_err(|_| {
                    RuntimeProbeError::Output("runtime stderr reader panicked".to_string())
                })?
                .map_err(RuntimeProbeError::Output)?;
            return Ok((status, stdout, stderr));
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            let stdout = stdout_reader
                .join()
                .map_err(|_| {
                    RuntimeProbeError::Output("runtime stdout reader panicked".to_string())
                })?
                .map_err(RuntimeProbeError::Output)?;
            let stderr = stderr_reader
                .join()
                .map_err(|_| {
                    RuntimeProbeError::Output("runtime stderr reader panicked".to_string())
                })?
                .map_err(RuntimeProbeError::Output)?;
            return Err(RuntimeProbeError::Timeout { stdout, stderr });
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn adapter_runtime_missing_message(runtime: &ConversationAdapterRuntime, program: &Path) -> String {
    let version = runtime
        .version
        .as_deref()
        .map(|version| format!(" {version}"))
        .unwrap_or_default();
    format!(
        "adapter runtime {}{} was not found{}: {}",
        runtime_display_name(&runtime.kind),
        version,
        runtime_program_location_suffix(program),
        program.display()
    )
}

fn runtime_version_from_output(stdout: &[u8], stderr: &[u8]) -> Option<String> {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn configured_runtime_program(kind: &ConversationAdapterRuntimeKind) -> PathBuf {
    crate::backend::app_settings::read_app_settings_value()
        .ok()
        .and_then(|settings| runtime_program_from_settings(kind, &settings))
        .unwrap_or_else(|| default_runtime_program(kind))
}

pub(super) fn runtime_program_from_settings(
    kind: &ConversationAdapterRuntimeKind,
    settings: &Value,
) -> Option<PathBuf> {
    let overrides = settings
        .get(CONVERSATION_RUNTIME_OVERRIDES_KEY)
        .and_then(Value::as_object)?;
    let key = match kind {
        ConversationAdapterRuntimeKind::Node => "node",
        ConversationAdapterRuntimeKind::Python => "python",
        ConversationAdapterRuntimeKind::Bash => "bash",
        ConversationAdapterRuntimeKind::Executable => return None,
    };
    let program = overrides.get(key)?.as_str()?.trim();
    (!program.is_empty() && program.len() <= 4096).then(|| PathBuf::from(program))
}

fn default_runtime_program(kind: &ConversationAdapterRuntimeKind) -> PathBuf {
    match kind {
        ConversationAdapterRuntimeKind::Node => PathBuf::from("node"),
        #[cfg(windows)]
        ConversationAdapterRuntimeKind::Python => PathBuf::from("py"),
        #[cfg(not(windows))]
        ConversationAdapterRuntimeKind::Python => PathBuf::from("python3"),
        ConversationAdapterRuntimeKind::Bash => PathBuf::from("bash"),
        ConversationAdapterRuntimeKind::Executable => PathBuf::new(),
    }
}

fn runtime_program_location_suffix(program: &Path) -> &'static str {
    if program.is_absolute() {
        ""
    } else {
        " on PATH"
    }
}

fn runtime_args(kind: &ConversationAdapterRuntimeKind) -> Vec<String> {
    match kind {
        #[cfg(windows)]
        ConversationAdapterRuntimeKind::Python => vec!["-3".to_string()],
        _ => Vec::new(),
    }
}

fn runtime_version_args(kind: &ConversationAdapterRuntimeKind) -> Vec<&'static str> {
    match kind {
        ConversationAdapterRuntimeKind::Node => vec!["--version"],
        #[cfg(windows)]
        ConversationAdapterRuntimeKind::Python => vec!["-3", "--version"],
        #[cfg(not(windows))]
        ConversationAdapterRuntimeKind::Python => vec!["--version"],
        ConversationAdapterRuntimeKind::Bash => vec!["--version"],
        ConversationAdapterRuntimeKind::Executable => Vec::new(),
    }
}

fn runtime_display_name(kind: &ConversationAdapterRuntimeKind) -> &'static str {
    match kind {
        ConversationAdapterRuntimeKind::Node => "node",
        ConversationAdapterRuntimeKind::Python => "python",
        ConversationAdapterRuntimeKind::Bash => "bash",
        ConversationAdapterRuntimeKind::Executable => "executable",
    }
}

fn is_javascript_adapter_command(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "cjs" | "js" | "mjs"
            )
        })
}

pub(super) fn read_capped<R: Read>(mut reader: R, cap: usize) -> AppResult<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        output.extend_from_slice(&buffer[..read]);
        if output.len() > cap {
            return Err(format!("adapter output exceeded cap of {cap} bytes"));
        }
    }
    Ok(output)
}

pub(super) fn hash_file(path: &Path) -> AppResult<String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    Ok(hash_bytes(&bytes))
}

pub(super) fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
