use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn test_pricing_catalog_persistence_and_metadata() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-pricing-repo-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();
    let tenant_id = "test-tenant";

    // 1. Metadata test
    set_pricing_metadata_sqlx(pool, tenant_id, "usd_to_cny_rate", "7.23")
        .await
        .expect("set metadata");
    let val = get_pricing_metadata_sqlx(pool, tenant_id, "usd_to_cny_rate")
        .await
        .expect("get metadata");
    assert_eq!(val.as_deref(), Some("7.23"));

    // 2. Upsert entries
    let entry = ModelPriceEntry {
        provider: "anthropic".to_string(),
        model_patterns: vec![
            "claude-3-7-sonnet".to_string(),
            "claude-3-7-sonnet*".to_string(),
        ],
        currency: "CNY".to_string(),
        input_per_million: 21.75,
        cache_read_per_million: 2.175,
        cache_write_per_million: 27.1875,
        output_per_million: 108.75,
        reasoning_per_million: 108.75,
        raw_currency: Some("USD".to_string()),
        raw_input_per_million: Some(3.0),
        raw_output_per_million: Some(15.0),
        ..Default::default()
    };

    let count = upsert_pricing_catalog_entries_sqlx(pool, tenant_id, &[entry], "litellm")
        .await
        .expect("upsert entries");
    assert_eq!(count, 2);

    // 3. Load entries
    let loaded = load_pricing_catalog_entries_sqlx(pool, tenant_id)
        .await
        .expect("load entries");
    assert_eq!(loaded.len(), 2);
    let loaded_item = loaded
        .iter()
        .find(|i| i.model_patterns.contains(&"claude-3-7-sonnet".to_string()))
        .unwrap();
    assert_eq!(loaded_item.currency, "CNY");
    assert!((loaded_item.input_per_million - 21.75).abs() < 1e-4);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_recalculate_usage_costs() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-pricing-recalc-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();
    let tenant_id = "test-tenant";

    // Insert an unpriced event
    let raw_event = crate::backend::store::conversations::usage_repo::RawUsageEventInput {
        external_event_id: "evt-unpriced-1".to_string(),
        session_id: Some("sess-1".to_string()),
        turn_id: Some("turn-1".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-10-05T08:00:00Z".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-3-7-sonnet-20250219".to_string(),
        status: Some("success".to_string()),
        input_tokens: 1_000_000,
        output_tokens: 1_000_000,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: None,
        currency: None,
    };

    let inserted = crate::backend::store::conversations::usage_repo::upsert_usage_events_sqlx(
        pool,
        tenant_id,
        "source-1",
        "adapter-1",
        &[raw_event],
        None,
    )
    .await
    .expect("insert usage event");
    assert_eq!(inserted, 1);

    // Now recalculate with a catalog that knows claude-3-7-sonnet
    let catalog = PriceCatalog::bundled_with_rate(7.0);
    let updated = recalculate_usage_costs_sqlx(pool, tenant_id, &catalog)
        .await
        .expect("recalculate costs");
    assert_eq!(updated, 1);

    // Verify event in database has CNY cost
    let row = sqlx::query(
        "SELECT catalog_estimated_cost, cost_basis, currency FROM conversation_usage_events WHERE tenant_id = ? AND external_event_id = ?"
    )
    .bind(tenant_id)
    .bind("evt-unpriced-1")
    .fetch_one(pool)
    .await
    .expect("fetch updated row");

    let cost: Option<f64> = row.get("catalog_estimated_cost");
    let cost_basis: String = row.get("cost_basis");
    let currency: String = row.get("currency");

    assert_eq!(cost_basis, "catalog_estimated");
    assert_eq!(currency, "CNY");
    assert!(cost.is_some());
    // input 3.0 USD * 7.0 + output 15.0 USD * 7.0 = 21.0 + 105.0 = 126.0 CNY
    assert!((cost.unwrap() - 126.0).abs() < 1e-3);

    drop(database);
    std::fs::remove_file(path).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_recalculate_usage_costs_includes_zero_host_cost() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-pricing-recalc-zero-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();
    let tenant_id = "test-tenant";

    // Insert an event directly with cost_basis = 'host_reported' and host_reported_cost = 0.0
    sqlx::query(
        r#"
        INSERT INTO conversation_usage_events (
            tenant_id, source_id, adapter_id, external_event_id, session_id, turn_id,
            logical_request_id, attempt_index, timestamp, provider, model, status,
            input_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
            output_tokens, total_input_tokens, total_output_tokens, total_tokens,
            host_reported_cost, catalog_estimated_cost, cost_basis, currency,
            created_at, updated_at
        ) VALUES (
            ?, 'src-1', 'cliproxy', 'evt-zero-1', 's1', 't1',
            'r1', 0, '2026-10-05T08:00:00Z', 'cliproxy', 'ZhipuAI/GLM-5', 'success',
            1000000, 0, 0, 0, 1000000, 1000000, 1000000, 2000000,
            0.0, NULL, 'host_reported', 'USD', 'now', 'now'
        )
        "#,
    )
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("insert row");

    // Recalculate with bundled catalog
    let catalog = PriceCatalog::bundled();
    let updated = recalculate_usage_costs_sqlx(pool, tenant_id, &catalog)
        .await
        .expect("recalculate costs");
    assert_eq!(updated, 1);

    let row = sqlx::query(
        "SELECT catalog_estimated_cost, cost_basis FROM conversation_usage_events WHERE tenant_id = ? AND external_event_id = 'evt-zero-1'"
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .expect("fetch row");

    let cost: Option<f64> = row.get("catalog_estimated_cost");
    let cost_basis: String = row.get("cost_basis");

    assert_eq!(cost_basis, "catalog_estimated");
    assert!(cost.is_some());
    // GLM-5: 4.0 input + 12.0 output = 16.0 CNY
    assert!((cost.unwrap() - 16.0).abs() < 1e-4);

    drop(database);
    std::fs::remove_file(path).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_recalculate_usage_costs_batch_multi_model() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-pricing-batch-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();
    let tenant_id = "test-tenant-batch";

    // 1. Insert multiple claude events
    for (i, (in_tok, out_tok)) in [(100_000, 50_000), (200_000, 100_000), (0, 10_000)]
        .into_iter()
        .enumerate()
    {
        sqlx::query(
            r#"
            INSERT INTO conversation_usage_events (
                tenant_id, source_id, adapter_id, external_event_id, timestamp,
                provider, model, status, input_tokens, output_tokens,
                cost_basis, currency, created_at, updated_at
            ) VALUES (
                ?, 'src-1', 'cliproxy', ?, '2026-10-05T08:00:00Z',
                'anthropic', 'claude-3-7-sonnet', 'success', ?, ?,
                'none', 'CNY', 'now', 'now'
            )
            "#,
        )
        .bind(tenant_id)
        .bind(format!("evt-claude-{i}"))
        .bind(in_tok)
        .bind(out_tok)
        .execute(pool)
        .await
        .expect("insert claude event");
    }

    // 2. Insert unknown model event (not in catalog)
    sqlx::query(
        r#"
        INSERT INTO conversation_usage_events (
            tenant_id, source_id, adapter_id, external_event_id, timestamp,
            provider, model, status, input_tokens, output_tokens,
            cost_basis, currency, created_at, updated_at
        ) VALUES (
            ?, 'src-1', 'cliproxy', 'evt-unknown-1', '2026-10-05T08:00:00Z',
            'unknown-provider', 'unknown-model-v99', 'success', 100000, 100000,
            'catalog_estimated', 'CNY', 'now', 'now'
        )
        "#,
    )
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("insert unknown event");

    // 3. Insert host-reported event with positive cost (should be untouched)
    sqlx::query(
        r#"
        INSERT INTO conversation_usage_events (
            tenant_id, source_id, adapter_id, external_event_id, timestamp,
            provider, model, status, input_tokens, output_tokens,
            host_reported_cost, cost_basis, currency, created_at, updated_at
        ) VALUES (
            ?, 'src-1', 'cliproxy', 'evt-host-billed', '2026-10-05T08:00:00Z',
            'anthropic', 'claude-3-7-sonnet', 'success', 1000000, 1000000,
            88.88, 'host_reported', 'USD', 'now', 'now'
        )
        "#,
    )
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("insert host billed event");

    // Run recalculation with rate = 7.0
    let catalog = PriceCatalog::bundled_with_rate(7.0);
    let updated = recalculate_usage_costs_sqlx(pool, tenant_id, &catalog)
        .await
        .expect("recalculate costs");

    // 3 claude events + 1 unknown event = 4 updated (host-billed event skipped)
    assert_eq!(updated, 4);

    // Verify Claude 0: 100k input * 21.0/M + 50k output * 105.0/M = 2.1 + 5.25 = 7.35 CNY
    let row0 = sqlx::query(
        "SELECT catalog_estimated_cost, cost_basis FROM conversation_usage_events WHERE tenant_id = ? AND external_event_id = 'evt-claude-0'"
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .expect("fetch claude-0");
    let c0: f64 = row0.get("catalog_estimated_cost");
    assert!((c0 - 7.35).abs() < 1e-4);
    assert_eq!(row0.get::<String, _>("cost_basis"), "catalog_estimated");

    // Verify unknown model: cost is NULL, cost_basis is 'none'
    let row_unk = sqlx::query(
        "SELECT catalog_estimated_cost, cost_basis FROM conversation_usage_events WHERE tenant_id = ? AND external_event_id = 'evt-unknown-1'"
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .expect("fetch unk");
    let c_unk: Option<f64> = row_unk.get("catalog_estimated_cost");
    assert!(c_unk.is_none());
    assert_eq!(row_unk.get::<String, _>("cost_basis"), "none");

    // Verify host billed is untouched
    let row_host = sqlx::query(
        "SELECT host_reported_cost, catalog_estimated_cost, cost_basis FROM conversation_usage_events WHERE tenant_id = ? AND external_event_id = 'evt-host-billed'"
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .expect("fetch host");
    let host_cost: f64 = row_host.get("host_reported_cost");
    assert!((host_cost - 88.88).abs() < 1e-4);
    assert_eq!(row_host.get::<String, _>("cost_basis"), "host_reported");

    drop(database);
    std::fs::remove_file(path).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_migration_adds_lock_and_aliases_and_cleans_dirty_rows() {
    use sqlx::sqlite::SqlitePoolOptions;

    let path = std::env::temp_dir().join(format!(
        "assetiweave-pricing-migration-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));

    // 1. Verify full Database::open_initialized_async applies the migration cleanly
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open migrated database");
    let cols = sqlx::query("PRAGMA table_info(model_pricing_catalog)")
        .fetch_all(database.pool())
        .await
        .expect("pragma table_info");
    let col_names: Vec<String> = cols.iter().map(|r| r.get::<String, _>("name")).collect();
    assert!(col_names.contains(&"price_locked".to_string()));
    assert!(col_names.contains(&"usage_aliases".to_string()));
    drop(database);
    std::fs::remove_file(&path).ok();

    // 2. Verify data backfill & dirty-row cleanup on pre-migration schema
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect memory sqlite");

    let initial_sql = include_str!("../../../migrations/20261005000001_model_pricing_catalog.sql");
    sqlx::raw_sql(initial_sql)
        .execute(&pool)
        .await
        .expect("apply initial pricing table migration");

    let seed_rows = [
        ("user_override", "gpt-4o"),
        ("user_override", "{custom-override}"),
        ("litellm", "claude-3-7-sonnet"),
        ("litellm", r#"dirty"quote"#),
        ("litellm", "dirty'single"),
        ("litellm", "dirty{brace"),
        ("litellm", "dirty}brace"),
        ("litellm", "dirty\nnewline"),
        ("litellm", "dirty\rcr"),
    ];
    for (channel, model_id) in seed_rows {
        sqlx::query(
            r#"
            INSERT INTO model_pricing_catalog (
                tenant_id, model_id, provider, currency,
                input_per_million, cache_read_per_million, cache_write_per_million,
                reasoning_per_million, output_per_million,
                raw_currency, raw_input_per_million, raw_output_per_million,
                source_channel, created_at, updated_at
            ) VALUES ('t1', ?, 'openai', 'CNY', 1.0, 0.0, 0.0, 0.0, 2.0, 'USD', 0.1, 0.2, ?, 'now', 'now')
            "#,
        )
        .bind(model_id)
        .bind(channel)
        .execute(&pool)
        .await
        .expect("seed row");
    }

    let migration_sql =
        include_str!("../../../migrations/20261116000001_model_pricing_lock_and_aliases.sql");
    sqlx::raw_sql(migration_sql)
        .execute(&pool)
        .await
        .expect("apply lock & aliases migration");

    let rows = sqlx::query(
        "SELECT model_id, source_channel, price_locked, usage_aliases FROM model_pricing_catalog ORDER BY model_id",
    )
    .fetch_all(&pool)
    .await
    .expect("query migrated rows");

    let actual: Vec<(String, String, i64, String)> = rows
        .into_iter()
        .map(|r| {
            (
                r.get("model_id"),
                r.get("source_channel"),
                r.get("price_locked"),
                r.get("usage_aliases"),
            )
        })
        .collect();

    assert_eq!(
        actual,
        vec![
            (
                "claude-3-7-sonnet".to_string(),
                "litellm".to_string(),
                0,
                "[]".to_string()
            ),
            (
                "gpt-4o".to_string(),
                "user_override".to_string(),
                1,
                "[]".to_string()
            ),
            (
                "{custom-override}".to_string(),
                "user_override".to_string(),
                1,
                "[]".to_string()
            ),
        ]
    );
}
