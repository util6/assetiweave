use super::*;

#[test]
fn test_bundled_catalog_parses_successfully() {
    let catalog = PriceCatalog::bundled();
    assert!(!catalog.catalog_version.is_empty());
    assert!(!catalog.models.is_empty());
    assert_eq!(catalog.usd_to_cny_rate, 7.25);
    // All models in the active catalog must be normalized to CNY
    for model in &catalog.models {
        assert_eq!(model.currency, "CNY");
    }
}

#[test]
fn test_gemini_37_flash_pricing_converted_to_cny() {
    let rate = 7.0;
    let catalog = PriceCatalog::bundled_with_rate(rate);
    let calc = catalog
        .calculate_cost(
            "google",
            "gemini-3.7-flash",
            1_000_000, // input: 0.10 USD * 7.0 = 0.70 CNY
            0,
            0,
            0,
            1_000_000, // output: 0.40 USD * 7.0 = 2.80 CNY
        )
        .expect("should match gemini-3.7-flash");

    assert_eq!(calc.currency, "CNY");
    assert_eq!(calc.catalog_version, "2026-09-01");
    // total: 0.70 + 2.80 = 3.50 CNY
    assert!((calc.estimated_cost - 3.50).abs() < 1e-4);
}

#[test]
fn test_cny_pricing_deepseek_retains_native_cny() {
    let catalog = PriceCatalog::bundled_with_rate(7.25);
    let calc = catalog
        .calculate_cost("deepseek", "deepseek-chat", 1_000_000, 0, 0, 0, 1_000_000)
        .expect("should match deepseek-chat");
    assert_eq!(calc.currency, "CNY");
    // input 1.0 CNY + output 2.0 CNY = 3.0 CNY (unaffected by USD rate)
    assert_eq!(calc.estimated_cost, 3.00);
}

#[test]
fn test_date_suffixed_model_matches_base() {
    let catalog = PriceCatalog::bundled();
    // claude-3-7-sonnet-20250219 should match claude-3-7-sonnet*
    let calc = catalog.calculate_cost(
        "anthropic",
        "claude-3-7-sonnet-20250219",
        1_000_000,
        0,
        0,
        0,
        0,
    );
    assert!(calc.is_some());
    assert_eq!(calc.as_ref().unwrap().currency, "CNY");
    // input is 3.0 USD * 7.25 = 21.75 CNY
    assert!((calc.unwrap().estimated_cost - 21.75).abs() < 1e-4);

    // gpt-4o-2024-08-06 should match gpt-4o*
    let gpt_calc = catalog.calculate_cost("openai", "gpt-4o-2024-08-06", 1_000_000, 0, 0, 0, 0);
    assert!(gpt_calc.is_some());
}

#[test]
fn test_gateway_prefix_and_alias_matching() {
    let catalog = PriceCatalog::bundled();
    // openrouter/anthropic/claude-3-5-sonnet should match
    let calc1 = catalog.calculate_cost(
        "openrouter",
        "openrouter/anthropic/claude-3-5-sonnet",
        1_000_000,
        0,
        0,
        0,
        0,
    );
    assert!(calc1.is_some());

    // chatgpt-4o-latest or gpt-4o-latest should match gpt-4o
    let calc2 = catalog.calculate_cost("openai", "gpt-4o-latest", 1_000_000, 0, 0, 0, 0);
    assert!(calc2.is_some());

    // models/gemini-2.0-flash should match gemini-2.0-flash
    let calc3 = catalog.calculate_cost("google", "models/gemini-2.0-flash", 1_000_000, 0, 0, 0, 0);
    assert!(calc3.is_some());
}

#[test]
fn test_unknown_model_returns_none() {
    let catalog = PriceCatalog::bundled();
    let calc = catalog.calculate_cost("custom", "my-nonexistent-private-llm", 1000, 0, 0, 0, 1000);
    assert!(calc.is_none());
}

