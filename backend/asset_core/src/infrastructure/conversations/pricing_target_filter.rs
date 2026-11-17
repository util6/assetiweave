//! 按目标模型（实际使用模型 / 手动改绑底座）按需抽取远端价格条目。
//!
//! 远端目录（LiteLLM）经常只提供带日期后缀的键（如 `claude-3-5-sonnet-20250219`），
//! 而本地目标模型是无日期的底座（如 `claude-3-5-sonnet`）。此处除常规的
//! 「目标 → 目录 pattern」匹配外，还会做「远端带日期键 → 本地底座」反查，
//! 保证底座拿到远端最新报价，而不是落回内置种子目录的旧价。

use crate::backend::domain::conversations::pricing::{
    base_model_id_from_catalog_key, strip_date_suffix, MatchSource, ModelPriceEntry, PriceCatalog,
    ResolvedModelMatch,
};

/// 单个目标模型在远端条目里命中的候选。
struct RawMatch<'a> {
    resolved: ResolvedModelMatch<'a>,
    score: usize,
    /// 反查命中的本地无日期底座；命中时需要把底座改写为 `model_patterns[0]`。
    reverse_base_id: Option<String>,
}

/// Filters and maps price entries on-demand for specific target models.
/// For each target model, it attempts to match against raw_entries first;
/// if not found, it falls back to bundled_catalog.
pub(crate) fn filter_entries_for_target_models(
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

        let raw_match =
            match raw_catalog
                .find_model_with_score("", trimmed)
                .and_then(|(_, score)| {
                    raw_catalog
                        .resolve_model("", trimmed)
                        .map(|resolved| RawMatch {
                            resolved,
                            score,
                            reverse_base_id: None,
                        })
                }) {
                Some(found) => Some(found),
                // 远端仅提供带日期后缀键时，反查本地无日期底座。
                None => find_reverse_dated_match(&raw_catalog, trimmed),
            };
        let bundled_match =
            bundled_catalog
                .find_model_with_score("", trimmed)
                .and_then(|(_, score)| {
                    bundled_catalog
                        .resolve_model("", trimmed)
                        .map(|r| (r, score))
                });

        let (chosen, reverse_base_id) = match (raw_match, bundled_match) {
            (Some(raw), Some((bundled_res, bundled_score))) => {
                let raw_has_price = has_reference_price(raw.resolved.entry);
                let bundled_has_price = has_reference_price(bundled_res.entry);

                if raw.reverse_base_id.is_some() && raw_has_price {
                    // 远端带日期的最新条目优先于内置种子目录的旧价。
                    (raw.resolved, raw.reverse_base_id)
                } else if raw_has_price && bundled_has_price {
                    if bundled_score > raw.score {
                        (bundled_res, None)
                    } else {
                        (raw.resolved, None)
                    }
                } else if raw_has_price {
                    (raw.resolved, raw.reverse_base_id)
                } else if bundled_has_price {
                    (bundled_res, None)
                } else {
                    continue;
                }
            }
            (Some(raw), None) => {
                if has_reference_price(raw.resolved.entry) {
                    (raw.resolved, raw.reverse_base_id)
                } else {
                    continue;
                }
            }
            (None, Some((bundled_res, _))) => {
                if has_reference_price(bundled_res.entry) {
                    (bundled_res, None)
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
        let canonical_base_id = if let Some(ref base) = reverse_base_id {
            base.clone()
        } else if primary_is_exact {
            first_pattern.to_string()
        } else if !chosen.matched_model_id.is_empty() && !chosen.matched_model_id.contains('*') {
            chosen.matched_model_id.clone()
        } else {
            first_pattern.trim_matches('*').to_string()
        };
        let rewrite_primary = reverse_base_id.is_some() || !primary_is_exact;

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
            if rewrite_primary {
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

fn has_reference_price(entry: &ModelPriceEntry) -> bool {
    entry.input_per_million > 0.0 || entry.output_per_million > 0.0
}

/// 「远端带日期键 → 本地无日期底座」反查。
///
/// 同一底座可能对应多条历史日期键（`...-20241022` / `...-20250219`），
/// 取日期后缀最大（最新）的一条，保证底座被刷成远端最新报价。
fn find_reverse_dated_match<'a>(catalog: &'a PriceCatalog, target: &str) -> Option<RawMatch<'a>> {
    let candidates = PriceCatalog::candidate_model_names(target);
    if candidates.is_empty() {
        return None;
    }

    let mut best: Option<(String, &'a ModelPriceEntry, String)> = None;
    for entry in &catalog.models {
        if entry.is_ignored || !has_reference_price(entry) {
            continue;
        }
        for pattern in &entry.model_patterns {
            let pattern = base_model_id_from_catalog_key(pattern.trim());
            let pattern = pattern.trim_matches('*').trim();
            let Some(stripped) = strip_date_suffix(pattern) else {
                continue;
            };
            let base_id = stripped.trim_matches('*').trim();
            if base_id.is_empty() {
                continue;
            }
            let Some(candidate) = candidates
                .iter()
                .find(|candidate| candidate.eq_ignore_ascii_case(base_id))
            else {
                continue;
            };
            let date_suffix = pattern
                .strip_prefix(stripped.as_str())
                .unwrap_or_default()
                .to_string();
            let replace = best.as_ref().map_or(true, |(best_suffix, ..)| {
                date_suffix.as_str() > best_suffix.as_str()
            });
            if replace {
                best = Some((date_suffix, entry, candidate.clone()));
            }
        }
    }

    let (_, entry, base_id) = best?;
    let score = base_id.chars().filter(|c| *c != '*').count();
    Some(RawMatch {
        resolved: ResolvedModelMatch {
            entry,
            matched_model_id: base_id.clone(),
            match_source: MatchSource::AutoMatched,
            price_locked: false,
            is_ignored: false,
        },
        score,
        reverse_base_id: Some(base_id),
    })
}

#[cfg(test)]
#[path = "pricing_target_filter_tests.rs"]
mod tests;
