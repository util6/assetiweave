use std::path::Path;
use walkdir::WalkDir;

use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::{
    AssetKind, Source, SourceKind, SourceOrigin, SourceProbeCandidate, SourceProbeResult,
    SourceScannerKind,
};
use crate::backend::infrastructure::path_utils::{
    expand_path, find_git_root, git_repository_for_path, normalize_path_for_storage,
    normalize_relative_path, normalize_std_path_for_storage,
};

pub fn probe_source_path_inner(raw_path: &str) -> AppResult<SourceProbeResult> {
    let root = expand_path(raw_path)?;
    if !root.exists() {
        return Err(AppError::NotFound(format!(
            "source path does not exist: {}",
            root.display()
        )));
    }
    if !root.is_dir() {
        return Err(AppError::Validation(format!(
            "source path is not a directory: {}",
            root.display()
        )));
    }

    let root_path = normalize_path_for_storage(raw_path)?;
    let is_root_git = root.join(".git").exists() || find_git_root(&root) == Some(root.clone());

    if is_root_git {
        let remote_url = git_repository_for_path(&root).and_then(|info| info.remote_url);
        let skill_count = count_skills_in_dir(&root);
        let name = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("skill-source")
            .to_string();

        return Ok(SourceProbeResult {
            root_path: root_path.clone(),
            is_container: false,
            default_source_origin: SourceOrigin::GitRepo,
            candidates: vec![SourceProbeCandidate {
                name,
                path: root_path,
                relative_path: ".".to_string(),
                is_git_repo: true,
                remote_url,
                skill_count,
            }],
        });
    }

    let mut candidates = Vec::new();
    collect_git_repositories(&root, &mut candidates)?;

    if !candidates.is_empty() {
        candidates.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(SourceProbeResult {
            root_path,
            is_container: true,
            default_source_origin: SourceOrigin::LocalFolder,
            candidates,
        })
    } else {
        Ok(SourceProbeResult {
            root_path,
            is_container: false,
            default_source_origin: SourceOrigin::LocalFolder,
            candidates: vec![],
        })
    }
}

fn collect_git_repositories(
    root: &Path,
    candidates: &mut Vec<SourceProbeCandidate>,
) -> AppResult<()> {
    let Ok(entries) = root.read_dir() else {
        return Ok(());
    };

    for entry in entries.flatten() {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }

        let file_name = entry_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if should_ignore_dir(file_name) {
            continue;
        }

        if entry_path.join(".git").exists() {
            let relative = entry_path.strip_prefix(root).unwrap_or(&entry_path);
            let relative_path = normalize_relative_path(relative);
            let path = normalize_std_path_for_storage(&entry_path)?;
            let remote_url = git_repository_for_path(&entry_path).and_then(|info| info.remote_url);
            let skill_count = count_skills_in_dir(&entry_path);
            candidates.push(SourceProbeCandidate {
                name: file_name.to_string(),
                path,
                relative_path,
                is_git_repo: true,
                remote_url,
                skill_count,
            });
        } else if let Ok(sub_entries) = entry_path.read_dir() {
            for sub_entry in sub_entries.flatten() {
                let sub_path = sub_entry.path();
                if !sub_path.is_dir() {
                    continue;
                }
                let sub_name = sub_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if should_ignore_dir(sub_name) {
                    continue;
                }
                if sub_path.join(".git").exists() {
                    let relative = sub_path.strip_prefix(root).unwrap_or(&sub_path);
                    let relative_path = normalize_relative_path(relative);
                    let path = normalize_std_path_for_storage(&sub_path)?;
                    let remote_url =
                        git_repository_for_path(&sub_path).and_then(|info| info.remote_url);
                    let skill_count = count_skills_in_dir(&sub_path);
                    candidates.push(SourceProbeCandidate {
                        name: sub_name.to_string(),
                        path,
                        relative_path,
                        is_git_repo: true,
                        remote_url,
                        skill_count,
                    });
                }
            }
        }
    }

    Ok(())
}

fn should_ignore_dir(name: &str) -> bool {
    name.starts_with('.')
        || name == "node_modules"
        || name == "target"
        || name == "dist"
        || name == "build"
        || name == "__pycache__"
}

fn count_skills_in_dir(path: &Path) -> usize {
    let mut count = 0;
    let mut it = WalkDir::new(path).follow_links(false).into_iter();
    while let Some(entry) = it.next() {
        let Ok(entry) = entry else {
            continue;
        };
        if entry.file_type().is_dir() {
            let name = entry.file_name().to_string_lossy();
            if should_ignore_dir(&name) {
                it.skip_current_dir();
                continue;
            }
        } else if entry.file_name() == "SKILL.md" {
            count += 1;
        }
    }
    count
}

impl AppService {
    pub async fn probe_source_path(&self, raw_path: &str) -> AppResult<SourceProbeResult> {
        probe_source_path_inner(raw_path)
    }

    pub async fn split_container_source(&self, source_id: &str) -> AppResult<Vec<Source>> {
        let sources = self.list_sources().await?;
        let source = sources
            .into_iter()
            .find(|s| s.id == source_id)
            .ok_or_else(|| AppError::NotFound(format!("source not found: {source_id}")))?;

        let probe_result = probe_source_path_inner(&source.root_path)?;
        if !probe_result.is_container || probe_result.candidates.is_empty() {
            return Err(AppError::Validation(
                "source is not a multi-repo container or no sub-repositories found".to_string(),
            ));
        }

        let mut new_sources = Vec::new();
        for (index, candidate) in probe_result.candidates.into_iter().enumerate() {
            let input = SourceInput {
                id: None,
                name: candidate.name,
                kind: SourceKind::Import,
                root_path: candidate.path,
                scanner_kind: Some(SourceScannerKind::Skill),
                source_origin: Some(SourceOrigin::GitRepo),
                repo_root: None,
                scan_root: None,
                origin_app_kind: None,
                origin_provider_id: None,
                include_globs: source.include_globs.clone(),
                exclude_globs: source.exclude_globs.clone(),
                default_kind: Some(AssetKind::Skill),
                enabled: source.enabled,
                priority: source.priority + (index as i32),
            };
            let new_source = self.add_source(input).await?;
            new_sources.push(new_source);
        }

        self.remove_source(crate::backend::application::SourceRemoveParams {
            id: source.id,
            dry_run: false,
            yes: true,
        })
        .await?;

        let _ = self.scan_skill_sources().await;

        Ok(new_sources)
    }
}

#[cfg(test)]
#[path = "source_probe_tests.rs"]
mod tests;
