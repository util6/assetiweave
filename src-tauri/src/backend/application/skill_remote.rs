use super::prelude::*;

const SKILL_REMOTE_SECURITY_NOTICE: &str =
    "Review remote Skill contents before importing; AssetIWeave does not execute or trust remote code automatically.";

impl AppService {
    pub(crate) fn search_skills(&self, params: SkillSearchParams) -> AppResult<SkillSearchResult> {
        let query = params.query.trim();
        if query.is_empty() {
            return Err("skill search query is required".to_string());
        }
        let provider = normalize_skill_search_provider(params.provider.as_deref())?;
        let limit = params.limit.unwrap_or(10).clamp(1, 20);
        let (mut candidates, warnings) = match provider.as_str() {
            "github" => github_repository_skill_search(query, limit)?,
            "github-code" => github_code_skill_search(query, limit)?,
            _ => return Err(format!("unsupported skill search provider: {provider}")),
        };
        let query_terms = search_query_terms(query);
        candidates.sort_by(|left, right| {
            skill_candidate_score(right, &query_terms)
                .cmp(&skill_candidate_score(left, &query_terms))
                .then_with(|| {
                    right
                        .stars
                        .unwrap_or_default()
                        .cmp(&left.stars.unwrap_or_default())
                })
                .then_with(|| left.name.cmp(&right.name))
        });
        candidates.truncate(limit);
        Ok(SkillSearchResult {
            query: query.to_string(),
            provider,
            candidates,
            warnings,
        })
    }

    pub(crate) fn acquire_skill(&self, params: SkillAcquireParams) -> AppResult<Value> {
        if !params.dry_run && !params.yes {
            return Err("skill acquire requires --yes".to_string());
        }
        let location = parse_github_skill_location(
            &params.url,
            params.branch.as_deref(),
            params.path.as_deref(),
        )?;
        let raw_name = params
            .name
            .clone()
            .or_else(|| location.skill_name_hint())
            .unwrap_or_else(|| location.repo.clone());
        let name = slug_path_segment(&raw_name);
        let staging_dir = capabilities::skill_backup_root_sqlx(&self.db)?
            .join("staging")
            .join(format!("{}-{}", slug_path_segment(&name), short_uuid()));
        let skill_path_hint = location.skill_path_hint(&staging_dir);

        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "provider": "github",
                "url": params.url,
                "repo_url": location.repo_url,
                "branch": location.branch,
                "path": location.path,
                "name": name,
                "staging_path": staging_dir,
                "skill_path": skill_path_hint,
                "security_notice": SKILL_REMOTE_SECURITY_NOTICE,
            }));
        }

        clone_github_skill(&location, &staging_dir)?;
        let skill_dir = resolve_cloned_skill_dir(&staging_dir, location.path.as_deref())?;
        let acquired_tree_sha = git_skill_tree_sha(&staging_dir, location.path.as_deref());
        let acquired_branch = location
            .branch
            .clone()
            .or_else(|| git_current_branch(&staging_dir))
            .unwrap_or_else(|| "HEAD".to_string());
        let import_result = self.import_skill(ImportSkillParams {
            from: skill_dir.to_string_lossy().to_string(),
            name: Some(name.clone()),
            dry_run: false,
        })?;
        let imported_asset = import_result
            .get("asset")
            .cloned()
            .ok_or_else(|| "skill import result did not include asset".to_string())
            .and_then(|value| {
                serde_json::from_value::<Asset>(value)
                    .map_err(|error| format!("skill import result asset was invalid: {error}"))
            })?;
        let remote_source = SkillRemoteSource {
            asset_id: imported_asset.id.clone(),
            provider: "github".to_string(),
            source_url: params.url.clone(),
            repo_url: location.repo_url.clone(),
            branch: acquired_branch.clone(),
            path: location.path.clone(),
            acquired_at: Utc::now().to_rfc3339(),
            acquired_tree_sha,
            local_content_hash: imported_asset.content_hash.clone(),
            last_checked_at: None,
            latest_tree_sha: None,
            status: "unknown".to_string(),
            message: Some(
                "Remote source recorded; run skill remote check to detect drift".to_string(),
            ),
        };
        let pool = self.db.pool().clone();
        let remote_source_to_save = remote_source.clone();
        self.db.block_on(async move {
            crate::backend::store::upsert_skill_remote_source_sqlx(&pool, &remote_source_to_save)
                .await
        })?;
        Ok(json!({
            "dry_run": false,
            "provider": "github",
            "url": params.url,
            "repo_url": location.repo_url,
            "branch": acquired_branch,
            "path": location.path,
            "name": name,
            "staging_path": staging_dir,
            "skill_path": skill_dir,
            "import": import_result,
            "remote_source": remote_source,
            "security_notice": SKILL_REMOTE_SECURITY_NOTICE,
        }))
    }

    pub(crate) fn list_skill_remote_sources(&self) -> AppResult<Vec<SkillRemoteSource>> {
        let pool = self.db.pool().clone();
        self.db.block_on(async move {
            crate::backend::store::delete_orphan_skill_remote_sources_sqlx(&pool).await?;
            crate::backend::store::list_skill_remote_sources_sqlx(&pool).await
        })
    }

    pub(crate) fn check_skill_remote_sources(
        &self,
        params: SkillRemoteCheckParams,
    ) -> AppResult<Vec<SkillRemoteSource>> {
        let sources = if let Some(asset_id) = params
            .asset_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            let pool = self.db.pool().clone();
            vec![self
                .db
                .block_on(async move {
                    crate::backend::store::delete_orphan_skill_remote_sources_sqlx(&pool).await?;
                    crate::backend::store::load_skill_remote_source_sqlx(&pool, asset_id).await
                })?
                .ok_or_else(|| format!("skill remote source not found: {asset_id}"))?]
        } else {
            self.list_skill_remote_sources()?
        };

        let mut checked = Vec::with_capacity(sources.len());
        for source in sources {
            let source = check_skill_remote_source(source);
            let pool = self.db.pool().clone();
            let source_to_save = source.clone();
            self.db.block_on(async move {
                crate::backend::store::update_skill_remote_check_result_sqlx(&pool, &source_to_save)
                    .await
            })?;
            checked.push(source);
        }
        Ok(checked)
    }
}

