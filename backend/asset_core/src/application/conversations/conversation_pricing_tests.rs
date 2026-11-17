use super::*;
use crate::backend::store::conversations::pricing_repo::upsert_pricing_catalog_entries_sqlx;

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
    assert_eq!(status.models_count, catalog.models.len());
    assert_eq!(status.local_models_count, catalog.local_models.len());
    assert!(catalog.local_models.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_local_models_strict_scoping_unpriced_first_sorting_and_reference_parity() {
    use crate::backend::domain::conversations::UsageDashboardFilter;
    use crate::backend::store::conversations::usage_repo::{
        upsert_usage_events_sqlx, RawUsageEventInput,
    };

    let path = std::env::temp_dir().join(format!(
        "assetiweave-pricing-app-scope-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let app_service = AppService::open_with_db_path(path)
        .await
        .expect("open test app service");
    let pool = app_service.db.pool();
    let tenant_id = app_service.tenant_id();

    // Seed a DB reference row (e.g. gpt-4o updated price) and an extra non-used reference model
    // plus a custom locked override with no usage events.
    let db_ref = vec![
        ModelPriceEntry {
            provider: "openai".to_string(),
            model_patterns: vec!["gpt-4o".to_string()],
            currency: "CNY".to_string(),
            input_per_million: 20.0,
            cache_read_per_million: 10.0,
            cache_write_per_million: 20.0,
            output_per_million: 80.0,
            reasoning_per_million: 80.0,
            raw_currency: Some("USD".to_string()),
            raw_input_per_million: Some(2.5),
            raw_output_per_million: Some(10.0),
            source_channel: "litellm".to_string(),
            ..Default::default()
        },
        ModelPriceEntry {
            provider: "anthropic".to_string(),
            model_patterns: vec!["unused-cached-reference-model".to_string()],
            currency: "CNY".to_string(),
            input_per_million: 15.0,
            output_per_million: 60.0,
            source_channel: "litellm".to_string(),
            ..Default::default()
        },
    ];
    upsert_pricing_catalog_entries_sqlx(pool, tenant_id, &db_ref, "litellm")
        .await
        .expect("seed reference rows");

    // Seed actual usage events: one matched, one unmatched (later alphabetically)
    let events = vec![
        RawUsageEventInput {
            external_event_id: "ev-matched".to_string(),
            session_id: Some("sess-1".to_string()),
            turn_id: None,
            logical_request_id: None,
            attempt_index: None,
            timestamp: "2026-11-15T10:00:00Z".to_string(),
            provider: "anthropic".to_string(),
            model: "claude-3-7-sonnet-20250219".to_string(),
            status: Some("completed".to_string()),
            input_tokens: 1_000_000,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
            output_tokens: 0,
            host_reported_cost: None,
            currency: None,
        },
        RawUsageEventInput {
            external_event_id: "ev-unmatched".to_string(),
            session_id: Some("sess-1".to_string()),
            turn_id: None,
            logical_request_id: None,
            attempt_index: None,
            timestamp: "2026-11-15T10:01:00Z".to_string(),
            provider: "zhipu-custom".to_string(),
            model: "brand-new-unknown-model".to_string(),
            status: Some("completed".to_string()),
            input_tokens: 1_000_000,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
            output_tokens: 0,
            host_reported_cost: None,
            currency: None,
        },
    ];
    upsert_usage_events_sqlx(pool, tenant_id, "src-1", "claude-code", &events, None)
        .await
        .expect("seed usage events");

    let catalog = app_service
        .get_model_pricing_catalog()
        .await
        .expect("get catalog");
    let status = app_service
        .get_model_pricing_status()
        .await
        .expect("get status");

    // Parity between top badge and tab lists
    assert_eq!(status.models_count, catalog.models.len());
    assert_eq!(status.local_models_count, catalog.local_models.len());

    // Local models must NOT dump unused cached reference rows
    assert_eq!(catalog.local_models.len(), 2);
    assert!(
        !catalog
            .local_models
            .iter()
            .any(|m| m.raw_model == "unused-cached-reference-model"),
        "unused cached reference model must not leak into local_models"
    );

    // Unmatched item must be sorted FIRST despite provider 'zhipu-custom' > 'anthropic'
    assert_eq!(catalog.local_models[0].raw_model, "brand-new-unknown-model");
    assert_eq!(catalog.local_models[0].matched_model_id, None);
    assert_eq!(catalog.local_models[0].match_source, "unmatched");

    assert_eq!(
        catalog.local_models[1].raw_model,
        "claude-3-7-sonnet-20250219"
    );
    assert!(catalog.local_models[1].matched_model_id.is_some());
    assert_eq!(catalog.local_models[1].match_source, "auto_matched");

    // Reference catalog deduplicates gpt-4o preferring DB row (20.0 instead of bundled 18.13)
    let gpt4o_refs: Vec<_> = catalog
        .models
        .iter()
        .filter(|m| m.matched_model_id.as_deref() == Some("gpt-4o"))
        .collect();
    assert_eq!(gpt4o_refs.len(), 1, "gpt-4o must be deduplicated");
    assert!((gpt4o_refs[0].input_per_million - 20.0).abs() < 1e-2);
    assert_eq!(gpt4o_refs[0].match_source, "reference");
    assert!(!gpt4o_refs[0].price_locked);

    // Dashboard sanity check
    let dashboard = app_service
        .get_conversation_usage_dashboard(UsageDashboardFilter::default())
        .await
        .expect("get dashboard");
    assert_eq!(dashboard.models.len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rebind_lock_sync_protection_and_unlock_recalculates_costs() {
    use crate::backend::domain::conversations::UsageDashboardFilter;
    use crate::backend::infrastructure::conversations::pricing_fetcher::{
        PricingSourceChannel, TieredFetchOutcome,
    };
    use crate::backend::store::conversations::usage_repo::{
        upsert_usage_events_sqlx, RawUsageEventInput,
    };
    use std::sync::{Arc, Mutex};

    let path = std::env::temp_dir().join(format!(
        "assetiweave-pricing-app-rebind-lock-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let app_service = AppService::open_with_db_path(path)
        .await
        .expect("open test app service");
    let pool = app_service.db.pool();
    let tenant_id = app_service.tenant_id();

    // 1. Seed usage events:
    //    - unknown-alias-v1 (unmatched initially)
    //    - gpt-4o (auto_matched to bundled gpt-4o)
    let events = vec![
        RawUsageEventInput {
            external_event_id: "ev-rebind-target".to_string(),
            session_id: Some("sess-rebind".to_string()),
            turn_id: None,
            logical_request_id: None,
            attempt_index: None,
            timestamp: "2026-11-15T10:00:00Z".to_string(),
            provider: "custom".to_string(),
            model: "unknown-alias-v1".to_string(),
            status: Some("completed".to_string()),
            input_tokens: 1_000_000,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
            output_tokens: 0,
            host_reported_cost: None,
            currency: None,
        },
        RawUsageEventInput {
            external_event_id: "ev-gpt4o".to_string(),
            session_id: Some("sess-rebind".to_string()),
            turn_id: None,
            logical_request_id: None,
            attempt_index: None,
            timestamp: "2026-11-15T10:01:00Z".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            status: Some("completed".to_string()),
            input_tokens: 1_000_000,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
            output_tokens: 0,
            host_reported_cost: None,
            currency: None,
        },
    ];
    upsert_usage_events_sqlx(pool, tenant_id, "src-1", "claude-code", &events, None)
        .await
        .expect("seed events");

    // 2. Manual rebind unknown-alias-v1 -> deepseek-chat (CNY 1.0 input / 2.0 output)
    app_service
        .rebind_model_pricing_alias(
            "unknown-alias-v1".to_string(),
            Some("deepseek-chat".to_string()),
        )
        .await
        .expect("rebind alias");

    let catalog = app_service
        .get_model_pricing_catalog()
        .await
        .expect("catalog after rebind");
    let rebound = catalog
        .local_models
        .iter()
        .find(|m| m.raw_model == "unknown-alias-v1")
        .expect("rebound model present");
    assert_eq!(rebound.matched_model_id.as_deref(), Some("deepseek-chat"));
    assert_eq!(rebound.match_source, "manual_rebind");
    assert!(rebound.is_manual_rebind);
    assert!(!rebound.price_locked);
    assert!((rebound.input_per_million - 1.0).abs() < 1e-2);

    // Historical cost recalculated to CNY 1.0
    let find_rebound_model = |dash: &crate::backend::domain::conversations::UsageDashboardDto| {
        dash.models
            .iter()
            .find(|m| {
                m.model == "unknown-alias-v1"
                    || m.raw_models.iter().any(|r| r == "unknown-alias-v1")
            })
            .cloned()
            .expect("rebound model present in dashboard")
    };
    let dash1 = app_service
        .get_conversation_usage_dashboard(UsageDashboardFilter::default())
        .await
        .expect("dashboard after rebind");
    let ds_row = find_rebound_model(&dash1);
    assert!((ds_row.cost.unwrap_or(-1.0) - 1.0).abs() < 1e-4);

    // 3. Edit base price of deepseek-chat -> auto-locks __override__::deepseek-chat to 5.0
    //    Also save a standalone custom locked override with no usage events.
    let mut edited = rebound.clone();
    edited.input_per_million = 5.0;
    edited.output_per_million = 10.0;
    app_service
        .save_model_price_entry(edited)
        .await
        .expect("save locked override");

    app_service
        .save_model_price_entry(ModelPriceEntryDto {
            provider: "custom".to_string(),
            model_patterns: vec!["standalone-locked-model".to_string()],
            currency: "CNY".to_string(),
            input_per_million: 3.5,
            cache_read_per_million: 0.0,
            cache_write_per_million: 0.0,
            output_per_million: 7.0,
            reasoning_per_million: 0.0,
            raw_currency: Some("CNY".to_string()),
            raw_input_per_million: Some(3.5),
            raw_output_per_million: Some(7.0),
            raw_model: "standalone-locked-model".to_string(),
            matched_model_id: Some("standalone-locked-model".to_string()),
            match_source: "user_override".to_string(),
            price_locked: false,
            is_ignored: false,
            is_manual_rebind: false,
        })
        .await
        .expect("save standalone locked override");

    let catalog2 = app_service
        .get_model_pricing_catalog()
        .await
        .expect("catalog after save");
    let rebound_locked = catalog2
        .local_models
        .iter()
        .find(|m| m.raw_model == "unknown-alias-v1")
        .unwrap();
    assert!(rebound_locked.price_locked);
    assert_eq!(rebound_locked.match_source, "manual_rebind");
    assert!((rebound_locked.input_per_million - 5.0).abs() < 1e-2);

    // Standalone locked override appears in local_models
    let standalone = catalog2
        .local_models
        .iter()
        .find(|m| m.raw_model == "standalone-locked-model")
        .expect("standalone locked model visible in local_models");
    assert!(standalone.price_locked);
    assert_eq!(standalone.match_source, "user_override");

    // Verify historical cost updated to 5.0
    let dash2 = app_service
        .get_conversation_usage_dashboard(UsageDashboardFilter::default())
        .await
        .expect("dashboard after save");
    let ds_row2 = find_rebound_model(&dash2);
    assert!((ds_row2.cost.unwrap_or(-1.0) - 5.0).abs() < 1e-4);

    // 4. Online sync:
    //    - Must union usage models AND rebind/locked base model IDs ("deepseek-chat", "standalone-locked-model")
    //    - Updates reference row for deepseek-chat to 2.5 and gpt-4o to 30.0
    //    - Preserves locked override on deepseek-chat (stays 5.0)
    //    - Stage 3 recalculation uses DB effective catalog (so unknown-alias-v1 stays 5.0, gpt-4o becomes 30.0)
    let captured_targets = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured_ref = captured_targets.clone();
    let sync_result = app_service
        .sync_model_pricing_catalog_with_fetcher(
            false,
            None,
            None,
            None,
            move |_rate, _cached, targets| {
                let target_list = targets.unwrap_or_default();
                *captured_ref.lock().unwrap() = target_list;
                (
                    7.25,
                    TieredFetchOutcome {
                        channel: PricingSourceChannel::LiteLlm,
                        entries: vec![
                            ModelPriceEntry {
                                provider: "deepseek".to_string(),
                                model_patterns: vec!["deepseek-chat".to_string()],
                                currency: "CNY".to_string(),
                                input_per_million: 2.5,
                                output_per_million: 5.0,
                                source_channel: "litellm".to_string(),
                                ..Default::default()
                            },
                            ModelPriceEntry {
                                provider: "openai".to_string(),
                                model_patterns: vec!["gpt-4o".to_string()],
                                currency: "CNY".to_string(),
                                input_per_million: 30.0,
                                output_per_million: 120.0,
                                source_channel: "litellm".to_string(),
                                ..Default::default()
                            },
                        ],
                        rate: 7.25,
                        catalog_version: "litellm-2026-11-16".to_string(),
                        downgraded: false,
                        warning: None,
                    },
                )
            },
        )
        .await
        .expect("sync with mock fetcher");

    let seen_targets = captured_targets.lock().unwrap().clone();
    assert!(
        seen_targets.iter().any(|t| t == "deepseek-chat"),
        "sync target models must include rebind target deepseek-chat: {seen_targets:?}"
    );
    assert!(
        seen_targets.iter().any(|t| t == "gpt-4o"),
        "sync target models must include usage model gpt-4o: {seen_targets:?}"
    );
    assert_eq!(
        sync_result.models_count, 1,
        "sync_result.models_count must exclude locked model deepseek-chat while updating unlocked gpt-4o"
    );

    // Locked deepseek-chat must stay 5.0 during & after sync!
    let dash3 = app_service
        .get_conversation_usage_dashboard(UsageDashboardFilter::default())
        .await
        .expect("dashboard after sync");
    let ds_row3 = find_rebound_model(&dash3);
    assert!(
        (ds_row3.cost.unwrap_or(-1.0) - 5.0).abs() < 1e-4,
        "locked price must survive sync recalculation"
    );
    let gpt_row3 = dash3.models.iter().find(|m| m.model == "gpt-4o").unwrap();
    assert!((gpt_row3.cost.unwrap_or(-1.0) - 30.0).abs() < 1e-4);

    // 5. Unlock deepseek-chat -> immediately falls back to synced reference price (2.5) and recalculates!
    app_service
        .set_model_pricing_lock("deepseek-chat".to_string(), false)
        .await
        .expect("unlock deepseek-chat");

    let catalog3 = app_service
        .get_model_pricing_catalog()
        .await
        .expect("catalog after unlock");
    let unlocked = catalog3
        .local_models
        .iter()
        .find(|m| m.raw_model == "unknown-alias-v1")
        .unwrap();
    assert!(!unlocked.price_locked);
    assert_eq!(unlocked.match_source, "manual_rebind");
    assert!((unlocked.input_per_million - 2.5).abs() < 1e-2);

    let dash4 = app_service
        .get_conversation_usage_dashboard(UsageDashboardFilter::default())
        .await
        .expect("dashboard after unlock");
    let ds_row4 = find_rebound_model(&dash4);
    assert!((ds_row4.cost.unwrap_or(-1.0) - 2.5).abs() < 1e-4);

    // 6. Unbind unknown-alias-v1 -> returns to unmatched
    app_service
        .rebind_model_pricing_alias("unknown-alias-v1".to_string(), None)
        .await
        .expect("unbind alias");
    let catalog4 = app_service
        .get_model_pricing_catalog()
        .await
        .expect("catalog after unbind");
    let unbound = catalog4
        .local_models
        .iter()
        .find(|m| m.raw_model == "unknown-alias-v1")
        .unwrap();
    assert_eq!(unbound.match_source, "unmatched");
    assert_eq!(unbound.matched_model_id, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_locked_zero_and_ignored_dirty_string_classification_and_sorting() {
    use crate::backend::domain::conversations::UsageDashboardFilter;
    use crate::backend::store::conversations::usage_repo::{
        upsert_usage_events_sqlx, RawUsageEventInput,
    };
    use sqlx::Row as SqlxRow;

    let path = std::env::temp_dir().join(format!(
        "assetiweave-pricing-app-zero-ignore-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let app_service = AppService::open_with_db_path(path)
        .await
        .expect("open test app service");
    let pool = app_service.db.pool();
    let tenant_id = app_service.tenant_id();

    let dirty_model = "{\"error\": \"bad\nmodel\"}".to_string();
    let events = vec![
        RawUsageEventInput {
            external_event_id: "ev-free".to_string(),
            session_id: Some("sess-z".to_string()),
            turn_id: None,
            logical_request_id: None,
            attempt_index: None,
            timestamp: "2026-11-15T10:00:00Z".to_string(),
            provider: "xiaomi".to_string(),
            model: "mimo-v2.5-free".to_string(),
            status: Some("completed".to_string()),
            input_tokens: 1_000_000,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
            output_tokens: 1_000_000,
            host_reported_cost: None,
            currency: None,
        },
        RawUsageEventInput {
            external_event_id: "ev-dirty".to_string(),
            session_id: Some("sess-z".to_string()),
            turn_id: None,
            logical_request_id: None,
            attempt_index: None,
            timestamp: "2026-11-15T10:01:00Z".to_string(),
            provider: "aaa-provider".to_string(),
            model: dirty_model.clone(),
            status: Some("completed".to_string()),
            input_tokens: 500_000,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
            output_tokens: 500_000,
            host_reported_cost: None,
            currency: None,
        },
        RawUsageEventInput {
            external_event_id: "ev-real-unmatched".to_string(),
            session_id: Some("sess-z".to_string()),
            turn_id: None,
            logical_request_id: None,
            attempt_index: None,
            timestamp: "2026-11-15T10:02:00Z".to_string(),
            provider: "zzz-provider".to_string(),
            model: "real-unpriced-model".to_string(),
            status: Some("completed".to_string()),
            input_tokens: 500_000,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
            output_tokens: 500_000,
            host_reported_cost: None,
            currency: None,
        },
    ];
    upsert_usage_events_sqlx(pool, tenant_id, "src-z", "claude-code", &events, None)
        .await
        .expect("seed events");

    // 1. Lock mimo-v2.5 at ¥0 -> mimo-v2.5-free resolves to ¥0 locked override
    app_service
        .save_model_price_entry(ModelPriceEntryDto {
            provider: "xiaomi".to_string(),
            model_patterns: vec!["mimo-v2.5".to_string()],
            currency: "CNY".to_string(),
            input_per_million: 0.0,
            cache_read_per_million: 0.0,
            cache_write_per_million: 0.0,
            output_per_million: 0.0,
            reasoning_per_million: 0.0,
            raw_currency: Some("CNY".to_string()),
            raw_input_per_million: Some(0.0),
            raw_output_per_million: Some(0.0),
            raw_model: "mimo-v2.5-free".to_string(),
            matched_model_id: Some("mimo-v2.5".to_string()),
            match_source: "auto_matched".to_string(),
            price_locked: false,
            is_ignored: false,
            is_manual_rebind: false,
        })
        .await
        .expect("save locked zero override");

    // 2. Ignore dirty string
    app_service
        .set_model_pricing_ignored(dirty_model.clone(), Some("aaa-provider".to_string()), true)
        .await
        .expect("ignore dirty model");

    let catalog = app_service
        .get_model_pricing_catalog()
        .await
        .expect("get catalog");
    assert_eq!(catalog.local_models.len(), 3);

    // Sorting: real-unpriced-model MUST be first! Ignored dirty string (even with 'aaa-provider') is NOT pinned at top.
    assert_eq!(catalog.local_models[0].raw_model, "real-unpriced-model");
    assert_eq!(catalog.local_models[0].match_source, "unmatched");
    assert!(!catalog.local_models[0].is_ignored);

    let ignored_item = catalog
        .local_models
        .iter()
        .find(|m| m.raw_model == dirty_model)
        .expect("ignored item present");
    assert_eq!(ignored_item.match_source, "ignored");
    assert!(ignored_item.is_ignored);
    assert!(ignored_item.price_locked);
    assert_eq!(ignored_item.matched_model_id, None);
    assert_eq!(ignored_item.input_per_million, 0.0);
    assert_eq!(ignored_item.output_per_million, 0.0);

    let free_item = catalog
        .local_models
        .iter()
        .find(|m| m.raw_model == "mimo-v2.5-free")
        .expect("free locked item present");
    assert!(free_item.price_locked);
    assert!(!free_item.is_ignored);
    assert_eq!(free_item.matched_model_id.as_deref(), Some("mimo-v2.5"));
    assert_eq!(free_item.input_per_million, 0.0);

    // Check DB cost_basis and catalog_estimated_cost for ev-free & ev-dirty
    let rows = sqlx::query(
        "SELECT external_event_id, cost_basis, catalog_estimated_cost FROM conversation_usage_events WHERE tenant_id = ? ORDER BY external_event_id",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .expect("query usage events");

    for row in rows {
        let ev_id: String = row.get("external_event_id");
        let basis: String = row.get("cost_basis");
        let est: Option<f64> = row.get("catalog_estimated_cost");
        match ev_id.as_str() {
            "ev-free" | "ev-dirty" => {
                assert_eq!(
                    basis, "catalog_estimated",
                    "event {ev_id} should be covered"
                );
                assert_eq!(est, Some(0.0), "event {ev_id} should cost 0.0");
            }
            "ev-real-unmatched" => {
                assert_eq!(basis, "none");
                assert_eq!(est, None);
            }
            other => panic!("unexpected event {other}"),
        }
    }

    // Dashboard coverage: 2 covered (ev-free + ev-dirty), 1 uncovered
    let dash = app_service
        .get_conversation_usage_dashboard(UsageDashboardFilter::default())
        .await
        .expect("get dashboard");
    assert_eq!(dash.hero.cost_covered_requests, 2);

    // 3. Unignore dirty string -> reverts to unmatched & cost_basis = 'none', sorted alongside unpriced
    app_service
        .set_model_pricing_ignored(dirty_model.clone(), None, false)
        .await
        .expect("unignore dirty model");
    let catalog_after = app_service
        .get_model_pricing_catalog()
        .await
        .expect("catalog after unignore");
    assert_eq!(catalog_after.local_models[0].raw_model, dirty_model);
    assert_eq!(catalog_after.local_models[0].match_source, "unmatched");
    assert!(!catalog_after.local_models[0].is_ignored);
}
