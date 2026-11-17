use super::*;
use crate::backend::infrastructure::conversations::pricing_fetcher::extract_patterns_from_remote_key;

fn remote_entry(key: &str, input_per_million: f64, output_per_million: f64) -> ModelPriceEntry {
    let provider = key.split('/').next().unwrap_or("").to_string();
    ModelPriceEntry {
        provider,
        model_patterns: extract_patterns_from_remote_key(key),
        currency: "CNY".to_string(),
        input_per_million,
        output_per_million,
        ..Default::default()
    }
}

#[test]
fn filter_entries_resolves_promotional_and_vendor_models() {
    let raw = vec![
        ModelPriceEntry {
            provider: "azure_ai".to_string(),
            model_patterns: extract_patterns_from_remote_key("azure_ai/FW-GLM-5"),
            currency: "CNY".to_string(),
            input_per_million: 7.7,
            cache_read_per_million: 1.0,
            cache_write_per_million: 7.7,
            output_per_million: 24.6,
            reasoning_per_million: 24.6,
            raw_currency: Some("USD".to_string()),
            raw_input_per_million: Some(1.1),
            raw_output_per_million: Some(3.52),
            ..Default::default()
        },
        ModelPriceEntry {
            provider: "bedrock".to_string(),
            model_patterns: extract_patterns_from_remote_key(
                "bedrock/ap-northeast-1/minimax.minimax-m2.5",
            ),
            currency: "CNY".to_string(),
            input_per_million: 2.3,
            cache_read_per_million: 0.5,
            cache_write_per_million: 2.3,
            output_per_million: 9.2,
            reasoning_per_million: 9.2,
            raw_currency: Some("USD".to_string()),
            raw_input_per_million: Some(0.33),
            raw_output_per_million: Some(1.32),
            ..Default::default()
        },
    ];

    let bundled = PriceCatalog::bundled();
    let targets = vec!["ZhipuAI/GLM-5".to_string(), "minimax-m2.5-free".to_string()];

    let selected = filter_entries_for_target_models(&raw, &targets, &bundled);
    assert_eq!(selected.len(), 2);

    let glm = selected
        .iter()
        .find(|s| s.model_patterns.first().map(String::as_str) == Some("glm-5"))
        .expect("must keep canonical native base model_id glm-5");
    assert!(!glm.model_patterns.iter().any(|p| p == "zhipuai/glm-5"));
    assert!(!glm.model_patterns.iter().any(|p| p == "ZhipuAI/GLM-5"));

    let minimax = selected
        .iter()
        .find(|s| s.model_patterns.first().map(String::as_str) == Some("minimax-m2.5"))
        .expect("must keep canonical native base model_id minimax-m2.5");
    assert!(!minimax
        .model_patterns
        .iter()
        .any(|p| p == "minimax-m2.5-free"));
}

#[test]
fn filter_entries_matches_display_names_without_injecting_duplicates() {
    let raw = vec![
        ModelPriceEntry {
            provider: "anthropic".to_string(),
            model_patterns: extract_patterns_from_remote_key("anthropic/claude-3.7-sonnet"),
            currency: "CNY".to_string(),
            input_per_million: 21.75,
            output_per_million: 108.75,
            ..Default::default()
        },
        ModelPriceEntry {
            provider: "google".to_string(),
            model_patterns: extract_patterns_from_remote_key("google/flash"),
            currency: "CNY".to_string(),
            input_per_million: 0.725,
            output_per_million: 2.9,
            ..Default::default()
        },
    ];
    let bundled = PriceCatalog::bundled();
    let targets = vec![
        "Claude 3.7 Sonnet (Thinking)".to_string(),
        "anthropic/claude-3.7-sonnet".to_string(),
        "Flash -N".to_string(),
        "Gemini 3.8 Flash (High)".to_string(),
    ];

    let selected = filter_entries_for_target_models(&raw, &targets, &bundled);
    assert_eq!(
        selected.len(),
        3,
        "Claude variants must deduplicate to 1 entry, plus flash and gemini-3.8-flash"
    );

    let claude = selected
        .iter()
        .find(|s| s.provider == "anthropic")
        .expect("claude entry");
    assert_eq!(
        claude.model_patterns.first().map(String::as_str),
        Some("claude-3.7-sonnet")
    );
    assert!(!claude
        .model_patterns
        .iter()
        .any(|p| p.contains("Thinking") || p.contains("thinking") || p.contains(' ')));

    let flash = selected
        .iter()
        .find(|s| s.model_patterns.first().map(String::as_str) == Some("flash"))
        .expect("flash entry");
    assert!(!flash
        .model_patterns
        .iter()
        .any(|p| p.eq_ignore_ascii_case("Flash -N") || p.eq_ignore_ascii_case("flash-n")));

    let gemini = selected
        .iter()
        .find(|s| s.model_patterns.first().map(String::as_str) == Some("gemini-3.8-flash"))
        .expect("gemini-3.8-flash entry");
    assert!(!gemini
        .model_patterns
        .iter()
        .any(|p| p.contains("High") || p.contains(' ')));
}

