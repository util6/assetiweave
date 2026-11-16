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
    assert_eq!(
        count, 1,
        "non-user_override upsert must write only primary base model_id"
    );

    // 3. Load entries
    let loaded = load_pricing_catalog_entries_sqlx(pool, tenant_id)
        .await
        .expect("load entries");
    assert_eq!(loaded.len(), 1);
    let loaded_item = &loaded[0];
    assert_eq!(loaded_item.model_patterns, vec!["claude-3-7-sonnet"]);
    assert_eq!(loaded_item.currency, "CNY");
    assert_eq!(loaded_item.source_channel, "litellm");
    assert!(!loaded_item.price_locked);
    assert!(!loaded_item.is_ignored);
    assert!(loaded_item.usage_aliases.is_empty());
    assert!((loaded_item.input_per_million - 21.75).abs() < 1e-4);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_upsert_non_overwrite_locked_and_override_and_loads_metadata_columns() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-pricing-non-overwrite-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();
    let tenant_id = "test-tenant";

    // Seed:
    // 1. Unlocked reference row with existing usage_aliases
    // 2. Locked reference row (price_locked = 1)
    // 3. User override row (source_channel = 'user_override', price_locked = 1, 0.0 price)
    // 4. Zero-priced row with non-empty usage_aliases
    // 5. Ignored row (__ignore__::deadbeef)
    sqlx::query(
        r#"
        INSERT INTO model_pricing_catalog (
            tenant_id, model_id, provider, currency,
            input_per_million, cache_read_per_million, cache_write_per_million,
            reasoning_per_million, output_per_million,
            raw_currency, raw_input_per_million, raw_output_per_million,
            source_channel, price_locked, usage_aliases, created_at, updated_at
        ) VALUES
            (?, 'gpt-4o', 'openai', 'CNY', 10.0, 1.0, 1.0, 1.0, 20.0, 'USD', 1.4, 2.8, 'bundled', 0, '["gpt4o-alias"]', 'now', 'now'),
            (?, 'deepseek-chat', 'deepseek', 'CNY', 5.0, 0.5, 0.5, 0.5, 9.0, 'CNY', 5.0, 9.0, 'litellm', 1, '["ds-alias"]', 'now', 'now'),
            (?, '__override__::mimo-v2.5-free', 'xiaomi', 'CNY', 0.0, 0.0, 0.0, 0.0, 0.0, 'CNY', 0.0, 0.0, 'user_override', 1, '[]', 'now', 'now'),
            (?, 'zero-with-alias', 'custom', 'CNY', 0.0, 0.0, 0.0, 0.0, 0.0, 'CNY', 0.0, 0.0, 'bundled', 0, '["custom-raw"]', 'now', 'now'),
            (?, '__ignore__::deadbeef', 'custom', 'CNY', 0.0, 0.0, 0.0, 0.0, 0.0, 'CNY', 0.0, 0.0, 'user_override', 1, '["dirty \"model\""]', 'now', 'now')
        "#,
    )
    .bind(tenant_id)
    .bind(tenant_id)
    .bind(tenant_id)
    .bind(tenant_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("seed rows");

    let sync_entries = vec![
        ModelPriceEntry {
            provider: "openai".to_string(),
            model_patterns: vec!["gpt-4o".to_string(), "gpt-4o*".to_string()],
            currency: "CNY".to_string(),
            input_per_million: 18.125,
            output_per_million: 72.5,
            ..Default::default()
        },
        ModelPriceEntry {
            provider: "deepseek".to_string(),
            model_patterns: vec!["deepseek-chat".to_string()],
            currency: "CNY".to_string(),
            input_per_million: 99.0,
            output_per_million: 199.0,
            ..Default::default()
        },
        ModelPriceEntry {
            provider: "xiaomi".to_string(),
            model_patterns: vec!["__override__::mimo-v2.5-free".to_string()],
            currency: "CNY".to_string(),
            input_per_million: 12.0,
            output_per_million: 24.0,
            ..Default::default()
        },
        ModelPriceEntry {
            provider: "anthropic".to_string(),
            model_patterns: vec!["claude-3-7-sonnet".to_string()],
            currency: "CNY".to_string(),
            input_per_million: 21.75,
            output_per_million: 108.75,
            ..Default::default()
        },
    ];

    let affected = upsert_pricing_catalog_entries_sqlx(pool, tenant_id, &sync_entries, "litellm")
        .await
        .expect("sync upsert");
    assert_eq!(
        affected, 2,
        "only unlocked gpt-4o and new claude-3-7-sonnet should count as affected"
    );

    let loaded = load_pricing_catalog_entries_sqlx(pool, tenant_id)
        .await
        .expect("load entries");
    assert_eq!(loaded.len(), 6);

    let gpt4o = loaded
        .iter()
        .find(|e| e.model_patterns == vec!["gpt-4o"])
        .expect("gpt-4o");
    assert!((gpt4o.input_per_million - 18.125).abs() < 1e-4);
    assert_eq!(gpt4o.source_channel, "litellm");
    assert!(!gpt4o.price_locked);
    assert_eq!(gpt4o.usage_aliases, vec!["gpt4o-alias"]);

    let ds = loaded
        .iter()
        .find(|e| e.model_patterns == vec!["deepseek-chat"])
        .expect("deepseek-chat");
    assert!(
        (ds.input_per_million - 5.0).abs() < 1e-4,
        "locked row must not be overwritten"
    );
    assert!(ds.price_locked);
    assert_eq!(ds.usage_aliases, vec!["ds-alias"]);

    let zero_alias = loaded
        .iter()
        .find(|e| e.model_patterns == vec!["zero-with-alias"])
        .expect("zero-priced row with usage_aliases must be loaded");
    assert_eq!(zero_alias.usage_aliases, vec!["custom-raw"]);

    let ignored = loaded
        .iter()
        .find(|e| e.model_patterns == vec!["__ignore__::deadbeef"])
        .expect("ignored row must be loaded");
    assert!(ignored.is_ignored);
    assert!(ignored.price_locked);
    assert_eq!(ignored.source_channel, "user_override");

    drop(database);
    std::fs::remove_file(path).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_locked_override_row_separation_unlock_and_zero_recalc() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-pricing-lock-sep-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();
    let tenant_id = "test-tenant";

    // Seed native reference row with an alias AND a legacy unprefixed user_override row
    sqlx::query(
        r#"
        INSERT INTO model_pricing_catalog (
            tenant_id, model_id, provider, currency,
            input_per_million, cache_read_per_million, cache_write_per_million,
            reasoning_per_million, output_per_million,
            raw_currency, raw_input_per_million, raw_output_per_million,
            source_channel, price_locked, usage_aliases, created_at, updated_at
        ) VALUES
            (?, 'claude-3-7-sonnet', 'anthropic', 'CNY', 21.75, 2.0, 27.0, 108.75, 108.75, 'USD', 3.0, 15.0, 'litellm', 0, '["custom-sonnet"]', 'now', 'now'),
            (?, 'legacy-model', 'custom', 'CNY', 10.0, 0.0, 0.0, 20.0, 20.0, 'CNY', 10.0, 20.0, 'user_override', 1, '[]', 'now', 'now')
        "#,
    )
    .bind(tenant_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("seed reference and legacy rows");

    // Seed an unpriced usage event for claude-3-7-sonnet
    sqlx::query(
        r#"
        INSERT INTO conversation_usage_events (
            tenant_id, source_id, adapter_id, external_event_id, timestamp,
            provider, model, status, input_tokens, output_tokens,
            cost_basis, currency, created_at, updated_at
        ) VALUES (
            ?, 'src-1', 'claude', 'evt-lock-1', '2026-10-05T08:00:00Z',
            'anthropic', 'claude-3-7-sonnet', 'success', 1000000, 1000000,
            'none', 'CNY', 'now', 'now'
        )
        "#,
    )
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("seed usage event");

    // 1. Save locked ¥0 override -> must create __override__::claude-3-7-sonnet and preserve reference row
    let zero_override = ModelPriceEntry {
        provider: "anthropic".to_string(),
        model_patterns: vec!["claude-3-7-sonnet".to_string()],
        currency: "CNY".to_string(),
        input_per_million: 0.0,
        output_per_million: 0.0,
        raw_currency: Some("CNY".to_string()),
        raw_input_per_million: Some(0.0),
        raw_output_per_million: Some(0.0),
        ..Default::default()
    };
    save_locked_override_sqlx(pool, tenant_id, "claude-3-7-sonnet", &zero_override)
        .await
        .expect("save locked override");

    let loaded = load_pricing_catalog_entries_sqlx(pool, tenant_id)
        .await
        .expect("load entries");
    let ref_row = loaded
        .iter()
        .find(|e| e.model_patterns == vec!["claude-3-7-sonnet"])
        .expect("native reference row preserved");
    assert!(!ref_row.price_locked);
    assert_eq!(ref_row.source_channel, "litellm");
    assert!((ref_row.input_per_million - 21.75).abs() < 1e-4);
    assert_eq!(ref_row.usage_aliases, vec!["custom-sonnet"]);

    let ov_row = loaded
        .iter()
        .find(|e| e.model_patterns == vec!["__override__::claude-3-7-sonnet"])
        .expect("override row created");
    assert!(ov_row.price_locked);
    assert_eq!(ov_row.source_channel, "user_override");
    assert_eq!(ov_row.input_per_million, 0.0);

    // 2. Recalculate with DB-loaded catalog -> locked ¥0 must become 0.0 + catalog_estimated
    let catalog = PriceCatalog::new("v1".into(), "now".into(), 7.25, loaded);
    let recalc_count = recalculate_usage_costs_sqlx(pool, tenant_id, &catalog)
        .await
        .expect("recalc zero locked");
    assert_eq!(recalc_count, 1);

    let row = sqlx::query(
        "SELECT catalog_estimated_cost, cost_basis FROM conversation_usage_events WHERE tenant_id = ? AND external_event_id = 'evt-lock-1'"
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .expect("fetch evt");
    assert_eq!(row.get::<String, _>("cost_basis"), "catalog_estimated");
    assert_eq!(
        row.get::<Option<f64>, _>("catalog_estimated_cost"),
        Some(0.0)
    );

    // 3. Unlock claude-3-7-sonnet -> removes __override__::claude-3-7-sonnet, falls back to 21.75/108.75
    set_model_pricing_lock_sqlx(pool, tenant_id, "claude-3-7-sonnet", false, None)
        .await
        .expect("unlock claude");
    let loaded_after_unlock = load_pricing_catalog_entries_sqlx(pool, tenant_id)
        .await
        .expect("load after unlock");
    assert!(!loaded_after_unlock
        .iter()
        .any(|e| e.model_patterns == vec!["__override__::claude-3-7-sonnet"]));

    let catalog_unlocked = PriceCatalog::new("v1".into(), "now".into(), 7.25, loaded_after_unlock);
    recalculate_usage_costs_sqlx(pool, tenant_id, &catalog_unlocked)
        .await
        .expect("recalc unlocked");
    let row_unlocked = sqlx::query(
        "SELECT catalog_estimated_cost, cost_basis FROM conversation_usage_events WHERE tenant_id = ? AND external_event_id = 'evt-lock-1'"
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .expect("fetch evt unlocked");
    assert_eq!(
        row_unlocked.get::<String, _>("cost_basis"),
        "catalog_estimated"
    );
    assert!(
        (row_unlocked
            .get::<Option<f64>, _>("catalog_estimated_cost")
            .unwrap()
            - 130.5)
            .abs()
            < 1e-4
    );

    // 4. Lock from existing DB reference row (fallback = None) and lock from bundled fallback (fallback = Some)
    set_model_pricing_lock_sqlx(pool, tenant_id, "claude-3-7-sonnet", true, None)
        .await
        .expect("re-lock from db reference");
    let gpt_fallback = ModelPriceEntry {
        provider: "openai".to_string(),
        model_patterns: vec!["gpt-4o".to_string()],
        currency: "CNY".to_string(),
        input_per_million: 18.125,
        output_per_million: 72.5,
        ..Default::default()
    };
    set_model_pricing_lock_sqlx(pool, tenant_id, "gpt-4o", true, Some(&gpt_fallback))
        .await
        .expect("lock with fallback");

    // 5. Unlock legacy unprefixed user_override row -> clears price_locked = 0
    set_model_pricing_lock_sqlx(pool, tenant_id, "legacy-model", false, None)
        .await
        .expect("unlock legacy row");
    let loaded_final = load_pricing_catalog_entries_sqlx(pool, tenant_id)
        .await
        .expect("load final");
    assert!(loaded_final
        .iter()
        .any(|e| e.model_patterns == vec!["__override__::claude-3-7-sonnet"] && e.price_locked));
    assert!(loaded_final
        .iter()
        .any(|e| e.model_patterns == vec!["__override__::gpt-4o"] && e.price_locked));
    let legacy = loaded_final
        .iter()
        .find(|e| e.model_patterns == vec!["legacy-model"])
        .expect("legacy row remains with price_locked=0");
    assert!(!legacy.price_locked);

    drop(database);
    std::fs::remove_file(path).ok();
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

#[tokio::test(flavor = "multi_thread")]
async fn test_rebind_model_pricing_alias_materializes_bundled_moves_and_unbinds() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-pricing-rebind-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();
    let tenant_id = "test-tenant";

    let bundled_gpt4o = ModelPriceEntry {
        provider: "openai".to_string(),
        model_patterns: vec!["gpt-4o".to_string(), "gpt-4o-2024*".to_string()],
        currency: "CNY".to_string(),
        input_per_million: 18.125,
        output_per_million: 72.5,
        raw_currency: Some("USD".to_string()),
        raw_input_per_million: Some(2.5),
        raw_output_per_million: Some(10.0),
        source_channel: "bundled".to_string(),
        ..Default::default()
    };

    // 1. Rebind to unmaterialized bundled entry -> materializes 'bundled' row with price_locked = 0
    rebind_model_pricing_alias_sqlx(pool, tenant_id, "Custom-X-Model", Some(&bundled_gpt4o))
        .await
        .expect("rebind to gpt-4o");

    let loaded = load_pricing_catalog_entries_sqlx(pool, tenant_id)
        .await
        .expect("load after rebind");
    let gpt_row = loaded
        .iter()
        .find(|e| e.model_patterns == vec!["gpt-4o"])
        .expect("materialized bundled row");
    assert_eq!(gpt_row.source_channel, "bundled");
    assert!(!gpt_row.price_locked);
    assert_eq!(gpt_row.usage_aliases, vec!["Custom-X-Model"]);

    // 2. Sync updates price on materialized row while preserving usage_aliases
    let synced_gpt4o = ModelPriceEntry {
        provider: "openai".to_string(),
        model_patterns: vec!["gpt-4o".to_string()],
        currency: "CNY".to_string(),
        input_per_million: 20.0,
        output_per_million: 80.0,
        ..Default::default()
    };
    upsert_pricing_catalog_entries_sqlx(pool, tenant_id, &[synced_gpt4o], "litellm")
        .await
        .expect("sync update");

    let loaded_synced = load_pricing_catalog_entries_sqlx(pool, tenant_id)
        .await
        .expect("load after sync");
    let gpt_synced = loaded_synced
        .iter()
        .find(|e| e.model_patterns == vec!["gpt-4o"])
        .expect("synced gpt-4o");
    assert!((gpt_synced.input_per_million - 20.0).abs() < 1e-4);
    assert_eq!(gpt_synced.source_channel, "litellm");
    assert_eq!(gpt_synced.usage_aliases, vec!["Custom-X-Model"]);

    // 3. Move alias (case-insensitive trimmed) to claude-3-7-sonnet
    let claude = ModelPriceEntry {
        provider: "anthropic".to_string(),
        model_patterns: vec!["claude-3-7-sonnet".to_string()],
        currency: "CNY".to_string(),
        input_per_million: 21.75,
        output_per_million: 108.75,
        ..Default::default()
    };
    rebind_model_pricing_alias_sqlx(pool, tenant_id, "  custom-x-model  ", Some(&claude))
        .await
        .expect("move alias to claude");

    let targets = get_rebind_target_model_ids_sqlx(pool, tenant_id)
        .await
        .expect("get rebind targets");
    assert_eq!(targets, vec!["claude-3-7-sonnet".to_string()]);

    let loaded_moved = load_pricing_catalog_entries_sqlx(pool, tenant_id)
        .await
        .expect("load moved");
    let gpt_after_move = loaded_moved
        .iter()
        .find(|e| e.model_patterns == vec!["gpt-4o"])
        .expect("gpt-4o");
    assert!(gpt_after_move.usage_aliases.is_empty());
    let claude_after_move = loaded_moved
        .iter()
        .find(|e| e.model_patterns == vec!["claude-3-7-sonnet"])
        .expect("claude-3-7-sonnet");
    assert_eq!(claude_after_move.usage_aliases, vec!["custom-x-model"]);

    // 4. Unbind (None) removes alias completely
    rebind_model_pricing_alias_sqlx(pool, tenant_id, "CUSTOM-X-MODEL", None)
        .await
        .expect("unbind alias");
    let targets_empty = get_rebind_target_model_ids_sqlx(pool, tenant_id)
        .await
        .expect("get targets after unbind");
    assert!(targets_empty.is_empty());

    drop(database);
    std::fs::remove_file(path).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_ignore_unignore_model_pricing_and_rebind_target_ids() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-pricing-ignore-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();
    let tenant_id = "test-tenant";

    let dirty_raw = "dirty \"bad\" model\nline2";

    // Seed a usage event with the dirty raw model
    sqlx::query(
        r#"
        INSERT INTO conversation_usage_events (
            tenant_id, source_id, adapter_id, external_event_id, timestamp,
            provider, model, status, input_tokens, output_tokens,
            cost_basis, currency, created_at, updated_at
        ) VALUES (
            ?, 'src-1', 'codex', 'evt-dirty-1', '2026-10-05T08:00:00Z',
            'codex', ?, 'success', 500000, 500000,
            'none', 'CNY', 'now', 'now'
        )
        "#,
    )
    .bind(tenant_id)
    .bind(dirty_raw)
    .execute(pool)
    .await
    .expect("seed dirty usage event");

    let gpt4o = ModelPriceEntry {
        provider: "openai".to_string(),
        model_patterns: vec!["gpt-4o".to_string()],
        currency: "CNY".to_string(),
        input_per_million: 18.125,
        output_per_million: 72.5,
        ..Default::default()
    };
    rebind_model_pricing_alias_sqlx(pool, tenant_id, dirty_raw, Some(&gpt4o))
        .await
        .expect("rebind dirty raw first");

    // Seed an active locked override and another aliased model
    let claude = ModelPriceEntry {
        provider: "anthropic".to_string(),
        model_patterns: vec!["claude-3-7-sonnet".to_string()],
        currency: "CNY".to_string(),
        input_per_million: 21.75,
        output_per_million: 108.75,
        ..Default::default()
    };
    save_locked_override_sqlx(pool, tenant_id, "claude-3-7-sonnet", &claude)
        .await
        .expect("save locked override");

    let deepseek = ModelPriceEntry {
        provider: "deepseek".to_string(),
        model_patterns: vec!["deepseek-chat".to_string()],
        currency: "CNY".to_string(),
        input_per_million: 1.0,
        output_per_million: 2.0,
        ..Default::default()
    };
    rebind_model_pricing_alias_sqlx(pool, tenant_id, "ds-v3-alias", Some(&deepseek))
        .await
        .expect("rebind deepseek alias");

    // Ignore dirty_raw -> removes from gpt-4o usage_aliases and creates __ignore__::...
    set_model_pricing_ignored_sqlx(pool, tenant_id, dirty_raw, Some("codex"), true)
        .await
        .expect("ignore dirty raw");

    let targets = get_rebind_target_model_ids_sqlx(pool, tenant_id)
        .await
        .expect("get rebind target ids");
    assert_eq!(
        targets,
        vec![
            "claude-3-7-sonnet".to_string(),
            "deepseek-chat".to_string()
        ],
        "must include locked override (stripped) and aliased rows, excluding ignored and empty-alias gpt-4o"
    );

    let loaded = load_pricing_catalog_entries_sqlx(pool, tenant_id)
        .await
        .expect("load entries");
    let ignored_entry = loaded
        .iter()
        .find(|e| e.is_ignored)
        .expect("ignored entry present");
    assert!(ignored_entry.model_patterns[0].starts_with(IGNORE_MODEL_ID_PREFIX));
    assert_eq!(ignored_entry.source_channel, "user_override");
    assert!(ignored_entry.price_locked);
    assert_eq!(ignored_entry.input_per_million, 0.0);
    assert_eq!(ignored_entry.usage_aliases, vec![dirty_raw.to_string()]);

    let catalog = PriceCatalog::new("v1".into(), "now".into(), 7.25, loaded);
    recalculate_usage_costs_sqlx(pool, tenant_id, &catalog)
        .await
        .expect("recalc ignored");
    let row = sqlx::query(
        "SELECT catalog_estimated_cost, cost_basis FROM conversation_usage_events WHERE tenant_id = ? AND external_event_id = 'evt-dirty-1'",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .expect("fetch ignored event");
    assert_eq!(row.get::<String, _>("cost_basis"), "catalog_estimated");
    assert_eq!(
        row.get::<Option<f64>, _>("catalog_estimated_cost"),
        Some(0.0)
    );

    // Unignore (case-insensitive) -> removes __ignore__ row and resets event to none
    set_model_pricing_ignored_sqlx(pool, tenant_id, "DIRTY \"BAD\" MODEL\nLINE2", None, false)
        .await
        .expect("unignore dirty raw");

    let loaded_unignored = load_pricing_catalog_entries_sqlx(pool, tenant_id)
        .await
        .expect("load after unignore");
    assert!(!loaded_unignored.iter().any(|e| e.is_ignored));

    let catalog_unignored = PriceCatalog::new("v1".into(), "now".into(), 7.25, loaded_unignored);
    recalculate_usage_costs_sqlx(pool, tenant_id, &catalog_unignored)
        .await
        .expect("recalc after unignore");
    let row_unignored = sqlx::query(
        "SELECT catalog_estimated_cost, cost_basis FROM conversation_usage_events WHERE tenant_id = ? AND external_event_id = 'evt-dirty-1'",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .expect("fetch unignored event");
    assert_eq!(row_unignored.get::<String, _>("cost_basis"), "none");
    assert_eq!(
        row_unignored.get::<Option<f64>, _>("catalog_estimated_cost"),
        None
    );

    drop(database);
    std::fs::remove_file(path).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_migration_purges_display_and_wildcard_rows_and_merges_aliases() {
    use sqlx::sqlite::SqlitePoolOptions;

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect memory sqlite");

    for migration in [
        include_str!("../../../migrations/20261005000001_model_pricing_catalog.sql"),
        include_str!("../../../migrations/20261116000001_model_pricing_lock_and_aliases.sql"),
    ] {
        sqlx::raw_sql(migration)
            .execute(&pool)
            .await
            .expect("apply pre-migration schema");
    }

    let seed_rows = [
        (
            "claude-3-5-sonnet",
            "litellm",
            0,
            3.0,
            15.0,
            r#"["existing-alias"]"#,
        ),
        (
            "claude-3-5-sonnet-20241022",
            "litellm",
            0,
            1.0,
            2.0,
            r#"["legacy-alias"]"#,
        ),
        ("claude-3-5*", "litellm", 0, 1.0, 2.0, "[]"),
        ("Claude 3.5 Sonnet (latest)", "litellm", 0, 1.0, 2.0, "[]"),
        (
            "__override__::claude-3-5-sonnet",
            "user_override",
            1,
            9.0,
            9.0,
            "[]",
        ),
        (
            "deepseek-chat",
            "litellm",
            1,
            5.0,
            9.0,
            r#"["locked-alias"]"#,
        ),
    ];
    for (model_id, channel, locked, input, output, aliases) in seed_rows {
        sqlx::query(
            r#"
            INSERT INTO model_pricing_catalog (
                tenant_id, model_id, provider, currency,
                input_per_million, cache_read_per_million, cache_write_per_million,
                reasoning_per_million, output_per_million,
                raw_currency, raw_input_per_million, raw_output_per_million,
                source_channel, price_locked, usage_aliases, created_at, updated_at
            ) VALUES ('t1', ?, 'anthropic', 'CNY', ?, 0.0, 0.0, 0.0, ?, 'USD', ?, ?, ?, ?, ?, 'now', 'now')
            "#,
        )
        .bind(model_id)
        .bind(input)
        .bind(output)
        .bind(input)
        .bind(output)
        .bind(channel)
        .bind(locked)
        .bind(aliases)
        .execute(&pool)
        .await
        .expect("seed pricing row");
    }

    let migration_sql =
        include_str!("../../../migrations/20261116000002_purge_dirty_pricing_catalog_rows.sql");
    for _ in 0..2 {
        sqlx::raw_sql(migration_sql)
            .execute(&pool)
            .await
            .expect("apply purge migration twice (idempotent)");
    }

    let rows = sqlx::query(
        "SELECT model_id, source_channel, price_locked, usage_aliases FROM model_pricing_catalog ORDER BY model_id",
    )
    .fetch_all(&pool)
    .await
    .expect("query migrated rows");

    let model_ids: Vec<String> = rows
        .iter()
        .map(|r| r.get::<String, _>("model_id"))
        .collect();
    assert_eq!(
        model_ids,
        vec![
            "__override__::claude-3-5-sonnet".to_string(),
            "claude-3-5-sonnet".to_string(),
            "deepseek-chat".to_string(),
        ],
        "display-name, wildcard and dated variant rows must be purged; override and locked rows kept"
    );

    let base = rows
        .iter()
        .find(|r| r.get::<String, _>("model_id") == "claude-3-5-sonnet")
        .expect("canonical base row");
    let aliases: Vec<String> =
        serde_json::from_str(&base.get::<String, _>("usage_aliases")).expect("aliases json");
    assert!(
        aliases.iter().any(|a| a == "existing-alias"),
        "base row keeps its own aliases"
    );
    assert!(
        aliases.iter().any(|a| a == "legacy-alias"),
        "removed display row aliases must be migrated onto the base row"
    );

    let locked = rows
        .iter()
        .find(|r| r.get::<String, _>("model_id") == "deepseek-chat")
        .expect("locked row");
    assert_eq!(locked.get::<i64, _>("price_locked"), 1);
    assert_eq!(
        locked.get::<String, _>("usage_aliases"),
        r#"["locked-alias"]"#
    );

    let override_row = rows
        .iter()
        .find(|r| r.get::<String, _>("model_id") == "__override__::claude-3-5-sonnet")
        .expect("override row");
    assert_eq!(
        override_row.get::<String, _>("source_channel"),
        "user_override"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_sync_prunes_superseded_display_rows_so_resolve_model_hits_new_price() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-pricing-sync-prune-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();
    let tenant_id = "test-tenant";

    sqlx::query(
        r#"
        INSERT INTO model_pricing_catalog (
            tenant_id, model_id, provider, currency,
            input_per_million, cache_read_per_million, cache_write_per_million,
            reasoning_per_million, output_per_million,
            raw_currency, raw_input_per_million, raw_output_per_million,
            source_channel, price_locked, usage_aliases, created_at, updated_at
        ) VALUES
            (?, 'claude-3-5-sonnet-20241022', 'anthropic', 'CNY', 1.0, 0.0, 0.0, 0.0, 2.0, 'USD', 0.1, 0.2, 'litellm', 0, '["legacy-sonnet-alias"]', 'now', 'now'),
            (?, 'claude-3-5*', 'anthropic', 'CNY', 1.0, 0.0, 0.0, 0.0, 2.0, 'USD', 0.1, 0.2, 'litellm', 0, '[]', 'now', 'now'),
            (?, 'gpt-4o-mini', 'openai', 'CNY', 0.5, 0.0, 0.0, 0.0, 1.0, 'USD', 0.1, 0.2, 'litellm', 0, '[]', 'now', 'now')
        "#,
    )
    .bind(tenant_id)
    .bind(tenant_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("seed legacy display rows");

    let synced = vec![
        ModelPriceEntry {
            provider: "anthropic".to_string(),
            model_patterns: vec!["claude-3-5-sonnet".to_string()],
            currency: "CNY".to_string(),
            input_per_million: 3.0,
            output_per_million: 15.0,
            ..Default::default()
        },
        ModelPriceEntry {
            provider: "openai".to_string(),
            model_patterns: vec!["gpt-4o".to_string()],
            currency: "CNY".to_string(),
            input_per_million: 2.0,
            output_per_million: 8.0,
            ..Default::default()
        },
    ];
    upsert_pricing_catalog_entries_sqlx(pool, tenant_id, &synced, "litellm")
        .await
        .expect("sync remote catalog");

    let loaded = load_pricing_catalog_entries_sqlx(pool, tenant_id)
        .await
        .expect("load catalog");
    assert!(
        !loaded
            .iter()
            .any(|e| e.model_patterns == vec!["claude-3-5-sonnet-20241022"]),
        "superseded display row must be pruned on successful remote sync"
    );
    assert!(
        loaded
            .iter()
            .any(|e| e.model_patterns == vec!["gpt-4o-mini"]),
        "distinct sibling model must survive base-model sync"
    );

    let base = loaded
        .iter()
        .find(|e| e.model_patterns == vec!["claude-3-5-sonnet"])
        .expect("canonical base row");
    assert!((base.input_per_million - 3.0).abs() < 1e-9);
    assert_eq!(base.usage_aliases, vec!["legacy-sonnet-alias"]);

    let catalog = PriceCatalog::new("v1".into(), "now".into(), 7.25, loaded);
    let resolved_base = catalog
        .resolve_model("anthropic", "claude-3-5-sonnet")
        .expect("resolve canonical base");
    assert!((resolved_base.entry.input_per_million - 3.0).abs() < 1e-9);
    assert_eq!(resolved_base.entry.source_channel, "litellm");

    let resolved_dated = catalog
        .resolve_model("anthropic", "claude-3-5-sonnet-20241022")
        .expect("resolve dated model name");
    assert!(
        (resolved_dated.entry.input_per_million - 3.0).abs() < 1e-9,
        "dated model must now resolve to the freshly synced base price, not the stale display row"
    );

    drop(database);
    std::fs::remove_file(path).ok();
}
