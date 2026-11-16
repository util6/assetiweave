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
        ..Default::default()
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
        ..Default::default()
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

#[test]
fn test_catalog_key_prefix_constants_and_base_model_id_helper() {
    assert_eq!(OVERRIDE_MODEL_ID_PREFIX, "__override__::");
    assert_eq!(IGNORE_MODEL_ID_PREFIX, "__ignore__::");
    assert_eq!(
        base_model_id_from_catalog_key("__override__::gpt-4o"),
        "gpt-4o"
    );
    assert_eq!(base_model_id_from_catalog_key("gpt-4o"), "gpt-4o");
    assert_eq!(
        base_model_id_from_catalog_key("  __override__::claude-3-7-sonnet  "),
        "claude-3-7-sonnet"
    );
}

#[test]
fn test_dirty_model_name_guard_blocks_candidates_and_matching() {
    let dirty_samples = [
        r#"model "gpt-4o""#,
        "model 'gpt-4o'",
        "{gpt-4o}",
        "gpt-4o}",
        "gpt-4o\nextra",
        "gpt-4o\rextra",
        "https://openrouter.ai/api/v1/gpt-4o",
        "http://localhost:11434/v1",
        &"a".repeat(97),
        "one two three four five six seven eight nine",
    ];

    for sample in dirty_samples {
        assert!(
            is_dirty_model_name(sample),
            "expected dirty for sample: {sample:?}"
        );
        assert!(
            PriceCatalog::candidate_model_names(sample).is_empty(),
            "expected empty candidates for dirty sample: {sample:?}"
        );
    }

    assert!(!is_dirty_model_name("Gemini 3.8 Flash (High)"));
    assert!(!is_dirty_model_name("Flash -N"));
    assert!(!is_dirty_model_name(
        "openrouter/anthropic/claude-3-7-sonnet"
    ));
    assert!(!is_dirty_model_name("Claude 3.7 Sonnet (Effort: High)"));

    let catalog = PriceCatalog::bundled();
    assert!(catalog
        .find_model("openai", r#"called "gpt-4o" from error log"#)
        .is_none());
    assert!(catalog
        .find_model(
            "google",
            "https://generativelanguage.googleapis.com/gemini-2.5-pro"
        )
        .is_none());
}

#[test]
fn test_universal_structural_normalization_and_multipass_suffix_stripping() {
    for variant in [
        "Gemini 3.8 Flash (High)",
        "Gemini 3.8 Flash (Medium)",
        "Gemini 3.8 Flash (Low)",
        "Gemini 3.8 Flash (Minimal)",
        "Gemini 3.8 Flash (Thinking)",
        "Gemini 3.8 Flash (Reasoning)",
        "Gemini 3.8 Flash (Fast)",
        "Gemini 3.8 Flash (Default)",
        "Gemini 3.8 Flash (Auto)",
        "Gemini 3.8 Flash (XHigh)",
        "Gemini 3.8 Flash (Effort: High)",
    ] {
        let cands = PriceCatalog::candidate_model_names(variant);
        assert!(
            cands.contains(&"gemini-3.8-flash".to_string()),
            "expected gemini-3.8-flash in {cands:?} for {variant}"
        );
    }

    let flash_n_cands = PriceCatalog::candidate_model_names("Flash -N");
    assert!(flash_n_cands.contains(&"flash-n".to_string()));
    assert!(flash_n_cands.contains(&"flash".to_string()));

    let tail_free_cands = PriceCatalog::candidate_model_names("gemini-3.8-flash-tail-free");
    assert!(tail_free_cands.contains(&"gemini-3.8-flash-tail".to_string()));
    assert!(tail_free_cands.contains(&"gemini-3.8-flash".to_string()));

    let n_high_cands = PriceCatalog::candidate_model_names("gemini-3.8-flash-n (High)");
    assert!(n_high_cands.contains(&"gemini-3.8-flash-n".to_string()));
    assert!(n_high_cands.contains(&"gemini-3.8-flash".to_string()));

    let catalog = PriceCatalog::bundled();
    assert!(catalog
        .find_model("google", "Gemini 3.8 Flash (High)")
        .is_some());
    assert!(catalog
        .find_model("google", "Gemini 3.8 Flash -N (Medium)")
        .is_some());
    assert!(catalog
        .find_model("anthropic", "claude-3-7-sonnet-tail-free (Thinking)")
        .is_some());
    assert!(catalog.find_model("xiaomi", "mimo-v2.5-tail").is_some());
}

#[test]
fn test_dto_serde_backward_compatibility_and_camel_case() {
    use crate::backend::domain::conversations::usage::{
        ModelPriceEntryDto, UsageModelBreakdownDto,
    };

    let legacy_price_json = r#"{
        "provider": "openai",
        "modelPatterns": ["gpt-4o"],
        "currency": "CNY",
        "inputPerMillion": 18.125,
        "cacheReadPerMillion": 9.0,
        "cacheWritePerMillion": 18.125,
        "outputPerMillion": 72.5,
        "reasoningPerMillion": 72.5,
        "rawCurrency": "USD",
        "rawInputPerMillion": 2.5,
        "rawOutputPerMillion": 10.0
    }"#;
    let price_dto: ModelPriceEntryDto =
        serde_json::from_str(legacy_price_json).expect("deserialize legacy ModelPriceEntryDto");
    assert_eq!(price_dto.raw_model, "");
    assert_eq!(price_dto.matched_model_id, None);
    assert_eq!(price_dto.match_source, "");
    assert!(!price_dto.price_locked);
    assert!(!price_dto.is_ignored);
    assert!(!price_dto.is_manual_rebind);

    let serialized_price = serde_json::to_value(&price_dto).expect("serialize ModelPriceEntryDto");
    assert!(serialized_price.get("rawModel").is_some());
    assert!(serialized_price.get("matchedModelId").is_some());
    assert!(serialized_price.get("matchSource").is_some());
    assert!(serialized_price.get("priceLocked").is_some());
    assert!(serialized_price.get("isIgnored").is_some());
    assert!(serialized_price.get("isManualRebind").is_some());

    let legacy_breakdown_json = r#"{
        "model": "gpt-4o",
        "provider": "openai",
        "requestsCount": 10,
        "totalTokens": 1000,
        "inputTokens": 600,
        "outputTokens": 400,
        "reasoningTokens": 0,
        "cacheTokens": 0,
        "cost": 1.25,
        "currency": "CNY",
        "costBasis": "catalog_estimated"
    }"#;
    let breakdown_dto: UsageModelBreakdownDto = serde_json::from_str(legacy_breakdown_json)
        .expect("deserialize legacy UsageModelBreakdownDto");
    assert!(breakdown_dto.raw_models.is_empty());
    assert_eq!(breakdown_dto.matched_model_id, None);

    let serialized_breakdown =
        serde_json::to_value(&breakdown_dto).expect("serialize UsageModelBreakdownDto");
    assert!(serialized_breakdown.get("rawModels").is_some());
    assert!(serialized_breakdown.get("matchedModelId").is_some());
}

