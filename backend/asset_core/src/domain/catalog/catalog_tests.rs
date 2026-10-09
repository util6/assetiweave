use super::*;

#[test]
fn system_skill_source_is_a_pure_catalog_descriptor() {
    let source = system_skill_source("~/.assetiweave/skills/.system".to_string());

    assert_eq!(source.id, SYSTEM_SKILL_SOURCE_ID);
    assert_eq!(source.root_path, "~/.assetiweave/skills/.system");
    assert_eq!(source.scanner_kind, SourceScannerKind::Skill);
    assert_eq!(source.source_origin, SourceOrigin::AssetiweaveSystem);
    assert_eq!(source.include_globs, vec!["**/SKILL.md"]);
    assert!(source.enabled);
}

#[test]
fn stable_asset_id_is_repeatable() {
    assert_eq!(
        stable_asset_id("source-a", "skills/foo/SKILL.md"),
        stable_asset_id("source-a", "skills/foo/SKILL.md")
    );
}

#[test]
fn stable_asset_id_depends_on_source() {
    assert_ne!(
        stable_asset_id("source-a", "skills/foo/SKILL.md"),
        stable_asset_id("source-b", "skills/foo/SKILL.md")
    );
}

#[test]
fn domain_source_kind_preserves_existing_serde_names() {
    let value = serde_json::to_value(SourceKind::Local).unwrap();
    assert_eq!(value, serde_json::json!("local"));
}
