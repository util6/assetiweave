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

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum CatalogError {
    #[error("Agent catalog exceeds the 5 MiB limit")]
    TooLarge,
    #[error("{0}")]
    InvalidJson(String),
    #[error("{0}")]
    Invalid(String),
}

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
        item.validate_basic()
            .map_err(|e| CatalogError::Invalid(e.to_string()))?;
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
#[path = "catalog_tests.rs"]
mod tests;
