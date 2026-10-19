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
