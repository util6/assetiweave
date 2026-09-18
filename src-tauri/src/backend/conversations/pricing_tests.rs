use super::*;

#[test]
fn test_bundled_catalog_parses_successfully() {
    let catalog = PriceCatalog::bundled();
    assert!(!catalog.catalog_version.is_empty());
    assert!(!catalog.models.is_empty());
}

#[test]
fn test_gemini_37_flash_pricing_match() {
    let catalog = PriceCatalog::bundled();
    let calc = catalog
        .calculate_cost(
            "google",
            "gemini-3.7-flash",
            7500, // input
            4000, // cache read
            0,    // cache write
            1000, // reasoning
            1000, // output
        )
        .expect("should match gemini-3.7-flash");

    assert_eq!(calc.currency, "USD");
    assert_eq!(calc.catalog_version, "2026-09-01");
    // input: 7500 * 0.10 / 1M = 0.00075
    // cache_read: 4000 * 0.025 / 1M = 0.0001
    // reasoning: 1000 * 0.40 / 1M = 0.0004
    // output: 1000 * 0.40 / 1M = 0.0004
    // total: 0.00165
    assert!((calc.estimated_cost - 0.00165).abs() < 1e-6);
}

#[test]
fn test_models_prefix_stripping() {
    let catalog = PriceCatalog::bundled();
    let calc = catalog.calculate_cost("google", "models/gemini-2.0-flash", 1_000_000, 0, 0, 0, 0);
    assert!(calc.is_some());
    assert_eq!(calc.unwrap().estimated_cost, 0.10);
}

#[test]
fn test_cny_pricing_deepseek() {
    let catalog = PriceCatalog::bundled();
    let calc = catalog
        .calculate_cost("deepseek", "deepseek-chat", 1_000_000, 0, 0, 0, 1_000_000)
        .expect("should match deepseek-chat");
    assert_eq!(calc.currency, "CNY");
    assert_eq!(calc.estimated_cost, 3.00);
}

#[test]
fn test_unknown_model_returns_none() {
    let catalog = PriceCatalog::bundled();
    let calc = catalog.calculate_cost("custom", "my-private-llm-v1", 1000, 0, 0, 0, 1000);
    assert!(calc.is_none());
}
