use std::path::{Path, PathBuf};

use crate::backend::infrastructure::agent_market::error::AgentMarketError;

pub(crate) fn default_runtime_root() -> Result<PathBuf, AgentMarketError> {
    let home = dirs::home_dir().ok_or_else(|| {
        AgentMarketError::new(
            "runtime_root_unavailable",
            "The user runtime directory is unavailable.",
            true,
        )
    })?;
    Ok(home.join(".assetiweave").join("agent-runtimes"))
}

pub(crate) fn ensure_runtime_root(path: &Path) -> Result<(), AgentMarketError> {
    std::fs::create_dir_all(path).map_err(|error| {
        let message = format!("{error}");
        AgentMarketError::new("runtime_root_unavailable", &message, true)
    })
}

pub(crate) fn is_safe_managed_install_path(
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
