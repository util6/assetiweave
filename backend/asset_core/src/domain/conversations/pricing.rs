use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
        let trimmed = raw.trim().to_ascii_lowercase();
        if let Some(stripped) = trimmed.strip_prefix("models/") {
            stripped.to_string()
        } else {
            trimmed
        }
    }

    pub fn candidate_model_names(raw: &str) -> Vec<String> {
        let mut candidates = Vec::new();
        let norm = Self::normalize_model_name(raw);
        if norm.is_empty() {
            return candidates;
        }
        candidates.push(norm.clone());

        // Strip gateway prefix e.g. "openrouter/anthropic/claude-3-7-sonnet" -> "anthropic/claude-3-7-sonnet"
        if let Some(stripped) = norm.strip_prefix("openrouter/") {
            candidates.push(stripped.to_string());
        }

        // If contains '/', add segments:
        // 1) Tail segment after last slash (e.g. "ZhipuAI/GLM-5" -> "glm-5")
        // 2) Subsegment after first slash if multiple (e.g. "openrouter/xiaomi/mimo-v2.5" -> "xiaomi/mimo-v2.5")
        if let Some(last_slash) = norm.rfind('/') {
            let tail = norm[last_slash + 1..].to_string();
            if !tail.is_empty() {
                candidates.push(tail);
            }
            if let Some(first_slash) = norm.find('/') {
                if first_slash != last_slash {
                    let sub = norm[first_slash + 1..].to_string();
                    if !sub.is_empty() {
                        candidates.push(sub);
                    }
                }
            }
        }

        // For each candidate, attempt stripping free suffixes, date suffixes and alias suffixes
        let mut additional = Vec::new();
        for name in &candidates {
            if let Some(stripped) = strip_free_suffix(name) {
                additional.push(stripped.clone());
                if let Some(s2) = strip_alias_suffix(&stripped) {
                    additional.push(s2);
                }
                if let Some(s3) = strip_date_suffix(&stripped) {
                    additional.push(s3);
                }
            }
            if let Some(stripped) = strip_date_suffix(name) {
                additional.push(stripped);
            }
            if let Some(stripped) = strip_alias_suffix(name) {
                additional.push(stripped.clone());
                if let Some(s2) = strip_free_suffix(&stripped) {
                    additional.push(s2);
                }
            }
        }
        candidates.extend(additional);

        // Also generate hyphen / numbering variants (e.g. glm5 <-> glm-5, qwen3 <-> qwen-3)
        let mut variants = Vec::new();
        for name in &candidates {
            if let Some(v) = generate_hyphen_variant(name) {
                variants.push(v);
            }
            if let Some(v) = strip_qwen_version(name) {
                variants.push(v);
            }
        }
        candidates.extend(variants);

        // Deduplicate while preserving order
        let mut seen = std::collections::HashSet::new();
        candidates.retain(|c| !c.is_empty() && seen.insert(c.clone()));
        candidates
    }

    pub fn matches_pattern(pattern: &str, candidate: &str) -> bool {
        let pat = pattern.trim().to_ascii_lowercase();
        let cand = candidate.trim().to_ascii_lowercase();
        if pat == "*" || pat == cand {
            return true;
        }
        if !pat.contains('*') {
            return pat == cand;
        }

        let parts: Vec<&str> = pat.split('*').collect();
        let starts_with_wildcard = pat.starts_with('*');
        let ends_with_wildcard = pat.ends_with('*');

        let non_empty: Vec<&str> = parts.into_iter().filter(|s| !s.is_empty()).collect();
        if non_empty.is_empty() {
            return true;
        }

        let mut current_idx = 0;
        for (i, part) in non_empty.iter().enumerate() {
            if i == 0 && !starts_with_wildcard {
                if !cand.starts_with(part) {
                    return false;
                }
                current_idx = part.len();
            } else if i == non_empty.len() - 1 && !ends_with_wildcard {
                if !cand[current_idx..].ends_with(part) {
                    return false;
                }
            } else {
                match cand[current_idx..].find(part) {
                    Some(found_idx) => {
                        current_idx += found_idx + part.len();
                    }
                    None => return false,
                }
            }
        }
        true
    }

    /// Calculates specificity score for pattern matching against candidate.
    /// Exact match gets 10,000 + length. Wildcards get count of non-wildcard chars.
    pub fn pattern_specificity(pattern: &str, candidate: &str) -> Option<usize> {
        if !Self::matches_pattern(pattern, candidate) {
            return None;
        }
        let pat = pattern.trim().to_ascii_lowercase();
        let cand = candidate.trim().to_ascii_lowercase();
        if pat == cand {
            Some(10_000 + pat.len())
        } else {
            let non_wildcards = pat.chars().filter(|&c| c != '*').count();
            Some(non_wildcards)
        }
    }

    pub fn find_model_with_score(
        &self,
        provider: &str,
        model: &str,
    ) -> Option<(&ModelPriceEntry, usize)> {
        let norm_provider = provider.trim().to_ascii_lowercase();
        let candidates = Self::candidate_model_names(model);

        let mut best: Option<(&ModelPriceEntry, usize)> = None;

        for candidate in &candidates {
            for entry in &self.models {
                // Skip zero-priced entries to prevent fake promotional or unpriced rules from poisoning valid catalog entries
                if entry.input_per_million <= 0.0 && entry.output_per_million <= 0.0 {
                    continue;
                }

                let provider_match = entry.provider.eq_ignore_ascii_case(&norm_provider)
                    || norm_provider.is_empty()
                    || entry.provider.is_empty();

                let mut entry_best_score = 0;
                for pattern in &entry.model_patterns {
                    if let Some(score) = Self::pattern_specificity(pattern, candidate) {
                        let total_score = if provider_match { score + 50 } else { score };
                        if total_score > entry_best_score {
                            entry_best_score = total_score;
                        }
                    }
                }

                if entry_best_score > 0 {
                    if let Some((_, current_best_score)) = best {
                        if entry_best_score > current_best_score {
                            best = Some((entry, entry_best_score));
                        }
                    } else {
                        best = Some((entry, entry_best_score));
                    }
                }
            }
        }

        best
    }

    pub fn find_model(&self, provider: &str, model: &str) -> Option<&ModelPriceEntry> {
        self.find_model_with_score(provider, model)
            .map(|(entry, _)| entry)
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
            currency: "CNY".to_string(),
        })
    }
}

