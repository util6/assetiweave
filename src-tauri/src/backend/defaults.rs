use crate::backend::dto::{HeaderTabItem, NavigationModel, RailMenuItem, SubNavItem};
use crate::backend::models::{
    AppKind, AssetKind, DeploymentStrategy, ProfileSafety, RuleSet, Source, SourceKind,
    SourceOrigin, SourceScannerKind, TargetProfile,
};
use std::collections::BTreeMap;

pub(crate) const DEFAULT_APP_PROFILE_IDS: &[&str] = &[
    "claude",
    "codex",
    "gemini",
    "opencode",
    "cursor",
    "antigravity",
    "openclaw",
];

pub(crate) fn is_default_app_profile_id(profile_id: &str) -> bool {
    DEFAULT_APP_PROFILE_IDS.contains(&profile_id)
}

pub(crate) fn default_sources_for_tenant(tenant_id: &str) -> Vec<Source> {
    let mut sources = Vec::new();
    let default_skill_root =
        crate::backend::path_utils::default_skill_backup_root_for_tenant(tenant_id)
            .and_then(|path| {
                crate::backend::path_utils::normalize_path_for_storage(&path.to_string_lossy())
            })
            .unwrap_or_else(|_| format!("~/.assetiweave/tenants/{tenant_id}/library/skills"));
    let candidates = [
        (
            "assetiweave-library-skills",
            "AssetIWeave Skill Backup Library",
            default_skill_root.as_str(),
            vec!["**/SKILL.md"],
            Some(AssetKind::Skill),
            SourceOrigin::AssetiweaveLibrary,
        ),
        (
            "codex-skills",
            "Codex Skills",
            "~/.codex/skills",
            vec!["**/SKILL.md"],
            Some(AssetKind::Skill),
            SourceOrigin::AppTarget,
        ),
        (
            "agents-skills",
            "Agents Skills",
            "~/.agents/skills",
            vec!["**/SKILL.md"],
            Some(AssetKind::Skill),
            SourceOrigin::GitRepo,
        ),
        (
            "project-specs",
            "当前项目 Specs",
            "specs",
            vec!["**/*.md"],
            Some(AssetKind::Rule),
            SourceOrigin::LocalFolder,
        ),
    ];

    for (priority, (id, name, path, includes, default_kind, source_origin)) in
        candidates.into_iter().enumerate()
    {
        sources.push(Source {
            id: id.to_string(),
            name: name.to_string(),
            kind: SourceKind::Local,
            root_path: path.to_string(),
            scanner_kind: if default_kind == Some(AssetKind::Skill) {
                SourceScannerKind::Skill
            } else {
                SourceScannerKind::Rule
            },
            source_origin,
            repo_root: None,
            scan_root: String::new(),
            origin_app_kind: None,
            include_globs: includes.into_iter().map(str::to_string).collect(),
            exclude_globs: vec![
                "**/.git/**".to_string(),
                "**/node_modules/**".to_string(),
                "**/target/**".to_string(),
                "**/dist/**".to_string(),
            ],
            default_kind,
            enabled: true,
            priority: priority as i32 * 10,
            last_scanned_at: None,
            last_scan_status: Some("pending".to_string()),
        });
    }

    sources
}

pub(crate) fn default_profiles() -> Vec<TargetProfile> {
    [
        ("codex", "Codex", AppKind::Codex),
        ("claude", "Claude", AppKind::Claude),
        ("cursor", "Cursor", AppKind::Cursor),
        ("opencode", "OpenCode", AppKind::OpenCode),
        ("gemini", "Gemini", AppKind::Gemini),
        ("antigravity", "Antigravity", AppKind::Antigravity),
        ("openclaw", "OpenClaw", AppKind::OpenClaw),
        ("custom", "Custom", AppKind::Custom),
    ]
    .into_iter()
    .map(|(id, name, app_kind)| TargetProfile {
        id: id.to_string(),
        name: name.to_string(),
        app_kind,
        target_paths: vec![
            crate::backend::app_paths::AppPathCatalog::default_skill_target(app_kind).to_string(),
        ],
        supported_kinds: vec![
            AssetKind::Skill,
            AssetKind::Prompt,
            AssetKind::Rule,
            AssetKind::Custom,
        ],
        deployment_strategy: DeploymentStrategy::SymlinkToSource,
        enabled: true,
        include: RuleSet {
            kinds: vec![AssetKind::Skill, AssetKind::Prompt, AssetKind::Rule],
            tags: vec![],
            groups: vec![],
            sources: vec![],
            path_patterns: vec![],
        },
        exclude: RuleSet {
            kinds: vec![AssetKind::Unclassified],
            tags: vec![],
            groups: vec![],
            sources: vec![],
            path_patterns: vec![],
        },
        safety: ProfileSafety {
            allow_remove: false,
            allow_overwrite: false,
        },
    })
    .collect()
}

