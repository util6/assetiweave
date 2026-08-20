use std::{collections::HashSet, sync::Arc};

use chrono::DateTime;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::types::{Catalog, CatalogItem, Distribution};

const CATALOG_SCHEMA: &str = "assetiweave.agent-market/v1";
const MAX_CATALOG_BYTES: usize = 5 * 1024 * 1024;
const BUNDLED_CATALOG: &str =
    include_str!("../../../../builtin-assets/agent-market/catalog-v1.json");

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CatalogError {
    TooLarge,
    InvalidJson(String),
    Invalid(String),
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge => formatter.write_str("Agent catalog exceeds the 5 MiB limit"),
            Self::InvalidJson(message) | Self::Invalid(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for CatalogError {}

#[derive(Clone, Debug)]
pub(crate) struct CatalogService {
    catalog: Arc<Catalog>,
}

impl CatalogService {
    pub(crate) fn bundled() -> Result<Self, CatalogError> {
        Ok(Self {
            catalog: Arc::new(parse_catalog(BUNDLED_CATALOG.as_bytes())?),
        })
    }

    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self, CatalogError> {
        Ok(Self {
            catalog: Arc::new(parse_catalog(bytes)?),
        })
    }

    pub(crate) fn from_catalog(catalog: Catalog) -> Self {
        Self {
            catalog: Arc::new(catalog),
        }
    }

    pub(crate) fn catalog(&self) -> Arc<Catalog> {
        Arc::clone(&self.catalog)
    }

    pub(crate) fn item(&self, agent_id: &str) -> Option<&CatalogItem> {
        self.catalog.items.iter().find(|item| item.id == agent_id)
    }

    pub(crate) fn preview_token(
        &self,
        item: &CatalogItem,
        distribution_id: &str,
        action: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.catalog.catalog_version.as_bytes());
        hasher.update([0]);
        hasher.update(item.id.as_bytes());
        hasher.update([0]);
        hasher.update(item.version.as_bytes());
        hasher.update([0]);
        hasher.update(distribution_id.as_bytes());
        hasher.update([0]);
        hasher.update(action.as_bytes());
        hex_lower(&hasher.finalize())[..24].to_string()
    }
}

pub(crate) fn bundled_catalog() -> Result<Catalog, CatalogError> {
    parse_catalog(BUNDLED_CATALOG.as_bytes())
}

pub(crate) fn is_core_compatible(item: &CatalogItem) -> bool {
    let Ok(current) = semver::Version::parse(env!("CARGO_PKG_VERSION")) else {
        return false;
    };
    semver::VersionReq::parse(&format!(
        ">={}, <{}",
        item.core_compatibility.min, item.core_compatibility.max_exclusive
    ))
    .map(|requirement| requirement.matches(&current))
    .unwrap_or(false)
}

fn parse_catalog(bytes: &[u8]) -> Result<Catalog, CatalogError> {
    if bytes.len() > MAX_CATALOG_BYTES {
        return Err(CatalogError::TooLarge);
    }
    let catalog: Catalog = serde_json::from_slice(bytes)
        .map_err(|error| CatalogError::InvalidJson(error.to_string()))?;
    validate_catalog(&catalog)?;
    Ok(catalog)
}

fn validate_catalog(catalog: &Catalog) -> Result<(), CatalogError> {
    if catalog.schema != CATALOG_SCHEMA {
        return Err(CatalogError::Invalid(format!(
            "unsupported catalog schema: {}",
            catalog.schema
        )));
    }
    if catalog.catalog_version.trim().is_empty()
        || catalog.catalog_version.eq_ignore_ascii_case("latest")
    {
        return Err(CatalogError::Invalid(
            "catalog version must be a fixed non-empty value".to_string(),
        ));
    }
    DateTime::parse_from_rfc3339(&catalog.generated_at)
        .map_err(|_| CatalogError::Invalid("generatedAt must be RFC3339".to_string()))?;
    let mut item_ids = HashSet::new();
    for item in &catalog.items {
        if !item_ids.insert(item.id.as_str()) {
            return Err(CatalogError::Invalid(format!(
                "duplicate catalog item: {}",
                item.id
            )));
        }
        item.validate_basic().map_err(CatalogError::Invalid)?;
        if item.core_compatibility.min.trim().is_empty()
            || item.core_compatibility.max_exclusive.trim().is_empty()
        {
            return Err(CatalogError::Invalid(format!(
                "missing core compatibility for {}",
                item.id
            )));
        }
        if item.verification.evidence_id.is_none()
            && matches!(
                item.verification.status,
                super::types::VerificationStatus::Tested
            )
        {
            return Err(CatalogError::Invalid(format!(
                "tested item lacks evidence id: {}",
                item.id
            )));
        }
        validate_distribution_fields(item)?;
    }
    Ok(())
}

fn validate_distribution_fields(item: &CatalogItem) -> Result<(), CatalogError> {
    for distribution in &item.distributions {
        let has_secret_like_field = serde_json::to_value(distribution)
            .ok()
            .is_some_and(|value| contains_forbidden_catalog_field(&value));
        if has_secret_like_field {
            return Err(CatalogError::Invalid(format!(
                "distribution contains forbidden field: {}",
                distribution.id()
            )));
        }
        if matches!(distribution, Distribution::System { command_candidates, .. } if command_candidates.iter().any(|command| !super::types::is_safe_command_candidate(command)))
        {
            return Err(CatalogError::Invalid(format!(
                "system candidate is not an executable name: {}",
                distribution.id()
            )));
        }
    }
    Ok(())
}

fn contains_forbidden_catalog_field(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            matches!(key.as_str(), "env" | "hook" | "secret" | "token")
                || contains_forbidden_catalog_field(value)
        }),
        Value::Array(values) => values.iter().any(contains_forbidden_catalog_field),
        _ => false,
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalog_contains_all_initial_agents_without_execution_commands() {
        let catalog = bundled_catalog().expect("bundled catalog");
        let ids = catalog
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<HashSet<_>>();
        for id in ["opencode", "gemini", "claude", "codex", "pi", "qoder"] {
            assert!(ids.contains(id), "missing {id}");
        }
        let json = serde_json::to_string(&catalog).expect("catalog json");
        assert!(!json.contains("npx -y"));
        assert!(!json.contains("latest"));
    }

    #[test]
    fn invalid_cache_does_not_parse_as_catalog() {
        let error = CatalogService::from_bytes(br#"{"schema":"bad"}"#)
            .expect_err("invalid cache must fail");
        assert!(matches!(
            error,
            CatalogError::InvalidJson(_) | CatalogError::Invalid(_)
        ));
    }

    #[test]
    fn preview_token_changes_when_selected_distribution_changes() {
        let service = CatalogService::bundled().expect("bundled catalog");
        let item = service.item("opencode").expect("OpenCode item");
        let first = service.preview_token(item, item.distributions[0].id(), "install");
        let second = service.preview_token(
            item,
            &format!("{}-alternate", item.distributions[0].id()),
            "install",
        );
        assert_ne!(first, second);
        assert_eq!(first.len(), 24);
    }

    #[test]
    fn bundled_catalog_items_support_current_core_version() {
        let catalog = bundled_catalog().expect("bundled catalog");

        assert!(
            catalog.items.iter().all(is_core_compatible),
            "at least one bundled item does not support core {}",
            env!("CARGO_PKG_VERSION")
        );
    }
}
