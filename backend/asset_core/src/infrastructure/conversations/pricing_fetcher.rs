use crate::backend::domain::conversations::pricing::{
    ModelPriceEntry, PriceCatalog, DEFAULT_USD_TO_CNY_RATE,
};
use crate::backend::infrastructure::http_client::{get_with_redirects, shared_http_client};
use crate::backend::infrastructure::{InfraError, InfraResult};
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

pub const EXCHANGE_RATE_API_URL: &str = "https://open.er-api.com/v6/latest/USD";
pub const LITELLM_PRIMARY_URL: &str =
    "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";
pub const LITELLM_MIRROR_URL: &str =
    "https://cdn.jsdelivr.net/gh/BerriAI/litellm@main/model_prices_and_context_window.json";
pub const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

pub const FETCH_TIMEOUT_SECS: u64 = 4;
pub const RATE_FETCH_TIMEOUT_SECS: u64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PricingSourceChannel {
    LiteLlm,
    OpenRouter,
    Cache,
    Bundled,
}

impl PricingSourceChannel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LiteLlm => "litellm",
            Self::OpenRouter => "openrouter",
            Self::Cache => "cache",
            Self::Bundled => "bundled",
        }
    }
}

pub struct TieredFetchOutcome {
    pub channel: PricingSourceChannel,
    pub entries: Vec<ModelPriceEntry>,
    pub rate: f64,
    pub catalog_version: String,
}

/// Fetches the latest USD to CNY exchange rate from open API with fallback.
pub fn fetch_usd_to_cny_rate() -> Option<f64> {
    let client = shared_http_client().ok()?;
    let resp = get_with_redirects(
        &client,
        EXCHANGE_RATE_API_URL,
        HeaderMap::new(),
        Duration::from_secs(RATE_FETCH_TIMEOUT_SECS),
    )
    .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let json: Value = resp.json().ok()?;
    let cny = json.get("rates")?.get("CNY")?.as_f64()?;
    if cny > 0.0 {
        Some(cny)
    } else {
        None
    }
}

pub fn parse_litellm_json(raw_json: &str, rate: f64) -> InfraResult<Vec<ModelPriceEntry>> {
    let map: serde_json::Map<String, Value> = serde_json::from_str(raw_json)
        .map_err(|e| InfraError::external(format!("failed to parse litellm json: {e}")))?;

    let mut entries = Vec::with_capacity(map.len());

    for (model_key, val) in map {
        let obj = match val.as_object() {
            Some(o) => o,
            None => continue,
        };

        // Determine raw input/output costs per token
        let input_per_token = obj
            .get("input_cost_per_token")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let output_per_token = obj
            .get("output_cost_per_token")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let cache_read_per_token = obj
            .get("cache_read_input_token_cost")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let cache_write_per_token = obj
            .get("cache_creation_input_token_cost")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);

        // If all costs are zero, skip
        if input_per_token == 0.0 && output_per_token == 0.0 {
            continue;
        }

        let provider = obj
            .get("litellm_provider")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        let is_native_cny = provider.eq_ignore_ascii_case("deepseek")
            || provider.eq_ignore_ascii_case("qwen")
            || model_key.starts_with("deepseek")
            || model_key.starts_with("qwen");

        let multiplier = if is_native_cny {
            1_000_000.0
        } else {
            1_000_000.0 * rate
        };

        let raw_multiplier = 1_000_000.0;
        let raw_currency = if is_native_cny { "CNY" } else { "USD" };

        entries.push(ModelPriceEntry {
            provider,
            model_patterns: extract_patterns_from_remote_key(&model_key),
            currency: "CNY".to_string(),
            input_per_million: input_per_token * multiplier,
            cache_read_per_million: cache_read_per_token * multiplier,
            cache_write_per_million: cache_write_per_token * multiplier,
            output_per_million: output_per_token * multiplier,
            reasoning_per_million: output_per_token * multiplier,
            raw_currency: Some(raw_currency.to_string()),
            raw_input_per_million: Some(input_per_token * raw_multiplier),
            raw_output_per_million: Some(output_per_token * raw_multiplier),
            price_locked: false,
            usage_aliases: Vec::new(),
            source_channel: "litellm".to_string(),
            is_ignored: false,
        });
    }

    Ok(entries)
}

