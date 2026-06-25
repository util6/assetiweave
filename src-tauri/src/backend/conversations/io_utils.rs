use super::prelude::*;
use std::io::ErrorKind;

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
        program: runtime_program(&runtime.kind),
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

    let mut command = Command::new(&invocation.program);
    command.args(runtime_version_args(&runtime.kind));
    let output = command.output().map_err(|error| {
        if error.kind() == ErrorKind::NotFound {
            adapter_runtime_missing_message(runtime, &invocation.program)
        } else {
            format!(
                "failed to probe adapter runtime {} at {}: {error}",
                runtime_display_name(&runtime.kind),
                invocation.program.display()
            )
        }
    })?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "adapter runtime {} at {} failed version probe with status {}: {}",
            runtime_display_name(&runtime.kind),
            invocation.program.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn adapter_runtime_missing_message(runtime: &ConversationAdapterRuntime, program: &Path) -> String {
    let version = runtime
        .version
        .as_deref()
        .map(|version| format!(" {version}"))
        .unwrap_or_default();
    format!(
        "adapter runtime {}{} was not found on PATH: {}",
        runtime_display_name(&runtime.kind),
        version,
        program.display()
    )
}

fn runtime_program(kind: &ConversationAdapterRuntimeKind) -> PathBuf {
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
