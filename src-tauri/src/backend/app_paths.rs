use crate::backend::models::AppKind;

pub(crate) struct AppPathCatalog;

impl AppPathCatalog {
    pub(crate) fn default_skill_target(app_kind: AppKind) -> String {
        crate::backend::target_catalog::TargetCatalog::builtin()
            .ok()
            .and_then(|catalog| catalog.descriptor_for_app_kind(app_kind).cloned())
            .and_then(|descriptor| {
                descriptor
                    .default_targets
                    .first()
                    .map(|target| target.path.clone())
            })
            .unwrap_or_else(|| "~/assetiweave-target/skills".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_target_uses_config_anchor_instead_of_macos_literal_path() {
        assert_eq!(
            AppPathCatalog::default_skill_target(AppKind::Cursor),
            "@config/Cursor/skills".to_string()
        );
    }

    #[test]
    fn home_based_targets_remain_portable() {
        assert_eq!(
            AppPathCatalog::default_skill_target(AppKind::Codex),
            "~/.codex/skills".to_string()
        );
        assert_eq!(
            AppPathCatalog::default_skill_target(AppKind::Claude),
            "~/.claude/skills".to_string()
        );
    }
}
