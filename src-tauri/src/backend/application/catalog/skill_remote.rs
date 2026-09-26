use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const SKILL_REMOTE_SECURITY_NOTICE: &str =
    "Review remote Skill contents before importing; AssetIWeave does not execute or trust remote code automatically.";

struct StagingDirectoryGuard {
    path: PathBuf,
}

impl Drop for StagingDirectoryGuard {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

impl StagingDirectoryGuard {
    fn cleanup(&mut self) -> bool {
        fs::remove_dir_all(&self.path).is_ok() || !self.path.exists()
    }
}

fn report_skill_acquire_phase(phase_sink: Option<&(dyn Fn(&str) + Send + Sync)>, phase: &str) {
    if let Some(phase_sink) = phase_sink {
        phase_sink(phase);
    }
}

impl AppService {
    pub(crate) fn search_skills(&self, params: SkillSearchParams) -> AppResult<SkillSearchResult> {
        let query = params.query.trim();
        if query.is_empty() {
            return Err(AppError::Validation(
                "skill search query is required".to_string(),
            ));
        }
        let provider = normalize_skill_search_provider(params.provider.as_deref())?;
        let limit = params.limit.unwrap_or(10).clamp(1, 20);
        let (mut candidates, warnings) = match provider.as_str() {
            "github" => github_repository_skill_search(query, limit)?,
            "github-code" => github_code_skill_search(query, limit)?,
            _ => {
                return Err(AppError::Validation(format!(
                    "unsupported skill search provider: {provider}"
                )))
            }
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

    pub(crate) async fn acquire_skill(&self, params: SkillAcquireParams) -> AppResult<Value> {
        self.acquire_skill_with_cancellation(params, None).await
    }

    pub(crate) async fn acquire_skill_with_cancellation(
        &self,
        params: SkillAcquireParams,
        cancellation: Option<&CancellationToken>,
    ) -> AppResult<Value> {
        self.acquire_skill_with_cancellation_and_progress(params, cancellation, None)
            .await
    }

    pub(crate) async fn acquire_skill_with_cancellation_and_progress(
        &self,
        params: SkillAcquireParams,
        cancellation: Option<&CancellationToken>,
        phase_sink: Option<&(dyn Fn(&str) + Send + Sync)>,
    ) -> AppResult<Value> {
        if !params.dry_run && !params.yes {
            return Err(AppError::Validation(
                "skill acquire requires --yes".to_string(),
            ));
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
        let staging_dir =
            super::catalog_ops::skill_backup_root_sqlx(self.db.pool(), self.tenant_id())
                .await?
                .join(".staging")
                .join(format!("{}-{}", slug_path_segment(&name), short_uuid()));
        let skill_path_hint = location.skill_path_hint(&staging_dir);

        if params.dry_run {
            report_skill_acquire_phase(phase_sink, "preparing");
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

        ensure_not_cancelled(cancellation)?;
        report_skill_acquire_phase(phase_sink, "cloning");
        let mut staging_guard = StagingDirectoryGuard {
            path: staging_dir.clone(),
        };
        clone_github_skill(&location, &staging_dir, cancellation).await?;
        ensure_not_cancelled(cancellation)?;
        let skill_dir = resolve_cloned_skill_dir(&staging_dir, location.path.as_deref())?;
        let acquired_tree_sha = git_skill_tree_sha(&staging_dir, location.path.as_deref()).await;
        let acquired_branch = match location.branch.clone() {
            Some(branch) => branch,
            None => git_current_branch(&staging_dir)
                .await
                .unwrap_or_else(|| "HEAD".to_string()),
        };
        report_skill_acquire_phase(phase_sink, "importing");
        let import_result = self
            .import_skill_with_progress(
                ImportSkillParams {
                    from: skill_dir.to_string_lossy().to_string(),
                    name: Some(name.clone()),
                    dry_run: false,
                },
                phase_sink,
            )
            .await?;
        ensure_not_cancelled(cancellation)?;
        let imported_asset = import_result
            .get("asset")
            .cloned()
            .ok_or_else(|| {
                AppError::Validation("skill import result did not include asset".to_string())
            })
            .and_then(|value| {
                serde_json::from_value::<Asset>(value).map_err(|error| {
                    AppError::Validation(format!("skill import result asset was invalid: {error}"))
                })
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
        crate::backend::store::upsert_skill_remote_source_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &remote_source,
        )
        .await
        .map_err(AppError::external)?;
        let staging_cleaned = staging_guard.cleanup();
        if !staging_cleaned {
            return Err(AppError::Storage(
                "remote Skill staging cleanup failed".to_string(),
            ));
        }
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
            "staging_cleaned": staging_cleaned,
            "import": import_result,
            "remote_source": remote_source,
            "security_notice": SKILL_REMOTE_SECURITY_NOTICE,
        }))
    }

    pub(crate) async fn list_skill_remote_sources(&self) -> AppResult<Vec<SkillRemoteSource>> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        crate::backend::store::delete_orphan_skill_remote_sources_sqlx(pool, tenant_id)
            .await
            .map_err(AppError::external)?;
        crate::backend::store::list_skill_remote_sources_sqlx(pool, tenant_id)
            .await
            .map_err(AppError::external)
    }

    pub(crate) async fn check_skill_remote_sources(
        &self,
        params: SkillRemoteCheckParams,
    ) -> AppResult<Vec<SkillRemoteSource>> {
        let sources = if let Some(asset_id) = params
            .asset_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            let pool = self.db.pool();
            let tenant_id = self.tenant_id();
            crate::backend::store::delete_orphan_skill_remote_sources_sqlx(pool, tenant_id)
                .await
                .map_err(AppError::external)?;
            vec![
                crate::backend::store::load_skill_remote_source_sqlx(pool, tenant_id, asset_id)
                    .await
                    .map_err(AppError::external)?
                    .ok_or_else(|| {
                        AppError::NotFound(format!("skill remote source not found: {asset_id}"))
                    })?,
            ]
        } else {
            self.list_skill_remote_sources().await?
        };

        let mut checked = Vec::with_capacity(sources.len());
        for source in sources {
            let source = check_skill_remote_source(source);
            crate::backend::store::update_skill_remote_check_result_sqlx(
                self.db.pool(),
                self.tenant_id(),
                &source,
            )
            .await
            .map_err(AppError::external)?;
            checked.push(source);
        }
        Ok(checked)
    }
}

pub(crate) use super::skill_remote_client::*;
pub(crate) use super::skill_remote_search::*;

fn ensure_not_cancelled(cancellation: Option<&CancellationToken>) -> AppResult<()> {
    if cancellation.is_some_and(CancellationToken::is_cancelled) {
        Err(AppError::Cancelled("skill acquire cancelled".to_string()))
    } else {
        Ok(())
    }
}

async fn clone_github_skill(
    location: &GitHubSkillLocation,
    target: &Path,
    cancellation: Option<&CancellationToken>,
) -> AppResult<()> {
    if target.exists() {
        return Err(AppError::Conflict(format!(
            "skill staging path already exists: {}",
            target.display()
        )));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| AppError::Storage(error.to_string()))?;
    }

