//! Test-only helpers that stage adapter package fixtures on disk.
//!
//! Production binaries never ship adapter script copies; the release pipeline
//! installs real packages into the managed user directory. Tests still need a
//! realistic package, so they copy the repository fixtures from
//! `builtin-assets/adapters` into an isolated `ASSETIWEAVE_HOME`.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Adapters that used to be embedded in the binary and therefore always
/// present for tests before the managed-directory discovery refactor.
const BUILTIN_ADAPTER_FIXTURES: &[&str] = &[
    "codex",
    "claude-code",
    "opencode",
    "antigravity",
    "zcode",
    "codebuddy",
];

fn builtin_adapters_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../builtin-assets/adapters")
        .canonicalize()
        .unwrap_or_else(|_| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../builtin-assets/adapters")
        })
}

/// Copy one repository adapter package into `target_dir`.
pub fn copy_builtin_adapter(adapter_id: &str, target_dir: &Path) -> io::Result<()> {
    copy_tree(&builtin_adapters_root().join(adapter_id), target_dir)
}

/// Stage every historical built-in adapter under `managed_root` when missing.
///
/// Existing files are left untouched so a test that edits an adapter keeps its
/// revision across repeated bootstrap passes.
pub fn materialize_builtin_adapter_fixtures(managed_root: &Path) -> io::Result<()> {
    for adapter_id in BUILTIN_ADAPTER_FIXTURES {
        copy_missing_tree(
            &builtin_adapters_root().join(adapter_id),
            &managed_root.join(adapter_id),
        )?;
    }
    Ok(())
}

fn copy_tree(source: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let destination = target.join(entry.file_name());
        if file_type.is_dir() {
            copy_tree(&entry.path(), &destination)?;
        } else {
            fs::copy(entry.path(), &destination)?;
        }
    }
    Ok(())
}

fn copy_missing_tree(source: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let destination = target.join(entry.file_name());
        if file_type.is_dir() {
            copy_missing_tree(&entry.path(), &destination)?;
        } else if !destination.exists() {
            fs::copy(entry.path(), &destination)?;
        }
    }
    Ok(())
}
