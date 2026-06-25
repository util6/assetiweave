use super::prelude::*;
use std::io::ErrorKind;

const CONVERSATION_RUNTIME_OVERRIDES_KEY: &str = "conversationRuntimeOverrides";
const ADAPTER_RUNTIME_PROBE_TIMEOUT_MS: u64 = 3_000;
const ADAPTER_RUNTIME_PROBE_OUTPUT_CAP: usize = 16 * 1024;
pub(super) const LEGACY_JAVASCRIPT_COMMAND_NODE_VERSION: &str = ">=20";

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
    if let Some(runtime) = adapter_execution_runtime(manifest) {
        return Ok(build_adapter_runtime_invocation(
            manifest_dir,
            &runtime,
            &[],
        ));
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

pub(super) fn adapter_execution_runtime(
    manifest: &ConversationAdapterManifest,
) -> Option<ConversationAdapterRuntime> {
    if let Some(runtime) = manifest.runtime.as_ref() {
        return Some(runtime.clone());
    }
    let (command, args) = manifest.command.split_first()?;
    if !is_javascript_adapter_command(Path::new(command)) {
        return None;
    }
    Some(ConversationAdapterRuntime {
        kind: ConversationAdapterRuntimeKind::Node,
        entry: command.clone(),
        args: args.to_vec(),
        version: Some(LEGACY_JAVASCRIPT_COMMAND_NODE_VERSION.to_string()),
    })
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

    let status = probe_adapter_runtime_status_with_requirement(
        &runtime.kind,
        invocation.program.clone(),
        runtime.version.as_deref(),
    );
    if status.available {
        Ok(())
    } else {
        Err(status
            .error
            .unwrap_or_else(|| adapter_runtime_missing_message(runtime, &invocation.program)))
    }
}

pub(super) fn list_adapter_runtime_statuses(
    requirements: &[(ConversationAdapterRuntimeKind, String)],
) -> Vec<ConversationAdapterRuntimeStatus> {
    [
        ConversationAdapterRuntimeKind::Node,
        ConversationAdapterRuntimeKind::Python,
        ConversationAdapterRuntimeKind::Bash,
    ]
    .into_iter()
    .map(|kind| {
        let program = configured_runtime_program(&kind);
        let required_version = requirements
            .iter()
            .find(|(requirement_kind, _)| *requirement_kind == kind)
            .map(|(_, version)| version.as_str());
        probe_adapter_runtime_status_with_requirement(&kind, program, required_version)
    })
    .collect()
}

pub(super) fn adapter_runtime_requirements(
    adapters: &[ConversationAdapter],
) -> Vec<(ConversationAdapterRuntimeKind, String)> {
    let mut requirements: Vec<(ConversationAdapterRuntimeKind, String)> = Vec::new();
    for adapter in adapters {
        if !adapter.enabled {
            continue;
        }
        let Some(manifest_path) = adapter.manifest_path.as_deref() else {
            continue;
        };
        let Ok(manifest_text) = fs::read_to_string(manifest_path) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_str::<ConversationAdapterManifest>(&manifest_text)
        else {
            continue;
        };
        if let Some(runtime) = manifest.runtime.as_ref() {
            let Some(version) = runtime.version.as_deref() else {
                continue;
            };
            if matches!(runtime.kind, ConversationAdapterRuntimeKind::Executable)
                || validate_runtime_version_constraint(version).is_err()
            {
                continue;
            }
            upsert_highest_runtime_requirement(&mut requirements, &runtime.kind, version);
        } else if manifest
            .command
            .first()
            .is_some_and(|command| is_javascript_adapter_command(Path::new(command)))
        {
            upsert_highest_runtime_requirement(
                &mut requirements,
                &ConversationAdapterRuntimeKind::Node,
                LEGACY_JAVASCRIPT_COMMAND_NODE_VERSION,
            );
        }
    }
    sort_runtime_requirements(requirements)
}

fn upsert_highest_runtime_requirement(
    requirements: &mut Vec<(ConversationAdapterRuntimeKind, String)>,
    kind: &ConversationAdapterRuntimeKind,
    version: &str,
) {
    if let Some((_, current_version)) = requirements
        .iter_mut()
        .find(|(requirement_kind, _)| requirement_kind == kind)
    {
        if runtime_requirement_is_higher(version, current_version).unwrap_or(false) {
            *current_version = version.to_string();
        }
    } else {
        requirements.push((kind.clone(), version.to_string()));
    }
}

fn runtime_requirement_is_higher(candidate: &str, current: &str) -> AppResult<bool> {
    let candidate = parse_minimum_version_constraint(candidate)?;
    let current = parse_minimum_version_constraint(current)?;
    Ok(compare_versions(&candidate, &current) == std::cmp::Ordering::Greater)
}

fn sort_runtime_requirements(
    mut requirements: Vec<(ConversationAdapterRuntimeKind, String)>,
) -> Vec<(ConversationAdapterRuntimeKind, String)> {
    let order = [
        ConversationAdapterRuntimeKind::Node,
        ConversationAdapterRuntimeKind::Python,
        ConversationAdapterRuntimeKind::Bash,
    ];
    requirements.sort_by_key(|(kind, _)| {
        order
            .iter()
            .position(|ordered_kind| ordered_kind == kind)
            .unwrap_or(order.len())
    });
    requirements
}

#[cfg(test)]
pub(super) fn probe_adapter_runtime_status(
    kind: &ConversationAdapterRuntimeKind,
    program: PathBuf,
) -> ConversationAdapterRuntimeStatus {
    probe_adapter_runtime_status_with_requirement(kind, program, None)
}

pub(super) fn probe_adapter_runtime_status_with_requirement(
    kind: &ConversationAdapterRuntimeKind,
    program: PathBuf,
    required_version: Option<&str>,
) -> ConversationAdapterRuntimeStatus {
    let mut command = Command::new(&program);
    command.args(runtime_version_args(kind));
    let required_version = required_version.map(str::to_string);
    match run_runtime_probe(
        command,
        Duration::from_millis(ADAPTER_RUNTIME_PROBE_TIMEOUT_MS),
    ) {
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
        Err(RuntimeProbeError::Spawn(error)) if error.kind() == ErrorKind::NotFound => {
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

fn runtime_status_from_success(
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
                    Err(error) => Some(error),
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

fn runtime_version_mismatch_error(
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

pub(super) fn validate_runtime_version_constraint(requirement: &str) -> AppResult<()> {
    parse_minimum_version_constraint(requirement).map(|_| ())
}

pub(super) fn runtime_version_satisfies_constraint(
    detected_version: &str,
    requirement: &str,
) -> AppResult<bool> {
    let minimum = parse_minimum_version_constraint(requirement)?;
    let detected = parse_detected_runtime_version(detected_version).ok_or_else(|| {
        format!("could not parse adapter runtime version from output: {detected_version}")
    })?;
    Ok(compare_versions(&detected, &minimum) != std::cmp::Ordering::Less)
}

fn parse_minimum_version_constraint(requirement: &str) -> AppResult<Vec<u64>> {
    let requirement = requirement.trim();
    let version = requirement.strip_prefix(">=").ok_or_else(|| {
        format!("adapter runtime version constraint must use >=x[.y[.z]]: {requirement}")
    })?;
    parse_exact_runtime_version(version.trim()).ok_or_else(|| {
        format!("adapter runtime version constraint must use >=x[.y[.z]]: {requirement}")
    })
}

fn parse_exact_runtime_version(version: &str) -> Option<Vec<u64>> {
    if version.is_empty() {
        return None;
    }
    let parts = version.split('.').collect::<Vec<_>>();
    if parts.len() > 3 || parts.iter().any(|part| part.is_empty()) {
        return None;
    }
    parts
        .into_iter()
        .map(|part| {
            part.chars()
                .all(|character| character.is_ascii_digit())
                .then(|| part.parse::<u64>().ok())
                .flatten()
        })
        .collect()
}

fn parse_detected_runtime_version(output: &str) -> Option<Vec<u64>> {
    let start = output
        .char_indices()
        .find(|(_, character)| character.is_ascii_digit())
        .map(|(index, _)| index)?;
    let version = output[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit() || *character == '.')
        .collect::<String>();
    parse_exact_runtime_version(version.trim_end_matches('.'))
}

fn compare_versions(left: &[u64], right: &[u64]) -> std::cmp::Ordering {
    let max_len = left.len().max(right.len());
    for index in 0..max_len {
        match left
            .get(index)
            .copied()
            .unwrap_or_default()
            .cmp(&right.get(index).copied().unwrap_or_default())
        {
            std::cmp::Ordering::Equal => {}
            ordering => return ordering,
        }
    }
    std::cmp::Ordering::Equal
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
    (!program.is_empty() && program.len() <= 4096 && is_absolute_runtime_program(program))
        .then(|| PathBuf::from(program))
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

fn is_absolute_runtime_program(program: &str) -> bool {
    Path::new(program).is_absolute() || looks_like_windows_rooted_runtime_program(program)
}

fn looks_like_windows_rooted_runtime_program(program: &str) -> bool {
    let bytes = program.as_bytes();
    if program.starts_with("\\\\") || program.starts_with('\\') {
        return true;
    }
    bytes.len() >= 3
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
        && bytes[0].is_ascii_alphabetic()
}

fn runtime_program_location_suffix(program: &Path) -> &'static str {
    if program.is_absolute() {
        ""
    } else {
        " on PATH"
    }
}

fn runtime_remediation_hint(kind: &ConversationAdapterRuntimeKind, program: &Path) -> String {
    let runtime_name = runtime_display_name(kind);
    let configured_path_hint = if program.is_absolute() {
        " Verify that the configured path exists and can run --version, or clear the custom runtime path to use PATH."
    } else {
        " Install it and ensure it is available on PATH, or set an absolute runtime path in Settings > Conversations > Conversation Parsers."
    };
    match kind {
        ConversationAdapterRuntimeKind::Node => {
            format!("Install Node.js 20 or newer.{configured_path_hint}")
        }
        ConversationAdapterRuntimeKind::Python => {
            format!("Install Python 3.10 or newer.{configured_path_hint}")
        }
        ConversationAdapterRuntimeKind::Bash => {
            format!("Install bash or configure a bash-compatible shell path.{configured_path_hint}")
        }
        ConversationAdapterRuntimeKind::Executable => {
            format!("Check the executable runtime path for {runtime_name}.")
        }
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
