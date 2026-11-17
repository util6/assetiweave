use serde::{Deserialize, Serialize};

pub use super::pricing_normalize::{
    base_model_id_from_catalog_key, candidate_model_names, generate_hyphen_variant,
    is_dirty_model_name, normalize_model_name, strip_date_suffix, IGNORE_MODEL_ID_PREFIX,
    OVERRIDE_MODEL_ID_PREFIX,
};
use super::pricing_normalize::{clean_base_id_from_pattern, matches_pattern, pattern_specificity};

pub const BUNDLED_MODEL_PRICES_JSON: &str =
    include_str!("../../../../../builtin-assets/pricing/model_prices.json");

pub const DEFAULT_USD_TO_CNY_RATE: f64 = 7.25;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RawBundledCatalog {
    pub catalog_version: String,
    pub updated_at: String,
    pub models: Vec<RawBundledModelPriceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RawBundledModelPriceEntry {
    pub provider: String,
    pub model_patterns: Vec<String>,
    pub currency: String,
    pub input_per_million: f64,
    pub cache_read_per_million: f64,
    pub cache_write_per_million: f64,
    pub output_per_million: f64,
    pub reasoning_per_million: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ModelPriceEntry {
    pub provider: String,
    pub model_patterns: Vec<String>,
    pub currency: String, // Normalized final currency (always "CNY")
    pub input_per_million: f64,
    pub cache_read_per_million: f64,
    pub cache_write_per_million: f64,
    pub output_per_million: f64,
    pub reasoning_per_million: f64,
    #[serde(default)]
    pub raw_currency: Option<String>,
    #[serde(default)]
    pub raw_input_per_million: Option<f64>,
    #[serde(default)]
    pub raw_output_per_million: Option<f64>,
    #[serde(default)]
    pub price_locked: bool,
    #[serde(default)]
    pub usage_aliases: Vec<String>,
    #[serde(default)]
    pub source_channel: String,
    #[serde(default)]
    pub is_ignored: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PriceCatalog {
    pub catalog_version: String,
    pub updated_at: String,
    pub usd_to_cny_rate: f64,
    pub models: Vec<ModelPriceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CostCalculation {
    pub catalog_version: String,
    pub estimated_cost: f64,
    pub currency: String, // Always "CNY"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchSource {
    Ignored,
    ManualRebind,
    UserOverride,
    AutoMatched,
}

impl MatchSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ignored => "ignored",
            Self::ManualRebind => "manual_rebind",
            Self::UserOverride => "user_override",
            Self::AutoMatched => "auto_matched",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedModelMatch<'a> {
    pub entry: &'a ModelPriceEntry,
    pub matched_model_id: String,
    pub match_source: MatchSource,
    pub price_locked: bool,
    pub is_ignored: bool,
}

impl PriceCatalog {
    pub fn new(
        catalog_version: String,
        updated_at: String,
        usd_to_cny_rate: f64,
        mut models: Vec<ModelPriceEntry>,
    ) -> Self {
        let bundled = Self::bundled_with_rate(usd_to_cny_rate);
        models.extend(bundled.models);
        Self {
            catalog_version,
            updated_at,
            usd_to_cny_rate,
            models,
        }
    }

    pub fn bundled() -> Self {
        Self::bundled_with_rate(DEFAULT_USD_TO_CNY_RATE)
    }

    pub fn bundled_with_rate(rate: f64) -> Self {
        let raw: RawBundledCatalog = serde_json::from_str(BUNDLED_MODEL_PRICES_JSON)
            .expect("bundled model_prices.json must be valid");

        let models = raw
            .models
            .into_iter()
            .map(|m| {
                let is_usd = m.currency.eq_ignore_ascii_case("USD");
                let multiplier = if is_usd { rate } else { 1.0 };
                ModelPriceEntry {
                    provider: m.provider,
                    model_patterns: m.model_patterns,
                    currency: "CNY".to_string(),
                    input_per_million: m.input_per_million * multiplier,
                    cache_read_per_million: m.cache_read_per_million * multiplier,
                    cache_write_per_million: m.cache_write_per_million * multiplier,
                    output_per_million: m.output_per_million * multiplier,
                    reasoning_per_million: m.reasoning_per_million * multiplier,
                    raw_currency: Some(m.currency),
                    raw_input_per_million: Some(m.input_per_million),
                    raw_output_per_million: Some(m.output_per_million),
                    source_channel: "bundled".to_string(),
                    ..Default::default()
                }
            })
            .collect();

        Self {
            catalog_version: raw.catalog_version,
            updated_at: raw.updated_at,
            usd_to_cny_rate: rate,
            models,
        }
    }

    pub fn normalize_model_name(raw: &str) -> String {
        normalize_model_name(raw)
    }

    pub fn candidate_model_names(raw: &str) -> Vec<String> {
        candidate_model_names(raw)
    }

    pub fn matches_pattern(pattern: &str, candidate: &str) -> bool {
        matches_pattern(pattern, candidate)
    }

    /// Calculates specificity score for pattern matching against candidate.
    /// Exact match gets 10,000 + length. Wildcards get count of non-wildcard chars.
    pub fn pattern_specificity(pattern: &str, candidate: &str) -> Option<usize> {
        pattern_specificity(pattern, candidate)
    }

    pub fn resolve_model(&self, provider: &str, raw_model: &str) -> Option<ResolvedModelMatch<'_>> {
        self.resolve_model_with_score(provider, raw_model)
            .map(|(resolved, _)| resolved)
    }

    fn resolve_model_with_score(
        &self,
        provider: &str,
        raw_model: &str,
    ) -> Option<(ResolvedModelMatch<'_>, usize)> {
        let trimmed_raw = raw_model.trim();
        if trimmed_raw.is_empty() {
            return None;
        }
        let norm_provider = provider.trim().to_ascii_lowercase();

        // Tier 1: Explicit ignore (works even on dirty strings)
        if let Some(ignored_entry) = self.pick_alias_match(&norm_provider, trimmed_raw, true) {
            return Some((
                ResolvedModelMatch {
                    entry: ignored_entry,
                    matched_model_id: String::new(),
                    match_source: MatchSource::Ignored,
                    price_locked: true,
                    is_ignored: true,
                },
                100_000,
            ));
        }

        // Tier 2: Manual explicit rebind via usage_aliases (works even on dirty strings)
        if let Some(rebind_entry) = self.pick_alias_match(&norm_provider, trimmed_raw, false) {
            if let Some(target_base_id) = rebind_entry
                .model_patterns
                .first()
                .map(|p| clean_base_id_from_pattern(p, None))
                .filter(|s| !s.is_empty())
            {
                let (effective_entry, price_locked) =
                    if let Some(ov) = self.find_locked_override(&norm_provider, &target_base_id) {
                        (ov, true)
                    } else {
                        (rebind_entry, rebind_entry.price_locked)
                    };
                return Some((
                    ResolvedModelMatch {
                        entry: effective_entry,
                        matched_model_id: target_base_id,
                        match_source: MatchSource::ManualRebind,
                        price_locked,
                        is_ignored: false,
                    },
                    50_000,
                ));
            }
        }

        // Tier 3: Self locked override (blocked when raw_model is dirty)
        if !is_dirty_model_name(raw_model) {
            let norm_raw = Self::normalize_model_name(raw_model);
            let norm_raw_base = base_model_id_from_catalog_key(&norm_raw);
            if let Some((ov_entry, matched_id)) =
                self.find_direct_locked_override(&norm_provider, trimmed_raw, norm_raw_base)
            {
                let score = 20_000 + matched_id.len();
                return Some((
                    ResolvedModelMatch {
                        entry: ov_entry,
                        matched_model_id: matched_id,
                        match_source: MatchSource::UserOverride,
                        price_locked: true,
                        is_ignored: false,
                    },
                    score,
                ));
            }
        }

        // Tier 4: Universal normalization + specificity scoring match
        let candidates = Self::candidate_model_names(raw_model);
        if candidates.is_empty() {
            return None;
        }

        let mut best: Option<((usize, bool, bool), &ModelPriceEntry, &str, &str)> = None;

        for candidate in &candidates {
            for entry in &self.models {
                if entry.is_ignored
                    || (!entry.price_locked
                        && entry.input_per_million <= 0.0
                        && entry.output_per_million <= 0.0)
                {
                    continue;
                }

                let provider_match = entry.provider.eq_ignore_ascii_case(&norm_provider)
                    || norm_provider.is_empty()
                    || entry.provider.is_empty();

                for pattern in &entry.model_patterns {
                    let pat_key = base_model_id_from_catalog_key(pattern);
                    let Some(score) = Self::pattern_specificity(pat_key, candidate) else {
                        continue;
                    };
                    let total_score = if provider_match { score + 50 } else { score };
                    if total_score == 0 {
                        continue;
                    }
                    let rank = (total_score, !pat_key.starts_with('*'), !entry.price_locked);
                    if best.map_or(true, |(best_rank, ..)| rank > best_rank) {
                        best = Some((rank, entry, pattern.as_str(), candidate.as_str()));
                    }
                }
            }
        }

        if let Some(((win_score, ..), win_entry, win_pattern, win_candidate)) = best {
            let matched_base_id = clean_base_id_from_pattern(win_pattern, Some(win_candidate));
            let (final_entry, price_locked) =
                if let Some(ov) = self.find_locked_override(&norm_provider, &matched_base_id) {
                    (ov, true)
                } else {
                    (win_entry, win_entry.price_locked)
                };
            let match_source =
                if win_entry.price_locked && !self.has_non_locked_entry_for(&matched_base_id) {
                    MatchSource::UserOverride
                } else {
                    MatchSource::AutoMatched
                };
            return Some((
                ResolvedModelMatch {
                    entry: final_entry,
                    matched_model_id: matched_base_id,
                    match_source,
                    price_locked,
                    is_ignored: false,
                },
                win_score,
            ));
        }

        // Tier 5: Unmatched
        None
    }

    fn pick_alias_match(
        &self,
        norm_provider: &str,
        trimmed_raw: &str,
        want_ignored: bool,
    ) -> Option<&ModelPriceEntry> {
        let mut fallback: Option<&ModelPriceEntry> = None;
        for entry in &self.models {
            if want_ignored {
                if !(entry.is_ignored && entry.price_locked) {
                    continue;
                }
            } else if entry.is_ignored {
                continue;
            }
            let has_alias = entry
                .usage_aliases
                .iter()
                .any(|a| a.trim().eq_ignore_ascii_case(trimmed_raw));
            if !has_alias {
                continue;
            }
            if !norm_provider.is_empty() && entry.provider.eq_ignore_ascii_case(norm_provider) {
                return Some(entry);
            }
            if fallback.is_none() || (!want_ignored && !entry.price_locked) {
                fallback = Some(entry);
            }
        }
        fallback
    }

    fn find_locked_override(
        &self,
        norm_provider: &str,
        base_model_id: &str,
    ) -> Option<&ModelPriceEntry> {
        let mut fallback: Option<&ModelPriceEntry> = None;
        for entry in &self.models {
            if !entry.price_locked || entry.is_ignored {
                continue;
            }
            let matches_base = entry.model_patterns.iter().any(|p| {
                base_model_id_from_catalog_key(p)
                    .trim_matches('*')
                    .eq_ignore_ascii_case(base_model_id)
            });
            if !matches_base {
                continue;
            }
            if !norm_provider.is_empty() && entry.provider.eq_ignore_ascii_case(norm_provider) {
                return Some(entry);
            }
            if fallback.is_none() {
                fallback = Some(entry);
            }
        }
        fallback
    }

    fn find_direct_locked_override(
        &self,
        norm_provider: &str,
        trimmed_raw: &str,
        norm_raw_base: &str,
    ) -> Option<(&ModelPriceEntry, String)> {
        let mut fallback: Option<(&ModelPriceEntry, String)> = None;
        for entry in &self.models {
            if !entry.price_locked || entry.is_ignored {
                continue;
            }
            for pattern in &entry.model_patterns {
                let pat_base = base_model_id_from_catalog_key(pattern);
                if pat_base.eq_ignore_ascii_case(norm_raw_base)
                    || pattern.trim().eq_ignore_ascii_case(trimmed_raw)
                {
                    let cleaned = pat_base.trim_matches('*').to_string();
                    if !norm_provider.is_empty()
                        && entry.provider.eq_ignore_ascii_case(norm_provider)
                    {
                        return Some((entry, cleaned));
                    }
                    if fallback.is_none() {
                        fallback = Some((entry, cleaned));
                    }
                }
            }
        }
        fallback
    }

    fn has_non_locked_entry_for(&self, base_model_id: &str) -> bool {
        self.models.iter().any(|entry| {
            !entry.is_ignored
                && !entry.price_locked
                && (entry.input_per_million > 0.0 || entry.output_per_million > 0.0)
                && entry.model_patterns.iter().any(|p| {
                    let pat = base_model_id_from_catalog_key(p);
                    Self::matches_pattern(pat, base_model_id)
                })
        })
    }

    pub fn find_model_with_score(
        &self,
        provider: &str,
        model: &str,
    ) -> Option<(&ModelPriceEntry, usize)> {
        self.resolve_model_with_score(provider, model)
            .map(|(resolved, score)| (resolved.entry, score))
    }

    pub fn find_model(&self, provider: &str, model: &str) -> Option<&ModelPriceEntry> {
        self.resolve_model(provider, model).map(|r| r.entry)
    }

    pub fn calculate_cost(
        &self,
        provider: &str,
        model: &str,
        input_tokens: i64,
        cache_read_tokens: i64,
        cache_write_tokens: i64,
        reasoning_tokens: i64,
        output_tokens: i64,
    ) -> Option<CostCalculation> {
        let resolved = self.resolve_model(provider, model)?;
        if resolved.is_ignored {
            return Some(CostCalculation {
                catalog_version: self.catalog_version.clone(),
                estimated_cost: 0.0,
                currency: "CNY".to_string(),
            });
        }
        let entry = resolved.entry;
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
            currency: "CNY".to_string(),
        })
    }
}

#[cfg(test)]
#[path = "pricing_tests.rs"]
mod tests;
