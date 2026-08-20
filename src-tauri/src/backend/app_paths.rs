use crate::backend::{models::AppKind, target_catalog::TargetCatalog};

pub(crate) struct AppPathCatalog;

impl AppPathCatalog {
    pub(crate) fn default_skill_target(catalog: &TargetCatalog, app_kind: AppKind) -> String {
        catalog
            .descriptor_for_app_kind(app_kind)
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

    fn builtin_catalog() -> TargetCatalog {
        TargetCatalog::builtin().expect("target catalog")
    }

    #[test]
    fn cursor_target_uses_config_anchor_instead_of_macos_literal_path() {
        let catalog = builtin_catalog();
        assert_eq!(
            AppPathCatalog::default_skill_target(&catalog, AppKind::Cursor),
            "@config/Cursor/skills".to_string()
        );
    }

    #[test]
    fn home_based_targets_remain_portable() {
        let catalog = builtin_catalog();
        assert_eq!(
            AppPathCatalog::default_skill_target(&catalog, AppKind::Codex),
            "~/.codex/skills".to_string()
        );
        assert_eq!(
            AppPathCatalog::default_skill_target(&catalog, AppKind::Claude),
            "~/.claude/skills".to_string()
        );
    }
}
