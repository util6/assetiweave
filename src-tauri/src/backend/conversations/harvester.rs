use super::io_utils::{
    build_adapter_runtime_invocation_with_settings, ensure_adapter_runtime_available,
    sort_runtime_requirements, upsert_highest_runtime_requirement,
    validate_runtime_version_constraint, AdapterCommandInvocation,
    LEGACY_JAVASCRIPT_COMMAND_NODE_VERSION,
};
use super::prelude::*;

const HARVESTER_MANIFEST_FILE: &str = "harvester.json";
const HARVESTER_TIMEOUT_MS: u64 = 10 * 60 * 1000;
const OUTPUT_CAPTURE_LIMIT: usize = 64 * 1024;

#[derive(Debug, Deserialize)]
struct HarvesterManifest {
    id: String,
    #[serde(default)]
    entrypoint: Vec<String>,
    #[serde(default)]
    runtime: Option<ConversationAdapterRuntime>,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) async fn run_conversation_harvester_for_source_with_settings(
    source: &ConversationSource,
    settings: &Value,
) -> AppResult<()> {
    let source_dir = crate::backend::path_utils::expand_path(&source.location)?;
    let work_dir = resolve_harvester_work_dir(&source_dir);
    run_conversation_harvester_in_dir(&work_dir, false, settings, None)
        .await
        .map(|_| ())
}

