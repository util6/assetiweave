#[cfg(test)]
use super::prelude::*;
use super::{refresh_recorded_asset, scan_skill_source};
use crate::backend::models::{SourceKind, SourceOrigin};
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

#[test]
fn scan_skill_source_finds_nested_skill_directories() {
    let root = unique_temp_dir("assetiweave-scan-nested-skills");
    fs::create_dir_all(root.join("office-utils").join("kitchen")).expect("create nested skill");
    fs::create_dir_all(root.join("codex-token-usage")).expect("create top skill");
    fs::write(
        root.join("office-utils").join("kitchen").join("SKILL.md"),
        "description: kitchen",
    )
    .expect("write nested skill");
    fs::write(
        root.join("codex-token-usage").join("SKILL.md"),
        "description: token usage",
    )
    .expect("write top skill");

    let assets = scan_skill_source(&test_source(&root)).expect("scan skills");

    fs::remove_dir_all(&root).ok();
    let asset_names: Vec<_> = assets.iter().map(|asset| asset.name.as_str()).collect();
    assert_eq!(asset_names, vec!["codex-token-usage", "kitchen"]);
}

#[test]
fn scan_skill_source_keeps_canonical_locale_for_duplicate_skill_names() {
    let root = unique_temp_dir("assetiweave-scan-canonical-skill");
    fs::create_dir_all(root.join("zh-cn").join("learn-project")).expect("create zh skill");
    fs::create_dir_all(root.join("en-us").join("learn-project")).expect("create en skill");
    fs::create_dir_all(
        root.join("en-us")
            .join("office-utils")
            .join("learn-project"),
    )
    .expect("create nested en skill");
    fs::write(
        root.join("zh-cn").join("learn-project").join("SKILL.md"),
        "description: zh",
    )
    .expect("write zh skill");
    fs::write(
        root.join("en-us").join("learn-project").join("SKILL.md"),
        "description: en",
    )
    .expect("write en skill");
    fs::write(
        root.join("en-us")
            .join("office-utils")
            .join("learn-project")
            .join("SKILL.md"),
        "description: nested en",
    )
    .expect("write nested en skill");

    let assets = scan_skill_source(&test_source(&root)).expect("scan skills");

    fs::remove_dir_all(&root).ok();
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].name, "learn-project");
    assert_eq!(assets[0].relative_path, "zh-cn/learn-project");
    assert_eq!(assets[0].description.as_deref(), Some("zh"));
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
