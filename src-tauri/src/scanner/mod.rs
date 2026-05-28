use crate::{
    path_utils::{expand_path, hash_path, normalize_relative_path},
    types::AppResult,
};
use assetiweave_core::{stable_asset_id, Asset, AssetFormat, AssetKind, Source, SourceScannerKind};
use chrono::Utc;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};
use walkdir::WalkDir;

trait AssetScanner {
    fn scan(&self, source: &Source) -> AppResult<Vec<Asset>>;
}

struct SkillScanner;
struct MixedScanner;

pub(crate) fn scan_source(source: &Source) -> AppResult<Vec<Asset>> {
    match source.scanner_kind {
        SourceScannerKind::Skill => SkillScanner.scan(source),
        _ => MixedScanner.scan(source),
    }
}

pub(crate) fn scan_skill_source(source: &Source) -> AppResult<Vec<Asset>> {
    SkillScanner.scan(source)
}

pub(crate) fn refresh_recorded_asset(
    source: &Source,
    asset: &Asset,
    timestamp: &str,
) -> AppResult<Option<Asset>> {
    let asset_path = Path::new(&asset.absolute_path);
    if !asset_path.exists() {
        return Ok(None);
    }

    let description_path = recorded_description_path(source, asset, asset_path);
    let content_path = if matches!(asset.format, AssetFormat::Directory) {
        asset_path
    } else {
        description_path.as_path()
    };
    let description = extract_description(&description_path);
    let content_hash = hash_path(content_path)?;
    let changed = asset.content_hash.as_deref() != Some(content_hash.as_str())
        || asset.description != description;

    let mut refreshed = asset.clone();
    refreshed.description = description;
    refreshed.content_hash = Some(content_hash);
    if changed {
        refreshed.updated_at = timestamp.to_string();
    }
    Ok(Some(refreshed))
}

impl AssetScanner for SkillScanner {
    fn scan(&self, source: &Source) -> AppResult<Vec<Asset>> {
        scan_skill_assets(source)
    }
}

impl AssetScanner for MixedScanner {
    fn scan(&self, source: &Source) -> AppResult<Vec<Asset>> {
        scan_mixed_assets(source)
    }
}

fn scan_skill_assets(source: &Source) -> AppResult<Vec<Asset>> {
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

    let include_set = build_glob_set(&source.include_globs, &["**/SKILL.md"])?;
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

        if !entry.file_type().is_file()
            || path.file_name().and_then(|name| name.to_str()) != Some("SKILL.md")
            || !include_set.is_match(&relative_string)
        {
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
    }

    let mut assets: Vec<_> = assets_by_id.into_values().collect();
    assets.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(assets)
}

fn scan_mixed_assets(source: &Source) -> AppResult<Vec<Asset>> {
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
    let description_path = entry_file.unwrap_or(asset_path);
    let content_path = if matches!(format, AssetFormat::Directory) {
        asset_path
    } else {
        description_path
    };
    let description = extract_description(description_path);
    let content_hash = hash_path(content_path).ok();
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

fn recorded_description_path(
    source: &Source,
    asset: &Asset,
    asset_path: &Path,
) -> std::path::PathBuf {
    if let Some(entry_file) = asset.entry_file.as_deref() {
        if let Ok(root) = expand_path(&source.root_path) {
            let entry_path = root.join(entry_file);
            if entry_path.exists() {
                return entry_path;
            }
        }
    }

    if matches!(asset.format, AssetFormat::Directory) {
        let skill_entry = asset_path.join("SKILL.md");
        if skill_entry.exists() {
            return skill_entry;
        }
    }

    asset_path.to_path_buf()
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

#[cfg(test)]
mod tests {
    use super::*;
    use assetiweave_core::{SourceKind, SourceOrigin};
    use std::{fs, path::PathBuf};

    #[test]
    fn refresh_recorded_asset_returns_none_for_deleted_asset() {
        let root = unique_temp_dir("assetiweave-recorded-missing");
        fs::create_dir_all(&root).expect("create source root");
        let source = test_source(&root);
        let asset = test_skill_asset(&root.join("missing-skill"));

        let refreshed =
            refresh_recorded_asset(&source, &asset, "2026-01-01T00:00:00Z").expect("refresh asset");

        fs::remove_dir_all(&root).ok();
        assert!(refreshed.is_none());
    }

    #[test]
    fn refresh_recorded_asset_updates_existing_directory_without_discovery() {
        let root = unique_temp_dir("assetiweave-recorded-existing");
        let recorded_dir = root.join("recorded-skill");
        let new_dir = root.join("new-skill");
        fs::create_dir_all(&recorded_dir).expect("create recorded skill");
        fs::create_dir_all(&new_dir).expect("create new skill");
        fs::write(recorded_dir.join("SKILL.md"), "description: recorded")
            .expect("write recorded skill");
        fs::write(recorded_dir.join("script.sh"), "one").expect("write script");
        fs::write(new_dir.join("SKILL.md"), "description: new").expect("write new skill");

        let source = test_source(&root);
        let mut asset = test_skill_asset(&recorded_dir);
        asset.content_hash = Some("stale".to_string());
        let refreshed =
            refresh_recorded_asset(&source, &asset, "2026-01-01T00:00:00Z").expect("refresh asset");

        fs::remove_dir_all(&root).ok();
        let refreshed = refreshed.expect("recorded asset should remain");
        assert_eq!(refreshed.name, "recorded-skill");
        assert_eq!(refreshed.description.as_deref(), Some("recorded"));
        assert_ne!(refreshed.content_hash, Some("stale".to_string()));
    }

    fn test_source(root: &Path) -> Source {
        Source {
            id: "source-a".to_string(),
            name: "Source A".to_string(),
            kind: SourceKind::Local,
            root_path: root.to_string_lossy().to_string(),
            scanner_kind: SourceScannerKind::Skill,
            source_origin: SourceOrigin::GitRepo,
            repo_root: None,
            scan_root: String::new(),
            origin_app_kind: None,
            include_globs: vec!["**/SKILL.md".to_string()],
            exclude_globs: vec![],
            default_kind: Some(AssetKind::Skill),
            enabled: true,
            priority: 0,
            last_scanned_at: None,
            last_scan_status: None,
        }
    }

    fn test_skill_asset(path: &Path) -> Asset {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("skill")
            .to_string();
        Asset {
            id: format!("asset-{name}"),
            source_id: "source-a".to_string(),
            name: name.clone(),
            kind: AssetKind::Skill,
            format: AssetFormat::Directory,
            relative_path: name.clone(),
            absolute_path: path.to_string_lossy().to_string(),
            entry_file: Some(format!("{name}/SKILL.md")),
            description: None,
            content_hash: None,
            discovered_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()))
    }
}
