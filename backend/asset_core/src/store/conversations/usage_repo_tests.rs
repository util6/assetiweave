use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn test_usage_events_ingest_and_dashboard_query() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-usage-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();

    let event1 = RawUsageEventInput {
        external_event_id: "evt-1".to_string(),
        session_id: Some("th-1".to_string()),
        turn_id: Some("msg-1".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-09-14T08:00:00Z".to_string(),
        provider: "openai".to_string(),
        model: "gpt-4o".to_string(),
        status: Some("success".to_string()),
        input_tokens: 1000,
        output_tokens: 500,
        cache_read_tokens: 200,
        cache_write_tokens: 50,
        reasoning_tokens: 100,
        host_reported_cost: None,
        currency: Some("USD".to_string()),
    };

    let event2 = RawUsageEventInput {
        external_event_id: "evt-2".to_string(),
        session_id: Some("th-2".to_string()),
        turn_id: Some("msg-2".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-09-14T09:00:00Z".to_string(),
        provider: "deepseek".to_string(),
        model: "deepseek-chat".to_string(),
        status: Some("success".to_string()),
        input_tokens: 2000,
        output_tokens: 1000,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: Some(0.015),
        currency: Some("CNY".to_string()),
    };

    // 1. Ingest events
    let inserted = upsert_usage_events_sqlx(
        pool,
        "default",
        "codex-src-1",
        "codex",
        &[event1.clone(), event2.clone()],
        None,
    )
    .await
    .expect("insert usage events");
    assert_eq!(inserted, 2);

    // 2. Duplicate ingestion should update without error or duplicate count
    let inserted_dupe = upsert_usage_events_sqlx(
        pool,
        "default",
        "codex-src-1",
        "codex",
        &[event1.clone()],
        None,
    )
    .await
    .expect("upsert usage event");
    assert_eq!(inserted_dupe, 1);

    // 3. Record source state
    let state = UsageSourceState {
        tenant_id: "default".to_string(),
        source_id: "codex-src-1".to_string(),
        adapter_id: "codex".to_string(),
        decoder_profile: Some("rollout".to_string()),
        schema_fingerprint: None,
        opaque_cursor: Some("cur-1".to_string()),
        last_success_scan_at: Some("2026-09-14T09:30:00Z".to_string()),
        last_full_scan_at: Some("2026-09-14T09:30:00Z".to_string()),
        status: "ready".to_string(),
        diagnostics_json: None,
        created_at: "2026-09-14T00:00:00Z".to_string(),
        updated_at: "2026-09-14T09:30:00Z".to_string(),
    };
    save_usage_source_state_sqlx(pool, &state)
        .await
        .expect("record source state");

    let states = load_all_usage_source_states_sqlx(pool, "default")
        .await
        .expect("load source states");
    assert_eq!(states.len(), 1);
    assert_eq!(states[0].source_id, "codex-src-1");
    assert_eq!(states[0].opaque_cursor.as_deref(), Some("cur-1"));

    // 4. Query Dashboard
    let filter = UsageDashboardFilter {
        time_range: Some("all_time".to_string()),
        start_date: None,
        end_date: None,
        adapter_id: None,
        source_id: None,
        model: None,
        timezone_offset_minutes: Some(0),
    };
    let dashboard = query_usage_dashboard_sqlx(pool, "default", &filter)
        .await
        .expect("query dashboard");

    assert_eq!(dashboard.hero.total_tokens, 4850);
    assert_eq!(dashboard.hero.requests_count, 2);
    assert_eq!(dashboard.models.len(), 2);
    assert_eq!(dashboard.sources.len(), 1);

    // Universal currency presentation check (CNY)
    let cny = dashboard
        .hero
        .costs_by_currency
        .iter()
        .find(|c| c.currency == "CNY");
    assert!(cny.is_some(), "CNY cost must be present");
    assert_eq!(cny.unwrap().amount, 0.079);

    drop(database);
    std::fs::remove_file(path).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zero_host_cost_falls_back_to_catalog_estimation() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-usage-zero-cost-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();

    // Event with host_reported_cost: Some(0.0) on a promotional model
    let promo_event = RawUsageEventInput {
        external_event_id: "evt-free-1".to_string(),
        session_id: Some("th-free".to_string()),
        turn_id: Some("msg-free".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-09-14T10:00:00Z".to_string(),
        provider: "xiaomi".to_string(),
        model: "mimo-v2.5-free".to_string(),
        status: Some("success".to_string()),
        input_tokens: 1_000_000,
        output_tokens: 1_000_000,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: Some(0.0), // Adapter reported 0.0
        currency: Some("USD".to_string()),
    };

    let inserted = upsert_usage_events_sqlx(
        pool,
        "default",
        "opencode-1",
        "opencode",
        &[promo_event],
        None,
    )
    .await
    .expect("insert event");
    assert_eq!(inserted, 1);

    // Verify in database that cost_basis is catalog_estimated (not host_reported) and cost > 0
    let row = sqlx::query(
        "SELECT catalog_estimated_cost, cost_basis, host_reported_cost FROM conversation_usage_events WHERE external_event_id = 'evt-free-1'"
    )
    .fetch_one(pool)
    .await
    .expect("fetch event row");

    let cost_basis: String = row.get("cost_basis");
    let catalog_cost: Option<f64> = row.get("catalog_estimated_cost");
    let host_cost: Option<f64> = row.get("host_reported_cost");

    assert_eq!(cost_basis, "catalog_estimated");
    assert!(catalog_cost.is_some());
    assert!(catalog_cost.unwrap() > 0.0);
    assert_eq!(host_cost, None);

    // Query dashboard to verify display
    let filter = UsageDashboardFilter {
        time_range: Some("all_time".to_string()),
        start_date: None,
        end_date: None,
        adapter_id: None,
        source_id: None,
        model: None,
        timezone_offset_minutes: Some(0),
    };
    let dashboard = query_usage_dashboard_sqlx(pool, "default", &filter)
        .await
        .expect("query dashboard");

    let model = dashboard
        .models
        .iter()
        .find(|m| m.model == "mimo-v2.5-free");
    assert!(model.is_some());
    assert!(model.unwrap().cost.is_some());
    assert!(model.unwrap().cost.unwrap() > 0.0);

    drop(database);
    std::fs::remove_file(path).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_usage_dashboard_custom_date_range_bounded() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-usage-custom-date-range-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open db");
    let pool = database.pool();

    // Insert 3 usage events across different dates
    let event1 = RawUsageEventInput {
        external_event_id: "evt-1".to_string(),
        session_id: Some("s-1".to_string()),
        turn_id: Some("turn-1".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-03-01T12:00:00Z".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-3-opus".to_string(),
        status: Some("success".to_string()),
        input_tokens: 500,
        output_tokens: 500,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: Some(0.01),
        currency: Some("USD".to_string()),
    };
    let event2 = RawUsageEventInput {
        external_event_id: "evt-2".to_string(),
        session_id: Some("s-2".to_string()),
        turn_id: Some("turn-2".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-03-05T12:00:00Z".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-3-opus".to_string(),
        status: Some("success".to_string()),
        input_tokens: 1000,
        output_tokens: 1000,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: Some(0.02),
        currency: Some("USD".to_string()),
    };
    let event3 = RawUsageEventInput {
        external_event_id: "evt-3".to_string(),
        session_id: Some("s-3".to_string()),
        turn_id: Some("turn-3".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-03-15T12:00:00Z".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-3-opus".to_string(),
        status: Some("success".to_string()),
        input_tokens: 1500,
        output_tokens: 1500,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: Some(0.03),
        currency: Some("USD".to_string()),
    };

    upsert_usage_events_sqlx(
        pool,
        "default",
        "src-1",
        "opencode",
        &[event1, event2, event3],
        None,
    )
    .await
    .expect("insert usage events");

    // Query custom range: 2026-03-03 to 2026-03-07 (should only include 2026-03-05 event)
    let filter = UsageDashboardFilter {
        time_range: Some("custom".to_string()),
        start_date: Some("2026-03-03".to_string()),
        end_date: Some("2026-03-07".to_string()),
        adapter_id: None,
        source_id: None,
        model: None,
        timezone_offset_minutes: Some(0),
    };
    let dashboard = query_usage_dashboard_sqlx(pool, "default", &filter)
        .await
        .expect("query dashboard");

    assert_eq!(dashboard.hero.total_tokens, 2000);
    assert_eq!(dashboard.hero.requests_count, 1);

    // Verify daily buckets are strictly bounded between 2026-03-03 and 2026-03-07 (5 days)
    assert_eq!(dashboard.daily_trend.len(), 5);
    assert_eq!(dashboard.daily_trend.first().unwrap().date, "2026-03-03");
    assert_eq!(dashboard.daily_trend.last().unwrap().date, "2026-03-07");

    let march_5_bucket = dashboard
        .daily_trend
        .iter()
        .find(|b| b.date == "2026-03-05")
        .expect("march 5 bucket");
    assert_eq!(march_5_bucket.total_tokens, 2000);
    assert_eq!(march_5_bucket.requests_count, 1);

    let march_3_bucket = dashboard
        .daily_trend
        .iter()
        .find(|b| b.date == "2026-03-03")
        .expect("march 3 bucket");
    assert_eq!(march_3_bucket.total_tokens, 0);
    assert_eq!(march_3_bucket.requests_count, 0);

    drop(database);
    std::fs::remove_file(path).ok();
}
