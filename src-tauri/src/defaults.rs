use crate::types::{HeaderTabItem, NavigationModel, RailMenuItem, SubNavItem};
use assetiweave_core::{
    AppKind, AssetKind, DeploymentStrategy, ProfileSafety, RuleSet, Source, SourceKind,
    SourceOrigin, SourceScannerKind, TargetProfile,
};
use std::collections::BTreeMap;

pub(crate) fn default_sources() -> Vec<Source> {
    let mut sources = Vec::new();
    let candidates = [
        (
            "assetiweave-library-skills",
            "AssetIWeave Library Skills",
            "~/.assetiweave/library/skills",
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
        ("codex", "Codex", AppKind::Codex, "~/.codex/skills"),
        ("claude", "Claude", AppKind::Claude, "~/.claude/skills"),
        (
            "cursor",
            "Cursor",
            AppKind::Cursor,
            "~/Library/Application Support/Cursor/skills",
        ),
        (
            "opencode",
            "OpenCode",
            AppKind::OpenCode,
            "~/.opencode/skills",
        ),
        ("gemini", "Gemini", AppKind::Gemini, "~/.gemini/skills"),
        (
            "antigravity",
            "Antigravity",
            AppKind::Antigravity,
            "~/.antigravity/skills",
        ),
        (
            "openclaw",
            "OpenClaw",
            AppKind::OpenClaw,
            "~/.openclaw/skills",
        ),
        (
            "custom",
            "Custom",
            AppKind::Custom,
            "~/assetiweave-target/skills",
        ),
    ]
    .into_iter()
    .map(|(id, name, app_kind, target)| TargetProfile {
        id: id.to_string(),
        name: name.to_string(),
        app_kind,
        target_paths: vec![target.to_string()],
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
            rail_item("home", "启动台", "rocket", "global", "primary"),
            rail_item("dashboard", "运行概览", "gauge", "global", "primary"),
            rail_item("routes", "路由", "navigation", "global", "primary"),
            rail_item("knowledge", "知识资产", "brain", "asset-catalog", "primary"),
            rail_item(
                "sources",
                "Source 管理",
                "layers",
                "asset-catalog",
                "primary",
            ),
            rail_item("profiles", "Profile", "boxes", "profile", "primary"),
            rail_item("commands", "命令", "command", "profile", "primary"),
            rail_item("catalog", "资产目录", "archive", "asset-catalog", "primary"),
            rail_item("apps", "App 管理", "grid", "profile", "primary"),
            rail_item("security", "安全策略", "shield", "settings", "secondary"),
            rail_item("docs", "文档", "file-code", "global", "secondary"),
            rail_item("settings", "设置", "settings", "settings", "secondary"),
        ],
        header_tabs: vec![
            header_tab("skills", "Skills", Some("skill")),
            header_tab("mcp", "MCP", Some("mcp")),
            header_tab("prompts", "Prompts", Some("prompt")),
            header_tab("rules", "Rules", Some("rule")),
            header_tab("profiles", "Profiles", Some("profile")),
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
        ]),
    }
}

pub(crate) fn default_app_shortcuts() -> Vec<(&'static str, &'static str, &'static str, bool)> {
    vec![
        ("claude", "C", "#f59e0b", true),
        ("codex", "◎", "#10b981", true),
        ("gemini", "✦", "#0ea5e9", true),
        ("opencode", "□", "#6366f1", true),
        ("cursor", "⌘", "#94a3b8", true),
        ("antigravity", "A", "#a78bfa", false),
        ("openclaw", "O", "#f43f5e", false),
        ("custom", "+", "#8c909f", false),
    ]
}

fn rail_item(id: &str, label: &str, icon: &str, scope: &str, position: &str) -> RailMenuItem {
    RailMenuItem {
        id: id.to_string(),
        label: label.to_string(),
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
        asset_kind: asset_kind.map(str::to_string),
        enabled: true,
    }
}

fn sub_nav(id: &str, label: &str, route_key: &str) -> SubNavItem {
    SubNavItem {
        id: id.to_string(),
        label: label.to_string(),
        route_key: route_key.to_string(),
        enabled: true,
    }
}
