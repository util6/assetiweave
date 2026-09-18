use super::{
    default_app_shortcuts, default_navigation_model, default_profiles_from_catalog,
    default_sources_for_tenant,
};
use crate::backend::models::TargetProfile;
use crate::backend::target_catalog::TargetCatalog;

fn default_profiles() -> Vec<TargetProfile> {
    let catalog = TargetCatalog::builtin_for_tests().expect("builtin target descriptors");
    default_profiles_from_catalog(&catalog)
}

#[test]
fn memory_is_an_independent_default_navigation_module() {
    let navigation = default_navigation_model();
    let memory = navigation
        .header_tabs
        .iter()
        .find(|tab| tab.id == "memory")
        .expect("memory header tab");

    assert_eq!(memory.asset_kind, None);
    assert_eq!(
        navigation.sub_nav_items["memory"]
            .iter()
            .map(|item| item.route_key.as_str())
            .collect::<Vec<_>>(),
        vec!["memory.recent", "memory.recall"]
    );
}

#[test]
fn team_is_an_independent_default_navigation_module() {
    let navigation = default_navigation_model();
    let team = navigation
        .header_tabs
        .iter()
        .find(|tab| tab.id == "team")
        .expect("team header tab");

    assert_eq!(team.asset_kind, None);
    assert_eq!(
        navigation.sub_nav_items["team"]
            .iter()
            .map(|item| item.route_key.as_str())
            .collect::<Vec<_>>(),
        vec!["team.overview"]
    );
}

#[test]
fn opencode_default_profile_uses_config_skills_path() {
    let profile = default_profiles()
        .into_iter()
        .find(|profile| profile.id == "opencode")
        .expect("opencode profile");

    assert_eq!(profile.target_paths, vec!["~/.config/opencode/skills"]);
}

#[test]
fn cursor_default_profile_uses_cross_platform_config_anchor() {
    let profile = default_profiles()
        .into_iter()
        .find(|profile| profile.id == "cursor")
        .expect("cursor profile");

    assert_eq!(profile.target_paths, vec!["@config/Cursor/skills"]);
}

#[test]
fn every_builtin_app_icon_has_a_profile_and_shortcut() {
    let builtin_app_ids = [
        "antigravity",
        "claude",
        "codex",
        "cursor",
        "gemini",
        "hermes",
        "kiro",
        "openclaw",
        "opencode",
        "qoder",
        "zcode",
    ];
    let profiles = default_profiles();
    let shortcuts = default_app_shortcuts();

    for app_id in builtin_app_ids {
        assert!(
            profiles.iter().any(|profile| profile.id == app_id),
            "missing profile: {app_id}"
        );
        assert!(
            shortcuts.iter().any(|shortcut| shortcut.0 == app_id),
            "missing shortcut: {app_id}"
        );
    }
}

#[test]
fn skill_sources_scope_only_assetiweave_library_path_by_tenant() {
    let tenant_a_sources = default_sources_for_tenant("tenant-a");
    let tenant_b_sources = default_sources_for_tenant("tenant-b");

    let tenant_a_library = tenant_a_sources
        .iter()
        .find(|source| source.id == "assetiweave-library-skills")
        .expect("tenant a skill library source");
    let tenant_b_library = tenant_b_sources
        .iter()
        .find(|source| source.id == "assetiweave-library-skills")
        .expect("tenant b skill library source");
    assert_eq!(
        tenant_a_library.root_path,
        "~/.assetiweave/tenants/tenant-a/library/skills"
    );
    assert_eq!(
        tenant_b_library.root_path,
        "~/.assetiweave/tenants/tenant-b/library/skills"
    );
    assert!(tenant_a_library
        .root_path
        .ends_with(".assetiweave/tenants/tenant-a/library/skills"));
    assert!(tenant_b_library
        .root_path
        .ends_with(".assetiweave/tenants/tenant-b/library/skills"));
    assert_ne!(tenant_a_library.root_path, tenant_b_library.root_path);

    let tenant_a_codex = tenant_a_sources
        .iter()
        .find(|source| source.id == "codex-skills")
        .expect("tenant a codex source");
    let tenant_b_codex = tenant_b_sources
        .iter()
        .find(|source| source.id == "codex-skills")
        .expect("tenant b codex source");
    assert_eq!(tenant_a_codex.root_path, "~/.codex/skills");
    assert_eq!(tenant_a_codex.root_path, tenant_b_codex.root_path);

    let tenant_a_agents = tenant_a_sources
        .iter()
        .find(|source| source.id == "agents-skills")
        .expect("tenant a agents source");
    let tenant_b_agents = tenant_b_sources
        .iter()
        .find(|source| source.id == "agents-skills")
        .expect("tenant b agents source");
    assert_eq!(tenant_a_agents.root_path, tenant_b_agents.root_path);
}

#[test]
fn system_skill_source_is_shared_across_tenants() {
    let tenant_a = default_sources_for_tenant("tenant-a")
        .into_iter()
        .find(|source| source.id == "assetiweave-system-skills")
        .expect("tenant a system Skill source");
    let tenant_b = default_sources_for_tenant("tenant-b")
        .into_iter()
        .find(|source| source.id == "assetiweave-system-skills")
        .expect("tenant b system Skill source");

    assert_eq!(tenant_a.root_path, "~/.assetiweave/skills/.system");
    assert_eq!(tenant_a, tenant_b);
}