pub fn extract_patterns_from_remote_key(key: &str) -> Vec<String> {
    let k_lower = key.trim().to_ascii_lowercase();
    let mut patterns = Vec::new();
    if k_lower.is_empty() {
        return patterns;
    }

    let parts: Vec<&str> = k_lower.split('/').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        patterns.push(k_lower.clone());
        patterns.push(format!("{}*", k_lower));
        return patterns;
    }

    let last = parts[parts.len() - 1];
    let mut clean_last = last;
    if let Some(stripped) = clean_last.strip_prefix("fw-") {
        clean_last = stripped;
    } else if let Some(stripped) = clean_last.strip_prefix("@cf/") {
        clean_last = stripped;
    } else if let Some(stripped) = clean_last.strip_prefix("minimax.") {
        clean_last = stripped;
    }

    for sfx in [":batch", ":free", "-free", "_free"] {
        if let Some(stripped) = clean_last.strip_suffix(sfx) {
            clean_last = stripped;
            break;
        }
    }

    // Ensure model_patterns[0] is the canonical native base model_id
    if clean_last.len() >= 2 && clean_last != "*" {
        patterns.push(clean_last.to_string());
        if clean_last.len() > 2 {
            patterns.push(format!("{}*", clean_last));
        }
    }

    patterns.push(k_lower.clone());
    patterns.push(format!("{}*", k_lower));

    for item in [clean_last, last] {
        if item.len() > 2 && item != "*" {
            patterns.push(item.to_string());
            patterns.push(format!("{}*", item));
            if let Some(v) =
                crate::backend::domain::conversations::pricing::generate_hyphen_variant(item)
            {
                patterns.push(v.clone());
                patterns.push(format!("{}*", v));
            }
        }
    }

    if parts.len() >= 2 && clean_last.len() > 2 {
        let vendor = parts[parts.len() - 2];
        let vendor_and_model = format!("{}/{}", vendor, clean_last);
        patterns.push(vendor_and_model.clone());
        patterns.push(format!("{}*", vendor_and_model));
    }

    let mut seen = std::collections::HashSet::new();
    patterns.retain(|p| p != "*" && !p.is_empty() && seen.insert(p.clone()));
    patterns
}

