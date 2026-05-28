use crate::types::AppResult;
use assetiweave_core::AppKind;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, path::PathBuf};
use walkdir::WalkDir;

pub(crate) fn app_db_path() -> AppResult<PathBuf> {
    let mut data_dir = dirs::data_dir().ok_or("无法确定系统数据目录")?;
    data_dir.push("AssetIWeave");
    fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
    Ok(data_dir.join("app.db"))
}

pub(crate) fn ensure_app_library_dirs() -> AppResult<()> {
    fs::create_dir_all(app_library_skill_root()?).map_err(|error| error.to_string())
}

pub(crate) fn app_library_skill_root() -> AppResult<PathBuf> {
    let home = dirs::home_dir().ok_or("无法确定用户主目录")?;
    Ok(home.join(".assetiweave").join("library").join("skills"))
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

pub(crate) fn find_git_root(path: &Path) -> Option<PathBuf> {
    let mut current = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };

    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

pub(crate) fn detect_app_target(path: &Path) -> Option<AppKind> {
    let home = dirs::home_dir()?;
    let normalized = normalize_absolute_path(path);
    let candidates = [
        (home.join(".codex").join("skills"), AppKind::Codex),
        (home.join(".claude").join("skills"), AppKind::Claude),
        (home.join(".opencode").join("skills"), AppKind::OpenCode),
        (home.join(".gemini").join("skills"), AppKind::Gemini),
        (
            home.join(".antigravity").join("skills"),
            AppKind::Antigravity,
        ),
        (home.join(".openclaw").join("skills"), AppKind::OpenClaw),
    ];

    candidates
        .into_iter()
        .find(|(candidate, _)| normalized.starts_with(&normalize_absolute_path(candidate)))
        .map(|(_, app_kind)| app_kind)
}

pub(crate) fn is_app_library_path(path: &Path) -> bool {
    let Ok(library_root) = app_library_skill_root() else {
        return false;
    };
    normalize_absolute_path(path).starts_with(&normalize_absolute_path(&library_root))
}

pub(crate) fn normalize_relative_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn normalize_absolute_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn hash_file(path: &Path) -> AppResult<String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

pub(crate) fn hash_path(path: &Path) -> AppResult<String> {
    if path.is_dir() {
        hash_dir(path)
    } else {
        hash_file(path)
    }
}

fn hash_dir(path: &Path) -> AppResult<String> {
    let mut files = Vec::new();
    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        files.push(entry.path().to_path_buf());
    }
    files.sort();

    let mut hasher = Sha256::new();
    for file in files {
        let relative = file.strip_prefix(path).map_err(|error| error.to_string())?;
        hasher.update(normalize_relative_path(relative).as_bytes());
        hasher.update(b"\0");
        hasher.update(fs::read(file).map_err(|error| error.to_string())?);
        hasher.update(b"\0");
    }

    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::hash_path;
    use std::{fs, path::PathBuf};

    #[test]
    fn hash_path_changes_when_directory_file_changes() {
        let root = unique_temp_dir("assetiweave-hash-test");
        fs::create_dir_all(&root).expect("create temp dir");
        let skill_file = root.join("SKILL.md");
        let script_file = root.join("script.sh");
        fs::write(&skill_file, "skill").expect("write skill");
        fs::write(&script_file, "one").expect("write script");

        let first_hash = hash_path(&root).expect("hash dir");
        fs::write(&script_file, "two").expect("update script");
        let second_hash = hash_path(&root).expect("hash dir again");

        fs::remove_dir_all(&root).ok();
        assert_ne!(first_hash, second_hash);
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()))
    }
}
