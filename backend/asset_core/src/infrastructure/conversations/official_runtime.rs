//! Host-owned conversation runtime helpers, externalized into the managed
//! user directory.
//!
//! The scripts embedded here (`payload-policy.mjs`, `reproject-runner.mjs`,
//! `shell-projector.cjs`, `projector-adapter.cjs` and their manifest) drive the
//! host's own command projection and legacy session reprojection. They are not
//! adapter plugin copies: the authoritative copy of every conversation adapter
//! lives in the managed user directory (`conversation-adapters/**`) and is only
//! ever read by [`super::official`].
//!
//! The authoritative location for these helpers is
//! `$ASSETIWEAVE_HOME/conversation-adapters/common/`. Runners resolve the
//! external file directly, so a user edit takes effect on the next sync without
//! recompiling the binary. The embedded copies are released only when the
//! target file is missing; an existing file is never overwritten, so user edits
//! stay authoritative.

use crate::backend::infrastructure::app_settings::conversation_adapter_dir;
use crate::backend::{
    domain::{ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState},
    infrastructure::error::{InfraError, InfraResult},
};
use chrono::Utc;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

pub const COMMON_RUNTIME_DIR_NAME: &str = "common";
pub const PAYLOAD_POLICY_FILE_NAME: &str = "payload-policy.mjs";
pub const REPROJECT_RUNNER_FILE_NAME: &str = "reproject-runner.mjs";
pub const SHELL_PROJECTOR_FILE_NAME: &str = "shell-projector.cjs";
pub const SHELL_PROJECTOR_ADAPTER_FILE_NAME: &str = "projector-adapter.cjs";
pub const SHELL_PROJECTOR_MANIFEST_FILE_NAME: &str = "projector-manifest.json";

/// Every host-owned helper that lives in `common/`.
pub const COMMON_RUNTIME_FILES: [&str; 5] = [
    PAYLOAD_POLICY_FILE_NAME,
    REPROJECT_RUNNER_FILE_NAME,
    SHELL_PROJECTOR_FILE_NAME,
    SHELL_PROJECTOR_ADAPTER_FILE_NAME,
    SHELL_PROJECTOR_MANIFEST_FILE_NAME,
];

const PAYLOAD_POLICY_SCRIPT: &str =
    include_str!("../../../../../builtin-assets/adapters/common/payload-policy.mjs");
const REPROJECT_RUNNER_SCRIPT: &str =
    include_str!("../../../../../builtin-assets/adapters/common/reproject-runner.mjs");
const SHELL_PROJECTOR_SCRIPT: &str =
    include_str!("../../../../../builtin-assets/adapters/common/shell-projector.cjs");
const SHELL_PROJECTOR_ADAPTER_SCRIPT: &str =
    include_str!("../../../../../builtin-assets/adapters/common/projector-adapter.cjs");
const SHELL_PROJECTOR_MANIFEST: &str =
    include_str!("../../../../../builtin-assets/adapters/common/projector-manifest.json");

static COMMON_RUNTIME_MATERIALIZE_LOCK: Mutex<()> = Mutex::new(());

fn lock_materialization() -> InfraResult<std::sync::MutexGuard<'static, ()>> {
    COMMON_RUNTIME_MATERIALIZE_LOCK
        .lock()
        .map_err(|_| InfraError::external("conversation runtime helper lock poisoned"))
}

/// Resolve the authoritative `common/` directory inside the managed root.
pub fn common_runtime_dir() -> InfraResult<PathBuf> {
    Ok(conversation_adapter_dir()?.join(COMMON_RUNTIME_DIR_NAME))
}

/// Ensure every host-owned helper exists on disk and return the `common/` dir.
pub fn ensure_common_runtime_scripts() -> InfraResult<PathBuf> {
    let root = common_runtime_dir()?;
    ensure_common_runtime_scripts_at(&root)
}

/// Same as [`ensure_common_runtime_scripts`] against an explicit root.
pub fn ensure_common_runtime_scripts_at(root: &Path) -> InfraResult<PathBuf> {
    let _guard = lock_materialization()?;
    materialize_common_runtime_scripts_at(root)
}

/// Release missing fallback helpers into `root` without touching existing files.
///
/// The caller is responsible for serialization; public entry points go through
/// [`ensure_common_runtime_scripts_at`].
pub fn materialize_common_runtime_scripts_at(root: &Path) -> InfraResult<PathBuf> {
    fs::create_dir_all(root)?;
    write_if_missing(
        &root.join(PAYLOAD_POLICY_FILE_NAME),
        PAYLOAD_POLICY_SCRIPT.as_bytes(),
    )?;
    let runner_created = write_if_missing(
        &root.join(REPROJECT_RUNNER_FILE_NAME),
        REPROJECT_RUNNER_SCRIPT.as_bytes(),
    )?;
    write_if_missing(
        &root.join(SHELL_PROJECTOR_FILE_NAME),
        SHELL_PROJECTOR_SCRIPT.as_bytes(),
    )?;
    let adapter_created = write_if_missing(
        &root.join(SHELL_PROJECTOR_ADAPTER_FILE_NAME),
        SHELL_PROJECTOR_ADAPTER_SCRIPT.as_bytes(),
    )?;
    write_if_missing(
        &root.join(SHELL_PROJECTOR_MANIFEST_FILE_NAME),
        SHELL_PROJECTOR_MANIFEST.as_bytes(),
    )?;
    if runner_created {
        make_executable(&root.join(REPROJECT_RUNNER_FILE_NAME))?;
    }
    if adapter_created {
        make_executable(&root.join(SHELL_PROJECTOR_ADAPTER_FILE_NAME))?;
    }
    Ok(root.to_path_buf())
}

/// Resolve the reproject runner from `common/`, releasing the fallback only
/// when the external script is missing.
pub fn ensure_reproject_runner_scripts() -> InfraResult<PathBuf> {
    let root = ensure_common_runtime_scripts()?;
    Ok(root.join(REPROJECT_RUNNER_FILE_NAME))
}

/// Resolve the core shell command projector from `common/`, releasing missing
/// helpers only when needed.
pub fn ensure_shell_command_projector() -> InfraResult<ConversationAdapter> {
    let root = ensure_common_runtime_scripts()?;
    validate_common_shell_projector(&root)
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

fn validate_common_shell_projector(root: &Path) -> InfraResult<ConversationAdapter> {
    let manifest_path = root.join(SHELL_PROJECTOR_MANIFEST_FILE_NAME);
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

/// Release a fallback helper file only when it is missing.
///
/// Returns whether the file was written. An existing file is never touched —
/// neither its bytes nor its permissions.
fn write_if_missing(path: &Path, bytes: &[u8]) -> InfraResult<bool> {
    if path.exists() {
        return Ok(false);
    }
    fs::write(path, bytes).map_err(InfraError::from)?;
    Ok(true)
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