pub(crate) async fn run_conversation_harvester_with_control(
    adapter: Option<&ConversationAdapter>,
    source: &ConversationSource,
    full_reparse: bool,
    settings: &Value,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> AppResult<()> {
    super::external::ensure_read_not_cancelled(cancellation)?;
    let source_dir = crate::backend::path_utils::expand_path(&source.location)?;
    let work_dir = resolve_harvester_work_dir(&source_dir);

    if work_dir.join(HARVESTER_MANIFEST_FILE).is_file() {
        if run_conversation_harvester_in_dir(&work_dir, full_reparse, settings, cancellation)
            .await?
        {
            return Ok(());
        }
    }

    if let Some(adapter_dir) = adapter.and_then(adapter_manifest_dir) {
        if adapter_dir.join(HARVESTER_MANIFEST_FILE).is_file() {
            if run_conversation_harvester_with_manifest_root_and_work_dir(
                &adapter_dir,
                &work_dir,
                full_reparse,
                settings,
                cancellation,
            )
            .await?
            {
                return Ok(());
            }
        }
    }

    run_conversation_harvester_in_dir(&source_dir, full_reparse, settings, cancellation)
        .await
        .map(|_| ())
}

pub(crate) fn resolve_harvester_work_dir(source_path: &Path) -> PathBuf {
    if source_path.ends_with("output/normalized") {
        if let Some(parent) = source_path.parent().and_then(|p| p.parent()) {
            return parent.to_path_buf();
        }
    } else if source_path.ends_with("output") {
        if let Some(parent) = source_path.parent() {
            return parent.to_path_buf();
        }
    }
    source_path.to_path_buf()
}

fn adapter_manifest_dir(adapter: &ConversationAdapter) -> Option<PathBuf> {
    let manifest_path = adapter.manifest_path.as_deref()?;
    let manifest_path = crate::backend::path_utils::expand_path(manifest_path).ok()?;
    manifest_path.parent().map(Path::to_path_buf)
}

async fn run_conversation_harvester_in_dir(
    work_dir: &Path,
    full_reparse: bool,
    settings: &Value,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> AppResult<bool> {
    run_conversation_harvester_with_manifest_root_and_work_dir(
        work_dir,
        work_dir,
        full_reparse,
        settings,
        cancellation,
    )
    .await
}

async fn run_conversation_harvester_with_manifest_root_and_work_dir(
    manifest_root: &Path,
    work_dir: &Path,
    full_reparse: bool,
    settings: &Value,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> AppResult<bool> {
    let manifest_path = manifest_root.join(HARVESTER_MANIFEST_FILE);
    if !manifest_path.is_file() {
        return Ok(false);
    }

    let manifest_text = fs::read_to_string(&manifest_path).map_err(AppError::external)?;
    let manifest: HarvesterManifest =
        serde_json::from_str(&manifest_text).map_err(AppError::external)?;
    let invocation =
        resolve_harvester_invocation_with_settings(manifest_root, &manifest, settings)?;
    if let Some(runtime) = harvester_execution_runtime(&manifest) {
        ensure_adapter_runtime_available(&runtime, &invocation).await?;
    }

    fs::create_dir_all(work_dir).map_err(AppError::external)?;

    let mut env = vec![
        (
            "ASSETIWEAVE_HARVESTER_DIR".to_string(),
            work_dir.to_string_lossy().into_owned(),
        ),
        ("ASSETIWEAVE_HARVESTER_ID".to_string(), manifest.id.clone()),
    ];
    if full_reparse {
        env.push(("ASSETIWEAVE_FULL_REPARSE".to_string(), "1".to_string()));
    }
    let output = match crate::backend::host_process::run_host_command_async(
        crate::backend::host_process::HostCommandSpec {
            program: invocation.program.clone(),
            args: invocation.args.clone(),
            env,
            working_dir: Some(work_dir.to_path_buf()),
            stdin: crate::backend::host_process::HostInput::Null,
            timeout: Duration::from_millis(HARVESTER_TIMEOUT_MS),
            stdout_limit: OUTPUT_CAPTURE_LIMIT,
            stderr_limit: OUTPUT_CAPTURE_LIMIT,
        },
        cancellation,
    )
    .await
    {
        Ok(output) => output,
        Err(crate::backend::host_process::HostProcessError::MissingProgram { program }) => {
            return Err(AppError::external(format!(
                "run harvester {}: program not found: {}",
                manifest.id,
                program.display()
            )));
        }
        Err(crate::backend::host_process::HostProcessError::Spawn(error)) => {
            return Err(AppError::external(format!(
                "run harvester {}: {error}",
                manifest.id
            )));
        }
        Err(crate::backend::host_process::HostProcessError::Output(error)) => {
            return Err(AppError::external(format!(
                "capture harvester {} output: {error}",
                manifest.id
            )));
        }
        Err(crate::backend::host_process::HostProcessError::Timeout {
            stdout,
            stderr,
            stdout_truncated,
            stderr_truncated,
        }) => {
            let mut message = format!(
                "harvester {} timed out after {} ms",
                manifest.id, HARVESTER_TIMEOUT_MS
            );
            append_captured_output(&mut message, "stdout", &stdout, stdout_truncated);
            append_captured_output(&mut message, "stderr", &stderr, stderr_truncated);
            return Err(AppError::external(message));
        }
        Err(crate::backend::host_process::HostProcessError::Cancelled) => {
            return Err(AppError::Cancelled(format!(
                "harvester {} was cancelled",
                manifest.id
            )));
        }
        Err(crate::backend::host_process::HostProcessError::Cleanup(error)) => {
            return Err(AppError::external(format!(
                "cleanup harvester {} process: {error}",
                manifest.id
            )));
        }
        Err(crate::backend::host_process::HostProcessError::OutputLimitExceeded { .. }) => {
            return Err(AppError::external(format!(
                "harvester {} output exceeded configured limit",
                manifest.id
            )));
        }
    };
    if !output.status.success() {
        let mut message = format!(
            "harvester {} failed with status {}",
            manifest.id, output.status
        );
        append_captured_output(
            &mut message,
            "stdout",
            &output.stdout,
            output.stdout_truncated,
        );
        append_captured_output(
            &mut message,
            "stderr",
            &output.stderr,
            output.stderr_truncated,
        );
        return Err(AppError::external(message));
    }
    Ok(true)
}

pub(super) fn append_harvester_runtime_requirements(
    requirements: &mut Vec<(ConversationAdapterRuntimeKind, String)>,
    sources: &[ConversationSource],
) {
    for source in sources {
        if !source.enabled {
            continue;
        }
        let Ok(source_dir) = crate::backend::path_utils::expand_path(&source.location) else {
            continue;
        };
        let manifest_path = source_dir.join(HARVESTER_MANIFEST_FILE);
        let Ok(manifest_text) = fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_str::<HarvesterManifest>(&manifest_text) else {
            continue;
        };
        if manifest.runtime.is_some() && !manifest.entrypoint.is_empty() {
            continue;
        }
        let Some(runtime) = harvester_execution_runtime(&manifest) else {
            continue;
        };
        if validate_harvester_runtime(&manifest.id, &runtime).is_err() {
            continue;
        }
        let Some(version) = runtime.version.as_deref() else {
            continue;
        };
        if matches!(runtime.kind, ConversationAdapterRuntimeKind::Executable)
            || validate_runtime_version_constraint(version).is_err()
        {
            continue;
        }
        upsert_highest_runtime_requirement(requirements, &runtime.kind, version);
    }
    *requirements = sort_runtime_requirements(std::mem::take(requirements));
}

fn resolve_harvester_invocation_with_settings(
    root: &Path,
    manifest: &HarvesterManifest,
    settings: &Value,
) -> AppResult<AdapterCommandInvocation> {
    if manifest.runtime.is_some() && !manifest.entrypoint.is_empty() {
        return Err(AppError::external(format!(
            "harvester {} must not declare both runtime and entrypoint",
            manifest.id
        )));
    }
    if let Some(runtime) = manifest.runtime.as_ref() {
        validate_harvester_runtime(&manifest.id, runtime)?;
        validate_harvester_relative_entry(root, &manifest.id, "runtime entry", &runtime.entry)?;
        return Ok(build_adapter_runtime_invocation_with_settings(
            root,
            runtime,
            &[],
            settings,
        ));
    }
    let raw_command = manifest
        .entrypoint
        .first()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::external(format!("harvester {} has no entrypoint", manifest.id))
        })?;
    let command_path =
        validate_harvester_relative_entry(root, &manifest.id, "entrypoint", raw_command)?;
    if let Some(runtime) = harvester_execution_runtime(manifest) {
        return Ok(build_adapter_runtime_invocation_with_settings(
            root,
            &runtime,
            &[],
            settings,
        ));
    }
    Ok(AdapterCommandInvocation {
        program: command_path.clone(),
        args: manifest.entrypoint.iter().skip(1).cloned().collect(),
        display_path: command_path,
    })
}

fn harvester_execution_runtime(manifest: &HarvesterManifest) -> Option<ConversationAdapterRuntime> {
    if let Some(runtime) = manifest.runtime.as_ref() {
        return Some(runtime.clone());
    }
    let (entry, args) = manifest.entrypoint.split_first()?;
    if !is_javascript_harvester_entrypoint(Path::new(entry)) {
        return None;
    }
    Some(ConversationAdapterRuntime {
        kind: ConversationAdapterRuntimeKind::Node,
        entry: entry.trim().to_string(),
        args: args.to_vec(),
        version: Some(LEGACY_JAVASCRIPT_COMMAND_NODE_VERSION.to_string()),
    })
}

fn validate_harvester_runtime(
    harvester_id: &str,
    runtime: &ConversationAdapterRuntime,
) -> AppResult<()> {
    if runtime.entry.trim().is_empty() {
        return Err(AppError::external(format!(
            "harvester {harvester_id} runtime entry is required"
        )));
    }
    if runtime
        .version
        .as_deref()
        .is_some_and(|version| version.trim().is_empty())
    {
        return Err(AppError::external(format!(
            "harvester {harvester_id} runtime version must not be empty"
        )));
    }
    if let Some(version) = runtime.version.as_deref() {
        validate_runtime_version_constraint(version)?;
    }
    Ok(())
}

fn validate_harvester_relative_entry(
    root: &Path,
    harvester_id: &str,
    field: &str,
    raw: &str,
) -> AppResult<PathBuf> {
    let trimmed = raw.trim();
    let relative = Path::new(trimmed);
    if relative.is_absolute()
        || looks_like_windows_rooted_path(trimmed)
        || trimmed
            .split(['/', '\\'])
            .any(|component| component == "..")
    {
        return Err(AppError::external(format!(
            "unsafe harvester {field} for {harvester_id}: {raw}"
        )));
    }
    let path = root.join(relative);
    if !path.is_file() {
        return Err(AppError::external(format!(
            "harvester {field} not found for {harvester_id}: {}",
            path.to_string_lossy()
        )));
    }
    Ok(path)
}

fn looks_like_windows_rooted_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    if path.starts_with("\\\\") || path.starts_with('\\') || path.starts_with('/') {
        return true;
    }
    bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic()
}

fn is_javascript_harvester_entrypoint(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "cjs" | "js" | "mjs"
            )
        })
}

fn append_captured_output(message: &mut String, label: &str, bytes: &[u8], truncated: bool) {
    let text = String::from_utf8_lossy(bytes);
    if text.trim().is_empty() && !truncated {
        return;
    }
    message.push('\n');
    message.push_str(label);
    message.push_str(":\n");
    message.push_str(text.trim());
    if truncated {
        message.push_str("\n... [truncated]");
    }
}

#[cfg(test)]
#[path = "harvester_tests.rs"]
mod tests;
