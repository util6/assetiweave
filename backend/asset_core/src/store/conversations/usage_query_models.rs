use crate::backend::domain::conversations::{pricing::PriceCatalog, UsageModelBreakdownDto};
use std::collections::HashMap;

use super::LoadedUsageRow;

#[derive(Clone)]
struct CachedModelResolution {
    bucket_key: String,
    matched_model_id: Option<String>,
    resolved_provider: Option<String>,
    unit_price_per_million: Option<f64>,
}

struct ModelAgg {
    matched_model_id: Option<String>,
    resolved_provider: Option<String>,
    unit_price_per_million: Option<f64>,
    provider_counts: HashMap<String, i64>,
    raw_model_counts: HashMap<String, i64>,
    requests: i64,
    total_tokens: i64,
    input_tokens: i64,
    output_tokens: i64,
    reasoning_tokens: i64,
    cache_tokens: i64,
    cost: f64,
    currency: String,
    cost_basis: String,
}

/// Aggregates usage events into base-model breakdown buckets while preserving
/// per-bucket raw model counts and request-frequency ordering.
pub(super) struct ModelBreakdownAccumulator {
    resolution_cache: HashMap<(String, String), CachedModelResolution>,
    buckets: HashMap<String, ModelAgg>,
}

impl ModelBreakdownAccumulator {
    pub(super) fn new() -> Self {
        Self {
            resolution_cache: HashMap::new(),
            buckets: HashMap::new(),
        }
    }

    pub(super) fn record_event(
        &mut self,
        catalog: &PriceCatalog,
        ev: &LoadedUsageRow,
        ev_cny_cost: f64,
    ) {
        let resolution = self.resolve_cached(catalog, &ev.provider, &ev.model);
        let entry = self
            .buckets
            .entry(resolution.bucket_key.clone())
            .or_insert_with(|| ModelAgg {
                matched_model_id: resolution.matched_model_id.clone(),
                resolved_provider: resolution.resolved_provider.clone(),
                unit_price_per_million: resolution.unit_price_per_million,
                provider_counts: HashMap::new(),
                raw_model_counts: HashMap::new(),
                requests: 0,
                total_tokens: 0,
                input_tokens: 0,
                output_tokens: 0,
                reasoning_tokens: 0,
                cache_tokens: 0,
                cost: 0.0,
                currency: "CNY".to_string(),
                cost_basis: ev.cost_basis.clone(),
            });

        if entry.resolved_provider.is_none() && resolution.resolved_provider.is_some() {
            entry.resolved_provider = resolution.resolved_provider;
        }
        if entry.unit_price_per_million.is_none() && resolution.unit_price_per_million.is_some() {
            entry.unit_price_per_million = resolution.unit_price_per_million;
        }

        let trimmed_provider = ev.provider.trim();
        let provider_key = if trimmed_provider.is_empty() {
            ev.provider.clone()
        } else {
            trimmed_provider.to_string()
        };
        *entry.provider_counts.entry(provider_key).or_insert(0) += 1;
        *entry.raw_model_counts.entry(ev.model.clone()).or_insert(0) += 1;

        entry.requests += 1;
        entry.total_tokens += ev.total_tokens;
        entry.input_tokens += ev.input_tokens + ev.cache_read_tokens + ev.cache_write_tokens;
        entry.output_tokens += ev.reasoning_tokens + ev.output_tokens;
        entry.reasoning_tokens += ev.reasoning_tokens;
        entry.cache_tokens += ev.cache_read_tokens + ev.cache_write_tokens;
        if ev.cost_basis != "none" {
            entry.cost += ev_cny_cost;
            entry.cost_basis = ev.cost_basis.clone();
        }
    }

    fn resolve_cached(
        &mut self,
        catalog: &PriceCatalog,
        ev_provider: &str,
        ev_model: &str,
    ) -> CachedModelResolution {
        let cache_key = (ev_provider.to_string(), ev_model.to_string());
        if let Some(cached) = self.resolution_cache.get(&cache_key) {
            return cached.clone();
        }

        let resolved_val = match catalog.resolve_model(ev_provider, ev_model) {
            Some(resolved) => {
                let resolved_provider = {
                    let trimmed = resolved.entry.provider.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                };
                if resolved.is_ignored {
                    CachedModelResolution {
                        bucket_key: ev_model.to_string(),
                        matched_model_id: None,
                        resolved_provider,
                        unit_price_per_million: Some(0.0),
                    }
                } else {
                    let unit_price = (resolved.entry.input_per_million * 100.0).round() / 100.0;
                    if resolved.matched_model_id.is_empty() {
                        CachedModelResolution {
                            bucket_key: ev_model.to_string(),
                            matched_model_id: None,
                            resolved_provider,
                            unit_price_per_million: Some(unit_price),
                        }
                    } else {
                        CachedModelResolution {
                            bucket_key: resolved.matched_model_id.clone(),
                            matched_model_id: Some(resolved.matched_model_id),
                            resolved_provider,
                            unit_price_per_million: Some(unit_price),
                        }
                    }
                }
            }
            None => CachedModelResolution {
                bucket_key: ev_model.to_string(),
                matched_model_id: None,
                resolved_provider: None,
                unit_price_per_million: None,
            },
        };

        self.resolution_cache
            .insert(cache_key, resolved_val.clone());
        resolved_val
    }

    pub(super) fn into_sorted_dtos(self) -> Vec<UsageModelBreakdownDto> {
        let mut models = self
            .buckets
            .into_iter()
            .map(|(model, agg)| {
                let provider = agg.resolved_provider.unwrap_or_else(|| {
                    let mut providers: Vec<(String, i64)> =
                        agg.provider_counts.into_iter().collect();
                    providers.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                    providers
                        .into_iter()
                        .next()
                        .map(|(p, _)| p)
                        .unwrap_or_default()
                });

                let mut raw_models_with_counts: Vec<(String, i64)> =
                    agg.raw_model_counts.into_iter().collect();
                raw_models_with_counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                let raw_models = raw_models_with_counts
                    .into_iter()
                    .map(|(raw, _)| raw)
                    .collect();

                UsageModelBreakdownDto {
                    model,
                    provider,
                    requests_count: agg.requests,
                    total_tokens: agg.total_tokens,
                    input_tokens: agg.input_tokens,
                    output_tokens: agg.output_tokens,
                    reasoning_tokens: agg.reasoning_tokens,
                    cache_tokens: agg.cache_tokens,
                    cost: if agg.cost_basis == "none" {
                        None
                    } else {
                        Some((agg.cost * 1000.0).round() / 1000.0)
                    },
                    currency: agg.currency,
                    cost_basis: agg.cost_basis,
                    unit_price_per_million: agg.unit_price_per_million,
                    raw_models,
                    matched_model_id: agg.matched_model_id,
                }
            })
            .collect::<Vec<_>>();
        models.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens));
        models
    }
}
