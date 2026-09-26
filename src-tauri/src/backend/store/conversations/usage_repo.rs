use crate::backend::{
    domain::conversations::pricing::PriceCatalog,
    domain::conversations::{
        CurrencyCostDto, UsageDailyTrendBucketDto, UsageDashboardDto, UsageDashboardFilter,
        UsageDateBreakdownDto, UsageHeroOverviewDto, UsageModelBreakdownDto, UsageScanStatusDto,
        UsageSourceBreakdownDto,
    },
    store::StoreResult,
};
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{AssertSqlSafe, Row as SqlxRow, SqlitePool};
use std::collections::{BTreeMap, HashMap};

pub(crate) use super::usage_query::*;

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
) -> StoreResult<usize> {
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
) -> StoreResult<u64> {
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
) -> StoreResult<Option<UsageSourceState>> {
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
) -> StoreResult<()> {
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
) -> StoreResult<Vec<UsageSourceState>> {
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

#[cfg(test)]
#[path = "usage_repo_tests.rs"]
mod tests;
