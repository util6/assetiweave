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

    // Dual currency check
    let usd = dashboard
        .hero
        .costs_by_currency
        .iter()
        .find(|c| c.currency == "USD");
    let cny = dashboard
        .hero
        .costs_by_currency
        .iter()
        .find(|c| c.currency == "CNY");
    assert!(usd.is_some(), "USD cost must be present");
    assert!(cny.is_some(), "CNY cost must be present");
    assert_eq!(cny.unwrap().amount, 0.015);

    drop(database);
    std::fs::remove_file(path).ok();
}
