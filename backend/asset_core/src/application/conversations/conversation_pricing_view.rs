use crate::backend::domain::conversations::pricing::{
    base_model_id_from_catalog_key, MatchSource, ModelPriceEntry, PriceCatalog,
    IGNORE_MODEL_ID_PREFIX, OVERRIDE_MODEL_ID_PREFIX,
};
use crate::backend::domain::conversations::usage::ModelPriceEntryDto;
use crate::backend::store::conversations::usage_repo::DistinctUsageModelInfo;
use std::collections::HashSet;

pub(super) struct ActivePricingContext {
    pub(super) rate: f64,
    pub(super) channel: String,
    pub(super) version: String,
    pub(super) last_updated: String,
    pub(super) cached_models: Vec<ModelPriceEntry>,
    pub(super) catalog: PriceCatalog,
}

pub(super) fn round_cny(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn is_override_or_ignored_entry(entry: &ModelPriceEntry) -> bool {
    entry.is_ignored
        || entry.source_channel == "user_override"
        || entry.model_patterns.first().is_some_and(|p| {
            let trimmed = p.trim();
            trimmed.starts_with(OVERRIDE_MODEL_ID_PREFIX)
                || trimmed.starts_with(IGNORE_MODEL_ID_PREFIX)
        })
}

pub(super) fn canonical_base_model_id(entry: &ModelPriceEntry) -> Option<String> {
    let first = entry.model_patterns.first()?.trim();
    if first.is_empty() || first.starts_with(IGNORE_MODEL_ID_PREFIX) {
        return None;
    }
    let cleaned = base_model_id_from_catalog_key(first)
        .trim_matches('*')
        .trim_start_matches('-')
        .trim()
        .to_string();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

pub(super) fn build_reference_catalog_pairs(
    ctx: &ActivePricingContext,
) -> Vec<(String, ModelPriceEntry)> {
    let mut seen_ids = HashSet::new();
    let mut entries = Vec::new();

    for entry in &ctx.cached_models {
        if is_override_or_ignored_entry(entry) {
            continue;
        }
        if entry.input_per_million <= 0.0 && entry.output_per_million <= 0.0 {
            continue;
        }
        let Some(base_id) = canonical_base_model_id(entry) else {
            continue;
        };
        if seen_ids.insert(base_id.to_ascii_lowercase()) {
            let mut normalized = entry.clone();
            normalized.model_patterns = vec![base_id.clone()];
            normalized.price_locked = false;
            normalized.is_ignored = false;
            entries.push((base_id, normalized));
        }
    }

    let bundled = PriceCatalog::bundled_with_rate(ctx.rate);
    for entry in bundled.models {
        let Some(base_id) = canonical_base_model_id(&entry) else {
            continue;
        };
        if seen_ids.insert(base_id.to_ascii_lowercase()) {
            let mut normalized = entry;
            if normalized
                .model_patterns
                .first()
                .is_none_or(|p| p.trim() != base_id)
            {
                normalized
                    .model_patterns
                    .retain(|p| !p.trim().eq_ignore_ascii_case(&base_id));
                normalized.model_patterns.insert(0, base_id.clone());
            }
            normalized.price_locked = false;
            normalized.is_ignored = false;
            entries.push((base_id, normalized));
        }
    }

    entries.sort_by(|(a_id, a_entry), (b_id, b_entry)| {
        a_entry
            .provider
            .to_ascii_lowercase()
            .cmp(&b_entry.provider.to_ascii_lowercase())
            .then_with(|| a_entry.provider.cmp(&b_entry.provider))
            .then_with(|| a_id.to_ascii_lowercase().cmp(&b_id.to_ascii_lowercase()))
            .then_with(|| a_id.cmp(b_id))
    });

    entries
}

pub(super) fn find_reference_target_entry(
    ctx: &ActivePricingContext,
    target_model_id: &str,
) -> Option<ModelPriceEntry> {
    let target_id = base_model_id_from_catalog_key(target_model_id).trim();
    if target_id.is_empty() {
        return None;
    }

    let pairs = build_reference_catalog_pairs(ctx);
    if let Some((_, entry)) = pairs
        .iter()
        .find(|(base_id, _)| base_id.eq_ignore_ascii_case(target_id))
    {
        return Some(entry.clone());
    }

    if let Some((_, entry)) = pairs.iter().find(|(_, entry)| {
        entry.model_patterns.iter().any(|p| {
            base_model_id_from_catalog_key(p)
                .trim_matches('*')
                .eq_ignore_ascii_case(target_id)
        })
    }) {
        let mut cloned = entry.clone();
        cloned.model_patterns = vec![target_id.to_string()];
        return Some(cloned);
    }

    ctx.catalog
        .resolve_model("", target_id)
        .filter(|resolved| !resolved.is_ignored)
        .map(|resolved| {
            let mut cloned = resolved.entry.clone();
            cloned.model_patterns = vec![target_id.to_string()];
            cloned
        })
}

pub(super) fn build_reference_catalog_dtos(ctx: &ActivePricingContext) -> Vec<ModelPriceEntryDto> {
    build_reference_catalog_pairs(ctx)
        .into_iter()
        .map(|(base_id, m)| ModelPriceEntryDto {
            provider: m.provider,
            model_patterns: m.model_patterns,
            currency: if m.currency.trim().is_empty() {
                "CNY".to_string()
            } else {
                m.currency
            },
            input_per_million: round_cny(m.input_per_million),
            cache_read_per_million: round_cny(m.cache_read_per_million),
            cache_write_per_million: round_cny(m.cache_write_per_million),
            output_per_million: round_cny(m.output_per_million),
            reasoning_per_million: round_cny(m.reasoning_per_million),
            raw_currency: m.raw_currency,
            raw_input_per_million: m.raw_input_per_million,
            raw_output_per_million: m.raw_output_per_million,
            raw_model: base_id.clone(),
            matched_model_id: Some(base_id),
            match_source: "reference".to_string(),
            price_locked: false,
            is_ignored: false,
            is_manual_rebind: false,
        })
        .collect()
}

pub(super) fn build_local_pricing_entries(
    ctx: &ActivePricingContext,
    distinct_local: Vec<DistinctUsageModelInfo>,
) -> Vec<ModelPriceEntryDto> {
    let mut local_entries = Vec::new();
    let mut seen_local = HashSet::new();
    let mut covered_base_ids = HashSet::new();

    for item in distinct_local {
        let key = format!(
            "{}::{}",
            item.provider.trim().to_ascii_lowercase(),
            item.model.trim().to_ascii_lowercase()
        );
        if !seen_local.insert(key) {
            continue;
        }

        match ctx.catalog.resolve_model(&item.provider, &item.model) {
            Some(resolved)
                if resolved.match_source == MatchSource::Ignored || resolved.is_ignored =>
            {
                local_entries.push(ModelPriceEntryDto {
                    provider: item.provider,
                    model_patterns: vec![item.model.clone()],
                    currency: "CNY".to_string(),
                    input_per_million: 0.0,
                    cache_read_per_million: 0.0,
                    cache_write_per_million: 0.0,
                    output_per_million: 0.0,
                    reasoning_per_million: 0.0,
                    raw_currency: Some("CNY".to_string()),
                    raw_input_per_million: Some(0.0),
                    raw_output_per_million: Some(0.0),
                    raw_model: item.model,
                    matched_model_id: None,
                    match_source: MatchSource::Ignored.as_str().to_string(),
                    price_locked: true,
                    is_ignored: true,
                    is_manual_rebind: false,
                });
            }
            Some(resolved) => {
                let entry = resolved.entry;
                if !resolved.matched_model_id.is_empty() {
                    covered_base_ids.insert(resolved.matched_model_id.to_ascii_lowercase());
                }
                let matched_id = if resolved.matched_model_id.is_empty() {
                    None
                } else {
                    Some(resolved.matched_model_id.clone())
                };
                local_entries.push(ModelPriceEntryDto {
                    provider: item.provider,
                    model_patterns: vec![item.model.clone()],
                    currency: "CNY".to_string(),
                    input_per_million: round_cny(entry.input_per_million),
                    cache_read_per_million: round_cny(entry.cache_read_per_million),
                    cache_write_per_million: round_cny(entry.cache_write_per_million),
                    output_per_million: round_cny(entry.output_per_million),
                    reasoning_per_million: round_cny(entry.reasoning_per_million),
                    raw_currency: entry.raw_currency.clone(),
                    raw_input_per_million: entry.raw_input_per_million,
                    raw_output_per_million: entry.raw_output_per_million,
                    raw_model: item.model,
                    matched_model_id: matched_id,
                    match_source: resolved.match_source.as_str().to_string(),
                    price_locked: resolved.price_locked,
                    is_ignored: false,
                    is_manual_rebind: resolved.match_source == MatchSource::ManualRebind,
                });
            }
            None => {
                local_entries.push(ModelPriceEntryDto {
                    provider: item.provider,
                    model_patterns: vec![item.model.clone()],
                    currency: "CNY".to_string(),
                    input_per_million: 0.0,
                    cache_read_per_million: 0.0,
                    cache_write_per_million: 0.0,
                    output_per_million: 0.0,
                    reasoning_per_million: 0.0,
                    raw_currency: Some("CNY".to_string()),
                    raw_input_per_million: Some(0.0),
                    raw_output_per_million: Some(0.0),
                    raw_model: item.model,
                    matched_model_id: None,
                    match_source: "unmatched".to_string(),
                    price_locked: false,
                    is_ignored: false,
                    is_manual_rebind: false,
                });
            }
        }
    }

    for cached in &ctx.cached_models {
        if !cached.price_locked || cached.is_ignored {
            continue;
        }
        let Some(base_id) = canonical_base_model_id(cached) else {
            continue;
        };
        if covered_base_ids.insert(base_id.to_ascii_lowercase()) {
            local_entries.push(ModelPriceEntryDto {
                provider: cached.provider.clone(),
                model_patterns: vec![base_id.clone()],
                currency: if cached.currency.trim().is_empty() {
                    "CNY".to_string()
                } else {
                    cached.currency.clone()
                },
                input_per_million: round_cny(cached.input_per_million),
                cache_read_per_million: round_cny(cached.cache_read_per_million),
                cache_write_per_million: round_cny(cached.cache_write_per_million),
                output_per_million: round_cny(cached.output_per_million),
                reasoning_per_million: round_cny(cached.reasoning_per_million),
                raw_currency: cached.raw_currency.clone(),
                raw_input_per_million: cached.raw_input_per_million,
                raw_output_per_million: cached.raw_output_per_million,
                raw_model: base_id.clone(),
                matched_model_id: Some(base_id),
                match_source: MatchSource::UserOverride.as_str().to_string(),
                price_locked: true,
                is_ignored: false,
                is_manual_rebind: false,
            });
        }
    }

    local_entries.sort_by(|a, b| {
        let a_unpriced = a.matched_model_id.is_none() && !a.is_ignored;
        let b_unpriced = b.matched_model_id.is_none() && !b.is_ignored;
        b_unpriced
            .cmp(&a_unpriced)
            .then_with(|| {
                a.provider
                    .to_ascii_lowercase()
                    .cmp(&b.provider.to_ascii_lowercase())
            })
            .then_with(|| a.provider.cmp(&b.provider))
            .then_with(|| {
                a.raw_model
                    .to_ascii_lowercase()
                    .cmp(&b.raw_model.to_ascii_lowercase())
            })
            .then_with(|| a.raw_model.cmp(&b.raw_model))
    });

    local_entries
}
