use crate::backend::models::AppKind;

pub(crate) struct AppPathCatalog;

impl AppPathCatalog {
    pub(crate) fn default_skill_target(app_kind: AppKind) -> &'static str {
        match app_kind {
            AppKind::Codex => "~/.codex/skills",
            AppKind::Claude => "~/.claude/skills",
            AppKind::Cursor => "@config/Cursor/skills",
            AppKind::OpenCode => "~/.config/opencode/skills",
            AppKind::Gemini => "~/.gemini/skills",
            AppKind::Antigravity => "~/.antigravity/skills",
            AppKind::OpenClaw => "~/.openclaw/skills",
            AppKind::Custom => "~/assetiweave-target/skills",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_target_uses_config_anchor_instead_of_macos_literal_path() {
        assert_eq!(
            AppPathCatalog::default_skill_target(AppKind::Cursor),
            "@config/Cursor/skills"
        );
    }

    #[test]
    fn home_based_targets_remain_portable() {
        assert_eq!(
            AppPathCatalog::default_skill_target(AppKind::Codex),
            "~/.codex/skills"
        );
        assert_eq!(
            AppPathCatalog::default_skill_target(AppKind::Claude),
            "~/.claude/skills"
        );
    }
}