pub fn parse_openrouter_json(raw_json: &str, rate: f64) -> InfraResult<Vec<ModelPriceEntry>> {
    let root: Value = serde_json::from_str(raw_json)
        .map_err(|e| InfraError::external(format!("failed to parse openrouter json: {e}")))?;

    let data = root
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| InfraError::external("missing 'data' array in openrouter response"))?;

    let mut entries = Vec::with_capacity(data.len());

    for item in data {
        let model_id = match item.get("id").and_then(Value::as_str) {
            Some(id) if !id.trim().is_empty() => id.trim(),
            _ => continue,
        };

        let pricing = match item.get("pricing") {
            Some(p) => p,
            None => continue,
        };

        let parse_cost = |field: &str| -> f64 {
            pricing
                .get(field)
                .and_then(|v| {
                    if let Some(num) = v.as_f64() {
                        Some(num)
                    } else if let Some(s) = v.as_str() {
                        s.parse::<f64>().ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(0.0)
        };

        let prompt_cost = parse_cost("prompt");
        let completion_cost = parse_cost("completion");

        if prompt_cost == 0.0 && completion_cost == 0.0 {
            continue;
        }

        // OpenRouter prices are per-token in USD
        let input_cny_m = prompt_cost * 1_000_000.0 * rate;
        let output_cny_m = completion_cost * 1_000_000.0 * rate;

        let provider = model_id.split('/').next().unwrap_or("").trim().to_string();

        entries.push(ModelPriceEntry {
            provider,
            model_patterns: extract_patterns_from_remote_key(model_id),
            currency: "CNY".to_string(),
            input_per_million: input_cny_m,
            cache_read_per_million: input_cny_m * 0.25,
            cache_write_per_million: input_cny_m,
            output_per_million: output_cny_m,
            reasoning_per_million: output_cny_m,
            raw_currency: Some("USD".to_string()),
            raw_input_per_million: Some(prompt_cost * 1_000_000.0),
            raw_output_per_million: Some(completion_cost * 1_000_000.0),
            price_locked: false,
            usage_aliases: Vec::new(),
            source_channel: "openrouter".to_string(),
            is_ignored: false,
        });
    }

    Ok(entries)
}

/// Filters and maps price entries on-demand for specific target models.
/// For each target model, it attempts to match against raw_entries first;
/// if not found, it falls back to bundled_catalog.
pub fn filter_entries_for_target_models(
    raw_entries: &[ModelPriceEntry],
    target_models: &[String],
    bundled_catalog: &PriceCatalog,
) -> Vec<ModelPriceEntry> {
    let raw_catalog = PriceCatalog {
        catalog_version: "temp".to_string(),
        updated_at: String::new(),
        usd_to_cny_rate: 1.0,
        models: raw_entries.to_vec(),
    };
    let mut selected = Vec::new();
    let mut seen_keys = std::collections::HashSet::new();

    for target in target_models {
        let trimmed = target.trim();
        if trimmed.is_empty() {
            continue;
        }

        let raw_match = raw_catalog
            .find_model_with_score("", trimmed)
            .and_then(|(_, score)| raw_catalog.resolve_model("", trimmed).map(|r| (r, score)));
        let bundled_match =
            bundled_catalog
                .find_model_with_score("", trimmed)
                .and_then(|(_, score)| {
                    bundled_catalog
                        .resolve_model("", trimmed)
                        .map(|r| (r, score))
                });

        let chosen = match (raw_match, bundled_match) {
            (Some((raw_res, raw_score)), Some((bundled_res, bundled_score))) => {
                let raw_has_price =
                    raw_res.entry.input_per_million > 0.0 || raw_res.entry.output_per_million > 0.0;
                let bundled_has_price = bundled_res.entry.input_per_million > 0.0
                    || bundled_res.entry.output_per_million > 0.0;

                if raw_has_price && bundled_has_price {
                    if bundled_score > raw_score {
                        bundled_res
                    } else {
                        raw_res
                    }
                } else if raw_has_price {
                    raw_res
                } else if bundled_has_price {
                    bundled_res
                } else {
                    continue;
                }
            }
            (Some((raw_res, _)), None) => {
                if raw_res.entry.input_per_million > 0.0 || raw_res.entry.output_per_million > 0.0 {
                    raw_res
                } else {
                    continue;
                }
            }
            (None, Some((bundled_res, _))) => {
                if bundled_res.entry.input_per_million > 0.0
                    || bundled_res.entry.output_per_million > 0.0
                {
                    bundled_res
                } else {
                    continue;
                }
            }
            (None, None) => continue,
        };

        let first_pattern = chosen
            .entry
            .model_patterns
            .first()
            .map(String::as_str)
            .unwrap_or("");
        let primary_is_exact = !first_pattern.is_empty() && !first_pattern.contains('*');
        let canonical_base_id = if primary_is_exact {
            first_pattern.to_string()
        } else if !chosen.matched_model_id.is_empty() && !chosen.matched_model_id.contains('*') {
            chosen.matched_model_id.clone()
        } else {
            first_pattern.trim_matches('*').to_string()
        };

        if canonical_base_id.is_empty() {
            continue;
        }

        let key = format!(
            "{}:{}",
            chosen.entry.provider.to_ascii_lowercase(),
            canonical_base_id.to_ascii_lowercase()
        );
        if seen_keys.insert(key) {
            let mut customized = chosen.entry.clone();
            if !primary_is_exact {
                customized
                    .model_patterns
                    .retain(|p| !p.eq_ignore_ascii_case(&canonical_base_id));
                customized.model_patterns.insert(0, canonical_base_id);
            }
            selected.push(customized);
        }
    }
    selected
}

/// Executes the four-tier fallback chain:
/// 1. Primary remote: LiteLLM
/// 2. Secondary remote: OpenRouter
/// 3. Local SQLite cache (passed in)
/// 4. Bundled seed catalog (built-in)
///
/// If `target_models` is provided and non-empty, only the pricing entries matching
/// the target models will be extracted and returned, minimizing storage and redundant data.
pub fn fetch_tiered_pricing_catalog(
    rate: f64,
    cached_entries: Vec<ModelPriceEntry>,
    target_models: Option<&[String]>,
) -> TieredFetchOutcome {
    let bundled = PriceCatalog::bundled_with_rate(rate);
    let now_date = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let mut outcome = 'fetch: {
        // 1. Try LiteLLM primary and mirror
        if let Ok(client) = shared_http_client() {
            for url in [LITELLM_PRIMARY_URL, LITELLM_MIRROR_URL] {
                if let Ok(resp) = get_with_redirects(
                    &client,
                    url,
                    HeaderMap::new(),
                    Duration::from_secs(FETCH_TIMEOUT_SECS),
                ) {
                    if resp.status().is_success() {
                        if let Ok(text) = resp.text() {
                            if let Ok(entries) = parse_litellm_json(&text, rate) {
                                if entries.len() >= 20 {
                                    break 'fetch TieredFetchOutcome {
                                        channel: PricingSourceChannel::LiteLlm,
                                        entries,
                                        rate,
                                        catalog_version: format!("litellm-{now_date}"),
                                    };
                                }
                            }
                        }
                    }
                }
            }

            // 2. Try OpenRouter Models API
            if let Ok(resp) = get_with_redirects(
                &client,
                OPENROUTER_MODELS_URL,
                HeaderMap::new(),
                Duration::from_secs(FETCH_TIMEOUT_SECS),
            ) {
                if resp.status().is_success() {
                    if let Ok(text) = resp.text() {
                        if let Ok(entries) = parse_openrouter_json(&text, rate) {
                            if entries.len() >= 20 {
                                break 'fetch TieredFetchOutcome {
                                    channel: PricingSourceChannel::OpenRouter,
                                    entries,
                                    rate,
                                    catalog_version: format!("openrouter-{now_date}"),
                                };
                            }
                        }
                    }
                }
            }
        }

        // 3. Try Local Cache
        if !cached_entries.is_empty() {
            break 'fetch TieredFetchOutcome {
                channel: PricingSourceChannel::Cache,
                entries: cached_entries,
                rate,
                catalog_version: format!("cache-{now_date}"),
            };
        }

        // 4. Bundled Fallback
        TieredFetchOutcome {
            channel: PricingSourceChannel::Bundled,
            entries: bundled.models.clone(),
            rate,
            catalog_version: bundled.catalog_version.clone(),
        }
    };

    // If target models are specified, filter and extract on-demand
    if let Some(targets) = target_models {
        if !targets.is_empty() {
            outcome.entries = filter_entries_for_target_models(&outcome.entries, targets, &bundled);
        }
    }

    outcome
}

#[cfg(test)]
#[path = "pricing_fetcher_tests.rs"]
mod tests;
