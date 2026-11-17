use crate::backend::domain::conversations::pricing::{
    ModelPriceEntry, PriceCatalog, DEFAULT_USD_TO_CNY_RATE, IGNORE_MODEL_ID_PREFIX,
    OVERRIDE_MODEL_ID_PREFIX,
};
use crate::backend::infrastructure::http_client::{get_with_redirects, shared_http_client};
use crate::backend::infrastructure::{InfraError, InfraResult};
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

use super::pricing_target_filter::filter_entries_for_target_models;

pub const EXCHANGE_RATE_API_URL: &str = "https://open.er-api.com/v6/latest/USD";
pub const LITELLM_PRIMARY_URL: &str =
    "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";
pub const LITELLM_MIRROR_URL: &str =
    "https://cdn.jsdelivr.net/gh/BerriAI/litellm@main/model_prices_and_context_window.json";
pub const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

/// 远端价格目录（LiteLLM / OpenRouter 全量 JSON）拉取超时。
///
/// 该 JSON 体积处于数 MB 到数十 MB 量级，慢速网络下 4 秒级别的短超时会让同步静默
/// 退回缓存/内置目录，因此放宽到 15 秒级别；与汇率接口的短超时彼此独立，
/// 严禁复用同一个常量。
pub const FETCH_TIMEOUT_SECS: u64 = 15;
/// 汇率接口独立短超时：汇率只是附属信息，不能拖累价格目录拉取。
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
    /// 所有远端渠道均不可达、最终回落到本地缓存或内置目录时为 true。
    pub downgraded: bool,
    /// 降级原因说明（仅在 `downgraded` 为 true 时存在）。
    pub warning: Option<String>,
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

/// 远端目录是否可用：LiteLLM 主站/镜像优先，其次 OpenRouter；均需解析出足够条目。
fn fetch_remote_catalog(rate: f64) -> Option<TieredFetchOutcome> {
    let now_date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let client = shared_http_client().ok()?;

    for url in [LITELLM_PRIMARY_URL, LITELLM_MIRROR_URL] {
        let Ok(resp) = get_with_redirects(
            &client,
            url,
            HeaderMap::new(),
            Duration::from_secs(FETCH_TIMEOUT_SECS),
        ) else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        let Ok(text) = resp.text() else { continue };
        let Ok(entries) = parse_litellm_json(&text, rate) else {
            continue;
        };
        if entries.len() >= 20 {
            return Some(TieredFetchOutcome {
                channel: PricingSourceChannel::LiteLlm,
                entries,
                rate,
                catalog_version: format!("litellm-{now_date}"),
                downgraded: false,
                warning: None,
            });
        }
    }

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
                        return Some(TieredFetchOutcome {
                            channel: PricingSourceChannel::OpenRouter,
                            entries,
                            rate,
                            catalog_version: format!("openrouter-{now_date}"),
                            downgraded: false,
                            warning: None,
                        });
                    }
                }
            }
        }
    }

    None
}

/// 缓存兜底池只接受纯净参考行。
///
/// `source_channel == 'user_override'`、`__override__::` 与 `__ignore__::` 语义行
/// 会抢走降级匹配，随后又在入库阶段被过滤，最终表现为「更新数为 0，渠道却写成 cache」。
fn is_reference_cache_entry(entry: &ModelPriceEntry) -> bool {
    if entry.is_ignored || entry.source_channel.eq_ignore_ascii_case("user_override") {
        return false;
    }
    !entry.model_patterns.iter().any(|pattern| {
        let pattern = pattern.trim();
        pattern.starts_with(OVERRIDE_MODEL_ID_PREFIX) || pattern.starts_with(IGNORE_MODEL_ID_PREFIX)
    })
}

/// 分层抓取内核：远端探测由 `remote` 提供（测试可注入离线实现，确定性覆盖兜底链路）。
fn fetch_tiered_pricing_catalog_inner<Remote>(
    rate: f64,
    cached_entries: Vec<ModelPriceEntry>,
    target_models: Option<&[String]>,
    remote: Remote,
) -> TieredFetchOutcome
where
    Remote: FnOnce() -> Option<TieredFetchOutcome>,
{
    let bundled = PriceCatalog::bundled_with_rate(rate);

    let mut outcome = match remote() {
        Some(outcome) => outcome,
        None => {
            let reference_cache: Vec<ModelPriceEntry> = cached_entries
                .into_iter()
                .filter(is_reference_cache_entry)
                .collect();
            if reference_cache.is_empty() {
                TieredFetchOutcome {
                    channel: PricingSourceChannel::Bundled,
                    entries: bundled.models.clone(),
                    rate,
                    catalog_version: bundled.catalog_version.clone(),
                    downgraded: true,
                    warning: Some(
                        "远端价格渠道均不可达，且本地无可用缓存，已降级使用内置价格目录"
                            .to_string(),
                    ),
                }
            } else {
                let cache_count = reference_cache.len();
                TieredFetchOutcome {
                    channel: PricingSourceChannel::Cache,
                    entries: reference_cache,
                    rate,
                    catalog_version: format!("cache-{}", chrono::Utc::now().format("%Y-%m-%d")),
                    downgraded: true,
                    warning: Some(format!(
                        "远端价格渠道均不可达，已降级使用本地缓存的 {cache_count} 条参考价格"
                    )),
                }
            }
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

/// Executes the four-tier fallback chain:
/// 1. Primary remote: LiteLLM
/// 2. Secondary remote: OpenRouter
/// 3. Local SQLite cache (passed in, filtered to pure reference rows)
/// 4. Bundled seed catalog (built-in)
///
/// If `target_models` is provided and non-empty, only the pricing entries matching
/// the target models will be extracted and returned, minimizing storage and redundant data.
pub fn fetch_tiered_pricing_catalog(
    rate: f64,
    cached_entries: Vec<ModelPriceEntry>,
    target_models: Option<&[String]>,
) -> TieredFetchOutcome {
    fetch_tiered_pricing_catalog_inner(rate, cached_entries, target_models, || {
        fetch_remote_catalog(rate)
    })
}

#[cfg(test)]
#[path = "pricing_fetcher_tests.rs"]
mod tests;
