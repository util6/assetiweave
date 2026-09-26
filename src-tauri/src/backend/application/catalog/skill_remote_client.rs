use super::skill_remote_search::github_tree_sha_for_skill_path;
use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use std::time::Duration;

#[derive(Debug, Clone)]
pub(crate) struct GitHubSkillLocation {
    pub(crate) repo: String,
    pub(crate) repo_url: String,
    pub(crate) branch: Option<String>,
    pub(crate) path: Option<String>,
}

impl GitHubSkillLocation {
    pub(crate) fn skill_name_hint(&self) -> Option<String> {
        self.path
            .as_deref()
            .and_then(|path| path.split('/').next_back())
            .filter(|name| !name.is_empty())
            .map(str::to_string)
    }

    pub(crate) fn skill_path_hint(&self, staging_dir: &Path) -> PathBuf {
        self.path
            .as_deref()
            .map(|path| staging_dir.join(path))
            .unwrap_or_else(|| staging_dir.to_path_buf())
    }
}

pub(crate) fn github_get_json(url: &str, context: &str) -> AppResult<Value> {
    let client = crate::backend::infrastructure::http_client::shared_http_client()?;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::USER_AGENT,
        reqwest::header::HeaderValue::from_static("AssetIWeave/0.1 skill-search"),
    );
    headers.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/vnd.github+json"),
    );
    if let Some(token) = github_api_token() {
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(AppError::external)?,
        );
    }
    let response = crate::backend::infrastructure::http_client::get_with_redirects(
        &client,
        url,
        headers,
        std::time::Duration::from_secs(15),
    )
    .map_err(|error| AppError::External(format!("{context} request failed: {error}")))?;
    let response = response
        .error_for_status()
        .map_err(|error| AppError::External(format!("{context} request failed: {error}")))?;
    response
        .json()
        .map_err(|error| AppError::External(format!("{context} response was not JSON: {error}")))
}

pub(crate) fn check_skill_remote_source(mut source: SkillRemoteSource) -> SkillRemoteSource {
    source.last_checked_at = Some(Utc::now().to_rfc3339());
    if source.provider != "github" {
        source.status = "error".to_string();
        source.message = Some(format!(
            "unsupported Skill remote provider: {}",
            source.provider
        ));
        return source;
    }

    let Some(full_name) = github_full_name_from_repo_url(&source.repo_url) else {
        source.status = "error".to_string();
        source.message = Some(format!(
            "unsupported GitHub repository URL: {}",
            source.repo_url
        ));
        return source;
    };
    let url = format!(
        "https://api.github.com/repos/{}/git/trees/{}?recursive=1",
        full_name,
        percent_encode_path_segment(&source.branch)
    );
    match github_get_json(&url, "GitHub skill drift check")
        .and_then(|value| github_tree_sha_for_skill_path(&value, source.path.as_deref()))
    {
        Ok(latest_tree_sha) => {
            source.latest_tree_sha = Some(latest_tree_sha.clone());
            match source.acquired_tree_sha.as_deref() {
                Some(acquired_tree_sha) if acquired_tree_sha == latest_tree_sha => {
                    source.status = "current".to_string();
                    source.message = Some("Remote Skill matches acquired tree".to_string());
                }
                Some(_) => {
                    source.status = "changed".to_string();
                    source.message = Some("Remote Skill changed since acquisition".to_string());
                }
                None => {
                    source.status = "unknown".to_string();
                    source.message =
                        Some("Remote Skill was acquired before tree SHA tracking".to_string());
                }
            }
        }
        Err(error) => {
            source.status = "error".to_string();
            source.message = Some(error.to_string());
        }
    }
    source
}

fn github_full_name_from_repo_url(repo_url: &str) -> Option<String> {
    let path = repo_url
        .trim()
        .trim_end_matches('/')
        .strip_prefix("https://github.com/")?
        .trim_end_matches(".git");
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        Some(format!("{}/{}", parts[0], parts[1]))
    } else {
        None
    }
}

fn github_api_token() -> Option<String> {
    env::var("GITHUB_TOKEN")
        .or_else(|_| env::var("GH_TOKEN"))
        .ok()
        .and_then(|token| clean_non_empty_string(&token))
}

pub(crate) fn percent_encode_query(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char);
            }
            b' ' => encoded.push('+'),
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

pub(crate) fn percent_encode_path_segment(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char);
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

pub(crate) fn parse_github_skill_location(
    url: &str,
    branch_override: Option<&str>,
    path_override: Option<&str>,
) -> AppResult<GitHubSkillLocation> {
    let trimmed = url
        .trim()
        .split('#')
        .next()
        .unwrap_or_default()
        .split('?')
        .next()
        .unwrap_or_default()
        .trim_end_matches('/');
    let path = trimmed.strip_prefix("https://github.com/").ok_or_else(|| {
        AppError::Validation("skill acquire only supports https://github.com URLs".to_string())
    })?;
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(AppError::Validation(
            "GitHub URL must include owner and repository".to_string(),
        ));
    }

    let owner = parts[0];
    let repo = parts[1].trim_end_matches(".git");
    if repo.is_empty() {
        return Err(AppError::Validation(
            "GitHub URL must include repository name".to_string(),
        ));
    }

    let mut branch = branch_override.and_then(clean_non_empty_string);
    let mut skill_path = path_override.and_then(clean_skill_subpath);
    if skill_path.is_none() && parts.len() >= 4 && matches!(parts[2], "tree" | "blob") {
        branch = branch.or_else(|| clean_non_empty_string(parts[3]));
        if parts.len() > 4 {
            skill_path = clean_skill_subpath(&parts[4..].join("/"));
        }
    }

    Ok(GitHubSkillLocation {
        repo: repo.to_string(),
        repo_url: format!("https://github.com/{owner}/{repo}.git"),
        branch,
        path: skill_path,
    })
}

pub(crate) fn clean_non_empty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) fn clean_skill_subpath(value: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in value.trim().trim_matches('/').split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part == ".git" || part.contains('\\') || part.contains(':') {
            return None;
        }
        parts.push(part);
    }
    if matches!(parts.last().copied(), Some("SKILL.md")) {
        parts.pop();
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}
