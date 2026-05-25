use crate::{
    path_utils::{expand_path, hash_file, normalize_relative_path},
    types::AppResult,
};
use assetiweave_core::{stable_asset_id, Asset, AssetFormat, AssetKind, Source};
use chrono::Utc;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};
use walkdir::WalkDir;

pub(crate) fn scan_source(source: &Source) -> AppResult<Vec<Asset>> {
    let root = expand_path(&source.root_path)?;
    if !root.exists() {
        return Err(format!("source path does not exist: {}", root.display()));
    }
    if !root.is_dir() {
        return Err(format!(
            "source path is not a directory: {}",
            root.display()
        ));
    }

    let include_set = build_glob_set(&source.include_globs, &["**/*"])?;
    let exclude_set = build_glob_set(&source.exclude_globs, &[])?;
    let mut assets_by_id = HashMap::new();
    let mut seen_skill_dirs = HashSet::new();
    let now = Utc::now().to_rfc3339();

    for entry in WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        let relative = match path.strip_prefix(&root) {
            Ok(relative) if !relative.as_os_str().is_empty() => relative,
            _ => continue,
        };
        let relative_string = normalize_relative_path(relative);

        if exclude_set.is_match(&relative_string) {
            continue;
        }

        if entry.file_type().is_file()
            && path.file_name().and_then(|name| name.to_str()) == Some("SKILL.md")
        {
            if !include_set.is_match(&relative_string) {
                continue;
            }
            let skill_dir = path.parent().unwrap_or(path);
            let skill_relative = skill_dir.strip_prefix(&root).unwrap_or(relative);
            let skill_relative_string = normalize_relative_path(skill_relative);
            if seen_skill_dirs.insert(skill_relative_string.clone()) {
                let asset = build_asset(
                    source,
                    &root,
                    skill_dir,
                    &skill_relative_string,
                    Some(path),
                    AssetKind::Skill,
                    AssetFormat::Directory,
                    &now,
                )?;
                assets_by_id.insert(asset.id.clone(), asset);
            }
            continue;
        }

        if !entry.file_type().is_file() || !include_set.is_match(&relative_string) {
            continue;
        }

        let format = detect_format(path);
        let kind = classify_asset(source, path, &relative_string, format);
        if matches!(format, AssetFormat::Unknown) && matches!(kind, AssetKind::Unclassified) {
            continue;
        }

        let asset = build_asset(
            source,
            &root,
            path,
            &relative_string,
            None,
            kind,
            format,
            &now,
        )?;
        assets_by_id.insert(asset.id.clone(), asset);
    }

    let mut assets: Vec<_> = assets_by_id.into_values().collect();
    assets.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(assets)
}

fn build_asset(
    source: &Source,
    root: &Path,
    asset_path: &Path,
    relative_path: &str,
    entry_file: Option<&Path>,
    kind: AssetKind,
    format: AssetFormat,
    timestamp: &str,
) -> AppResult<Asset> {
    let name = if matches!(format, AssetFormat::Directory) {
        asset_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(relative_path)
            .to_string()
    } else {
        asset_path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or(relative_path)
            .to_string()
    };
    let hash_path = entry_file.unwrap_or(asset_path);
    let description = extract_description(hash_path);
    let content_hash = hash_file(hash_path).ok();
    let id = stable_asset_id(&source.id, relative_path);
    let entry_file_relative = entry_file
        .and_then(|entry| entry.strip_prefix(root).ok())
        .map(normalize_relative_path);

    Ok(Asset {
        id,
        source_id: source.id.clone(),
        name,
        kind,
        format,
        relative_path: relative_path.to_string(),
        absolute_path: asset_path.to_string_lossy().to_string(),
        entry_file: entry_file_relative,
        description,
        content_hash,
        discovered_at: timestamp.to_string(),
        updated_at: timestamp.to_string(),
    })
}

fn classify_asset(
    source: &Source,
    path: &Path,
    relative_path: &str,
    format: AssetFormat,
) -> AssetKind {
    let lower = relative_path.to_lowercase();
    if lower.contains("prompt") {
        return AssetKind::Prompt;
    }
    if lower.contains("rule")
        || lower.contains(".cursorrules")
        || lower.contains("requirements")
        || lower.contains("design")
    {
        return AssetKind::Rule;
    }
    if lower.contains("memory") {
        return AssetKind::Memory;
    }
    if lower.contains("agent") {
        return AssetKind::Agent;
    }
    if lower.contains("workflow") {
        return AssetKind::Workflow;
    }
    if lower.contains("command") || lower.contains("slash") {
        return AssetKind::Command;
    }
    if matches!(
        format,
        AssetFormat::Json | AssetFormat::Yaml | AssetFormat::Toml
    ) && lower.contains("mcp")
    {
        return AssetKind::Mcp;
    }
    if let Some(default_kind) = source.default_kind {
        return default_kind;
    }
    if path.extension().and_then(|extension| extension.to_str()) == Some("md") {
        return AssetKind::Custom;
    }
    AssetKind::Unclassified
}

fn detect_format(path: &Path) -> AssetFormat {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_lowercase();
    match extension.as_str() {
        "md" | "mdx" => AssetFormat::Markdown,
        "json" => AssetFormat::Json,
        "yaml" | "yml" => AssetFormat::Yaml,
        "toml" => AssetFormat::Toml,
        "sh" | "bash" | "zsh" | "js" | "ts" | "py" => AssetFormat::Script,
        "sqlite" | "sqlite3" | "db" => AssetFormat::Sqlite,
        _ => AssetFormat::Unknown,
    }
}

fn extract_description(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    for line in text.lines().map(str::trim) {
        if line.is_empty()
            || line == "---"
            || line.starts_with('#')
            || line.starts_with("name:")
            || line.starts_with("description:")
        {
            if let Some(description) = line.strip_prefix("description:") {
                let cleaned = description.trim().trim_matches('"').to_string();
                if !cleaned.is_empty() {
                    return Some(cleaned);
                }
            }
            continue;
        }
        return Some(line.chars().take(260).collect());
    }
    None
}

fn build_glob_set(patterns: &[String], fallback: &[&str]) -> AppResult<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    let effective: Vec<String> = if patterns.is_empty() {
        fallback
            .iter()
            .map(|pattern| (*pattern).to_string())
            .collect()
    } else {
        patterns.to_vec()
    };
    for pattern in effective {
        builder.add(Glob::new(&pattern).map_err(|error| error.to_string())?);
    }
    builder.build().map_err(|error| error.to_string())
}
