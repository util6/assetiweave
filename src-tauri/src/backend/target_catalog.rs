use crate::backend::models::TargetProfileDescriptor;
use crate::backend::runtime::{AppError, AppResult};

include!(concat!(env!("OUT_DIR"), "/builtin_target_descriptors.rs"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TargetCatalog {
    descriptors: Vec<TargetProfileDescriptor>,
}

impl TargetCatalog {
    pub(crate) fn builtin() -> AppResult<Self> {
        let descriptors = BUILTIN_TARGET_DESCRIPTORS
            .iter()
            .map(|source| {
                serde_json::from_str(source).map_err(|error| AppError::External(error.to_string()))
            })
            .collect::<AppResult<Vec<TargetProfileDescriptor>>>()?;
        Self::from_descriptors(descriptors)
    }

    pub(crate) fn from_descriptors(descriptors: Vec<TargetProfileDescriptor>) -> AppResult<Self> {
        let mut ids = std::collections::BTreeSet::new();
        for descriptor in &descriptors {
            if descriptor.id.trim().is_empty() || !ids.insert(descriptor.id.clone()) {
                return Err(AppError::Validation(format!(
                    "target provider descriptor id is empty or duplicated: {}",
                    descriptor.id
                )));
            }
            if descriptor.default_targets.is_empty() {
                return Err(AppError::Validation(format!(
                    "target provider descriptor has no default target: {}",
                    descriptor.id
                )));
            }
        }
        Ok(Self { descriptors })
    }

    pub(crate) fn descriptors(&self) -> &[TargetProfileDescriptor] {
        &self.descriptors
    }

    pub(crate) fn descriptor(&self, provider_id: &str) -> Option<&TargetProfileDescriptor> {
        self.descriptors
            .iter()
            .find(|descriptor| descriptor.id == provider_id)
    }

    pub(crate) fn require_descriptor(
        &self,
        provider_id: &str,
    ) -> AppResult<&TargetProfileDescriptor> {
        self.descriptor(provider_id).ok_or_else(|| {
            AppError::NotFound(format!(
                "target_provider_missing: target provider is not available: {provider_id}"
            ))
        })
    }

    #[cfg(test)]
    pub(crate) fn builtin_for_tests() -> AppResult<Self> {
        Self::builtin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::models::{AppKind, AssetKind};

    #[test]
    fn builtin_descriptors_cover_legacy_app_kinds() {
        let catalog = TargetCatalog::builtin().expect("builtin target catalog");
        for app_kind in [
            AppKind::Codex,
            AppKind::Claude,
            AppKind::Cursor,
            AppKind::OpenCode,
            AppKind::Gemini,
            AppKind::Antigravity,
            AppKind::OpenClaw,
            AppKind::Kiro,
            AppKind::Zcode,
            AppKind::Qoder,
            AppKind::Hermes,
            AppKind::Custom,
        ] {
            let descriptor = catalog
                .descriptors()
                .iter()
                .find(|descriptor| descriptor.app_kind_compat == Some(app_kind))
                .expect("legacy app kind descriptor");
            assert!(descriptor.supported_kinds.contains(&AssetKind::Skill));
        }
    }

    #[test]
    fn a_new_provider_can_be_loaded_without_core_enum_changes() {
        let catalog = TargetCatalog::from_descriptors(vec![TargetProfileDescriptor {
            id: "fixture-agent".to_string(),
            name: "Fixture Agent".to_string(),
            app_kind_compat: None,
            default_targets: vec![crate::backend::models::TargetPathRule {
                asset_kind: AssetKind::Skill,
                path: "~/fixture-agent/skills".to_string(),
            }],
            supported_kinds: vec![AssetKind::Skill],
            deployment_strategy: crate::backend::models::DeploymentStrategy::SymlinkToSource,
            icon: None,
        }])
        .expect("fixture descriptor");
        assert_eq!(
            catalog.descriptor("fixture-agent").unwrap().name,
            "Fixture Agent"
        );
    }
}
