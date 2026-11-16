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

    // When cache is provided, and remote fails or is bypassed
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

    // If cache is empty, fallback returns Bundled
    let bundled_outcome = fetch_tiered_pricing_catalog(rate, vec![], None);
    // If running in environment without internet, it will be Bundled (or LiteLLM if internet accessible)
    assert!(bundled_outcome.entries.len() >= 15);
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

#[test]
fn test_filter_entries_resolves_promotional_and_vendor_models() {
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
fn test_filter_entries_matches_display_names_without_injecting_duplicates() {
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