#[test]
fn test_real_world_user_models_matching() {
    let catalog = PriceCatalog::bundled();

    // 1. Google Gemini 3.8 / 2.5
    assert!(catalog.find_model("google", "gemini-3.8-flash").is_some());
    assert!(catalog.find_model("google", "gemini-3.8-flash-n").is_some());
    assert!(catalog.find_model("google", "gemini-2.5-pro").is_some());

    // 2. OpenAI Luna / Sol / Astra
    assert!(catalog.find_model("openai", "gpt-5.6-luna").is_some());
    assert!(catalog.find_model("openai", "gpt-5.6-sol").is_some());
    assert!(catalog.find_model("openai", "gpt-6-luna").is_some());
    assert!(catalog.find_model("openai", "gpt-6-sol").is_some());
    assert!(catalog.find_model("openai", "gpt-6-astra").is_some());

    // 3. Domestic models (Kimi, Hunyuan, Mimo, MiniMax, GLM, Ling)
    assert!(catalog.find_model("moonshot", "kimi-k3-1").is_some());
    assert!(catalog.find_model("tencent", "hy4-preview-f").is_some());
    assert!(catalog.find_model("tencent", "hy3-free").is_some());
    assert!(catalog.find_model("xiaomi", "mimo-v2.5-free").is_some());
    assert!(catalog.find_model("minimax", "minimax-m2.5-free").is_some());
    assert!(catalog.find_model("minimax", "minimax-m2.1-free").is_some());
    assert!(catalog.find_model("modelscope", "ZhipuAI/GLM-5").is_some());
    assert!(catalog.find_model("nvidia", "z-ai/glm5").is_some());
    assert!(catalog.find_model("nvidia", "z-ai/glm-5.3-flash").is_some());
    assert!(catalog.find_model("ling", "ling-2.6-flash-free").is_some());
    assert!(catalog.find_model("qwen", "qwen3.6-plus-free").is_some());
    assert!(catalog.find_model("google", "gemini-default").is_some());

    // Check pricing calculation
    let luna_calc = catalog
        .calculate_cost("openai", "gpt-5.6-luna", 1_000_000, 0, 0, 0, 1_000_000)
        .expect("luna cost");
    assert_eq!(luna_calc.currency, "CNY");
    // input: 0.20 USD * 7.25 = 1.45 CNY; output: 1.20 USD * 7.25 = 8.70 CNY; sum = 10.15 CNY
    assert!((luna_calc.estimated_cost - 10.15).abs() < 1e-4);

    // mimo-v2.5-free and mimo-v2.6-flash-free strips -free to mimo-v2.5 and uses genuine model pricing even under opencode provider
    let mimo_calc = catalog
        .calculate_cost("opencode", "mimo-v2.5-free", 1_000_000, 0, 0, 0, 1_000_000)
        .expect("mimo cost");
    assert_eq!(mimo_calc.currency, "CNY");
    assert!((mimo_calc.estimated_cost - 2.85).abs() < 1e-4);

    let mimo_flash_calc = catalog
        .calculate_cost(
            "opencode",
            "mimo-v2.6-flash-free",
            1_000_000,
            0,
            0,
            0,
            1_000_000,
        )
        .expect("mimo flash cost");
    assert_eq!(mimo_flash_calc.currency, "CNY");
    // input: 0.20 CNY; output: 0.40 CNY; sum = 0.60 CNY
    assert!((mimo_flash_calc.estimated_cost - 0.60).abs() < 1e-4);

    // ZhipuAI/GLM-5 strips vendor prefix and matches GLM-5
    let glm_calc = catalog
        .calculate_cost("modelscope", "ZhipuAI/GLM-5", 1_000_000, 0, 0, 0, 1_000_000)
        .expect("glm cost");
    assert_eq!(glm_calc.currency, "CNY");
    // input: 4.0 CNY; output: 12.0 CNY; sum = 16.0 CNY
    assert!((glm_calc.estimated_cost - 16.00).abs() < 1e-4);
}

#[test]
fn test_find_model_ignores_zero_priced_entries() {
    let zero_entry = ModelPriceEntry {
        provider: "xiaomi".to_string(),
        model_patterns: vec!["mimo-v2.5-free".to_string()],
        currency: "CNY".to_string(),
        input_per_million: 0.0,
        cache_read_per_million: 0.0,
        cache_write_per_million: 0.0,
        reasoning_per_million: 0.0,
        output_per_million: 0.0,
        raw_currency: Some("USD".to_string()),
        raw_input_per_million: Some(0.0),
        raw_output_per_million: Some(0.0),
    };
    let real_entry = ModelPriceEntry {
        provider: "xiaomi".to_string(),
        model_patterns: vec!["mimo-v2.5*".to_string()],
        currency: "CNY".to_string(),
        input_per_million: 0.95,
        cache_read_per_million: 0.20,
        cache_write_per_million: 0.95,
        reasoning_per_million: 1.90,
        output_per_million: 1.90,
        raw_currency: Some("CNY".to_string()),
        raw_input_per_million: Some(0.95),
        raw_output_per_million: Some(1.90),
    };

    let catalog = PriceCatalog::new(
        "test".to_string(),
        "".to_string(),
        7.25,
        vec![zero_entry, real_entry],
    );

    let found = catalog
        .find_model("opencode", "mimo-v2.5-free")
        .expect("must find non-zero entry");
    assert!(found.input_per_million > 0.0);
    assert_eq!(found.input_per_million, 0.95);
}
