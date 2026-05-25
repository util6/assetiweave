use assetiweave_core::{
    AppKind, AssetKind, DeploymentStrategy, ProfileSafety, RuleSet, Source, SourceKind,
    TargetProfile,
};

pub(crate) fn default_sources() -> Vec<Source> {
    let mut sources = Vec::new();
    let candidates = [
        (
            "codex-skills",
            "Codex Skills",
            "~/.codex/skills",
            vec!["**/SKILL.md"],
            Some(AssetKind::Skill),
        ),
        (
            "agents-skills",
            "Agents Skills",
            "~/.agents/skills",
            vec!["**/SKILL.md"],
            Some(AssetKind::Skill),
        ),
        (
            "project-specs",
            "当前项目 Specs",
            "specs",
            vec!["**/*.md"],
            Some(AssetKind::Rule),
        ),
    ];

    for (priority, (id, name, path, includes, default_kind)) in candidates.into_iter().enumerate() {
        sources.push(Source {
            id: id.to_string(),
            name: name.to_string(),
            kind: SourceKind::Local,
            root_path: path.to_string(),
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
        ("codex", "Codex", AppKind::Codex, "~/.codex/assetiweave"),
        ("claude", "Claude", AppKind::Claude, "~/.claude/assetiweave"),
        (
            "cursor",
            "Cursor",
            AppKind::Cursor,
            "~/Library/Application Support/Cursor/assetiweave",
        ),
        (
            "opencode",
            "OpenCode",
            AppKind::OpenCode,
            "~/.opencode/assetiweave",
        ),
        ("gemini", "Gemini", AppKind::Gemini, "~/.gemini/assetiweave"),
        (
            "antigravity",
            "Antigravity",
            AppKind::Antigravity,
            "~/.antigravity/assetiweave",
        ),
        (
            "openclaw",
            "OpenClaw",
            AppKind::OpenClaw,
            "~/.openclaw/assetiweave",
        ),
        ("custom", "Custom", AppKind::Custom, "~/assetiweave-target"),
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
        deployment_strategy: DeploymentStrategy::Symlink,
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
