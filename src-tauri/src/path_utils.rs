use crate::types::{AppResult, GitRepositoryInfo};
use assetiweave_core::AppKind;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, path::PathBuf, process::Command};
use walkdir::WalkDir;

pub(crate) fn app_db_path() -> AppResult<PathBuf> {
    let mut data_dir = dirs::data_dir().ok_or("无法确定系统数据目录")?;
    data_dir.push("AssetIWeave");
    fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
    Ok(data_dir.join("app.db"))
}

pub(crate) fn ensure_app_library_dirs() -> AppResult<()> {
    fs::create_dir_all(default_skill_backup_root()?).map_err(|error| error.to_string())
}

pub(crate) fn default_skill_backup_root() -> AppResult<PathBuf> {
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

pub(crate) fn git_repository_for_path(path: &Path) -> Option<GitRepositoryInfo> {
    let root = find_git_root(path)?;
    let remote_url = git_remote_url(&root);
    let web_url = remote_url
        .as_deref()
        .and_then(|remote| git_browser_url(remote, &root, path));
    Some(GitRepositoryInfo {
        root_path: root.to_string_lossy().to_string(),
        remote_url,
        web_url,
    })
}

fn git_remote_url(root: &Path) -> Option<String> {
    git_output(root, &["remote", "get-url", "origin"])
        .or_else(|| {
            let first_remote = git_output(root, &["remote"])?
                .lines()
                .map(str::trim)
                .find(|remote| !remote.is_empty())?
                .to_string();
            git_output(root, &["remote", "get-url", &first_remote])
        })
        .map(|remote| sanitize_git_remote(&remote))
}

fn sanitize_git_remote(remote: &str) -> String {
    let Some(scheme_end) = remote.find("://") else {
        return remote.to_string();
    };
    let authority_start = scheme_end + 3;
    let suffix = &remote[authority_start..];
    let authority_end = suffix.find(['/', '?', '#']).unwrap_or(suffix.len());
    let authority = &suffix[..authority_end];
    let Some(userinfo_end) = authority.rfind('@') else {
        return remote.to_string();
    };

    format!(
        "{}://{}{}",
        &remote[..scheme_end],
        &authority[userinfo_end + 1..],
        &suffix[authority_end..]
    )
}

pub(crate) fn git_browser_url(remote: &str, root: &Path, path: &Path) -> Option<String> {
    let repo_base = github_repo_base(remote)?;
    let branch = git_current_branch(root).unwrap_or_else(|| "HEAD".to_string());
    let relative = path.strip_prefix(root).ok()?;
    let relative = normalize_relative_path(relative);
    let branch = encode_url_component(&branch);
    if relative.is_empty() {
        Some(format!("{repo_base}/tree/{branch}"))
    } else {
        Some(format!(
            "{repo_base}/tree/{branch}/{}",
            encode_url_path(&relative)
        ))
    }
}

fn github_repo_base(remote: &str) -> Option<String> {
    let trimmed = remote.trim().trim_end_matches(".git");
    if let Some(path) = trimmed.strip_prefix("git@github.com:") {
        return Some(format!("https://github.com/{path}"));
    }
    if let Some(path) = trimmed.strip_prefix("ssh://git@github.com/") {
        return Some(format!("https://github.com/{path}"));
    }
    trimmed
        .strip_prefix("https://github.com/")
        .map(|path| format!("https://github.com/{path}"))
        .or_else(|| {
            trimmed
                .strip_prefix("http://github.com/")
                .map(|path| format!("https://github.com/{path}"))
        })
}

fn git_current_branch(root: &Path) -> Option<String> {
    git_output(root, &["branch", "--show-current"])
        .or_else(|| {
            git_output(root, &["rev-parse", "--abbrev-ref", "HEAD"])
                .filter(|branch| branch != "HEAD")
        })
        .or_else(|| {
            git_output(
                root,
                &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
            )
            .and_then(|branch| branch.strip_prefix("origin/").map(str::to_string))
        })
}

fn git_output(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn encode_url_path(path: &str) -> String {
    path.split('/')
        .map(encode_url_component)
        .collect::<Vec<_>>()
        .join("/")
}

fn encode_url_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

pub(crate) fn detect_app_target(path: &Path) -> Option<AppKind> {
    let home = dirs::home_dir()?;
    let normalized = normalize_absolute_path(path);
    let candidates = [
        (home.join(".codex").join("skills"), AppKind::Codex),
        (home.join(".claude").join("skills"), AppKind::Claude),
        (
            home.join(".config").join("opencode").join("skills"),
            AppKind::OpenCode,
        ),
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
    let Ok(library_root) = default_skill_backup_root() else {
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
    use super::{git_repository_for_path, hash_path, sanitize_git_remote};
    use std::{fs, path::PathBuf, process::Command};

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

    #[test]
    fn git_repository_for_path_prefers_the_nearest_nested_repository() {
        let root = unique_temp_dir("assetiweave-git-nested-test");
        let outer_repo = root.join("outer");
        let nested_repo = outer_repo.join("repos").join("nested");
        let nested_skill = nested_repo.join("skills").join("demo");
        fs::create_dir_all(&nested_skill).expect("create nested skill");
        init_git_repo(&outer_repo, "https://example.com/outer.git");
        init_git_repo(&nested_repo, "git@example.com:nested.git");

        let repository = git_repository_for_path(&nested_skill).expect("resolve nested repository");

        assert_eq!(PathBuf::from(repository.root_path), nested_repo);
        assert_eq!(
            repository.remote_url.as_deref(),
            Some("git@example.com:nested.git")
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn git_repository_for_path_reads_the_source_root_repository() {
        let root = unique_temp_dir("assetiweave-git-root-test");
        let skill = root.join("skills").join("demo");
        fs::create_dir_all(&skill).expect("create skill");
        init_git_repo(&root, "https://example.com/root.git");

        let repository = git_repository_for_path(&skill).expect("resolve source repository");

        assert_eq!(PathBuf::from(repository.root_path), root);
        assert_eq!(
            repository.remote_url.as_deref(),
            Some("https://example.com/root.git")
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn git_remote_display_removes_embedded_http_credentials() {
        assert_eq!(
            sanitize_git_remote("https://oauth2:secret@example.com/private/repo.git"),
            "https://example.com/private/repo.git"
        );
        assert_eq!(
            sanitize_git_remote("git@github.com:util6/util6-agents.git"),
            "git@github.com:util6/util6-agents.git"
        );
    }

    fn init_git_repo(path: &PathBuf, remote_url: &str) {
        fs::create_dir_all(path).expect("create repository directory");
        let init = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(path)
            .status()
            .expect("run git init");
        assert!(init.success());
        let remote = Command::new("git")
            .args(["remote", "add", "origin", remote_url])
            .current_dir(path)
            .status()
            .expect("add git remote");
        assert!(remote.success());
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()))
    }
}
