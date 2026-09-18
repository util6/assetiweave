use serde::{Deserialize, Serialize};

pub(crate) const BUNDLED_MODEL_PRICES_JSON: &str =
    include_str!("../../../../builtin-assets/pricing/model_prices.json");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct ModelPriceEntry {
    pub(crate) provider: String,
    pub(crate) model_patterns: Vec<String>,
    pub(crate) currency: String,
    pub(crate) input_per_million: f64,
    pub(crate) cache_read_per_million: f64,
    pub(crate) cache_write_per_million: f64,
    pub(crate) output_per_million: f64,
    pub(crate) reasoning_per_million: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct PriceCatalog {
    pub(crate) catalog_version: String,
    pub(crate) updated_at: String,
    pub(crate) models: Vec<ModelPriceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct CostCalculation {
    pub(crate) catalog_version: String,
    pub(crate) estimated_cost: f64,
    pub(crate) currency: String,
}

impl PriceCatalog {
    pub(crate) fn bundled() -> Self {
        serde_json::from_str(BUNDLED_MODEL_PRICES_JSON)
            .expect("bundled model_prices.json must be valid")
    }

    pub(crate) fn normalize_model_name(raw: &str) -> String {
        let trimmed = raw.trim().to_ascii_lowercase();
        if let Some(stripped) = trimmed.strip_prefix("models/") {
            stripped.to_string()
        } else {
            trimmed
        }
    }

    pub(crate) fn matches_pattern(pattern: &str, candidate: &str) -> bool {
        let pattern_lower = pattern.trim().to_ascii_lowercase();
        let candidate_lower = candidate.trim().to_ascii_lowercase();
        if pattern_lower == "*" {
            return true;
        }
        if let Some(prefix) = pattern_lower.strip_suffix('*') {
            candidate_lower.starts_with(prefix)
        } else {
            candidate_lower == pattern_lower
        }
    }

    pub(crate) fn find_model(&self, provider: &str, model: &str) -> Option<&ModelPriceEntry> {
        let norm_model = Self::normalize_model_name(model);
        let norm_provider = provider.trim().to_ascii_lowercase();

        // 1. Try matching both provider and model pattern
        if let Some(entry) = self.models.iter().find(|entry| {
            let provider_match = entry.provider.eq_ignore_ascii_case(&norm_provider)
                || norm_provider.is_empty()
                || entry.provider.is_empty();
            if !provider_match {
                return false;
            }
            entry
                .model_patterns
                .iter()
                .any(|pattern| Self::matches_pattern(pattern, &norm_model))
        }) {
            return Some(entry);
        }

        // 2. Fallback: match by model pattern only (in case provider was generic or unknown)
        self.models.iter().find(|entry| {
            entry
                .model_patterns
                .iter()
                .any(|pattern| Self::matches_pattern(pattern, &norm_model))
        })
    }

    pub(crate) fn calculate_cost(
        &self,
        provider: &str,
        model: &str,
        input_tokens: i64,
        cache_read_tokens: i64,
        cache_write_tokens: i64,
        reasoning_tokens: i64,
        output_tokens: i64,
    ) -> Option<CostCalculation> {
        let entry = self.find_model(provider, model)?;
        let input_cost = (input_tokens.max(0) as f64 / 1_000_000.0) * entry.input_per_million;
        let cache_read_cost =
            (cache_read_tokens.max(0) as f64 / 1_000_000.0) * entry.cache_read_per_million;
        let cache_write_cost =
            (cache_write_tokens.max(0) as f64 / 1_000_000.0) * entry.cache_write_per_million;
        let reasoning_cost =
            (reasoning_tokens.max(0) as f64 / 1_000_000.0) * entry.reasoning_per_million;
        let output_cost = (output_tokens.max(0) as f64 / 1_000_000.0) * entry.output_per_million;

        let total_cost =
            input_cost + cache_read_cost + cache_write_cost + reasoning_cost + output_cost;

        Some(CostCalculation {
            catalog_version: self.catalog_version.clone(),
            estimated_cost: (total_cost * 1_000_000.0).round() / 1_000_000.0,
            currency: entry.currency.clone(),
        })
    }
}

#[cfg(test)]
#[path = "pricing_tests.rs"]
mod tests;
