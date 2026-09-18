use super::*;

#[test]
fn app_kind_uses_frontend_compatible_names() {
    assert_eq!(
        serde_json::to_string(&AppKind::OpenCode).unwrap(),
        "\"opencode\""
    );
    assert_eq!(
        serde_json::to_string(&AppKind::OpenClaw).unwrap(),
        "\"openclaw\""
    );
    assert_eq!(
        serde_json::from_str::<AppKind>("\"open_code\"").unwrap(),
        AppKind::OpenCode
    );
    assert_eq!(
        serde_json::from_str::<AppKind>("\"open_claw\"").unwrap(),
        AppKind::OpenClaw
    );
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
