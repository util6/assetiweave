use crate::types::AppResult;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, path::PathBuf};

pub(crate) fn app_db_path() -> AppResult<PathBuf> {
    let mut data_dir = dirs::data_dir().ok_or("无法确定系统数据目录")?;
    data_dir.push("AssetIWeave");
    fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
    Ok(data_dir.join("app.db"))
}

pub(crate) fn expand_path(path: &str) -> AppResult<PathBuf> {
    #[cfg(windows)]
    if let Some(rest) = path.strip_prefix("%USERPROFILE%\\") {
        let home = dirs::home_dir().ok_or("无法确定用户主目录")?;
        return Ok(home.join(rest));
    }

    #[cfg(windows)]
    if let Some(rest) = path.strip_prefix("%USERPROFILE%/") {
        let home = dirs::home_dir().ok_or("无法确定用户主目录")?;
        return Ok(home.join(rest));
    }

    if let Some(rest) = path.strip_prefix("~/") {
        let home = dirs::home_dir().ok_or("无法确定用户主目录")?;
        return Ok(home.join(rest));
    }

    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        Ok(candidate)
    } else {
        let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
        let direct = cwd.join(&candidate);
        if direct.exists() {
            return Ok(direct);
        }
        if let Some(parent) = cwd.parent() {
            let parent_candidate = parent.join(&candidate);
            if parent_candidate.exists() {
                return Ok(parent_candidate);
            }
        }
        Ok(direct)
    }
}

pub(crate) fn normalize_relative_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn hash_file(path: &Path) -> AppResult<String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}
