use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn test_get_model_pricing_catalog_bundled_fallback() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-pricing-app-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));

    let app_service = AppService::open_with_db_path(path)
        .await
        .expect("open test app service");

    let catalog = app_service
        .get_model_pricing_catalog()
        .await
        .expect("get model pricing catalog");

    assert!(!catalog.models.is_empty());
    assert!(catalog.usd_to_cny_rate > 0.0);

    // DeepSeek and GPT models should be present in bundled catalog
    let has_deepseek = catalog
        .models
        .iter()
        .any(|m| m.provider.eq_ignore_ascii_case("deepseek"));
    assert!(has_deepseek, "bundled catalog should contain deepseek");

    let status = app_service
        .get_model_pricing_status()
        .await
        .expect("get pricing status");

    assert!(status.models_count > 0);
    assert_eq!(status.active_channel, "bundled");
}
