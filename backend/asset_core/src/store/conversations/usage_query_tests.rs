use super::*;
use crate::backend::store::conversations::usage_repo::{
    save_usage_source_state_sqlx, upsert_usage_events_sqlx, RawUsageEventInput, UsageSourceState,
};
use crate::backend::store::Database;

struct TestDb {
    database: Database,
    path: std::path::PathBuf,
}

impl TestDb {
    async fn open() -> Self {
        let path = std::env::temp_dir().join(format!(
            "assetiweave-usage-query-test-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = Database::open_initialized_async(&path)
            .await
            .expect("open test database");
        Self { database, path }
    }

    fn pool(&self) -> &sqlx::SqlitePool {
        self.database.pool()
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        std::fs::remove_file(&self.path).ok();
    }
}

fn build_event(id: &str, timestamp: &str, provider: &str, model: &str) -> RawUsageEventInput {
    RawUsageEventInput {
        external_event_id: id.to_string(),
        session_id: Some(format!("session-{id}")),
        turn_id: None,
        logical_request_id: None,
        attempt_index: None,
        timestamp: timestamp.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        status: Some("success".to_string()),
        input_tokens: 1000,
        output_tokens: 500,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        host_reported_cost: Some(0.01),
        currency: Some("CNY".to_string()),
    }
}

fn all_time_filter() -> UsageDashboardFilter {
    UsageDashboardFilter {
        time_range: Some("all_time".to_string()),
        timezone_offset_minutes: Some(0),
        ..Default::default()
    }
}

fn build_source_state(
    source_id: &str,
    adapter_id: &str,
    status: &str,
    diagnostics: Option<Vec<String>>,
) -> UsageSourceState {
    UsageSourceState {
        tenant_id: "default".to_string(),
        source_id: source_id.to_string(),
        adapter_id: adapter_id.to_string(),
        decoder_profile: None,
        schema_fingerprint: None,
        opaque_cursor: None,
        last_success_scan_at: Some("2026-01-02T03:04:05Z".to_string()),
        last_full_scan_at: None,
        status: status.to_string(),
        diagnostics_json: diagnostics.map(|d| serde_json::to_string(&d).unwrap()),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-02T03:04:05Z".to_string(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn dashboard_scan_status_surfaces_error_source_diagnostics() {
    let db = TestDb::open().await;
    let pool = db.pool();

    save_usage_source_state_sqlx(
        pool,
        &build_source_state(
            "claude-src-1",
            "claude",
            "error",
            Some(vec!["读取用量失败：权限不足".to_string()]),
        ),
    )
    .await
    .expect("save error source state");

    let dashboard = query_usage_dashboard_sqlx(pool, "default", &all_time_filter())
        .await
        .expect("query dashboard");

    let diagnostic = dashboard
        .scan_status
        .source_diagnostics
        .iter()
        .find(|d| d.source_id == "claude-src-1")
        .expect("error source diagnostic must be surfaced");
    assert_eq!(diagnostic.level, "error");
    assert_eq!(diagnostic.adapter_id, "claude");
    assert_eq!(diagnostic.message, "读取用量失败：权限不足");
    assert_eq!(dashboard.scan_status.scanned_sources_count, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn dashboard_total_events_count_is_tenant_wide_not_window_scoped() {
    let db = TestDb::open().await;
    let pool = db.pool();

    let recent = Utc::now().to_rfc3339();
    upsert_usage_events_sqlx(
        pool,
        "default",
        "codex-src-1",
        "codex",
        &[
            build_event("evt-recent", &recent, "openai", "gpt-4o"),
            build_event("evt-old", "2020-01-05T00:00:00Z", "openai", "gpt-4o"),
        ],
        None,
    )
    .await
    .expect("insert usage events");

    let filter = UsageDashboardFilter {
        time_range: Some("last_7_days".to_string()),
        timezone_offset_minutes: Some(0),
        ..Default::default()
    };
    let dashboard = query_usage_dashboard_sqlx(pool, "default", &filter)
        .await
        .expect("query dashboard");

    // 窗口内请求数只含近 7 天的一条，已入库事件总数仍是全库两条。
    assert_eq!(dashboard.hero.requests_count, 1);
    assert_eq!(dashboard.scan_status.total_events_count, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn dashboard_model_filter_expands_base_model_to_raw_model_names() {
    let db = TestDb::open().await;
    let pool = db.pool();

    upsert_usage_events_sqlx(
        pool,
        "default",
        "xiaomi-src-1",
        "opencode",
        &[
            build_event(
                "evt-mimo",
                "2026-01-02T00:00:00Z",
                "xiaomi",
                "mimo-v2.5-free",
            ),
            build_event("evt-gpt", "2026-01-02T01:00:00Z", "openai", "gpt-4o"),
        ],
        None,
    )
    .await
    .expect("insert usage events");

    let filter = UsageDashboardFilter {
        model: Some("mimo-v2.5".to_string()),
        ..all_time_filter()
    };
    let dashboard = query_usage_dashboard_sqlx(pool, "default", &filter)
        .await
        .expect("query dashboard");

    assert_eq!(dashboard.hero.requests_count, 1);
    let model = dashboard
        .models
        .iter()
        .find(|m| m.model == "mimo-v2.5")
        .expect("base model bucket must be present");
    assert_eq!(model.raw_models, vec!["mimo-v2.5-free".to_string()]);
}
