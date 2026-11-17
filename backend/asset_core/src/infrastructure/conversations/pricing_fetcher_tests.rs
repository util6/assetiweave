use super::*;

#[test]
fn test_parse_litellm_json_success() {
    let mock_json = r#"{
        "gpt-4o": {
            "input_cost_per_token": 0.0000025,
            "output_cost_per_token": 0.00001,
            "cache_read_input_token_cost": 0.00000125,
            "cache_creation_input_token_cost": 0.0000025,
            "litellm_provider": "openai"
        },
        "deepseek-chat": {
            "input_cost_per_token": 0.000001,
            "output_cost_per_token": 0.000002,
            "litellm_provider": "deepseek"
        }
    }"#;

    let rate = 7.0;
    let entries = parse_litellm_json(mock_json, rate).expect("should parse litellm json");
    assert_eq!(entries.len(), 2);

    // gpt-4o should be converted to CNY
    let gpt4o = entries
        .iter()
        .find(|e| e.model_patterns.contains(&"gpt-4o".to_string()))
        .unwrap();
    assert_eq!(gpt4o.currency, "CNY");
    assert_eq!(gpt4o.raw_currency.as_deref(), Some("USD"));
    // input: 0.0000025 * 1M * 7.0 = 17.5 CNY
    assert!((gpt4o.input_per_million - 17.5).abs() < 1e-4);
    assert!((gpt4o.raw_input_per_million.unwrap() - 2.5).abs() < 1e-4);

    // deepseek-chat should remain native CNY
    let ds = entries
        .iter()
        .find(|e| e.model_patterns.contains(&"deepseek-chat".to_string()))
        .unwrap();
    assert_eq!(ds.currency, "CNY");
    assert_eq!(ds.raw_currency.as_deref(), Some("CNY"));
    // input: 0.000001 * 1M = 1.0 CNY
    assert!((ds.input_per_million - 1.0).abs() < 1e-4);
}

#[test]
fn test_parse_openrouter_json_success() {
    let mock_json = r#"{
        "data": [
            {
                "id": "anthropic/claude-3.7-sonnet",
                "pricing": {
                    "prompt": "0.000003",
                    "completion": "0.000015"
                }
            }
        ]
    }"#;

    let rate = 7.2;
    let entries = parse_openrouter_json(mock_json, rate).expect("should parse openrouter json");
    assert_eq!(entries.len(), 1);

    let claude = &entries[0];
    assert_eq!(claude.provider, "anthropic");
    assert_eq!(claude.currency, "CNY");
    // input: 0.000003 * 1M * 7.2 = 21.6 CNY
    assert!((claude.input_per_million - 21.6).abs() < 1e-4);
}

#[test]
fn test_fallback_chain_to_cache_and_bundled() {
    let rate = 7.25;

    let fake_cache_entry = ModelPriceEntry {
        provider: "custom".to_string(),
        model_patterns: vec!["cached-model".to_string()],
        currency: "CNY".to_string(),
        input_per_million: 5.0,
        cache_read_per_million: 1.0,
        cache_write_per_million: 5.0,
        output_per_million: 10.0,
        reasoning_per_million: 10.0,
        raw_currency: Some("CNY".to_string()),
        raw_input_per_million: Some(5.0),
        raw_output_per_million: Some(10.0),
        ..Default::default()
    };

    // 远端不可达 + 有缓存 → Cache 兜底
    let cache_outcome =
        fetch_tiered_pricing_catalog_inner(rate, vec![fake_cache_entry], None, || None);
    assert_eq!(cache_outcome.channel, PricingSourceChannel::Cache);
    assert_eq!(cache_outcome.entries.len(), 1);

    // 远端不可达 + 无缓存 → Bundled 兜底
    let bundled_outcome = fetch_tiered_pricing_catalog_inner(rate, vec![], None, || None);
    assert_eq!(bundled_outcome.channel, PricingSourceChannel::Bundled);
    assert!(bundled_outcome.entries.len() >= 15);
}

#[test]
fn fetch_timeouts_keep_remote_budget_and_rate_budget_independent() {
    assert!(
        FETCH_TIMEOUT_SECS >= 15,
        "远端大体积价格 JSON 拉取超时必须放宽到 15 秒级别"
    );
    assert!(
        RATE_FETCH_TIMEOUT_SECS < FETCH_TIMEOUT_SECS,
        "汇率取数必须使用独立的短超时，不得与价格拉取复用同一常量"
    );
    assert!(
        RATE_FETCH_TIMEOUT_SECS <= 5,
        "汇率接口属附属信息，超时必须保持秒级短预算"
    );
}