fn strip_date_suffix(name: &str) -> Option<String> {
    // 8-digit date like -20250219 or -20241022
    if name.len() >= 9 {
        let suffix = &name[name.len() - 9..];
        if suffix.starts_with('-') && suffix[1..].chars().all(|c| c.is_ascii_digit()) {
            return Some(name[..name.len() - 9].to_string());
        }
    }
    // YYYY-MM-DD like -2024-08-06 (11 chars)
    if name.len() >= 11 {
        let suffix = &name[name.len() - 11..];
        if suffix.starts_with('-') {
            let parts: Vec<&str> = suffix[1..].split('-').collect();
            if parts.len() == 3
                && parts[0].len() == 4
                && parts[0].chars().all(|c| c.is_ascii_digit())
                && parts[1].len() == 2
                && parts[1].chars().all(|c| c.is_ascii_digit())
                && parts[2].len() == 2
                && parts[2].chars().all(|c| c.is_ascii_digit())
            {
                return Some(name[..name.len() - 11].to_string());
            }
        }
    }
    // Shorter stamps like -001, -0125, -1106, -0613
    for len in [4, 5] {
        if name.len() >= len {
            let suffix = &name[name.len() - len..];
            if suffix.starts_with('-') && suffix[1..].chars().all(|c| c.is_ascii_digit()) {
                return Some(name[..name.len() - len].to_string());
            }
        }
    }
    None
}

fn strip_free_suffix(name: &str) -> Option<String> {
    for suffix in ["-free", ":free", "_free", "-chat-free", "-free-chat"] {
        if let Some(stripped) = name.strip_suffix(suffix) {
            if !stripped.is_empty() {
                return Some(stripped.to_string());
            }
        }
    }
    None
}

fn strip_alias_suffix(name: &str) -> Option<String> {
    for suffix in [
        "-latest",
        "-preview",
        "-preview-f",
        "-exp",
        "-experimental",
        "-chat",
        "-default",
        "-n",
        "-f",
        "-turbo",
        "-instruct",
        ":batch",
        "-batch",
    ] {
        if let Some(stripped) = name.strip_suffix(suffix) {
            return Some(stripped.to_string());
        }
    }
    if let Some(idx) = name.find("-preview") {
        let base = &name[..idx];
        if !base.is_empty() {
            return Some(base.to_string());
        }
    }
    None
}

pub fn generate_hyphen_variant(name: &str) -> Option<String> {
    // glm5 <-> glm-5
    if let Some(idx) = name.find("glm") {
        let after = &name[idx + 3..];
        if after.starts_with('-')
            && after[1..]
                .chars()
                .next()
                .map_or(false, |c| c.is_ascii_digit())
        {
            let mut s = String::with_capacity(name.len() - 1);
            s.push_str(&name[..idx + 3]);
            s.push_str(&after[1..]);
            return Some(s);
        } else if after.chars().next().map_or(false, |c| c.is_ascii_digit()) {
            let mut s = String::with_capacity(name.len() + 1);
            s.push_str(&name[..idx + 3]);
            s.push('-');
            s.push_str(after);
            return Some(s);
        }
    }
    // qwen3 <-> qwen-3
    if let Some(idx) = name.find("qwen") {
        let after = &name[idx + 4..];
        if after.starts_with('-')
            && after[1..]
                .chars()
                .next()
                .map_or(false, |c| c.is_ascii_digit())
        {
            let mut s = String::with_capacity(name.len() - 1);
            s.push_str(&name[..idx + 4]);
            s.push_str(&after[1..]);
            return Some(s);
        } else if after.chars().next().map_or(false, |c| c.is_ascii_digit()) {
            let mut s = String::with_capacity(name.len() + 1);
            s.push_str(&name[..idx + 4]);
            s.push('-');
            s.push_str(after);
            return Some(s);
        }
    }
    None
}

fn strip_qwen_version(name: &str) -> Option<String> {
    if let Some(stripped) = name.strip_prefix("qwen") {
        for sfx in ["-plus", "-max", "-turbo"] {
            if stripped.ends_with(sfx) {
                return Some(format!("qwen{}", sfx));
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "pricing_tests.rs"]
mod tests;
