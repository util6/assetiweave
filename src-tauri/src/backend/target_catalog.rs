use crate::backend::models::TargetProfileDescriptor;
use crate::backend::runtime::{AppError, AppResult};
use std::path::Path;

include!(concat!(env!("OUT_DIR"), "/builtin_target_descriptors.rs"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TargetCatalog {
    descriptors: Vec<TargetProfileDescriptor>,
}

impl TargetCatalog {
    pub(crate) fn builtin() -> AppResult<Self> {
        Self::from_descriptors(Self::builtin_descriptors()?)
    }

    pub(crate) fn load_with_overrides(directory: &Path) -> AppResult<Self> {
        std::fs::create_dir_all(directory).map_err(AppError::external)?;
        let mut descriptors = Self::builtin_descriptors()?
            .into_iter()
            .map(|descriptor| (descriptor.id.clone(), descriptor))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut files = std::fs::read_dir(directory)
            .map_err(AppError::external)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
            .collect::<Vec<_>>();
        files.sort();
        if files.len() > 256 {
            return Err(AppError::Validation(
                "target provider override directory contains more than 256 JSON files".to_string(),
            ));
        }
        for path in files {
            let metadata = std::fs::symlink_metadata(&path).map_err(AppError::external)?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(AppError::Validation(format!(
                    "target provider override must be a regular file: {}",
                    path.display()
                )));
            }
            if metadata.len() > 256 * 1024 {
                return Err(AppError::Validation(format!(
                    "target provider override exceeds 256 KiB: {}",
                    path.display()
                )));
            }
            let content = std::fs::read_to_string(&path).map_err(AppError::external)?;
            let descriptor: TargetProfileDescriptor =
                serde_json::from_str(&content).map_err(|error| {
                    AppError::Validation(format!(
                        "invalid target provider override {}: {error}",
                        path.display()
                    ))
                })?;
            descriptors.insert(descriptor.id.clone(), descriptor);
        }
        Self::from_descriptors(descriptors.into_values().collect())
    }

    fn builtin_descriptors() -> AppResult<Vec<TargetProfileDescriptor>> {
        BUILTIN_TARGET_DESCRIPTORS
            .iter()
            .map(|source| {
                serde_json::from_str(source).map_err(|error| AppError::External(error.to_string()))
            })
            .collect::<AppResult<Vec<TargetProfileDescriptor>>>()
    }

    pub(crate) fn from_descriptors(descriptors: Vec<TargetProfileDescriptor>) -> AppResult<Self> {
        let mut ids = std::collections::BTreeSet::new();
        let mut normalized_targets = std::collections::BTreeMap::<String, String>::new();
        for descriptor in &descriptors {
            if descriptor.id.trim().is_empty() || !ids.insert(descriptor.id.clone()) {
                return Err(AppError::Validation(format!(
                    "target provider descriptor id is empty or duplicated: {}",
                    descriptor.id
                )));
            }
            if descriptor.name.trim().is_empty() {
                return Err(AppError::Validation(format!(
                    "target provider descriptor name is empty: {}",
                    descriptor.id
                )));
            }
            let mut target_keys = std::collections::BTreeSet::new();
            for target in &descriptor.default_targets {
                let path = target.path.trim();
                if path.is_empty() {
                    return Err(AppError::Validation(format!(
                        "target provider descriptor has an empty target path: {}",
                        descriptor.id
                    )));
                }
                let key = format!("{:?}\u{0}{path}", target.asset_kind);
                if !target_keys.insert(key) {
                    return Err(AppError::Validation(format!(
                        "target provider descriptor has a duplicated target path: {}",
                        descriptor.id
                    )));
                }
                let normalized_path = Self::target_path_conflict_key(path)?;
                if let Some(existing_provider) = normalized_targets.get(&normalized_path) {
                    if existing_provider != &descriptor.id {
                        return Err(AppError::Validation(format!(
                            "target provider descriptors share the same target path: {} and {} ({path})",
                            existing_provider, descriptor.id
                        )));
                    }
                } else {
                    normalized_targets.insert(normalized_path, descriptor.id.clone());
                }
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

    pub(crate) fn target_path_conflict_key(path: &str) -> AppResult<String> {
        let expanded = crate::backend::path_utils::expand_path(path)?;
        let mut normalized = std::path::PathBuf::from(std::path::MAIN_SEPARATOR.to_string());
        for component in expanded.components() {
            match component {
                std::path::Component::RootDir | std::path::Component::Prefix(_) => {}
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    normalized.pop();
                }
                std::path::Component::Normal(value) => normalized.push(value),
            }
        }
        normalized.to_str().map(|s| s.to_string()).ok_or_else(|| {
            crate::backend::runtime::AppError::Validation(format!(
                "target path contains invalid UTF-8 characters: {path}"
            ))
        })
    }

    #[cfg(test)]
    pub(crate) fn builtin_for_tests() -> AppResult<Self> {
        Self::builtin()
    }
}

#[cfg(test)]
#[path = "target_catalog_tests.rs"]
mod tests;