fn cache_entry(model_id: &str, channel: &str) -> ModelPriceEntry {
    ModelPriceEntry {
        provider: "custom".to_string(),
        model_patterns: vec![model_id.to_string()],
        currency: "CNY".to_string(),
        input_per_million: 5.0,
        output_per_million: 10.0,
        source_channel: channel.to_string(),
        ..Default::default()
    }
}

#[test]
fn offline_cache_fallback_excludes_override_and_ignore_rows() {
    let mut ignored = cache_entry(&format!("{IGNORE_MODEL_ID_PREFIX}deadbeef"), "bundled");
    ignored.price_locked = true;
    ignored.is_ignored = true;

    let cached = vec![
        cache_entry(&format!("{OVERRIDE_MODEL_ID_PREFIX}gpt-4o"), "bundled"),
        ignored,
        cache_entry("gpt-4o", "user_override"),
        cache_entry("clean-model", "litellm"),
    ];

    let outcome = fetch_tiered_pricing_catalog_inner(
        7.25,
        cached,
        Some(&["clean-model".to_string()]),
        || None,
    );

    assert_eq!(outcome.channel, PricingSourceChannel::Cache);
    assert!(outcome.downgraded, "远端全不可达时必须标记降级");
    assert!(
        outcome
            .warning
            .as_deref()
            .is_some_and(|warning| warning.contains("缓存")),
        "降级原因必须明确说明落到缓存"
    );
    assert_eq!(outcome.entries.len(), 1, "缓存池必须只剩纯净参考行");
    assert_eq!(
        outcome.entries[0]
            .model_patterns
            .first()
            .map(String::as_str),
        Some("clean-model")
    );
    assert!(
        !outcome.entries.iter().any(|entry| {
            entry.source_channel.eq_ignore_ascii_case("user_override")
                || entry.model_patterns.iter().any(|pattern| {
                    pattern.starts_with(OVERRIDE_MODEL_ID_PREFIX)
                        || pattern.starts_with(IGNORE_MODEL_ID_PREFIX)
                })
        }),
        "兜底条目不得包含 user_override / __override__:: / __ignore__:: 语义行"
    );
}

#[test]
fn offline_falls_back_to_bundled_with_explicit_warning() {
    let outcome = fetch_tiered_pricing_catalog_inner(7.25, vec![], None, || None);

    assert_eq!(outcome.channel, PricingSourceChannel::Bundled);
    assert!(outcome.downgraded);
    assert!(
        outcome
            .warning
            .as_deref()
            .is_some_and(|warning| warning.contains("内置")),
        "无缓存时降级原因必须说明落到内置目录"
    );
    assert!(outcome.entries.len() >= 15);
}

#[test]
fn test_on_demand_filtering_for_target_models() {
    let rate = 7.25;
    let target_models = vec!["gpt-5.6-luna".to_string(), "gemini-3.8-flash".to_string()];

    let outcome = fetch_tiered_pricing_catalog(rate, vec![], Some(&target_models));
    // Must contain exactly the requested models
    assert_eq!(outcome.entries.len(), 2);
    let luna = outcome
        .entries
        .iter()
        .find(|e| e.model_patterns.iter().any(|p| p.contains("luna")))
        .unwrap();
    // GPT-5.6 Luna input price should be 0.20 USD * 7.25 = 1.45 CNY
    assert!((luna.input_per_million - 1.45).abs() < 1e-4);
    assert!((luna.output_per_million - (1.20 * 7.25)).abs() < 1e-4);
}

#[test]
fn test_extract_patterns_from_remote_key_cleans_prefixes() {
    let p1 = extract_patterns_from_remote_key("azure_ai/FW-GLM-5");
    assert_eq!(p1.first().map(String::as_str), Some("glm-5"));
    assert!(p1.contains(&"glm5".to_string()));

    let p2 = extract_patterns_from_remote_key("openrouter/xiaomi/mimo-v2.5");
    assert_eq!(p2.first().map(String::as_str), Some("mimo-v2.5"));
    assert!(p2.contains(&"xiaomi/mimo-v2.5".to_string()));

    let p3 = extract_patterns_from_remote_key("bedrock/ap-northeast-1/minimax.minimax-m2.5");
    assert_eq!(p3.first().map(String::as_str), Some("minimax-m2.5"));

    let p4 = extract_patterns_from_remote_key("sail/zai-org/GLM-5.3-Flash");
    assert_eq!(p4.first().map(String::as_str), Some("glm-5.3-flash"));

    let p5 = extract_patterns_from_remote_key("anthropic/claude-3.7-sonnet:free");
    assert_eq!(p5.first().map(String::as_str), Some("claude-3.7-sonnet"));
}
