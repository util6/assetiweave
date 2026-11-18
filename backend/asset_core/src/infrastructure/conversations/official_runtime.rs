//! Core runtime helpers that shell out to bundled JavaScript entrypoints.
//!
//! These are not adapter copies: `runtime/shell-projector-v1` and
//! `runtime/reproject-runner-v1` are host-owned snapshots used for command
//! projection and legacy session reprojection. Moving their externalization to
//! the managed `common/` directory is tracked separately; this module keeps the
//! current behaviour untouched while `official.rs` drops adapter embedding.

use crate::backend::infrastructure::app_settings::conversation_adapter_dir;
use crate::backend::{
    domain::{ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState},
    infrastructure::error::{InfraError, InfraResult},
};
use chrono::Utc;
use std::{fs, path::Path, sync::Mutex};

const PAYLOAD_POLICY_SCRIPT: &str =
    include_str!("../../../../../builtin-assets/adapters/common/payload-policy.mjs");
const REPROJECT_RUNNER_SCRIPT: &str =
    include_str!("../../../../../builtin-assets/adapters/common/reproject-runner.mjs");
const REPROJECT_RUNNER_RUNTIME_VERSION: &str = "reproject-runner-v1";
const SHELL_PROJECTOR_SCRIPT: &str =
    include_str!("../../../../../builtin-assets/adapters/common/shell-projector.cjs");
const SHELL_PROJECTOR_ADAPTER_SCRIPT: &str =
    include_str!("../../../../../builtin-assets/adapters/common/projector-adapter.cjs");
const SHELL_PROJECTOR_MANIFEST: &str =
    include_str!("../../../../../builtin-assets/adapters/common/projector-manifest.json");
const SHELL_PROJECTOR_RUNTIME_VERSION: &str = "shell-projector-v1";
static SHELL_PROJECTOR_MATERIALIZE_LOCK: Mutex<()> = Mutex::new(());
static REPROJECT_RUNNER_MATERIALIZE_LOCK: Mutex<()> = Mutex::new(());

pub fn ensure_shell_command_projector() -> InfraResult<ConversationAdapter> {
    let _guard = SHELL_PROJECTOR_MATERIALIZE_LOCK.lock().map_err(|_| {
        InfraError::external("shell command projector materialization lock poisoned")
    })?;
    let root = conversation_adapter_dir()?
        .join("runtime")
        .join(SHELL_PROJECTOR_RUNTIME_VERSION);
    materialize_shell_command_projector(&root)
}

pub fn ensure_reproject_runner_scripts() -> InfraResult<std::path::PathBuf> {
    let _guard = REPROJECT_RUNNER_MATERIALIZE_LOCK
        .lock()
        .map_err(|_| InfraError::external("reproject runner materialization lock poisoned"))?;
    let root = conversation_adapter_dir()?
        .join("runtime")
        .join(REPROJECT_RUNNER_RUNTIME_VERSION);
    fs::create_dir_all(&root)?;
    let policy_path = root.join("payload-policy.mjs");
    let runner_path = root.join("reproject-runner.mjs");
    write_managed_runtime_file(&policy_path, PAYLOAD_POLICY_SCRIPT.as_bytes())?;
    write_managed_runtime_file(&runner_path, REPROJECT_RUNNER_SCRIPT.as_bytes())?;
    make_executable(&runner_path)?;
    Ok(runner_path)
}

pub async fn run_reproject_runner(
    runner_path: &Path,
    node_program: &Path,
    payload_json: &[u8],
) -> InfraResult<Vec<u8>> {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;

    let mut command = tokio::process::Command::new(node_program);
    command
        .arg(runner_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|err| {
        InfraError::external(format!(
            "failed to spawn reproject runner with {}: {err}",
            node_program.display()
        ))
    })?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(payload_json)
            .await
            .map_err(InfraError::from)?;
        stdin.shutdown().await.map_err(InfraError::from)?;
    }
    let output = child.wait_with_output().await.map_err(InfraError::from)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(InfraError::external(format!(
            "reproject runner failed ({}): {}",
            output.status,
            stderr.trim()
        )));
    }
    Ok(output.stdout)
}

fn materialize_shell_command_projector(root: &Path) -> InfraResult<ConversationAdapter> {
    fs::create_dir_all(root)?;
    let manifest_path = root.join("conversation-adapter.json");
    let adapter_path = root.join("projector-adapter.cjs");
    let projector_path = root.join("shell-projector.cjs");
    write_managed_runtime_file(&manifest_path, SHELL_PROJECTOR_MANIFEST.as_bytes())?;
    write_managed_runtime_file(&adapter_path, SHELL_PROJECTOR_ADAPTER_SCRIPT.as_bytes())?;
    write_managed_runtime_file(&projector_path, SHELL_PROJECTOR_SCRIPT.as_bytes())?;
    make_executable(&adapter_path)?;

    let validation =
        super::external::validate_external_adapter_manifest(&manifest_path.to_string_lossy())?;
    let now = Utc::now().to_rfc3339();
    Ok(ConversationAdapter {
        id: validation.manifest.id.clone(),
        name: validation.manifest.name.clone(),
        kind: ConversationAdapterKind::External,
        version: validation.manifest.version.clone(),
        enabled: true,
        manifest_path: Some(validation.manifest_path.clone()),
        executable_path: Some(validation.executable_path.clone()),
        content_hash: Some(validation.content_hash.clone()),
        trusted_hash: Some(validation.content_hash.clone()),
        trust_state: ConversationAdapterTrustState::BuiltIn,
        protocol_version: Some(validation.manifest.protocol_version),
        capabilities: validation.manifest.capabilities.clone(),
        input_kinds: validation.manifest.input_kinds.clone(),
        card_contract_version: validation.manifest.card_contract_version,
        card_kinds: validation.manifest.card_kinds.clone(),
        projection_version: validation.manifest.projection_version,
        created_at: now.clone(),
        updated_at: now,
    })
}

fn write_managed_runtime_file(path: &Path, bytes: &[u8]) -> InfraResult<()> {
    if fs::read(path).ok().as_deref() == Some(bytes) {
        return Ok(());
    }
    fs::write(path, bytes).map_err(InfraError::from)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> InfraResult<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(InfraError::from)
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> InfraResult<()> {
    Ok(())
}

#[cfg(test)]
#[path = "official_runtime_tests.rs"]
mod tests;
