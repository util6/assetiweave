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
        project_path: None,
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
        project_path: None,
        timezone_offset_minutes: Some(0),
    };
    let dashboard = query_usage_dashboard_sqlx(pool, "default", &filter)
        .await
        .expect("query dashboard");

    let model = dashboard.models.iter().find(|m| m.model == "mimo-v2.5");
    assert!(model.is_some());
    let model = model.unwrap();
    assert_eq!(model.matched_model_id.as_deref(), Some("mimo-v2.5"));
    assert_eq!(model.raw_models, vec!["mimo-v2.5-free".to_string()]);
    assert!(model.cost.is_some());
    assert!(model.cost.unwrap() > 0.0);

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
        project_path: None,
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

#[tokio::test(flavor = "multi_thread")]
async fn test_usage_dashboard_query_by_project() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-usage-proj-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();

    // Set up temp directories: one Git repo, one non-Git dir
    let temp_root = std::env::temp_dir().join(format!(
        "assetiweave-usage-proj-dir-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_root).expect("create temp root");
    let repo_dir = temp_root.join("my-git-repo");
    std::fs::create_dir_all(repo_dir.join(".git")).expect("create .git");
    let subpkg_dir = repo_dir.join("packages").join("web");
    std::fs::create_dir_all(&subpkg_dir).expect("create subpkg");

    let non_git_dir = temp_root.join("standalone-folder").join("notes");
    std::fs::create_dir_all(&non_git_dir).expect("create non-git dir");

    let repo_path_str = repo_dir.to_str().unwrap().to_string();
    let subpkg_path_str = subpkg_dir.to_str().unwrap().to_string();
    let non_git_path_str = non_git_dir.to_str().unwrap().to_string();

    // 1. Insert sessions into conversation_sessions:
    // Session 1: in git subpkg
    // Session 2: in git root, but marked as missing = 1 (deleted session)
    // Session 3: in non-git directory
    // Session 4: no project path (unassigned)
    let insert_session_sql = r#"
        INSERT INTO conversation_sessions (
            tenant_id, id, source_id, adapter_id, external_id, title, project_path, started_at, updated_at,
            missing, created_at, imported_at, execution_origin, execution_purpose, user_visible
        ) VALUES (
            'default', ?1, 'src-proj', 'codex', ?2, ?3, ?4, '2026-03-01T00:00:00Z', '2026-03-01T00:00:00Z',
            ?5, '2026-03-01T00:00:00Z', '2026-03-01T00:00:00Z', 'user', NULL, 1
        )
    "#;

    sqlx::query(insert_session_sql)
        .bind("sess-1")
        .bind("ext-1")
        .bind("Session Subpkg")
        .bind(&subpkg_path_str)
        .bind(0i64)
        .execute(pool)
        .await
        .expect("insert sess-1");

    sqlx::query(insert_session_sql)
        .bind("sess-2")
        .bind("ext-2")
        .bind("Deleted Session")
        .bind(&repo_path_str)
        .bind(1i64) // missing = 1 (deleted!)
        .execute(pool)
        .await
        .expect("insert sess-2");

    sqlx::query(insert_session_sql)
        .bind("sess-3")
        .bind("ext-3")
        .bind("Non-Git Session")
        .bind(&non_git_path_str)
        .bind(0i64)
        .execute(pool)
        .await
        .expect("insert sess-3");

    sqlx::query(insert_session_sql)
        .bind("sess-4")
        .bind("ext-4")
        .bind("Unassigned Session")
        .bind(None::<String>)
        .bind(0i64)
        .execute(pool)
        .await
        .expect("insert sess-4");

    // 2. Insert usage events
    let make_event = |id: &str, sess_id: Option<&str>, tokens: i64| RawUsageEventInput {
        external_event_id: id.to_string(),
        session_id: sess_id.map(str::to_string),
        turn_id: None,
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-03-05T12:00:00Z".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-3-7-sonnet".to_string(),
        status: Some("success".to_string()),
        input_tokens: tokens / 2,
        output_tokens: tokens / 2,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: None,
        currency: Some("USD".to_string()),
    };

    let events = vec![
        make_event("evt-1", Some("ext-1"), 1000), // git subpkg -> should roll up to git root
        make_event("evt-2", Some("ext-2"), 2000), // deleted session -> should be preserved in git root
        make_event("evt-3", Some("ext-3"), 3000), // non-git -> standalone
        make_event("evt-4", Some("ext-4"), 4000), // session without project -> unassigned
        make_event("evt-5", None, 5000),          // orphan event without session -> unassigned
    ];

    upsert_usage_events_sqlx(pool, "default", "src-proj", "codex", &events, None)
        .await
        .expect("upsert usage events");

    // 3. Test Global Query (No project filter)
    let global_filter = UsageDashboardFilter {
        time_range: Some("all_time".to_string()),
        start_date: None,
        end_date: None,
        adapter_id: None,
        source_id: None,
        project_path: None,
        model: None,
        timezone_offset_minutes: Some(0),
    };
    let global_dashboard = query_usage_dashboard_sqlx(pool, "default", &global_filter)
        .await
        .expect("query global dashboard");

    assert_eq!(global_dashboard.hero.total_tokens, 15000);
    assert_eq!(global_dashboard.hero.requests_count, 5);

    // Verify projects breakdown
    assert_eq!(global_dashboard.projects.len(), 3); // git repo, non-git folder, unassigned

    let unassigned_proj = global_dashboard
        .projects
        .iter()
        .find(|p| p.is_unassigned)
        .expect("unassigned project breakdown");
    assert_eq!(unassigned_proj.total_tokens, 9000); // evt-4 (4000) + evt-5 (5000)
    assert_eq!(unassigned_proj.requests_count, 2);

    let git_proj = global_dashboard
        .projects
        .iter()
        .find(|p| p.display_name == "my-git-repo")
        .expect("git project breakdown");
    assert_eq!(git_proj.total_tokens, 3000); // evt-1 (1000) + evt-2 (2000 from deleted session)
    assert_eq!(git_proj.requests_count, 2);

    let non_git_proj = global_dashboard
        .projects
        .iter()
        .find(|p| p.display_name == "notes")
        .expect("non-git project breakdown");
    assert_eq!(non_git_proj.total_tokens, 3000); // evt-3 (3000)
    assert_eq!(non_git_proj.requests_count, 1);

    // 4. Test Filtering by Git Repo (passing the subpkg path should also match the rolled-up Git root)
    let git_filter = UsageDashboardFilter {
        project_path: Some(subpkg_path_str),
        ..global_filter.clone()
    };
    let git_dashboard = query_usage_dashboard_sqlx(pool, "default", &git_filter)
        .await
        .expect("query git project dashboard");
    assert_eq!(git_dashboard.hero.total_tokens, 3000);
    assert_eq!(git_dashboard.hero.requests_count, 2);
    // All projects list is still provided for switching
    assert_eq!(git_dashboard.projects.len(), 3);

    // 5. Test Filtering by Unassigned Project
    let unassigned_filter = UsageDashboardFilter {
        project_path: Some("__unassigned__".to_string()),
        ..global_filter.clone()
    };
    let unassigned_dashboard = query_usage_dashboard_sqlx(pool, "default", &unassigned_filter)
        .await
        .expect("query unassigned dashboard");
    assert_eq!(unassigned_dashboard.hero.total_tokens, 9000);
    assert_eq!(unassigned_dashboard.hero.requests_count, 2);

    drop(database);
    std::fs::remove_file(path).ok();
    std::fs::remove_dir_all(temp_root).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_turn_level_usage_cleanup_and_cursor_reset_migration() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-usage-cleanup-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();

    // 1. Ingest events representing legacy un-turned or non-turn formats alongside valid turn events
    let legacy_no_turn = RawUsageEventInput {
        external_event_id: "legacy-no-turn-1".to_string(),
        session_id: Some("sess-1".to_string()),
        turn_id: None,
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-10-28T08:00:00Z".to_string(),
        provider: "openai".to_string(),
        model: "gpt-4o".to_string(),
        status: Some("success".to_string()),
        input_tokens: 100,
        output_tokens: 100,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: None,
        currency: Some("USD".to_string()),
    };

    let legacy_empty_turn = RawUsageEventInput {
        external_event_id: "legacy-empty-turn-2".to_string(),
        session_id: Some("sess-1".to_string()),
        turn_id: Some("   ".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-10-28T09:00:00Z".to_string(),
        provider: "openai".to_string(),
        model: "gpt-4o".to_string(),
        status: Some("success".to_string()),
        input_tokens: 100,
        output_tokens: 100,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: None,
        currency: Some("USD".to_string()),
    };

    let legacy_unmatched_event_id = RawUsageEventInput {
        external_event_id: "legacy-line-3".to_string(),
        session_id: Some("sess-1".to_string()),
        turn_id: Some("turn-custom".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-10-28T10:00:00Z".to_string(),
        provider: "openai".to_string(),
        model: "gpt-4o".to_string(),
        status: Some("success".to_string()),
        input_tokens: 100,
        output_tokens: 100,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: None,
        currency: Some("USD".to_string()),
    };

    let valid_turn_event = RawUsageEventInput {
        external_event_id: "sess-1-turn-0".to_string(),
        session_id: Some("sess-1".to_string()),
        turn_id: Some("turn-0".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-10-29T10:00:00Z".to_string(),
        provider: "openai".to_string(),
        model: "gpt-4o".to_string(),
        status: Some("success".to_string()),
        input_tokens: 500,
        output_tokens: 500,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: None,
        currency: Some("USD".to_string()),
    };

    let valid_msg_event = RawUsageEventInput {
        external_event_id: "sess-1-msg-101".to_string(),
        session_id: Some("sess-1".to_string()),
        turn_id: Some("msg-101".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-10-29T11:00:00Z".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-3-7-sonnet".to_string(),
        status: Some("success".to_string()),
        input_tokens: 300,
        output_tokens: 300,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: None,
        currency: Some("USD".to_string()),
    };

    upsert_usage_events_sqlx(
        pool,
        "default",
        "src-cleanup",
        "codex",
        &[
            legacy_no_turn,
            legacy_empty_turn,
            legacy_unmatched_event_id,
            valid_turn_event,
            valid_msg_event,
        ],
        None,
    )
    .await
    .expect("insert test events");

    // 2. Set an active opaque_cursor on usage source state
    let state = UsageSourceState {
        tenant_id: "default".to_string(),
        source_id: "src-cleanup".to_string(),
        adapter_id: "codex".to_string(),
        decoder_profile: Some("rollout".to_string()),
        schema_fingerprint: None,
        opaque_cursor: Some("opaque-cursor-v1".to_string()),
        last_success_scan_at: Some("2026-10-28T12:00:00Z".to_string()),
        last_full_scan_at: Some("2026-10-28T12:00:00Z".to_string()),
        status: "ready".to_string(),
        diagnostics_json: None,
        created_at: "2026-10-28T12:00:00Z".to_string(),
        updated_at: "2026-10-28T12:00:00Z".to_string(),
    };
    save_usage_source_state_sqlx(pool, &state)
        .await
        .expect("save source state");

    // Verify initial count is 5
    let count_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_usage_events WHERE tenant_id = 'default' AND source_id = 'src-cleanup'",
    )
    .fetch_one(pool)
    .await
    .expect("count before");
    assert_eq!(count_before, 5);

    // 3. Execute the migration SQL
    let migration_sql = include_str!(
        "../../../migrations/20261030000002_turn_level_usage_cleanup_and_cursor_reset.sql"
    );
    let conn = rusqlite::Connection::open(&path).expect("open rusqlite connection");
    conn.execute_batch(migration_sql)
        .expect("execute migration sql");
    drop(conn);

    // 4. Verify post-migration state:
    // Only valid_turn_event and valid_msg_event should remain
    let remaining_ids: Vec<String> = sqlx::query_scalar(
        "SELECT external_event_id FROM conversation_usage_events WHERE tenant_id = 'default' AND source_id = 'src-cleanup' ORDER BY external_event_id",
    )
    .fetch_all(pool)
    .await
    .expect("fetch remaining external_event_ids");

    assert_eq!(
        remaining_ids,
        vec!["sess-1-msg-101".to_string(), "sess-1-turn-0".to_string()]
    );

    // Verify source state cursor reset to NULL
    let reloaded_state = load_usage_source_state_sqlx(pool, "default", "src-cleanup")
        .await
        .expect("load state")
        .expect("source state exists");
    assert_eq!(reloaded_state.opaque_cursor, None);

    drop(database);
    std::fs::remove_file(path).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_prune_unseen_usage_events_sqlx() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-usage-prune-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();

    let ev_old_1 = RawUsageEventInput {
        external_event_id: "sess-1-turn-0".to_string(),
        session_id: Some("sess-1".to_string()),
        turn_id: Some("turn-0".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-10-29T10:00:00Z".to_string(),
        provider: "openai".to_string(),
        model: "gpt-4o".to_string(),
        status: Some("success".to_string()),
        input_tokens: 100,
        output_tokens: 100,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: None,
        currency: Some("USD".to_string()),
    };
    let ev_old_2 = RawUsageEventInput {
        external_event_id: "sess-1-turn-1".to_string(),
        session_id: Some("sess-1".to_string()),
        turn_id: Some("turn-1".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-10-29T11:00:00Z".to_string(),
        provider: "openai".to_string(),
        model: "gpt-4o".to_string(),
        status: Some("success".to_string()),
        input_tokens: 200,
        output_tokens: 200,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: None,
        currency: Some("USD".to_string()),
    };
    let ev_other_source = RawUsageEventInput {
        external_event_id: "other-turn-0".to_string(),
        session_id: Some("other-sess".to_string()),
        turn_id: Some("turn-0".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-10-29T10:00:00Z".to_string(),
        provider: "openai".to_string(),
        model: "gpt-4o".to_string(),
        status: Some("success".to_string()),
        input_tokens: 500,
        output_tokens: 500,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: None,
        currency: Some("USD".to_string()),
    };

    // Ingest older run
    upsert_usage_events_sqlx(
        pool,
        "default",
        "src-prune",
        "codex",
        &[ev_old_1.clone(), ev_old_2.clone()],
        Some("run-v1"),
    )
    .await
    .expect("ingest old run");

    // Ingest other source on old run
    upsert_usage_events_sqlx(
        pool,
        "default",
        "src-other",
        "codex",
        &[ev_other_source],
        Some("run-v1"),
    )
    .await
    .expect("ingest other source");

    // In new run for src-prune, only ev_old_1 is seen (ev_old_2 was deleted or absent)
    upsert_usage_events_sqlx(
        pool,
        "default",
        "src-prune",
        "codex",
        &[ev_old_1],
        Some("run-v2"),
    )
    .await
    .expect("ingest new run");

    // Prune unseen events for src-prune with current observation run run-v2
    let pruned = prune_unseen_usage_events_sqlx(pool, "default", "src-prune", "run-v2")
        .await
        .expect("prune unseen");
    assert_eq!(pruned, 1); // ev_old_2 should be pruned

    // Verify remaining events for src-prune
    let remaining_prune_ids: Vec<String> = sqlx::query_scalar(
        "SELECT external_event_id FROM conversation_usage_events WHERE tenant_id = 'default' AND source_id = 'src-prune'",
    )
    .fetch_all(pool)
    .await
    .expect("fetch remaining");
    assert_eq!(remaining_prune_ids, vec!["sess-1-turn-0".to_string()]);

    // Verify src-other events were not pruned
    let remaining_other_ids: Vec<String> = sqlx::query_scalar(
        "SELECT external_event_id FROM conversation_usage_events WHERE tenant_id = 'default' AND source_id = 'src-other'",
    )
    .fetch_all(pool)
    .await
    .expect("fetch other");
    assert_eq!(remaining_other_ids, vec!["other-turn-0".to_string()]);

    drop(database);
    std::fs::remove_file(path).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_cross_day_continuation_idempotency_and_timestamp_retention() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-usage-cross-day-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();

    // Day 1: Session starts with Turn 0
    let turn_0_day_1 = RawUsageEventInput {
        external_event_id: "sess-cross-turn-0".to_string(),
        session_id: Some("sess-cross".to_string()),
        turn_id: Some("turn-0".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-10-29T10:00:00Z".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-3-7-sonnet".to_string(),
        status: Some("success".to_string()),
        input_tokens: 1000,
        output_tokens: 500,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: Some(0.015),
        currency: Some("USD".to_string()),
    };

    upsert_usage_events_sqlx(
        pool,
        "default",
        "src-cross",
        "antigravity",
        &[turn_0_day_1.clone()],
        Some("run-day-1"),
    )
    .await
    .expect("upsert day 1");

    // Day 2: Session continued with Turn 1.
    // The adapter re-reads the full log and emits Turn 0 (retaining Day 1 timestamp) and Turn 1 (Day 2 timestamp).
    let turn_0_day_2_scan = RawUsageEventInput {
        external_event_id: "sess-cross-turn-0".to_string(),
        session_id: Some("sess-cross".to_string()),
        turn_id: Some("turn-0".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-10-29T10:00:00Z".to_string(), // Crucial: historical timestamp preserved
        provider: "anthropic".to_string(),
        model: "claude-3-7-sonnet".to_string(),
        status: Some("success".to_string()),
        input_tokens: 1000,
        output_tokens: 500,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: Some(0.015),
        currency: Some("USD".to_string()),
    };

    let turn_1_day_2 = RawUsageEventInput {
        external_event_id: "sess-cross-turn-1".to_string(),
        session_id: Some("sess-cross".to_string()),
        turn_id: Some("turn-1".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-10-30T09:00:00Z".to_string(), // New turn today
        provider: "anthropic".to_string(),
        model: "claude-3-7-sonnet".to_string(),
        status: Some("success".to_string()),
        input_tokens: 2000,
        output_tokens: 800,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: Some(0.025),
        currency: Some("USD".to_string()),
    };

    upsert_usage_events_sqlx(
        pool,
        "default",
        "src-cross",
        "antigravity",
        &[turn_0_day_2_scan, turn_1_day_2],
        Some("run-day-2"),
    )
    .await
    .expect("upsert day 2");

    // Verify row count is exactly 2 (no duplicate turn 0)
    let total_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_usage_events WHERE tenant_id = 'default' AND source_id = 'src-cross'",
    )
    .fetch_one(pool)
    .await
    .expect("query row count");
    assert_eq!(total_rows, 2);

    // Verify timestamp of turn 0 is still 2026-10-29T10:00:00Z
    let turn_0_ts: String = sqlx::query_scalar(
        "SELECT timestamp FROM conversation_usage_events WHERE external_event_id = 'sess-cross-turn-0'",
    )
    .fetch_one(pool)
    .await
    .expect("query turn 0 timestamp");
    assert_eq!(turn_0_ts, "2026-10-29T10:00:00Z");

    // Query dashboard and verify daily buckets: one on 2026-10-29, one on 2026-10-30
    let filter = UsageDashboardFilter {
        time_range: Some("custom".to_string()),
        start_date: Some("2026-10-29".to_string()),
        end_date: Some("2026-10-30".to_string()),
        adapter_id: None,
        source_id: Some("src-cross".to_string()),
        model: None,
        project_path: None,
        timezone_offset_minutes: Some(0),
    };
    let dashboard = query_usage_dashboard_sqlx(pool, "default", &filter)
        .await
        .expect("query dashboard");

    assert_eq!(dashboard.hero.total_tokens, 4300); // 1500 + 2800
    assert_eq!(dashboard.hero.requests_count, 2);

    let bucket_29 = dashboard
        .daily_trend
        .iter()
        .find(|b| b.date == "2026-10-29")
        .expect("2026-10-29 bucket");
    assert_eq!(bucket_29.total_tokens, 1500);
    assert_eq!(bucket_29.requests_count, 1);

    let bucket_30 = dashboard
        .daily_trend
        .iter()
        .find(|b| b.date == "2026-10-30")
        .expect("2026-10-30 bucket");
    assert_eq!(bucket_30.total_tokens, 2800);
    assert_eq!(bucket_30.requests_count, 1);

    drop(database);
    std::fs::remove_file(path).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_adversarial_cross_day_continuation_model_and_tokens_mutation() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-usage-cross-day-mutation-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();

    // Day 1: Session starts with Turn 0
    let turn_0_day_1 = RawUsageEventInput {
        external_event_id: "sess-adv-turn-0".to_string(),
        session_id: Some("sess-adv".to_string()),
        turn_id: Some("turn-0".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-10-28T09:00:00Z".to_string(),
        provider: "openai".to_string(),
        model: "gpt-4o".to_string(),
        status: Some("success".to_string()),
        input_tokens: 500,
        output_tokens: 200,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: Some(0.01),
        currency: Some("USD".to_string()),
    };

    upsert_usage_events_sqlx(
        pool,
        "tenant-adv",
        "src-adv",
        "antigravity",
        &[turn_0_day_1],
        Some("run-day-1"),
    )
    .await
    .expect("upsert day 1");

    // Fetch initial created_at and updated_at
    let (initial_created_at, initial_updated_at): (String, String) = sqlx::query_as(
        "SELECT created_at, updated_at FROM conversation_usage_events WHERE tenant_id = 'tenant-adv' AND external_event_id = 'sess-adv-turn-0'",
    )
    .fetch_one(pool)
    .await
    .expect("fetch initial timestamps");

    // Day 2: Re-ingest Turn 0 with mutated tokens and mutated model, but keeping original Day 1 timestamp!
    // Plus a new Turn 1 on Day 2.
    let turn_0_day_2_mutated = RawUsageEventInput {
        external_event_id: "sess-adv-turn-0".to_string(),
        session_id: Some("sess-adv".to_string()),
        turn_id: Some("turn-0".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-10-28T09:00:00Z".to_string(), // Crucial: historical timestamp unchanged
        provider: "openai".to_string(),
        model: "gpt-4o-2024-11-20".to_string(), // Mutated model
        status: Some("success".to_string()),
        input_tokens: 800,  // Mutated input tokens
        output_tokens: 400, // Mutated output tokens (total 1200)
        cache_read_tokens: 50,
        cache_write_tokens: 0,
        reasoning_tokens: 100,
        host_reported_cost: Some(0.025), // Mutated cost
        currency: Some("USD".to_string()),
    };

    let turn_1_day_2 = RawUsageEventInput {
        external_event_id: "sess-adv-turn-1".to_string(),
        session_id: Some("sess-adv".to_string()),
        turn_id: Some("turn-1".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-10-29T10:00:00Z".to_string(),
        provider: "openai".to_string(),
        model: "gpt-4o-2024-11-20".to_string(),
        status: Some("success".to_string()),
        input_tokens: 1000,
        output_tokens: 500,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: Some(0.02),
        currency: Some("USD".to_string()),
    };

    upsert_usage_events_sqlx(
        pool,
        "tenant-adv",
        "src-adv",
        "antigravity",
        &[turn_0_day_2_mutated, turn_1_day_2],
        Some("run-day-2"),
    )
    .await
    .expect("upsert day 2");

    // Verify row count is 2 (no duplicate row for Turn 0)
    let total_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_usage_events WHERE tenant_id = 'tenant-adv' AND source_id = 'src-adv'",
    )
    .fetch_one(pool)
    .await
    .expect("count rows");
    assert_eq!(total_rows, 2);

    // Verify Turn 0 row details
    let row = sqlx::query(
        "SELECT model, input_tokens, output_tokens, total_tokens, host_reported_cost, timestamp, created_at, updated_at FROM conversation_usage_events WHERE tenant_id = 'tenant-adv' AND external_event_id = 'sess-adv-turn-0'",
    )
    .fetch_one(pool)
    .await
    .expect("fetch turn 0 row");

    let model: String = row.get("model");
    let input_tokens: i64 = row.get("input_tokens");
    let output_tokens: i64 = row.get("output_tokens");
    let total_tokens: i64 = row.get("total_tokens");
    let host_cost: Option<f64> = row.get("host_reported_cost");
    let ts: String = row.get("timestamp");
    let created_at: String = row.get("created_at");
    let updated_at: String = row.get("updated_at");

    assert_eq!(model, "gpt-4o-2024-11-20", "Model must update on conflict");
    assert_eq!(input_tokens, 800, "Input tokens must update");
    assert_eq!(output_tokens, 400, "Output tokens must update");
    assert_eq!(
        total_tokens, 1350,
        "Total tokens must update to sum of input + cache + reasoning + output"
    );
    assert_eq!(
        host_cost,
        Some(0.025 * 7.25),
        "Host cost must update (converted to CNY)"
    );
    assert_eq!(
        ts, "2026-10-28T09:00:00Z",
        "Timestamp must NOT change to Day 2"
    );
    assert_eq!(
        created_at, initial_created_at,
        "created_at must be preserved across upsert"
    );
    assert!(
        updated_at >= initial_updated_at,
        "updated_at must advance or match"
    );

    // Verify dashboard query aggregation by date
    let filter = UsageDashboardFilter {
        time_range: Some("custom".to_string()),
        start_date: Some("2026-10-28".to_string()),
        end_date: Some("2026-10-29".to_string()),
        adapter_id: None,
        source_id: Some("src-adv".to_string()),
        model: None,
        project_path: None,
        timezone_offset_minutes: Some(0),
    };
    let dashboard = query_usage_dashboard_sqlx(pool, "tenant-adv", &filter)
        .await
        .expect("query dashboard");

    assert_eq!(dashboard.hero.total_tokens, 2850); // 1350 + 1500
    assert_eq!(dashboard.hero.requests_count, 2);

    let bucket_day1 = dashboard
        .daily_trend
        .iter()
        .find(|b| b.date == "2026-10-28")
        .expect("day 1 bucket");
    assert_eq!(bucket_day1.total_tokens, 1350);
    assert_eq!(bucket_day1.requests_count, 1);

    let bucket_day2 = dashboard
        .daily_trend
        .iter()
        .find(|b| b.date == "2026-10-29")
        .expect("day 2 bucket");
    assert_eq!(bucket_day2.total_tokens, 1500);
    assert_eq!(bucket_day2.requests_count, 1);

    drop(database);
    std::fs::remove_file(path).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_adversarial_pruning_multi_tenant_and_multi_source_isolation() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-usage-prune-isolation-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();

    let make_event = |id: &str, sess: &str, turn: &str| RawUsageEventInput {
        external_event_id: id.to_string(),
        session_id: Some(sess.to_string()),
        turn_id: Some(turn.to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-10-29T10:00:00Z".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-3-7-sonnet".to_string(),
        status: Some("success".to_string()),
        input_tokens: 100,
        output_tokens: 100,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: None,
        currency: Some("USD".to_string()),
    };

    // 1. Tenant A - Source 1: 2 events with run-A1, 1 event with NULL run
    upsert_usage_events_sqlx(
        pool,
        "tenant-A",
        "src-shared-id",
        "codex",
        &[
            make_event("A-s1-t0", "sess-A1", "turn-0"),
            make_event("A-s1-t1", "sess-A1", "turn-1"),
        ],
        Some("run-A1"),
    )
    .await
    .expect("ingest tenant A src-1");

    upsert_usage_events_sqlx(
        pool,
        "tenant-A",
        "src-shared-id",
        "codex",
        &[make_event("A-s1-null", "sess-A1", "turn-null")],
        None, // observation_run_id is NULL
    )
    .await
    .expect("ingest tenant A src-1 null run");

    // 2. Tenant A - Source 2: 1 event with run-A1, 1 event with NULL run
    upsert_usage_events_sqlx(
        pool,
        "tenant-A",
        "src-distinct-id",
        "codex",
        &[make_event("A-s2-t0", "sess-A2", "turn-0")],
        Some("run-A1"),
    )
    .await
    .expect("ingest tenant A src-2");

    upsert_usage_events_sqlx(
        pool,
        "tenant-A",
        "src-distinct-id",
        "codex",
        &[make_event("A-s2-null", "sess-A2", "turn-null")],
        None,
    )
    .await
    .expect("ingest tenant A src-2 null run");

    // 3. Tenant B - Source 1 (SAME source_id "src-shared-id"!): 2 events with run-B1, 1 with NULL run
    upsert_usage_events_sqlx(
        pool,
        "tenant-B",
        "src-shared-id",
        "codex",
        &[
            make_event("B-s1-t0", "sess-B1", "turn-0"),
            make_event("B-s1-t1", "sess-B1", "turn-1"),
        ],
        Some("run-B1"),
    )
    .await
    .expect("ingest tenant B src-1");

    upsert_usage_events_sqlx(
        pool,
        "tenant-B",
        "src-shared-id",
        "codex",
        &[make_event("B-s1-null", "sess-B1", "turn-null")],
        None,
    )
    .await
    .expect("ingest tenant B src-1 null run");

    let count_rows = |pool: sqlx::SqlitePool, tenant: String, src: String| async move {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM conversation_usage_events WHERE tenant_id = ? AND source_id = ?",
        )
        .bind(tenant)
        .bind(src)
        .fetch_one(&pool)
        .await
        .unwrap();
        n
    };

    assert_eq!(
        count_rows(
            pool.clone(),
            "tenant-A".to_string(),
            "src-shared-id".to_string()
        )
        .await,
        3
    );
    assert_eq!(
        count_rows(
            pool.clone(),
            "tenant-A".to_string(),
            "src-distinct-id".to_string()
        )
        .await,
        2
    );
    assert_eq!(
        count_rows(
            pool.clone(),
            "tenant-B".to_string(),
            "src-shared-id".to_string()
        )
        .await,
        3
    );

    // 4. In run-A2, Tenant A src-shared-id ONLY sees A-s1-t0 (A-s1-t1 was deleted, A-s1-null was legacy)
    upsert_usage_events_sqlx(
        pool,
        "tenant-A",
        "src-shared-id",
        "codex",
        &[make_event("A-s1-t0", "sess-A1", "turn-0")],
        Some("run-A2"),
    )
    .await
    .expect("ingest run-A2");

    // Prune for Tenant A, src-shared-id with current run-A2
    let pruned = prune_unseen_usage_events_sqlx(pool, "tenant-A", "src-shared-id", "run-A2")
        .await
        .expect("prune");

    assert_eq!(pruned, 2, "Should prune exactly the 2 unseen events from Tenant A src-shared-id (A-s1-t1 and A-s1-null)");

    // 5. Verify isolation:
    // Tenant A src-shared-id has exactly 1 remaining event (A-s1-t0)
    assert_eq!(
        count_rows(
            pool.clone(),
            "tenant-A".to_string(),
            "src-shared-id".to_string()
        )
        .await,
        1
    );
    let remaining_a_s1: Vec<String> = sqlx::query_scalar(
        "SELECT external_event_id FROM conversation_usage_events WHERE tenant_id = 'tenant-A' AND source_id = 'src-shared-id'",
    )
    .fetch_all(pool)
    .await
    .expect("fetch A s1");
    assert_eq!(remaining_a_s1, vec!["A-s1-t0".to_string()]);

    // Tenant A src-distinct-id MUST be completely untouched (still 2 events)
    assert_eq!(
        count_rows(
            pool.clone(),
            "tenant-A".to_string(),
            "src-distinct-id".to_string()
        )
        .await,
        2,
        "Tenant A other source must not be pruned"
    );

    // Tenant B src-shared-id MUST be completely untouched (still 3 events), despite identical source_id!
    assert_eq!(
        count_rows(
            pool.clone(),
            "tenant-B".to_string(),
            "src-shared-id".to_string()
        )
        .await,
        3,
        "Tenant B must not be pruned when Tenant A prunes same source_id"
    );

    drop(database);
    std::fs::remove_file(path).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_adversarial_migration_boundary_and_fuzz() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-usage-migration-fuzz-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();

    // Insert 10 boundary cases into conversation_usage_events
    // 5 invalid (must be deleted) + 5 valid (must be preserved)
    let make_raw = |id: &str, turn: Option<&str>| RawUsageEventInput {
        external_event_id: id.to_string(),
        session_id: Some("sess-fuzz".to_string()),
        turn_id: turn.map(str::to_string),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-10-29T10:00:00Z".to_string(),
        provider: "openai".to_string(),
        model: "gpt-4o".to_string(),
        status: Some("success".to_string()),
        input_tokens: 100,
        output_tokens: 100,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: None,
        currency: Some("USD".to_string()),
    };

    let events = vec![
        // Invalid 1: turn_id is None
        make_raw("sess-turn-none", None),
        // Invalid 2: turn_id is empty string
        make_raw("sess-turn-empty", Some("")),
        // Invalid 3: turn_id is whitespace (spaces)
        make_raw("sess-turn-whitespace", Some("      ")),
        // Invalid 4: valid turn_id, but external_event_id has no -turn- or -msg-
        make_raw("sess_no_separator_turn_0", Some("turn-0")),
        // Invalid 5: external_event_id is "turn-0" (no leading session / hyphen before turn)
        make_raw("turn-0", Some("turn-0")),
        // Valid 1: standard turn format
        make_raw("sess-1-turn-0", Some("turn-0")),
        // Valid 2: standard message format
        make_raw("sess-2-msg-10", Some("msg-10")),
        // Valid 3: minimal valid -turn-
        make_raw("prefix-turn-suffix", Some("turn-x")),
        // Valid 4: minimal valid -msg-
        make_raw("prefix-msg-suffix", Some("msg-y")),
        // Valid 5: complex session id with hyphens and -turn-
        make_raw("project-sub-app-turn-999", Some("turn-999")),
    ];

    upsert_usage_events_sqlx(pool, "default", "src-fuzz", "antigravity", &events, None)
        .await
        .expect("upsert fuzz events");

    // Set cursors across 3 sources
    let make_state = |source_id: &str, cursor: Option<&str>| UsageSourceState {
        tenant_id: "default".to_string(),
        source_id: source_id.to_string(),
        adapter_id: "antigravity".to_string(),
        decoder_profile: None,
        schema_fingerprint: None,
        opaque_cursor: cursor.map(str::to_string),
        last_success_scan_at: None,
        last_full_scan_at: None,
        status: "ready".to_string(),
        diagnostics_json: None,
        created_at: "2026-10-29T00:00:00Z".to_string(),
        updated_at: "2026-10-29T00:00:00Z".to_string(),
    };

    save_usage_source_state_sqlx(pool, &make_state("src-1", Some("cur-1")))
        .await
        .unwrap();
    save_usage_source_state_sqlx(pool, &make_state("src-2", Some("cur-2")))
        .await
        .unwrap();
    save_usage_source_state_sqlx(pool, &make_state("src-3", None))
        .await
        .unwrap();

    let count_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_usage_events WHERE source_id = 'src-fuzz'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(count_before, 10);

    // Execute migration SQL
    let migration_sql = include_str!(
        "../../../migrations/20261030000002_turn_level_usage_cleanup_and_cursor_reset.sql"
    );
    let conn = rusqlite::Connection::open(&path).expect("open rusqlite connection");
    conn.execute_batch(migration_sql)
        .expect("execute migration sql");
    drop(conn);

    // Verify exactly the 5 valid events remain
    let remaining: Vec<String> = sqlx::query_scalar(
        "SELECT external_event_id FROM conversation_usage_events WHERE source_id = 'src-fuzz' ORDER BY external_event_id",
    )
    .fetch_all(pool)
    .await
    .unwrap();

    let expected = vec![
        "prefix-msg-suffix".to_string(),
        "prefix-turn-suffix".to_string(),
        "project-sub-app-turn-999".to_string(),
        "sess-1-turn-0".to_string(),
        "sess-2-msg-10".to_string(),
    ];
    assert_eq!(
        remaining, expected,
        "Only valid turn/msg records must survive migration"
    );

    // Verify all source state cursors reset to None
    let states = load_all_usage_source_states_sqlx(pool, "default")
        .await
        .unwrap();
    assert_eq!(states.len(), 3);
    for s in states {
        assert_eq!(
            s.opaque_cursor, None,
            "All source cursors must be reset to None"
        );
    }

    drop(database);
    std::fs::remove_file(path).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_usage_dashboard_merges_base_model_variants_and_rebinds_and_isolates_unmatched_and_ignored(
) {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-usage-merge-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();
    let tenant_id = "default";

    // Seed a manual rebind: "Flash -N" -> "gemini-3.8-flash"
    let gemini_target = crate::backend::domain::conversations::pricing::ModelPriceEntry {
        provider: "google".to_string(),
        model_patterns: vec!["gemini-3.8-flash".to_string()],
        currency: "CNY".to_string(),
        input_per_million: 0.725,
        cache_read_per_million: 0.18125,
        cache_write_per_million: 0.725,
        reasoning_per_million: 2.9,
        output_per_million: 2.9,
        raw_currency: Some("USD".to_string()),
        raw_input_per_million: Some(0.10),
        raw_output_per_million: Some(0.40),
        ..Default::default()
    };
    crate::backend::store::conversations::pricing_repo::rebind_model_pricing_alias_sqlx(
        pool,
        tenant_id,
        "Flash -N",
        Some(&gemini_target),
    )
    .await
    .expect("rebind Flash -N to gemini-3.8-flash");

    // Mark a dirty/noise raw string as ignored
    let ignored_raw = r#"noisy "log" line"#;
    crate::backend::store::conversations::pricing_repo::set_model_pricing_ignored_sqlx(
        pool,
        tenant_id,
        ignored_raw,
        Some("custom"),
        true,
    )
    .await
    .expect("ignore dirty model");

    let make_ev =
        |id: &str, provider: &str, model: &str, input: i64, output: i64| RawUsageEventInput {
            external_event_id: id.to_string(),
            session_id: Some("sess-merge".to_string()),
            turn_id: Some(format!("turn-{id}")),
            logical_request_id: None,
            attempt_index: None,
            timestamp: "2026-10-15T10:00:00Z".to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            status: Some("success".to_string()),
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
            host_reported_cost: None,
            currency: Some("CNY".to_string()),
        };

    // Counts per raw variant resolving to gemini-3.8-flash:
    // "Flash -N": 4 requests
    // "Gemini 3.8 Flash (High)": 2 requests
    // "gemini-3.8-flash": 1 request (lex tie-break check against "gemini-3.8-flash-tail")
    // "gemini-3.8-flash-tail": 1 request
    let events = vec![
        make_ev("e1", "antigravity", "Flash -N", 1000, 500),
        make_ev("e2", "antigravity", "Flash -N", 1000, 500),
        make_ev("e3", "antigravity", "Flash -N", 1000, 500),
        make_ev("e4", "antigravity", "Flash -N", 1000, 500),
        make_ev("e5", "google", "Gemini 3.8 Flash (High)", 2000, 1000),
        make_ev("e6", "google", "Gemini 3.8 Flash (High)", 2000, 1000),
        make_ev("e7", "google", "gemini-3.8-flash-tail", 500, 500),
        make_ev("e8", "google", "gemini-3.8-flash", 500, 500),
        // Unmatched model
        make_ev("e9", "private-gw", "my-custom-unmatched-v1", 300, 200),
        // Ignored model
        make_ev("e10", "custom", ignored_raw, 400, 100),
    ];

    let inserted =
        upsert_usage_events_sqlx(pool, tenant_id, "src-merge", "antigravity", &events, None)
            .await
            .expect("ingest events");
    assert_eq!(inserted, 10);

    let filter = UsageDashboardFilter {
        time_range: Some("all_time".to_string()),
        ..Default::default()
    };
    let dashboard = query_usage_dashboard_sqlx(pool, tenant_id, &filter)
        .await
        .expect("query dashboard");

    // Expect exactly 3 buckets:
    // 1. Merged "gemini-3.8-flash"
    // 2. Unmatched "my-custom-unmatched-v1"
    // 3. Ignored noisy raw model
    assert_eq!(dashboard.models.len(), 3);

    let gemini_row = dashboard
        .models
        .iter()
        .find(|m| m.model == "gemini-3.8-flash")
        .expect("merged gemini-3.8-flash row");
    assert_eq!(
        gemini_row.matched_model_id.as_deref(),
        Some("gemini-3.8-flash")
    );
    assert_eq!(gemini_row.provider, "google");
    assert_eq!(gemini_row.requests_count, 8);
    assert_eq!(gemini_row.input_tokens, 9000);
    assert_eq!(gemini_row.output_tokens, 5000);
    assert_eq!(gemini_row.total_tokens, 14000);
    assert_eq!(
        gemini_row.raw_models,
        vec![
            "Flash -N".to_string(),
            "Gemini 3.8 Flash (High)".to_string(),
            "gemini-3.8-flash".to_string(),
            "gemini-3.8-flash-tail".to_string(),
        ]
    );
    assert_eq!(gemini_row.cost_basis, "catalog_estimated");
    assert!(gemini_row.cost.is_some());
    assert_eq!(gemini_row.unit_price_per_million, Some(0.73)); // 0.725 rounded to 2 decimals

    let unmatched_row = dashboard
        .models
        .iter()
        .find(|m| m.model == "my-custom-unmatched-v1")
        .expect("unmatched row");
    assert_eq!(unmatched_row.matched_model_id, None);
    assert_eq!(unmatched_row.provider, "private-gw");
    assert_eq!(unmatched_row.requests_count, 1);
    assert_eq!(unmatched_row.total_tokens, 500);
    assert_eq!(unmatched_row.cost, None);
    assert_eq!(unmatched_row.cost_basis, "none");
    assert_eq!(unmatched_row.unit_price_per_million, None);
    assert_eq!(
        unmatched_row.raw_models,
        vec!["my-custom-unmatched-v1".to_string()]
    );

    let ignored_row = dashboard
        .models
        .iter()
        .find(|m| m.model == ignored_raw)
        .expect("ignored row isolated by raw string");
    assert_eq!(ignored_row.matched_model_id, None);
    assert_eq!(ignored_row.requests_count, 1);
    assert_eq!(ignored_row.cost, Some(0.0));
    assert_eq!(ignored_row.cost_basis, "catalog_estimated");
    assert_eq!(ignored_row.unit_price_per_million, Some(0.0));
    assert_eq!(ignored_row.raw_models, vec![ignored_raw.to_string()]);

    drop(database);
    std::fs::remove_file(path).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_usage_dashboard_locked_zero_override_merges_and_reports_zero_cost() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-usage-locked-zero-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_initialized_async(&path)
        .await
        .expect("open test database");
    let pool = database.pool();
    let tenant_id = "default";

    let events = vec![
        RawUsageEventInput {
            external_event_id: "lz-1".to_string(),
            session_id: Some("sess-lz".to_string()),
            turn_id: Some("turn-1".to_string()),
            logical_request_id: None,
            attempt_index: None,
            timestamp: "2026-10-16T08:00:00Z".to_string(),
            provider: "opencode".to_string(),
            model: "mimo-v2.5-free".to_string(),
            status: Some("success".to_string()),
            input_tokens: 1_000_000,
            output_tokens: 500_000,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
            host_reported_cost: None,
            currency: Some("CNY".to_string()),
        },
        RawUsageEventInput {
            external_event_id: "lz-2".to_string(),
            session_id: Some("sess-lz".to_string()),
            turn_id: Some("turn-2".to_string()),
            logical_request_id: None,
            attempt_index: None,
            timestamp: "2026-10-16T08:01:00Z".to_string(),
            provider: "xiaomi".to_string(),
            model: "mimo-v2.5-tail".to_string(),
            status: Some("success".to_string()),
            input_tokens: 500_000,
            output_tokens: 250_000,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
            host_reported_cost: None,
            currency: Some("CNY".to_string()),
        },
        RawUsageEventInput {
            external_event_id: "lz-3".to_string(),
            session_id: Some("sess-lz".to_string()),
            turn_id: Some("turn-3".to_string()),
            logical_request_id: None,
            attempt_index: None,
            timestamp: "2026-10-16T08:02:00Z".to_string(),
            provider: "xiaomi".to_string(),
            model: "mimo-v2.5-tail".to_string(),
            status: Some("success".to_string()),
            input_tokens: 500_000,
            output_tokens: 250_000,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
            host_reported_cost: None,
            currency: Some("CNY".to_string()),
        },
    ];

    upsert_usage_events_sqlx(pool, tenant_id, "src-lz", "opencode", &events, None)
        .await
        .expect("ingest initial events");

    // Apply locked ¥0 override to base model "mimo-v2.5" and recalculate costs
    let locked_zero = crate::backend::domain::conversations::pricing::ModelPriceEntry {
        provider: "xiaomi".to_string(),
        model_patterns: vec!["mimo-v2.5".to_string()],
        currency: "CNY".to_string(),
        input_per_million: 0.0,
        cache_read_per_million: 0.0,
        cache_write_per_million: 0.0,
        reasoning_per_million: 0.0,
        output_per_million: 0.0,
        raw_currency: Some("CNY".to_string()),
        raw_input_per_million: Some(0.0),
        raw_output_per_million: Some(0.0),
        ..Default::default()
    };
    crate::backend::store::conversations::pricing_repo::save_locked_override_sqlx(
        pool,
        tenant_id,
        "mimo-v2.5",
        &locked_zero,
    )
    .await
    .expect("save locked zero override");

    let loaded_entries =
        crate::backend::store::conversations::pricing_repo::load_pricing_catalog_entries_sqlx(
            pool, tenant_id,
        )
        .await
        .expect("load catalog entries");
    let catalog = crate::backend::domain::conversations::pricing::PriceCatalog::new(
        "v-locked-zero".to_string(),
        "2026-10-16T09:00:00Z".to_string(),
        7.25,
        loaded_entries,
    );
    let recalc = crate::backend::store::conversations::pricing_repo::recalculate_usage_costs_sqlx(
        pool, tenant_id, &catalog,
    )
    .await
    .expect("recalculate usage costs");
    assert_eq!(recalc, 3);

    let dashboard = query_usage_dashboard_sqlx(
        pool,
        tenant_id,
        &UsageDashboardFilter {
            time_range: Some("all_time".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("query dashboard");

    assert_eq!(dashboard.models.len(), 1);
    let row = &dashboard.models[0];
    assert_eq!(row.model, "mimo-v2.5");
    assert_eq!(row.matched_model_id.as_deref(), Some("mimo-v2.5"));
    assert_eq!(row.provider, "xiaomi");
    assert_eq!(row.requests_count, 3);
    assert_eq!(row.cost, Some(0.0));
    assert_eq!(row.cost_basis, "catalog_estimated");
    assert_eq!(row.unit_price_per_million, Some(0.0));
    assert_eq!(
        row.raw_models,
        vec!["mimo-v2.5-tail".to_string(), "mimo-v2.5-free".to_string()]
    );

    drop(database);
    std::fs::remove_file(path).ok();
}
