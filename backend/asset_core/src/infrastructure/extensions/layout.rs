use std::path::{Path, PathBuf};

use crate::backend::infrastructure::agent_market::error::AgentMarketError;

pub fn default_runtime_root() -> Result<PathBuf, AgentMarketError> {
    if let Ok(home) = crate::backend::infrastructure::path_utils::app_home_dir() {
        return Ok(home.join("agent-runtimes"));
    }
    let home = dirs::home_dir().ok_or_else(|| {
        AgentMarketError::new(
            "runtime_root_unavailable",
            "The user runtime directory is unavailable.",
            true,
        )
    })?;
    Ok(home.join(".assetiweave").join("agent-runtimes"))
}

pub fn ensure_runtime_root(path: &Path) -> Result<(), AgentMarketError> {
    std::fs::create_dir_all(path).map_err(|error| {
        let message = format!("{error}");
        AgentMarketError::new("runtime_root_unavailable", &message, true)
    })
}

pub fn is_safe_managed_install_path(
    runtime_root: &Path,
    installation_id: &str,
    install_dir: &Path,
) -> bool {
    if uuid::Uuid::parse_str(installation_id).is_err() {
        return false;
    }
    let expected = runtime_root.join("active").join(installation_id);
    if install_dir != expected || install_dir == runtime_root {
        return false;
    }
    if std::fs::symlink_metadata(install_dir)
        .ok()
        .is_some_and(|metadata| metadata.file_type().is_symlink())
    {
        return false;
    }
    if !install_dir.exists() {
        return true;
    }
    let Ok(root) = runtime_root.canonicalize() else {
        return false;
    };
    let Ok(active) = root.join("active").canonicalize() else {
        return false;
    };
    let Ok(canonical_install_dir) = install_dir.canonicalize() else {
        return false;
    };
    canonical_install_dir.starts_with(&active)
        && canonical_install_dir != root
        && canonical_install_dir == active.join(installation_id)
}

pub const STAGING_RETENTION: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

pub fn cleanup_runtime_directories(runtime_root: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    let now = std::time::SystemTime::now();
    let staging = runtime_root.join(".staging");
    if let Ok(entries) = std::fs::read_dir(&staging) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_old = entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age > STAGING_RETENTION);
            if is_old
                && entry
                    .file_type()
                    .map(|file_type| file_type.is_dir() && !file_type.is_symlink())
                    .unwrap_or(false)
            {
                if let Err(error) = std::fs::remove_dir_all(&path) {
                    warnings.push(format!("stale staging cleanup pending: {error}"));
                }
            }
        }
    }
    warnings
}