#[test]
fn test_resolve_model_tier1_explicit_ignore_even_dirty_and_zero_cost() {
    let ignored_entry = ModelPriceEntry {
        provider: "openai".to_string(),
        model_patterns: vec!["__ignore__::deadbeef".to_string()],
        currency: "CNY".to_string(),
        input_per_million: 0.0,
        cache_read_per_million: 0.0,
        cache_write_per_million: 0.0,
        reasoning_per_million: 0.0,
        output_per_million: 0.0,
        price_locked: true,
        usage_aliases: vec![r#"dirty "gpt-4o" log"#.to_string()],
        source_channel: "user_override".to_string(),
        is_ignored: true,
        ..Default::default()
    };

    let catalog = PriceCatalog::new("v1".to_string(), "".to_string(), 7.25, vec![ignored_entry]);
    let resolved = catalog
        .resolve_model("openai", r#"  DIRTY "GPT-4O" LOG  "#)
        .expect("ignored entry must resolve in tier 1");
    assert_eq!(resolved.match_source, MatchSource::Ignored);
    assert!(resolved.price_locked);
    assert!(resolved.is_ignored);
    assert_eq!(resolved.entry.input_per_million, 0.0);

    let calc = catalog
        .calculate_cost(
            "openai",
            r#"dirty "gpt-4o" log"#,
            1_000_000,
            0,
            0,
            0,
            1_000_000,
        )
        .expect("ignored model should calculate 0.0 cost");
    assert_eq!(calc.estimated_cost, 0.0);
    assert_eq!(calc.currency, "CNY");
}

#[test]
fn test_resolve_model_tier2_manual_rebind_with_and_without_locked_override() {
    let ref_entry = ModelPriceEntry {
        provider: "anthropic".to_string(),
        model_patterns: vec!["claude-3-7-sonnet".to_string()],
        currency: "CNY".to_string(),
        input_per_million: 21.75,
        output_per_million: 108.75,
        price_locked: false,
        usage_aliases: vec![
            "weird-local-sonnet".to_string(),
            "dirty {alias}".to_string(),
        ],
        source_channel: "litellm".to_string(),
        is_ignored: false,
        ..Default::default()
    };

    let catalog_without_lock = PriceCatalog::new(
        "v1".to_string(),
        "".to_string(),
        7.25,
        vec![ref_entry.clone()],
    );
    let r1 = catalog_without_lock
        .resolve_model("custom", "  WEIRD-LOCAL-SONNET ")
        .expect("tier 2 manual rebind should match");
    assert_eq!(r1.match_source, MatchSource::ManualRebind);
    assert_eq!(r1.matched_model_id, "claude-3-7-sonnet");
    assert!(!r1.price_locked);
    assert!(!r1.is_ignored);
    assert!((r1.entry.input_per_million - 21.75).abs() < 1e-6);

    let r_dirty = catalog_without_lock
        .resolve_model("custom", "dirty {alias}")
        .expect("tier 2 manual rebind should work even for dirty raw string");
    assert_eq!(r_dirty.match_source, MatchSource::ManualRebind);
    assert_eq!(r_dirty.matched_model_id, "claude-3-7-sonnet");

    let locked_override = ModelPriceEntry {
        provider: "anthropic".to_string(),
        model_patterns: vec!["__override__::claude-3-7-sonnet".to_string()],
        currency: "CNY".to_string(),
        input_per_million: 9.5,
        output_per_million: 19.0,
        price_locked: true,
        source_channel: "user_override".to_string(),
        is_ignored: false,
        ..Default::default()
    };
    let catalog_with_lock = PriceCatalog::new(
        "v1".to_string(),
        "".to_string(),
        7.25,
        vec![ref_entry, locked_override],
    );
    let r2 = catalog_with_lock
        .resolve_model("custom", "weird-local-sonnet")
        .expect("tier 2 should overlay locked override");
    assert_eq!(r2.match_source, MatchSource::ManualRebind);
    assert_eq!(r2.matched_model_id, "claude-3-7-sonnet");
    assert!(r2.price_locked);
    assert!((r2.entry.input_per_million - 9.5).abs() < 1e-6);
}

#[test]
fn test_resolve_model_tier3_self_locked_override_allows_zero_price() {
    let locked_free = ModelPriceEntry {
        provider: "xiaomi".to_string(),
        model_patterns: vec!["__override__::mimo-v2.5-free".to_string()],
        currency: "CNY".to_string(),
        input_per_million: 0.0,
        output_per_million: 0.0,
        price_locked: true,
        source_channel: "user_override".to_string(),
        is_ignored: false,
        ..Default::default()
    };

    let catalog = PriceCatalog::new("v1".to_string(), "".to_string(), 7.25, vec![locked_free]);
    let resolved = catalog
        .resolve_model("opencode", "mimo-v2.5-free")
        .expect("locked 0.0 override must resolve in tier 3");
    assert_eq!(resolved.match_source, MatchSource::UserOverride);
    assert_eq!(resolved.matched_model_id, "mimo-v2.5-free");
    assert!(resolved.price_locked);
    assert!(!resolved.is_ignored);
    assert_eq!(resolved.entry.input_per_million, 0.0);

    let calc = catalog
        .calculate_cost("opencode", "mimo-v2.5-free", 1_000_000, 0, 0, 0, 1_000_000)
        .expect("locked 0.0 override must calculate 0.0 cost");
    assert_eq!(calc.estimated_cost, 0.0);
}

#[test]
fn test_resolve_model_tier4_normalization_and_locked_overlay_and_tier5_none() {
    let non_locked_zero = ModelPriceEntry {
        provider: "google".to_string(),
        model_patterns: vec!["gemini-3.8-flash-free".to_string()],
        currency: "CNY".to_string(),
        input_per_million: 0.0,
        output_per_million: 0.0,
        price_locked: false,
        source_channel: "cache".to_string(),
        is_ignored: false,
        ..Default::default()
    };
    let locked_override = ModelPriceEntry {
        provider: "google".to_string(),
        model_patterns: vec!["__override__::gemini-3.8-flash".to_string()],
        currency: "CNY".to_string(),
        input_per_million: 0.50,
        output_per_million: 1.00,
        price_locked: true,
        source_channel: "user_override".to_string(),
        is_ignored: false,
        ..Default::default()
    };

    let catalog = PriceCatalog::new(
        "v1".to_string(),
        "".to_string(),
        7.25,
        vec![non_locked_zero, locked_override],
    );

    let resolved = catalog
        .resolve_model("google", "Gemini 3.8 Flash (High)")
        .expect("tier 4 should match normalized gemini-3.8-flash and overlay lock");
    assert_eq!(resolved.match_source, MatchSource::AutoMatched);
    assert_eq!(resolved.matched_model_id, "gemini-3.8-flash");
    assert!(resolved.price_locked);
    assert!((resolved.entry.input_per_million - 0.50).abs() < 1e-6);

    // Consistency with find_model / calculate_cost
    let found = catalog
        .find_model("google", "Gemini 3.8 Flash (High)")
        .expect("find_model must delegate consistently");
    assert!((found.input_per_million - 0.50).abs() < 1e-6);

    let calc = catalog
        .calculate_cost("google", "Gemini 3.8 Flash (High)", 1_000_000, 0, 0, 0, 0)
        .expect("calculate_cost");
    assert!((calc.estimated_cost - 0.50).abs() < 1e-6);

    // Tier 5: unmatched and dirty
    assert!(catalog
        .resolve_model("custom", "totally-unknown-llm")
        .is_none());
    assert!(catalog
        .resolve_model("google", r#"{"model": "gemini-3.8-flash"}"#)
        .is_none());
}
