use super::skill_remote_client::*;
use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use serde_json::Value;

pub(crate) fn normalize_skill_search_provider(provider: Option<&str>) -> AppResult<String> {
    match provider
        .and_then(clean_non_empty_string)
        .unwrap_or_else(|| "github".to_string())
        .as_str()
    {
        "github" => Ok("github".to_string()),
        "github-code" | "github_code" | "code" => Ok("github-code".to_string()),
        other => Err(AppError::Validation(format!(
            "unsupported skill search provider: {other}"
        ))),
    }
}

pub(crate) fn github_repository_skill_search(
    query: &str,
    limit: usize,
) -> AppResult<(Vec<SkillSearchCandidate>, Vec<String>)> {
    let repository_limit = limit.clamp(5, 10);
    let url = format!(
        "https://api.github.com/search/repositories?q={}&per_page={}",
        percent_encode_query(&format!("{query} skill")),
        repository_limit
    );
    let value = github_get_json(&url, "skill search")?;
    let mut candidates = Vec::new();
    let mut warnings = Vec::new();
    if let Some(items) = value.get("items").and_then(Value::as_array) {
        for item in items.iter().take(repository_limit) {
            if candidates.len() >= limit {
                break;
            }
            let Some(repo_candidate) = skill_search_candidate_from_github(item) else {
                continue;
            };
            let full_name = item.get("full_name").and_then(Value::as_str);
            let branch = repo_candidate
                .default_branch
                .as_deref()
                .unwrap_or("main")
                .to_string();
            let skill_candidates = match full_name {
                Some(full_name) => {
                    match github_skill_candidates_for_repo(full_name, &branch, &repo_candidate) {
                        Ok(candidates) => candidates,
                        Err(error) => {
                            warnings.push(format!(
                                "{full_name}: could not inspect GitHub tree on {branch}: {error}"
                            ));
                            Vec::new()
                        }
                    }
                }
                None => {
                    warnings.push(format!(
                        "{}: GitHub search result did not include full_name",
                        repo_candidate.name
                    ));
                    Vec::new()
                }
            };

            if skill_candidates.is_empty() {
                candidates.push(skill_search_repository_fallback_candidate(
                    repo_candidate,
                    &branch,
                ));
                continue;
            }
            candidates.extend(skill_candidates);
        }
    } else {
        warnings.push("GitHub search response did not include repository items".to_string());
    }
    Ok((candidates, warnings))
}

pub(crate) fn github_code_skill_search(
    query: &str,
    limit: usize,
) -> AppResult<(Vec<SkillSearchCandidate>, Vec<String>)> {
    let url = github_code_search_url(query, limit);
    let value = github_get_json(&url, "GitHub code skill search")?;
    let mut candidates = Vec::new();
    let mut warnings = Vec::new();
    if let Some(items) = value.get("items").and_then(Value::as_array) {
        for item in items.iter().take(limit) {
            match skill_search_candidate_from_github_code(item) {
                Some(candidate) => candidates.push(candidate),
                None => warnings
                    .push("GitHub code search returned an incomplete SKILL.md item".to_string()),
            }
        }
    } else {
        warnings.push("GitHub code search response did not include code items".to_string());
    }
    Ok((candidates, warnings))
}

pub(crate) fn github_code_search_url(query: &str, limit: usize) -> String {
    format!(
        "https://api.github.com/search/code?q={}&per_page={}",
        percent_encode_query(&format!("{query} filename:SKILL.md")),
        limit.clamp(1, 20)
    )
}

pub(crate) fn skill_search_candidate_from_github(item: &Value) -> Option<SkillSearchCandidate> {
    let url = item.get("html_url")?.as_str()?.to_string();
    let name = item
        .get("full_name")
        .and_then(Value::as_str)
        .or_else(|| item.get("name").and_then(Value::as_str))?
        .to_string();
    Some(SkillSearchCandidate {
        acquire_command: format!("assetiweave-cli skill acquire --url {url} --yes"),
        name,
        description: item
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        match_reason: None,
        url,
        path: None,
        clone_url: item
            .get("clone_url")
            .and_then(Value::as_str)
            .map(str::to_string),
        default_branch: item
            .get("default_branch")
            .and_then(Value::as_str)
            .map(str::to_string),
        stars: item.get("stargazers_count").and_then(Value::as_u64),
    })
}

pub(crate) fn skill_search_candidate_from_github_code(
    item: &Value,
) -> Option<SkillSearchCandidate> {
    let repository = item.get("repository")?;
    let full_name = repository
        .get("full_name")
        .and_then(Value::as_str)
        .or_else(|| repository.get("name").and_then(Value::as_str))?;
    let repo_url = repository.get("html_url")?.as_str()?;
    let skill_file_path = item.get("path")?.as_str()?.trim().trim_matches('/');
    if !skill_file_path.ends_with("SKILL.md") {
        return None;
    }
    let skill_path = clean_skill_subpath(skill_file_path);
    let branch = repository
        .get("default_branch")
        .and_then(Value::as_str)
        .unwrap_or("main");
    let url = github_skill_tree_url(repo_url, branch, skill_path.as_deref().unwrap_or_default());
    let name = skill_path
        .as_deref()
        .map(|path| format!("{full_name}/{path}"))
        .unwrap_or_else(|| full_name.to_string());
    Some(SkillSearchCandidate {
        acquire_command: format!("assetiweave-cli skill acquire --url {url} --yes"),
        name,
        description: repository
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        match_reason: Some(format!("GitHub code search matched {skill_file_path}")),
        url,
        path: skill_path,
        clone_url: repository
            .get("clone_url")
            .and_then(Value::as_str)
            .map(str::to_string),
        default_branch: Some(branch.to_string()),
        stars: repository.get("stargazers_count").and_then(Value::as_u64),
    })
}

