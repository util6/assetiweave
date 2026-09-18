use super::*;

#[test]
fn catalog_assets_expose_portable_display_paths_without_replacing_runtime_paths() {
    let absolute_path = dirs::home_dir()
        .expect("home directory")
        .join(".codex")
        .join("skills")
        .join("review")
        .to_string_lossy()
        .to_string();
    let asset = Asset {
        id: "review".to_string(),
        source_id: "codex".to_string(),
        name: "review".to_string(),
        kind: AssetKind::Skill,
        detector_id: "legacy.classifier".to_string(),
        detector_version: 1,
        format: crate::backend::models::AssetFormat::Directory,
        relative_path: "review".to_string(),
        absolute_path: absolute_path.clone(),
        entry_file: Some("SKILL.md".to_string()),
        description: None,
        content_hash: None,
        discovered_at: "2026-07-17T00:00:00Z".to_string(),
        updated_at: "2026-07-17T00:00:00Z".to_string(),
    };

    let catalog_asset = catalog_asset(asset, None);

    assert_eq!(catalog_asset.asset.absolute_path, absolute_path);
    assert_eq!(catalog_asset.display_path, "~/.codex/skills/review");
}

#[test]
fn system_skill_is_canonical_when_the_same_content_is_seen_elsewhere() {
    let system_source = test_source("assetiweave-system-skills", SourceOrigin::AssetiweaveSystem);
    let external_source = test_source("external-skills", SourceOrigin::LocalFolder);
    let assets = vec![
        test_skill_asset("external-skill", &external_source.id),
        test_skill_asset("system-skill", &system_source.id),
    ];

    let catalog = build_catalog_assets(assets, &[external_source, system_source]);

    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].asset.source_id, "assetiweave-system-skills");
}

#[test]
fn source_assets_keep_every_scanned_copy_for_source_views() {
    let system_source = test_source("assetiweave-system-skills", SourceOrigin::AssetiweaveSystem);
    let external_source = test_source("external-skills", SourceOrigin::LocalFolder);
    let assets = vec![
        test_skill_asset("external-skill", &external_source.id),
        test_skill_asset("system-skill", &system_source.id),
    ];

    let source_assets = build_source_assets(assets, &[external_source, system_source]);

    assert_eq!(source_assets.len(), 2);
    let source_ids = source_assets
        .iter()
        .map(|asset| asset.asset.source_id.as_str())
        .collect::<Vec<_>>();
    assert!(source_ids.contains(&"external-skills"));
    assert!(source_ids.contains(&"assetiweave-system-skills"));
}

fn test_source(id: &str, source_origin: SourceOrigin) -> Source {
    Source {
        id: id.to_string(),
        name: id.to_string(),
        kind: SourceKind::Local,
        root_path: format!("/tmp/{id}"),
        scanner_kind: SourceScannerKind::Skill,
        source_origin,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        origin_provider_id: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: Vec::new(),
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    }
}

fn test_skill_asset(id: &str, source_id: &str) -> Asset {
    Asset {
        id: id.to_string(),
        source_id: source_id.to_string(),
        name: "same-skill".to_string(),
        kind: AssetKind::Skill,
        detector_id: "legacy.classifier".to_string(),
        detector_version: 1,
        format: crate::backend::models::AssetFormat::Directory,
        relative_path: "same-skill".to_string(),
        absolute_path: format!("/tmp/{source_id}/same-skill"),
        entry_file: Some("SKILL.md".to_string()),
        description: None,
        content_hash: Some("same-content".to_string()),
        discovered_at: "2026-07-22T00:00:00Z".to_string(),
        updated_at: "2026-07-22T00:00:00Z".to_string(),
    }
}