pub(crate) fn default_navigation_model() -> NavigationModel {
    NavigationModel {
        active_rail_id: "catalog".to_string(),
        active_header_tab_id: "skills".to_string(),
        active_sub_nav_id: "overview".to_string(),
        rail_items: vec![
            rail_item("logs", "日志", "file-text", "global", "secondary"),
            rail_item("settings", "设置", "settings", "settings", "secondary"),
        ],
        header_tabs: vec![
            header_tab("skills", "Skills", Some("skill")),
            header_tab("mcp", "MCP", Some("mcp")),
            header_tab("prompts", "Prompts", Some("prompt")),
            header_tab("rules", "Rules", Some("rule")),
            header_tab("profiles", "Profiles", Some("profile")),
            header_tab("conversations", "Conversations", None),
        ],
        sub_nav_items: BTreeMap::from([
            (
                "skills".to_string(),
                vec![
                    sub_nav("overview", "目录总览", "skills.overview"),
                    sub_nav("groups", "分组管理", "skills.groups"),
                    sub_nav("sources", "Skill 源管理", "skills.sources"),
                    sub_nav("mounts", "挂载管理", "skills.mounts"),
                ],
            ),
            (
                "mcp".to_string(),
                vec![
                    sub_nav("overview", "服务总览", "mcp.overview"),
                    sub_nav("servers", "Server 管理", "mcp.servers"),
                    sub_nav("configs", "配置投影", "mcp.configs"),
                ],
            ),
            (
                "prompts".to_string(),
                vec![
                    sub_nav("overview", "Prompt 总览", "prompts.overview"),
                    sub_nav("templates", "模板管理", "prompts.templates"),
                    sub_nav("targets", "目标 App", "prompts.targets"),
                ],
            ),
            (
                "rules".to_string(),
                vec![
                    sub_nav("overview", "规则总览", "rules.overview"),
                    sub_nav("policies", "启用策略", "rules.policies"),
                    sub_nav("conflicts", "冲突检测", "rules.conflicts"),
                ],
            ),
            (
                "profiles".to_string(),
                vec![
                    sub_nav("overview", "App 总览", "profiles.overview"),
                    sub_nav("templates", "Profile 模板", "profiles.templates"),
                    sub_nav("plans", "部署计划", "profiles.plans"),
                ],
            ),
            (
                "conversations".to_string(),
                vec![
                    sub_nav("sessions", "Session 浏览", "conversations.sessions"),
                    sub_nav("web-records", "网页记录浏览", "conversations.web-records"),
                ],
            ),
        ]),
    }
}

pub(crate) fn default_app_shortcuts() -> Vec<(&'static str, &'static str, &'static str, bool)> {
    vec![
        ("claude", "app:claude", "#d97757", true),
        ("codex", "app:codex", "#10b981", true),
        ("gemini", "app:gemini", "#8e75b2", true),
        ("opencode", "app:opencode", "#6366f1", true),
        ("cursor", "app:cursor", "#94a3b8", true),
        ("antigravity", "app:antigravity", "#a78bfa", false),
        ("openclaw", "app:openclaw", "#f43f5e", false),
        ("custom", "+", "#8c909f", false),
    ]
}

fn rail_item(id: &str, label: &str, icon: &str, scope: &str, position: &str) -> RailMenuItem {
    RailMenuItem {
        id: id.to_string(),
        label: label.to_string(),
        labels: None,
        icon: icon.to_string(),
        scope: scope.to_string(),
        enabled: true,
        position: position.to_string(),
    }
}

fn header_tab(id: &str, label: &str, asset_kind: Option<&str>) -> HeaderTabItem {
    HeaderTabItem {
        id: id.to_string(),
        label: label.to_string(),
        labels: None,
        asset_kind: asset_kind.map(str::to_string),
        enabled: true,
    }
}

fn sub_nav(id: &str, label: &str, route_key: &str) -> SubNavItem {
    SubNavItem {
        id: id.to_string(),
        label: label.to_string(),
        labels: None,
        route_key: route_key.to_string(),
        enabled: true,
    }
}

#[cfg(test)]
mod tests {
    use super::{default_profiles, default_sources_for_tenant};

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
}
