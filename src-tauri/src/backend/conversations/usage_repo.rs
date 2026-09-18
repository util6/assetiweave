use crate::backend::{
    conversations::pricing::PriceCatalog,
    dto::{
        CurrencyCostDto, UsageDailyTrendBucketDto, UsageDashboardDto, UsageDashboardFilter,
        UsageDateBreakdownDto, UsageHeroOverviewDto, UsageModelBreakdownDto, UsageScanStatusDto,
        UsageSourceBreakdownDto,
    },
    runtime::AppResult,
};
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{AssertSqlSafe, Row as SqlxRow, SqlitePool};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct RawUsageEventInput {
    pub(crate) external_event_id: String,
    pub(crate) session_id: Option<String>,
    pub(crate) turn_id: Option<String>,
    pub(crate) logical_request_id: Option<String>,
    pub(crate) attempt_index: Option<u32>,
    pub(crate) timestamp: String,
    #[serde(default)]
    pub(crate) provider: String,
    #[serde(default)]
    pub(crate) model: String,
    pub(crate) status: Option<String>,
    #[serde(default)]
    pub(crate) input_tokens: i64,
    #[serde(default)]
    pub(crate) cache_read_tokens: i64,
    #[serde(default)]
    pub(crate) cache_write_tokens: i64,
    #[serde(default)]
    pub(crate) reasoning_tokens: i64,
    #[serde(default)]
    pub(crate) output_tokens: i64,
    #[serde(default, alias = "cost")]
    pub(crate) host_reported_cost: Option<f64>,
    pub(crate) currency: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct UsageSourceState {
    pub(crate) tenant_id: String,
    pub(crate) source_id: String,
    pub(crate) adapter_id: String,
    pub(crate) decoder_profile: Option<String>,
    pub(crate) schema_fingerprint: Option<String>,
    pub(crate) opaque_cursor: Option<String>,
    pub(crate) last_success_scan_at: Option<String>,
    pub(crate) last_full_scan_at: Option<String>,
    pub(crate) status: String,
    pub(crate) diagnostics_json: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

pub(crate) async fn upsert_usage_events_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
    adapter_id: &str,
    events: &[RawUsageEventInput],
    observation_run_id: Option<&str>,
) -> AppResult<usize> {
    if events.is_empty() {
        return Ok(0);
    }
    let catalog = PriceCatalog::bundled();
    let mut inserted_count = 0usize;
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;

    for ev in events {
        let input_tokens = ev.input_tokens.max(0);
        let cache_read_tokens = ev.cache_read_tokens.max(0);
        let cache_write_tokens = ev.cache_write_tokens.max(0);
        let reasoning_tokens = ev.reasoning_tokens.max(0);
        let output_tokens = ev.output_tokens.max(0);

        let total_input_tokens = input_tokens + cache_read_tokens + cache_write_tokens;
        let total_output_tokens = reasoning_tokens + output_tokens;
        let total_tokens = total_input_tokens + total_output_tokens;

        // Skip in-flight or empty placeholder records with zero tokens
        if total_tokens == 0 {
            continue;
        }

        let calculated = if ev.host_reported_cost.is_none() {
            catalog.calculate_cost(
                &ev.provider,
                &ev.model,
                input_tokens,
                cache_read_tokens,
                cache_write_tokens,
                reasoning_tokens,
                output_tokens,
            )
        } else {
            None
        };

        // Cost estimation / host reported cost logic
        let (host_cost, catalog_cost, cost_basis, currency, price_version) =
            if let Some(h_cost) = ev.host_reported_cost {
                (
                    Some(h_cost),
                    None,
                    "host_reported",
                    ev.currency.as_deref().unwrap_or("USD"),
                    None,
                )
            } else if let Some(ref calc) = calculated {
                (
                    None,
                    Some(calc.estimated_cost),
                    "catalog_estimated",
                    calc.currency.as_str(),
                    Some(calc.catalog_version.as_str()),
                )
            } else {
                (
                    None,
                    None,
                    "none",
                    ev.currency.as_deref().unwrap_or("USD"),
                    None,
                )
            };

        let status = ev.status.as_deref().unwrap_or("completed");
        let attempt_index = ev.attempt_index.unwrap_or(0) as i64;

        sqlx::query(
            r#"
            INSERT INTO conversation_usage_events (
                tenant_id, source_id, adapter_id, external_event_id, session_id, turn_id,
                logical_request_id, attempt_index, timestamp, provider, model, status,
                input_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
                output_tokens, total_input_tokens, total_output_tokens, total_tokens,
                host_reported_cost, catalog_estimated_cost, cost_basis, currency,
                price_catalog_version, observation_run_id, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(tenant_id, source_id, external_event_id) DO UPDATE SET
                session_id = excluded.session_id,
                turn_id = excluded.turn_id,
                logical_request_id = excluded.logical_request_id,
                attempt_index = excluded.attempt_index,
                timestamp = excluded.timestamp,
                provider = excluded.provider,
                model = excluded.model,
                status = excluded.status,
                input_tokens = excluded.input_tokens,
                cache_read_tokens = excluded.cache_read_tokens,
                cache_write_tokens = excluded.cache_write_tokens,
                reasoning_tokens = excluded.reasoning_tokens,
                output_tokens = excluded.output_tokens,
                total_input_tokens = excluded.total_input_tokens,
                total_output_tokens = excluded.total_output_tokens,
                total_tokens = excluded.total_tokens,
                host_reported_cost = excluded.host_reported_cost,
                catalog_estimated_cost = excluded.catalog_estimated_cost,
                cost_basis = excluded.cost_basis,
                currency = excluded.currency,
                price_catalog_version = excluded.price_catalog_version,
                observation_run_id = excluded.observation_run_id,
                updated_at = excluded.updated_at
            "#
        )
        .bind(tenant_id)
        .bind(source_id)
        .bind(adapter_id)
        .bind(&ev.external_event_id)
        .bind(&ev.session_id)
        .bind(&ev.turn_id)
        .bind(&ev.logical_request_id)
        .bind(attempt_index)
        .bind(&ev.timestamp)
        .bind(&ev.provider)
        .bind(&ev.model)
        .bind(status)
        .bind(input_tokens)
        .bind(cache_read_tokens)
        .bind(cache_write_tokens)
        .bind(reasoning_tokens)
        .bind(output_tokens)
        .bind(total_input_tokens)
        .bind(total_output_tokens)
        .bind(total_tokens)
        .bind(host_cost)
        .bind(catalog_cost)
        .bind(cost_basis)
        .bind(currency)
        .bind(price_version)
        .bind(observation_run_id)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        inserted_count += 1;
    }

    tx.commit().await?;
    Ok(inserted_count)
}

pub(crate) async fn prune_unseen_usage_events_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
    current_observation_run_id: &str,
) -> AppResult<u64> {
    let result = sqlx::query(
        "DELETE FROM conversation_usage_events WHERE tenant_id = ? AND source_id = ? AND (observation_run_id IS NULL OR observation_run_id != ?)"
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(current_observation_run_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

pub(crate) async fn load_usage_source_state_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
) -> AppResult<Option<UsageSourceState>> {
    let row = sqlx::query(
        r#"
        SELECT tenant_id, source_id, adapter_id, decoder_profile, schema_fingerprint,
               opaque_cursor, last_success_scan_at, last_full_scan_at, status,
               diagnostics_json, created_at, updated_at
        FROM conversation_usage_source_states
        WHERE tenant_id = ? AND source_id = ?
        "#,
    )
    .bind(tenant_id)
    .bind(source_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| UsageSourceState {
        tenant_id: r.get("tenant_id"),
        source_id: r.get("source_id"),
        adapter_id: r.get("adapter_id"),
        decoder_profile: r.get("decoder_profile"),
        schema_fingerprint: r.get("schema_fingerprint"),
        opaque_cursor: r.get("opaque_cursor"),
        last_success_scan_at: r.get("last_success_scan_at"),
        last_full_scan_at: r.get("last_full_scan_at"),
        status: r.get("status"),
        diagnostics_json: r.get("diagnostics_json"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }))
}

pub(crate) async fn save_usage_source_state_sqlx(
    pool: &SqlitePool,
    state: &UsageSourceState,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_usage_source_states (
            tenant_id, source_id, adapter_id, decoder_profile, schema_fingerprint,
            opaque_cursor, last_success_scan_at, last_full_scan_at, status,
            diagnostics_json, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(tenant_id, source_id) DO UPDATE SET
            adapter_id = excluded.adapter_id,
            decoder_profile = excluded.decoder_profile,
            schema_fingerprint = excluded.schema_fingerprint,
            opaque_cursor = excluded.opaque_cursor,
            last_success_scan_at = excluded.last_success_scan_at,
            last_full_scan_at = excluded.last_full_scan_at,
            status = excluded.status,
            diagnostics_json = excluded.diagnostics_json,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&state.tenant_id)
    .bind(&state.source_id)
    .bind(&state.adapter_id)
    .bind(&state.decoder_profile)
    .bind(&state.schema_fingerprint)
    .bind(&state.opaque_cursor)
    .bind(&state.last_success_scan_at)
    .bind(&state.last_full_scan_at)
    .bind(&state.status)
    .bind(&state.diagnostics_json)
    .bind(&state.created_at)
    .bind(&state.updated_at)
    .execute(pool)
    .await?;

    Ok(())
}

pub(crate) async fn load_all_usage_source_states_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<UsageSourceState>> {
    let rows = sqlx::query(
        r#"
        SELECT tenant_id, source_id, adapter_id, decoder_profile, schema_fingerprint,
               opaque_cursor, last_success_scan_at, last_full_scan_at, status,
               diagnostics_json, created_at, updated_at
        FROM conversation_usage_source_states
        WHERE tenant_id = ?
        "#,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| UsageSourceState {
            tenant_id: r.get("tenant_id"),
            source_id: r.get("source_id"),
            adapter_id: r.get("adapter_id"),
            decoder_profile: r.get("decoder_profile"),
            schema_fingerprint: r.get("schema_fingerprint"),
            opaque_cursor: r.get("opaque_cursor"),
            last_success_scan_at: r.get("last_success_scan_at"),
            last_full_scan_at: r.get("last_full_scan_at"),
            status: r.get("status"),
            diagnostics_json: r.get("diagnostics_json"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
        .collect())
}

#[derive(Debug)]
struct LoadedUsageRow {
    timestamp: String,
    adapter_id: String,
    source_id: String,
    provider: String,
    model: String,
    input_tokens: i64,
    cache_read_tokens: i64,
    cache_write_tokens: i64,
    reasoning_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
    cost: f64,
    cost_basis: String,
    currency: String,
}

pub(crate) async fn query_usage_dashboard_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    filter: &UsageDashboardFilter,
) -> AppResult<UsageDashboardDto> {
    let tz_offset_mins = filter.timezone_offset_minutes.unwrap_or(0);
    let tz_offset_duration = Duration::minutes(tz_offset_mins as i64);

    let now_utc = Utc::now();
    let now_local = now_utc + tz_offset_duration;
    let today_local = now_local.date_naive();

    let time_range = filter.time_range.as_deref().unwrap_or("last_30_days");
    let (start_date_local, start_utc_str, is_all_time) = match time_range {
        "today" => {
            let start = today_local;
            let start_local_dt = start.and_hms_opt(0, 0, 0).unwrap();
            let start_utc = DateTime::<Utc>::from_naive_utc_and_offset(
                start_local_dt - tz_offset_duration,
                Utc,
            );
            (start, start_utc.to_rfc3339(), false)
        }
        "last_7_days" => {
            let start = today_local - Duration::days(6);
            let start_local_dt = start.and_hms_opt(0, 0, 0).unwrap();
            let start_utc = DateTime::<Utc>::from_naive_utc_and_offset(
                start_local_dt - tz_offset_duration,
                Utc,
            );
            (start, start_utc.to_rfc3339(), false)
        }
        "last_90_days" => {
            let start = today_local - Duration::days(89);
            let start_local_dt = start.and_hms_opt(0, 0, 0).unwrap();
            let start_utc = DateTime::<Utc>::from_naive_utc_and_offset(
                start_local_dt - tz_offset_duration,
                Utc,
            );
            (start, start_utc.to_rfc3339(), false)
        }
        "this_month" => {
            let start =
                NaiveDate::from_ymd_opt(today_local.year(), today_local.month(), 1).unwrap();
            let start_local_dt = start.and_hms_opt(0, 0, 0).unwrap();
            let start_utc = DateTime::<Utc>::from_naive_utc_and_offset(
                start_local_dt - tz_offset_duration,
                Utc,
            );
            (start, start_utc.to_rfc3339(), false)
        }
        "all_time" => (
            NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            "1970-01-01T00:00:00Z".to_string(),
            true,
        ),
        _ => {
            // "last_30_days" is default
            let start = today_local - Duration::days(29);
            let start_local_dt = start.and_hms_opt(0, 0, 0).unwrap();
            let start_utc = DateTime::<Utc>::from_naive_utc_and_offset(
                start_local_dt - tz_offset_duration,
                Utc,
            );
            (start, start_utc.to_rfc3339(), false)
        }
    };

    // Construct SQL query with optional filters
    let mut sql = String::from(
        r#"
        SELECT timestamp, adapter_id, source_id, provider, model,
               input_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
               output_tokens, total_tokens,
               COALESCE(host_reported_cost, catalog_estimated_cost, 0.0) AS cost,
               cost_basis, currency
        FROM conversation_usage_events
        WHERE tenant_id = ?
        "#,
    );

    if !is_all_time {
        sql.push_str(" AND timestamp >= ?");
    }
    if filter.adapter_id.is_some() {
        sql.push_str(" AND adapter_id = ?");
    }
    if filter.source_id.is_some() {
        sql.push_str(" AND source_id = ?");
    }
    if filter.model.is_some() {
        sql.push_str(" AND model = ?");
    }
    sql.push_str(" ORDER BY timestamp ASC");

    let mut query = sqlx::query(AssertSqlSafe(sql.as_str())).bind(tenant_id);
    if !is_all_time {
        query = query.bind(&start_utc_str);
    }
    if let Some(ref aid) = filter.adapter_id {
        query = query.bind(aid);
    }
    if let Some(ref sid) = filter.source_id {
        query = query.bind(sid);
    }
    if let Some(ref m) = filter.model {
        query = query.bind(m);
    }

    let rows = query.fetch_all(pool).await?;
    let mut loaded_events = Vec::with_capacity(rows.len());
    for r in rows {
        loaded_events.push(LoadedUsageRow {
            timestamp: r.get("timestamp"),
            adapter_id: r.get("adapter_id"),
            source_id: r.get("source_id"),
            provider: r.get("provider"),
            model: r.get("model"),
            input_tokens: r.get("input_tokens"),
            cache_read_tokens: r.get("cache_read_tokens"),
            cache_write_tokens: r.get("cache_write_tokens"),
            reasoning_tokens: r.get("reasoning_tokens"),
            output_tokens: r.get("output_tokens"),
            total_tokens: r.get("total_tokens"),
            cost: r.get("cost"),
            cost_basis: r.get("cost_basis"),
            currency: r.get("currency"),
        });
    }

    // Load sources metadata from database to have clean names
    let sources_meta_rows =
        sqlx::query("SELECT id, name, adapter_id FROM conversation_sources WHERE tenant_id = ?")
            .bind(tenant_id)
            .fetch_all(pool)
            .await
            .unwrap_or_default();

    let mut source_name_map = HashMap::new();
    for sm in sources_meta_rows {
        let sid: String = sm.get("id");
        let sname: String = sm.get("name");
        source_name_map.insert(sid, sname);
    }

    // Aggregations:
    let mut total_tokens = 0i64;
    let mut total_input = 0i64;
    let mut total_output = 0i64;
    let mut cache_read_tokens = 0i64;
    let mut cache_write_tokens = 0i64;
    let mut reasoning_tokens = 0i64;
    let requests_count = loaded_events.len() as i64;
    let mut cost_by_currency_map: HashMap<String, f64> = HashMap::new();
    let mut cost_covered_tokens = 0i64;
    let mut cost_covered_requests = 0i64;

    // Daily buckets (local date -> stats)
    let mut daily_map: BTreeMap<NaiveDate, (i64, i64, i64, i64, i64, HashMap<String, f64>)> =
        BTreeMap::new();
    // Pre-populate daily_map for non-all_time with all dates from start_date_local to today_local
    if !is_all_time {
        let mut curr = start_date_local;
        while curr <= today_local {
            daily_map.insert(curr, (0, 0, 0, 0, 0, HashMap::new()));
            curr += Duration::days(1);
        }
    }

    // Model breakdown (model -> stats)
    struct ModelAgg {
        provider: String,
        requests: i64,
        total_tokens: i64,
        input_tokens: i64,
        output_tokens: i64,
        reasoning_tokens: i64,
        cache_tokens: i64,
        cost: f64,
        currency: String,
        cost_basis: String,
    }
    let mut model_map: HashMap<String, ModelAgg> = HashMap::new();

    // Source breakdown (source_id -> stats)
    struct SourceAgg {
        adapter_id: String,
        requests: i64,
        total_tokens: i64,
        input_tokens: i64,
        output_tokens: i64,
        cache_tokens: i64,
        costs: HashMap<String, f64>,
    }
    let mut source_map: HashMap<String, SourceAgg> = HashMap::new();

    for ev in &loaded_events {
        total_tokens += ev.total_tokens;
        total_input += ev.input_tokens + ev.cache_read_tokens + ev.cache_write_tokens;
        total_output += ev.reasoning_tokens + ev.output_tokens;
        cache_read_tokens += ev.cache_read_tokens;
        cache_write_tokens += ev.cache_write_tokens;
        reasoning_tokens += ev.reasoning_tokens;

        if ev.cost_basis != "none" {
            cost_covered_tokens += ev.total_tokens;
            cost_covered_requests += 1;
            *cost_by_currency_map
                .entry(ev.currency.clone())
                .or_insert(0.0) += ev.cost;
        }

        // Determine local date for ev.timestamp
        let ev_utc = DateTime::parse_from_rfc3339(&ev.timestamp)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let ev_local_date = (ev_utc + tz_offset_duration).date_naive();

        let day_entry = daily_map
            .entry(ev_local_date)
            .or_insert_with(|| (0, 0, 0, 0, 0, HashMap::new()));
        day_entry.0 += ev.total_tokens;
        day_entry.1 += 1; // requests
        day_entry.2 += ev.input_tokens + ev.cache_read_tokens + ev.cache_write_tokens;
        day_entry.3 += ev.reasoning_tokens + ev.output_tokens;
        day_entry.4 += ev.cache_read_tokens + ev.cache_write_tokens;
        if ev.cost_basis != "none" {
            *day_entry.5.entry(ev.currency.clone()).or_insert(0.0) += ev.cost;
        }

        // Model agg
        let m_entry = model_map
            .entry(ev.model.clone())
            .or_insert_with(|| ModelAgg {
                provider: ev.provider.clone(),
                requests: 0,
                total_tokens: 0,
                input_tokens: 0,
                output_tokens: 0,
                reasoning_tokens: 0,
                cache_tokens: 0,
                cost: 0.0,
                currency: ev.currency.clone(),
                cost_basis: ev.cost_basis.clone(),
            });
        m_entry.requests += 1;
        m_entry.total_tokens += ev.total_tokens;
        m_entry.input_tokens += ev.input_tokens + ev.cache_read_tokens + ev.cache_write_tokens;
        m_entry.output_tokens += ev.reasoning_tokens + ev.output_tokens;
        m_entry.reasoning_tokens += ev.reasoning_tokens;
        m_entry.cache_tokens += ev.cache_read_tokens + ev.cache_write_tokens;
        if ev.cost_basis != "none" {
            m_entry.cost += ev.cost;
            m_entry.cost_basis = ev.cost_basis.clone();
        }

        // Source agg
        let s_entry = source_map
            .entry(ev.source_id.clone())
            .or_insert_with(|| SourceAgg {
                adapter_id: ev.adapter_id.clone(),
                requests: 0,
                total_tokens: 0,
                input_tokens: 0,
                output_tokens: 0,
                cache_tokens: 0,
                costs: HashMap::new(),
            });
        s_entry.requests += 1;
        s_entry.total_tokens += ev.total_tokens;
        s_entry.input_tokens += ev.input_tokens + ev.cache_read_tokens + ev.cache_write_tokens;
        s_entry.output_tokens += ev.reasoning_tokens + ev.output_tokens;
        s_entry.cache_tokens += ev.cache_read_tokens + ev.cache_write_tokens;
        if ev.cost_basis != "none" {
            *s_entry.costs.entry(ev.currency.clone()).or_insert(0.0) += ev.cost;
        }
    }

    let cost_coverage_ratio = if total_tokens > 0 {
        cost_covered_tokens as f64 / total_tokens as f64
    } else {
        1.0
    };

    let costs_by_currency = cost_by_currency_map
        .into_iter()
        .map(|(currency, amount)| CurrencyCostDto {
            currency,
            amount: (amount * 1000.0).round() / 1000.0,
        })
        .collect::<Vec<_>>();

    let hero = UsageHeroOverviewDto {
        total_tokens,
        total_input_tokens: total_input,
        total_output_tokens: total_output,
        cache_read_tokens,
        cache_write_tokens,
        reasoning_tokens,
        requests_count,
        costs_by_currency,
        cost_covered_tokens,
        cost_coverage_ratio,
        cost_covered_requests,
    };

    let daily_trend = daily_map
        .into_iter()
        .map(
            |(date, (toks, reqs, in_toks, out_toks, cache_toks, costs))| UsageDailyTrendBucketDto {
                date: date.format("%Y-%m-%d").to_string(),
                total_tokens: toks,
                requests_count: reqs,
                input_tokens: in_toks,
                output_tokens: out_toks,
                cache_tokens: cache_toks,
                costs_by_currency: costs
                    .into_iter()
                    .map(|(currency, amount)| CurrencyCostDto {
                        currency,
                        amount: (amount * 1000.0).round() / 1000.0,
                    })
                    .collect(),
            },
        )
        .collect::<Vec<_>>();

    let mut models = model_map
        .into_iter()
        .map(|(model, agg)| UsageModelBreakdownDto {
            model,
            provider: agg.provider,
            requests_count: agg.requests,
            total_tokens: agg.total_tokens,
            input_tokens: agg.input_tokens,
            output_tokens: agg.output_tokens,
            reasoning_tokens: agg.reasoning_tokens,
            cache_tokens: agg.cache_tokens,
            cost: if agg.cost_basis == "none" {
                None
            } else {
                Some((agg.cost * 1000.0).round() / 1000.0)
            },
            currency: agg.currency,
            cost_basis: agg.cost_basis,
        })
        .collect::<Vec<_>>();
    models.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens));

    let mut sources = source_map
        .into_iter()
        .map(|(source_id, agg)| {
            let source_name = source_name_map
                .get(&source_id)
                .cloned()
                .unwrap_or_else(|| source_id.clone());
            let app_name = app_display_name_from_adapter(&agg.adapter_id);
            UsageSourceBreakdownDto {
                adapter_id: agg.adapter_id,
                source_id,
                source_name,
                app_name,
                requests_count: agg.requests,
                total_tokens: agg.total_tokens,
                input_tokens: agg.input_tokens,
                output_tokens: agg.output_tokens,
                cache_tokens: agg.cache_tokens,
                costs_by_currency: agg
                    .costs
                    .into_iter()
                    .map(|(currency, amount)| CurrencyCostDto {
                        currency,
                        amount: (amount * 1000.0).round() / 1000.0,
                    })
                    .collect(),
                status: "active".to_string(),
                diagnostics: Vec::new(),
            }
        })
        .collect::<Vec<_>>();
    sources.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens));

    let mut dates = daily_trend
        .iter()
        .filter(|d| d.requests_count > 0 || d.total_tokens > 0)
        .map(|d| UsageDateBreakdownDto {
            date: d.date.clone(),
            requests_count: d.requests_count,
            total_tokens: d.total_tokens,
            input_tokens: d.input_tokens,
            output_tokens: d.output_tokens,
            cache_tokens: d.cache_tokens,
            costs_by_currency: d.costs_by_currency.clone(),
        })
        .collect::<Vec<_>>();
    dates.reverse();

    // Source states & scan status
    let states = load_all_usage_source_states_sqlx(pool, tenant_id).await?;
    let last_scanned_at = states
        .iter()
        .filter_map(|s| s.last_success_scan_at.as_ref())
        .max()
        .cloned();

    let scan_status = UsageScanStatusDto {
        scanned_sources_count: states.len(),
        total_events_count: loaded_events.len(),
        last_scanned_at,
        active_scan_task_id: None,
        source_diagnostics: Vec::new(),
    };

    Ok(UsageDashboardDto {
        hero,
        daily_trend,
        models,
        sources,
        dates,
        scan_status,
    })
}

fn app_display_name_from_adapter(adapter_id: &str) -> String {
    if adapter_id.contains("antigravity") {
        "Antigravity".to_string()
    } else if adapter_id.contains("codex") {
        "Codex".to_string()
    } else if adapter_id.contains("claude") {
        "Claude Code".to_string()
    } else if adapter_id.contains("opencode") {
        "OpenCode".to_string()
    } else if adapter_id.contains("zcode") {
        "ZCode".to_string()
    } else {
        adapter_id.to_string()
    }
}

#[cfg(test)]
#[path = "usage_repo_tests.rs"]
mod tests;
