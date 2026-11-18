//! Repository-to-managed-directory synchronization for developer upgrades.
//!
//! A developer runs the CLI from a checkout, where the editable adapter sources
//! live under `builtin-assets/adapters/`. Every trust decision still happens in
//! the managed user directory (#202): the checkout is copied into
//! `$ASSETIWEAVE_HOME/conversation-adapters/<adapter_id>/` plus the host-owned
//! `common/` helpers first, and only then promoted through the same immutable
//! snapshot pipeline as user-state upgrades.
//!
//! Only the requested adapter directories are replaced. Unrelated managed
//! packages stay byte-for-byte untouched, and every copy is staged in a hidden
//! transfer directory before it is swapped into place so a failed sync never
//! leaves a half-written workspace behind.

use super::*;
use crate::backend::application::prelude::*;
use crate::backend::infrastructure::conversations::{
    COMMON_RUNTIME_DIR_NAME, COMMON_RUNTIME_FILES,
};

/// Managed-directory children that are host infrastructure, not editable
/// adapter workspaces. They must never be promoted as workspace packages.
pub const RESERVED_MANAGED_DIR_NAMES: &[&str] = &["packages", "staging", "common", "runtime"];

/// Whether a managed-directory child is reserved infrastructure.
///
/// Hidden entries are transient transfer/backup directories created by this
/// module and are excluded for the same reason.
pub fn is_reserved_managed_entry(name: &str) -> bool {
    name.starts_with('.') || RESERVED_MANAGED_DIR_NAMES.contains(&name)
}

/// Copy repository adapter packages and the repository `common/` helpers into
/// the managed user directory.
///
/// Returns the managed destination of every synced package, in input order, so
/// callers promote exactly what they just staged.
pub fn sync_developer_workspaces(
    adapter_root: &Path,
    managed_root: &Path,
    package_dirs: &[PathBuf],
) -> AppResult<Vec<PathBuf>> {
    fs::create_dir_all(managed_root).map_err(AppError::external)?;
    let transfer_root = managed_root.join(format!(".dev-sync-{}", short_uuid()));
    fs::create_dir_all(&transfer_root).map_err(AppError::external)?;

    let result: AppResult<Vec<PathBuf>> = (|| {
        let mut staged = Vec::with_capacity(package_dirs.len());
        for source in package_dirs {
            let validation =
                crate::backend::infrastructure::conversations::validate_conversation_adapter_package_dir(
                    source,
                )
                .map_err(AppError::external)?;
            let adapter_id = validation.adapter_validation.manifest.id.clone();
            let staged_dir = transfer_root.join(&adapter_id);
            crate::backend::infrastructure::filesystem::copy_dir(source, &staged_dir)?;
            staged.push((adapter_id, staged_dir));
        }

        // Refresh the shared helpers before any package is promoted so the
        // snapshot freezes the repository revision the developer is editing.
        sync_common_scripts(adapter_root, managed_root)?;

        let mut synced = Vec::with_capacity(staged.len());
        for (adapter_id, staged_dir) in staged {
            let target = managed_root.join(&adapter_id);
            replace_directory(&staged_dir, &target)?;
            synced.push(target);
        }
        Ok(synced)
    })();

    let _ = fs::remove_dir_all(&transfer_root);
    result
}

fn sync_common_scripts(adapter_root: &Path, managed_root: &Path) -> AppResult<()> {
    let source_common = adapter_root.join(COMMON_RUNTIME_DIR_NAME);
    if !source_common.is_dir() {
        return Err(AppError::Validation(format!(
            "repository common scripts directory was not found: {}",
            source_common.display()
        )));
    }
    let target_common = managed_root.join(COMMON_RUNTIME_DIR_NAME);
    fs::create_dir_all(&target_common).map_err(AppError::external)?;

    for name in COMMON_RUNTIME_FILES {
        let source = source_common.join(name);
        if !source.is_file() {
            continue;
        }
        let temporary = target_common.join(format!(".{name}.tmp-{}", short_uuid()));
        fs::copy(&source, &temporary).map_err(AppError::external)?;
        let destination = target_common.join(name);
        if destination.is_file() {
            fs::remove_file(&destination).map_err(AppError::external)?;
        }
        fs::rename(&temporary, destination).map_err(AppError::external)?;
    }
    Ok(())
}

/// Swap a fully staged directory into its final location.
///
/// The previous revision is parked next to the target first; if the final
/// rename fails the old directory is restored, so the managed root keeps a
/// complete workspace either way.
fn replace_directory(staged_dir: &Path, target: &Path) -> AppResult<()> {
    let target_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("adapter");
    let backup = target.with_file_name(format!(".{target_name}.backup-{}", short_uuid()));
    let had_target = target.exists();
    if had_target {
        fs::rename(target, &backup).map_err(AppError::external)?;
    }
    match fs::rename(staged_dir, target) {
        Ok(()) => {
            if had_target {
                let _ = fs::remove_dir_all(&backup);
            }
            Ok(())
        }
        Err(error) => {
            if had_target {
                let _ = fs::rename(&backup, target);
            }
            Err(AppError::external(error))
        }
    }
}
