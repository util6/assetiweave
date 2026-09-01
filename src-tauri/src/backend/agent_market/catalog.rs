use std::{collections::HashSet, sync::Arc};

use chrono::{DateTime, NaiveDate};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::types::{Catalog, CatalogItem, Distribution};

const CATALOG_SCHEMA: &str = "assetiweave.agent-market/v1";
const MAX_CATALOG_BYTES: usize = 5 * 1024 * 1024;
const BUNDLED_CATALOG: &str =
    include_str!("../../../../builtin-assets/agent-market/catalog-v1.json");

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct CatalogRevision {
    pub(crate) date: NaiveDate,
    pub(crate) sequence: u32,
}

impl CatalogRevision {
    pub(crate) fn parse(value: &str) -> Result<Self, CatalogError> {
        let mut parts = value.split('.');
        let year = parts
            .next()
            .and_then(|part| part.parse::<i32>().ok())
            .ok_or_else(|| CatalogError::Invalid("catalogVersion year is invalid".to_string()))?;
        let month = parts
            .next()
            .and_then(|part| part.parse::<u32>().ok())
            .ok_or_else(|| CatalogError::Invalid("catalogVersion month is invalid".to_string()))?;
        let day = parts
            .next()
            .and_then(|part| part.parse::<u32>().ok())
            .ok_or_else(|| CatalogError::Invalid("catalogVersion day is invalid".to_string()))?;
        let sequence = parts
            .next()
            .and_then(|part| part.parse::<u32>().ok())
            .ok_or_else(|| {
                CatalogError::Invalid("catalogVersion sequence is invalid".to_string())
            })?;
        if parts.next().is_some() {
            return Err(CatalogError::Invalid(
                "catalogVersion must use YYYY.MM.DD.N".to_string(),
            ));
        }
        let date = NaiveDate::from_ymd_opt(year, month, day)
            .ok_or_else(|| CatalogError::Invalid("catalogVersion date is invalid".to_string()))?;
        Ok(Self { date, sequence })
    }
}

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

    pub(crate) fn revision(&self) -> Result<CatalogRevision, CatalogError> {
        CatalogRevision::parse(&self.catalog.catalog_version)
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
        hasher.update(item.id.as_bytes());
        hasher.update([0]);
        if let Some(distribution) = item
            .distributions
            .iter()
            .find(|distribution| distribution.id() == distribution_id)
        {
            hasher.update(
                serde_json::to_vec(distribution)
                    .expect("validated catalog distribution must serialize"),
            );
        } else {
            hasher.update(distribution_id.as_bytes());
        }
        hasher.update([0]);
        hasher.update(action.as_bytes());
        hex_lower(&hasher.finalize())[..24].to_string()
    }
}

#[cfg(test)]
pub(crate) fn bundled_catalog() -> Result<Catalog, CatalogError> {
    parse_catalog(BUNDLED_CATALOG.as_bytes())
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
    CatalogRevision::parse(&catalog.catalog_version)?;
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
    use crate::backend::agent_market::types::AgentMarketProtocol;

    #[test]
    fn bundled_catalog_contains_all_initial_agents_without_execution_commands() {
        let catalog = bundled_catalog().expect("bundled catalog");
        let ids = catalog
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<HashSet<_>>();
        for id in [
            "opencode",
            "gemini",
            "antigravity",
            "claude",
            "codex",
            "pi",
            "qoder",
        ] {
            assert!(ids.contains(id), "missing {id}");
        }
        let antigravity = catalog
            .items
            .iter()
            .find(|item| item.id == "antigravity")
            .expect("Antigravity catalog item");
        assert_eq!(antigravity.protocol, AgentMarketProtocol::Native);
        assert!(!antigravity.capabilities.resume);
        assert!(!antigravity.capabilities.history_replay);
        assert!(!antigravity.capabilities.live_events);
        assert!(matches!(
            antigravity.distributions.as_slice(),
            [Distribution::System { command_candidates, .. }]
                if command_candidates == &["agy".to_string()]
        ));
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
    fn preview_token_is_bound_to_exact_lifecycle_action() {
        let service = CatalogService::bundled().expect("bundled catalog");
        let item = service.item("opencode").expect("OpenCode item");
        let install = service.preview_token(item, item.distributions[0].id(), "install");
        let update = service.preview_token(item, item.distributions[0].id(), "update");
        let reinstall = service.preview_token(item, item.distributions[0].id(), "reinstall");

        assert_ne!(install, update);
        assert_ne!(update, reinstall);
        assert_ne!(install, reinstall);
    }

    #[test]
    fn preview_token_ignores_observational_catalog_and_agent_versions() {
        let catalog = bundled_catalog().expect("bundled catalog");
        let service = CatalogService::from_catalog(catalog.clone());
        let item = service.item("opencode").expect("OpenCode item");
        let distribution_id = item.distributions[0].id().to_string();
        let token = service.preview_token(item, &distribution_id, "install");

        let mut changed = catalog;
        changed.catalog_version = "2099.01.01.1".to_string();
        changed.items[0].version = "999.0.0".to_string();
        let changed_service = CatalogService::from_catalog(changed);
        let changed_item = changed_service.item("opencode").expect("OpenCode item");

        assert_eq!(
            token,
            changed_service.preview_token(changed_item, &distribution_id, "install")
        );
    }

    #[test]
    fn bundled_opencode_keeps_cli_model_discovery_in_its_runtime_definition() {
        let service = CatalogService::bundled().expect("bundled catalog");
        let item = service.item("opencode").expect("OpenCode item");

        assert!(item.capabilities.model_discovery);
        let model_discovery_args = match &item.distributions[0] {
            Distribution::Binary {
                model_discovery_args,
                ..
            } => model_discovery_args,
            other => panic!("unexpected OpenCode distribution: {other:?}"),
        };
        assert_eq!(
            model_discovery_args.as_deref(),
            Some(["models".to_string()].as_slice())
        );
    }

    #[test]
    fn bundled_opencode_maps_session_delete_args_into_its_runtime_definition() {
        let service = CatalogService::bundled().expect("bundled catalog");
        let item = service.item("opencode").expect("OpenCode item");

        let session_cleanup_args = match &item.distributions[0] {
            Distribution::Binary {
                session_cleanup_args,
                ..
            } => session_cleanup_args,
            other => panic!("unexpected OpenCode distribution: {other:?}"),
        };
        assert_eq!(
            session_cleanup_args.as_deref(),
            Some(
                ["session", "delete", "{session_id}"]
                    .map(str::to_string)
                    .as_slice()
            )
        );
    }
}