#[derive(Debug)]
struct GitHubSkillLocation {
    repo: String,
    repo_url: String,
    branch: Option<String>,
    path: Option<String>,
}

impl GitHubSkillLocation {
    fn skill_name_hint(&self) -> Option<String> {
        self.path
            .as_deref()
            .and_then(|path| path.split('/').next_back())
            .filter(|name| !name.is_empty())
            .map(str::to_string)
    }

    fn skill_path_hint(&self, staging_dir: &Path) -> PathBuf {
        self.path
            .as_deref()
            .map(|path| staging_dir.join(path))
            .unwrap_or_else(|| staging_dir.to_path_buf())
    }
}

pub(super) fn normalize_skill_search_provider(provider: Option<&str>) -> AppResult<String> {
    match provider
        .and_then(clean_non_empty_string)
        .unwrap_or_else(|| "github".to_string())
        .as_str()
    {
        "github" => Ok("github".to_string()),
        "github-code" | "github_code" | "code" => Ok("github-code".to_string()),
        other => Err(format!("unsupported skill search provider: {other}")),
    }
}

fn github_repository_skill_search(
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

fn github_code_skill_search(
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

pub(super) fn github_code_search_url(query: &str, limit: usize) -> String {
    format!(
        "https://api.github.com/search/code?q={}&per_page={}",
        percent_encode_query(&format!("{query} filename:SKILL.md")),
        limit.clamp(1, 20)
    )
}

pub(super) fn skill_search_candidate_from_github(item: &Value) -> Option<SkillSearchCandidate> {
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

pub(super) fn skill_search_candidate_from_github_code(
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

pub(super) fn skill_search_repository_fallback_candidate(
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

pub(super) fn github_skill_paths_from_tree_value(value: &Value) -> Vec<String> {
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

pub(super) fn github_tree_sha_for_skill_path(
    value: &Value,
    path: Option<&str>,
) -> AppResult<String> {
    let Some(path) = path.and_then(clean_skill_subpath) else {
        return value
            .get("sha")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "GitHub tree response did not include root sha".to_string());
    };
    let Some(tree) = value.get("tree").and_then(Value::as_array) else {
        return Err("GitHub tree response did not include tree entries".to_string());
    };
    tree.iter()
        .find(|entry| {
            entry.get("type").and_then(Value::as_str) == Some("tree")
                && entry.get("path").and_then(Value::as_str) == Some(path.as_str())
        })
        .and_then(|entry| entry.get("sha").and_then(Value::as_str))
        .map(str::to_string)
        .ok_or_else(|| format!("GitHub tree response did not include Skill path: {path}"))
}

pub(super) fn skill_search_candidate_from_github_skill_path(
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

fn github_skill_tree_url(repo_url: &str, branch: &str, path: &str) -> String {
    let base = repo_url.trim_end_matches('/');
    if path.trim().is_empty() {
        format!("{base}/tree/{branch}")
    } else {
        format!("{base}/tree/{branch}/{}", path.trim().trim_matches('/'))
    }
}

fn github_get_json(url: &str, context: &str) -> AppResult<Value> {
    let mut request = ureq::get(url)
        .set("User-Agent", "AssetIWeave/0.1 skill-search")
        .set("Accept", "application/vnd.github+json");
    let authorization = github_api_token().map(|token| format!("Bearer {token}"));
    if let Some(authorization) = authorization.as_deref() {
        request = request.set("Authorization", authorization);
    }
    let response = request
        .call()
        .map_err(|error| format!("{context} request failed: {error}"))?;
    response
        .into_json()
        .map_err(|error| format!("{context} response was not JSON: {error}"))
}

fn check_skill_remote_source(mut source: SkillRemoteSource) -> SkillRemoteSource {
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
            source.message = Some(error);
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

pub(super) fn search_query_terms(query: &str) -> Vec<String> {
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

pub(super) fn skill_candidate_score(candidate: &SkillSearchCandidate, terms: &[String]) -> usize {
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

fn percent_encode_query(value: &str) -> String {
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

fn percent_encode_path_segment(value: &str) -> String {
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

fn parse_github_skill_location(
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
    let path = trimmed
        .strip_prefix("https://github.com/")
        .ok_or_else(|| "skill acquire only supports https://github.com URLs".to_string())?;
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err("GitHub URL must include owner and repository".to_string());
    }

    let owner = parts[0];
    let repo = parts[1].trim_end_matches(".git");
    if repo.is_empty() {
        return Err("GitHub URL must include repository name".to_string());
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

fn clean_non_empty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn clean_skill_subpath(value: &str) -> Option<String> {
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

fn clone_github_skill(location: &GitHubSkillLocation, target: &Path) -> AppResult<()> {
    if target.exists() {
        return Err(format!(
            "skill acquire staging path already exists: {}",
            target.display()
        ));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let mut command = Command::new("git");
    command.arg("clone").arg("--depth").arg("1");
    if let Some(branch) = &location.branch {
        command.arg("--branch").arg(branch);
    }
    let output = command
        .arg(&location.repo_url)
        .arg(target)
        .output()
        .map_err(|error| format!("failed to run git clone: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git clone failed: {stderr}"));
    }
    Ok(())
}

fn git_current_branch(repo: &Path) -> Option<String> {
    git_output(repo, &["rev-parse", "--abbrev-ref", "HEAD"]).filter(|branch| branch != "HEAD")
}

fn git_skill_tree_sha(repo: &Path, skill_path: Option<&str>) -> Option<String> {
    let revision = skill_path
        .and_then(clean_skill_subpath)
        .map(|path| format!("HEAD:{path}"))
        .unwrap_or_else(|| "HEAD^{tree}".to_string());
    git_output(repo, &["rev-parse", &revision])
}

fn git_output(repo: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn resolve_cloned_skill_dir(staging_dir: &Path, skill_path: Option<&str>) -> AppResult<PathBuf> {
    if let Some(skill_path) = skill_path {
        let candidate = staging_dir.join(skill_path);
        if candidate.join("SKILL.md").is_file() {
            return Ok(candidate);
        }
        return Err(format!(
            "cloned path does not contain SKILL.md: {}",
            candidate.display()
        ));
    }
    if staging_dir.join("SKILL.md").is_file() {
        return Ok(staging_dir.to_path_buf());
    }

    let mut candidates = Vec::new();
    for entry in walkdir::WalkDir::new(staging_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        if entry.file_name().to_str() == Some("SKILL.md") {
            if let Some(parent) = entry.path().parent() {
                candidates.push(parent.to_path_buf());
            }
        }
    }
    match candidates.as_slice() {
        [candidate] => Ok(candidate.clone()),
        [] => Err("cloned repository does not contain SKILL.md".to_string()),
        many => Err(format!(
            "cloned repository contains multiple skills; pass --path: {}",
            many.iter()
                .filter_map(|path| path.strip_prefix(staging_dir).ok())
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn short_uuid() -> String {
    Uuid::new_v4().to_string()[..8].to_string()
}
