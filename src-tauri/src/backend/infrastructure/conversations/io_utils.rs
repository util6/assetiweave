use super::prelude::*;
pub(crate) use super::runtime_probe::*;
pub(crate) use super::runtime_requirements::*;
#[allow(unused_imports)]
pub(crate) use semver::VersionReq;

pub(crate) fn resolve_command_path(manifest_dir: &Path, command: &str) -> PathBuf {
    let path = PathBuf::from(command);
    if path.is_absolute() {
        path
    } else {
        manifest_dir.join(path)
    }
}

pub(crate) fn resolve_adapter_entry_path(
    manifest_dir: &Path,
    manifest: &ConversationAdapterManifest,
) -> InfraResult<PathBuf> {
    if let Some(runtime) = manifest.runtime.as_ref() {
        return Ok(resolve_command_path(manifest_dir, &runtime.entry));
    }
    let command = manifest
        .command
        .first()
        .ok_or_else(|| InfraError::external("adapter command must include an executable"))?;
    Ok(resolve_command_path(manifest_dir, command))
}

pub(crate) struct AdapterCommandInvocation {
    pub(super) program: PathBuf,
    pub(super) args: Vec<String>,
    pub(super) display_path: PathBuf,
}

pub(crate) fn build_adapter_invocation_with_settings(
    manifest_dir: &Path,
    manifest: &ConversationAdapterManifest,
    settings: &Value,
) -> InfraResult<AdapterCommandInvocation> {
    if let Some(runtime) = adapter_execution_runtime(manifest) {
        return Ok(build_adapter_runtime_invocation_with_settings(
            manifest_dir,
            &runtime,
            &[],
            settings,
        ));
    }
    let (command, args) = manifest
        .command
        .split_first()
        .ok_or_else(|| InfraError::external("adapter command must include an executable"))?;
    Ok(build_adapter_command_invocation(
        manifest_dir,
        command,
        args,
    ))
}

pub(crate) fn adapter_execution_runtime(
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

pub(crate) fn build_adapter_command_invocation(
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

pub(crate) fn build_adapter_runtime_invocation_with_settings(
    manifest_dir: &Path,
    runtime: &ConversationAdapterRuntime,
    call_args: &[String],
    settings: &Value,
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
        program: configured_runtime_program(&runtime.kind, settings),
        args,
        display_path: entry_path,
    }
}

pub(crate) fn build_adapter_runtime_invocation(
    manifest_dir: &Path,
    runtime: &ConversationAdapterRuntime,
    call_args: &[String],
) -> AdapterCommandInvocation {
    build_adapter_runtime_invocation_with_settings(
        manifest_dir,
        runtime,
        call_args,
        &serde_json::json!({}),
    )
}
pub(crate) fn is_javascript_adapter_command(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "cjs" | "js" | "mjs"
            )
        })
}

pub(crate) fn hash_file(path: &Path) -> InfraResult<String> {
    let bytes = fs::read(path).map_err(InfraError::external)?;
    Ok(hash_bytes(&bytes))
}

pub(crate) fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
#[path = "io_utils_tests.rs"]
mod tests;