    let mut command_args = vec!["clone".to_string(), "--depth".to_string(), "1".to_string()];
    if let Some(branch) = &location.branch {
        command_args.extend(["--branch".to_string(), branch.clone()]);
    }
    command_args.extend([
        location.repo_url.clone(),
        target.to_string_lossy().to_string(),
    ]);
    let spec = crate::backend::infrastructure::host_process::HostCommandSpec {
        program: PathBuf::from("git"),
        args: command_args,
        env: Vec::new(),
        working_dir: None,
        stdin: crate::backend::infrastructure::host_process::HostInput::Null,
        timeout: Duration::from_secs(120),
        stdout_limit: 1024 * 1024,
        stderr_limit: 256 * 1024,
    };
    let output =
        crate::backend::infrastructure::host_process::run_host_command_async(spec, cancellation)
            .await
            .map_err(|error| match error {
                crate::backend::infrastructure::host_process::HostProcessError::Cancelled => {
                    AppError::Cancelled("skill acquire cancelled".to_string())
                }
                error => AppError::Process(format!("failed to run git clone: {error:?}")),
            })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AppError::Process(format!("git clone failed: {stderr}")));
    }
    Ok(())
}

async fn git_current_branch(repo: &Path) -> Option<String> {
    git_output(repo, &["rev-parse", "--abbrev-ref", "HEAD"])
        .await
        .filter(|branch| branch != "HEAD")
}

async fn git_skill_tree_sha(repo: &Path, skill_path: Option<&str>) -> Option<String> {
    let revision = skill_path
        .and_then(clean_skill_subpath)
        .map(|path| format!("HEAD:{path}"))
        .unwrap_or_else(|| "HEAD^{tree}".to_string());
    git_output(repo, &["rev-parse", &revision]).await
}

async fn git_output(repo: &Path, args: &[&str]) -> Option<String> {
    let mut command_args = vec!["-C".to_string(), repo.to_string_lossy().to_string()];
    command_args.extend(args.iter().map(|arg| (*arg).to_string()));
    let spec = crate::backend::infrastructure::host_process::HostCommandSpec {
        program: PathBuf::from("git"),
        args: command_args,
        env: Vec::new(),
        working_dir: None,
        stdin: crate::backend::infrastructure::host_process::HostInput::Null,
        timeout: Duration::from_secs(30),
        stdout_limit: 64 * 1024,
        stderr_limit: 64 * 1024,
    };
    let output = crate::backend::infrastructure::host_process::run_host_command_async(spec, None)
        .await
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
        return Err(AppError::Validation(format!(
            "cloned path does not contain SKILL.md: {}",
            candidate.display()
        )));
    }
    if staging_dir.join("SKILL.md").is_file() {
        return Ok(staging_dir.to_path_buf());
    }

    let mut candidates = Vec::new();
    for entry in walkdir::WalkDir::new(staging_dir) {
        let entry = entry.map_err(|error| {
            AppError::Storage(format!("failed to inspect cloned Skill tree: {error}"))
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.file_name().to_str() == Some("SKILL.md") {
            if let Some(parent) = entry.path().parent() {
                candidates.push(parent.to_path_buf());
            }
        }
    }
    match candidates.as_slice() {
        [candidate] => Ok(candidate.clone()),
        [] => Err(AppError::Validation(
            "cloned repository does not contain SKILL.md".to_string(),
        )),
        many => Err(AppError::Validation(format!(
            "cloned repository contains multiple skills; pass --path: {}",
            many.iter()
                .filter_map(|path| path.strip_prefix(staging_dir).ok())
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

fn short_uuid() -> String {
    Uuid::new_v4().to_string()[..8].to_string()
}

#[cfg(test)]
#[path = "skill_remote_tests.rs"]
mod tests;