pub(crate) fn skill_search_repository_fallback_candidate(
    mut candidate: SkillSearchCandidate,
    branch: &str,
) -> SkillSearchCandidate {
    candidate.match_reason = Some(format!(
        "Repository fallback: no concrete SKILL.md directory was resolved on branch {branch}"
    ));
    candidate
}

fn github_skill_candidates_for_repo(
    full_name: &str,
    branch: &str,
    repo_candidate: &SkillSearchCandidate,
) -> AppResult<Vec<SkillSearchCandidate>> {
    let url = format!(
        "https://api.github.com/repos/{}/git/trees/{}?recursive=1",
        full_name,
        percent_encode_path_segment(branch)
    );
    let value = github_get_json(&url, "GitHub skill tree")?;
    let mut candidates = github_skill_paths_from_tree_value(&value)
        .into_iter()
        .map(|path| {
            skill_search_candidate_from_github_skill_path(repo_candidate, full_name, branch, &path)
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(candidates)
}

pub(crate) fn github_skill_paths_from_tree_value(value: &Value) -> Vec<String> {
    let mut paths = BTreeSet::new();
    let Some(tree) = value.get("tree").and_then(Value::as_array) else {
        return Vec::new();
    };
    for entry in tree {
        if entry.get("type").and_then(Value::as_str) != Some("blob") {
            continue;
        }
        let Some(path) = entry.get("path").and_then(Value::as_str) else {
            continue;
        };
        let normalized_path = path.trim().trim_matches('/');
        if normalized_path == "SKILL.md" {
            paths.insert(String::new());
            continue;
        }
        let Some(skill_dir) = normalized_path.strip_suffix("/SKILL.md") else {
            continue;
        };
        if let Some(cleaned) = clean_skill_subpath(skill_dir) {
            paths.insert(cleaned);
        }
    }
    paths.into_iter().collect()
}

pub(crate) fn github_tree_sha_for_skill_path(
    value: &Value,
    path: Option<&str>,
) -> AppResult<String> {
    let Some(path) = path.and_then(clean_skill_subpath) else {
        return value
            .get("sha")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                AppError::External("GitHub tree response did not include root sha".to_string())
            });
    };
    let Some(tree) = value.get("tree").and_then(Value::as_array) else {
        return Err(AppError::External(
            "GitHub tree response did not include tree entries".to_string(),
        ));
    };
    tree.iter()
        .find(|entry| {
            entry.get("type").and_then(Value::as_str) == Some("tree")
                && entry.get("path").and_then(Value::as_str) == Some(path.as_str())
        })
        .and_then(|entry| entry.get("sha").and_then(Value::as_str))
        .map(str::to_string)
        .ok_or_else(|| {
            AppError::External(format!(
                "GitHub tree response did not include Skill path: {path}"
            ))
        })
}

pub(crate) fn skill_search_candidate_from_github_skill_path(
    repo_candidate: &SkillSearchCandidate,
    full_name: &str,
    branch: &str,
    path: &str,
) -> SkillSearchCandidate {
    let url = github_skill_tree_url(&repo_candidate.url, branch, path);
    let path = clean_skill_subpath(path);
    let skill_file = path
        .as_deref()
        .map(|path| format!("{path}/SKILL.md"))
        .unwrap_or_else(|| "SKILL.md".to_string());
    let name = path
        .as_deref()
        .map(|path| format!("{full_name}/{path}"))
        .unwrap_or_else(|| full_name.to_string());
    SkillSearchCandidate {
        acquire_command: format!("assetiweave-cli skill acquire --url {url} --yes"),
        name,
        description: repo_candidate.description.clone(),
        match_reason: Some(format!(
            "Resolved concrete Skill directory from {skill_file}"
        )),
        url,
        path,
        clone_url: repo_candidate.clone_url.clone(),
        default_branch: Some(branch.to_string()),
        stars: repo_candidate.stars,
    }
}

pub(crate) fn github_skill_tree_url(repo_url: &str, branch: &str, path: &str) -> String {
    let base = repo_url.trim_end_matches('/');
    if path.trim().is_empty() {
        format!("{base}/tree/{branch}")
    } else {
        format!("{base}/tree/{branch}/{}", path.trim().trim_matches('/'))
    }
}

pub(crate) fn search_query_terms(query: &str) -> Vec<String> {
    let terms = query
        .split(|character: char| !character.is_alphanumeric())
        .filter_map(clean_non_empty_string)
        .map(|term| term.to_lowercase())
        .collect::<Vec<_>>();
    if terms.is_empty() {
        let fallback = query.trim().to_lowercase();
        if fallback.is_empty() {
            Vec::new()
        } else {
            vec![fallback]
        }
    } else {
        terms
    }
}

pub(crate) fn skill_candidate_score(candidate: &SkillSearchCandidate, terms: &[String]) -> usize {
    let haystack = format!(
        "{} {} {} {}",
        candidate.name,
        candidate.path.as_deref().unwrap_or_default(),
        candidate.description.as_deref().unwrap_or_default(),
        candidate.url
    )
    .to_lowercase();
    let term_score = terms
        .iter()
        .filter(|term| haystack.contains(term.as_str()))
        .count()
        * 100;
    let concrete_skill_score = usize::from(candidate.path.is_some()) * 10;
    term_score + concrete_skill_score
}