#[test]
fn filter_entries_maps_remote_dated_key_onto_local_undated_base() {
    // 远端只提供带日期后缀的键；本地目标模型是无日期底座。
    let raw = vec![
        remote_entry("anthropic/claude-3-5-sonnet-20241022", 10.0, 40.0),
        remote_entry("anthropic/claude-3-5-sonnet-20250219", 12.0, 48.0),
    ];
    let bundled = PriceCatalog::bundled_with_rate(7.25);

    let selected =
        filter_entries_for_target_models(&raw, &["claude-3-5-sonnet".to_string()], &bundled);

    assert_eq!(selected.len(), 1, "两个日期变体必须收敛到同一底座条目");
    let entry = &selected[0];
    assert_eq!(
        entry.model_patterns.first().map(String::as_str),
        Some("claude-3-5-sonnet"),
        "落库底座键必须是无日期模型 id"
    );
    assert!(
        (entry.input_per_million - 12.0).abs() < 1e-6,
        "必须取远端日期最新的报价，而不是较早的历史条目"
    );

    // 外部可观测契约：底座模型解析结果必须是远端最新价，而不是内置种子旧价（3.00 USD ≈ 21.75 CNY）。
    let catalog = PriceCatalog::new("litellm-test".to_string(), String::new(), 7.25, selected);
    let resolved = catalog
        .resolve_model("", "claude-3-5-sonnet")
        .expect("本地底座必须能解析到远端条目");
    assert!((resolved.entry.input_per_million - 12.0).abs() < 1e-6);
    assert_eq!(resolved.matched_model_id, "claude-3-5-sonnet");
}

#[test]
fn filter_entries_keeps_distinct_siblings_apart() {
    // 反查只对同一底座生效，不能把 gpt-4o-mini 的报价塞给 gpt-4o。
    let raw = vec![
        remote_entry("openai/gpt-4o-2024-08-06", 18.0, 72.0),
        remote_entry("openai/gpt-4o-mini-2024-07-18", 1.1, 4.4),
    ];
    let bundled = PriceCatalog::bundled_with_rate(7.25);

    let selected = filter_entries_for_target_models(
        &raw,
        &["gpt-4o".to_string(), "gpt-4o-mini".to_string()],
        &bundled,
    );

    assert_eq!(selected.len(), 2);
    let gpt4o = selected
        .iter()
        .find(|s| s.model_patterns.first().map(String::as_str) == Some("gpt-4o"))
        .expect("gpt-4o 底座条目");
    assert!((gpt4o.input_per_million - 18.0).abs() < 1e-6);
    let mini = selected
        .iter()
        .find(|s| s.model_patterns.first().map(String::as_str) == Some("gpt-4o-mini"))
        .expect("gpt-4o-mini 底座条目");
    assert!((mini.input_per_million - 1.1).abs() < 1e-6);
}
