use crate::backend::{
    domain::conversations::pricing::{
        candidate_model_names, is_dirty_model_name, ModelPriceEntry, PriceCatalog,
        IGNORE_MODEL_ID_PREFIX, OVERRIDE_MODEL_ID_PREFIX,
    },
    store::StoreResult,
};
use chrono::Utc;
use sqlx::{Row as SqlxRow, Sqlite, SqlitePool, Transaction};

#[path = "pricing_repo_ops.rs"]
pub mod pricing_repo_ops;
pub use pricing_repo_ops::*;

pub async fn upsert_pricing_catalog_entries_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    entries: &[ModelPriceEntry],
    source_channel: &str,
) -> StoreResult<usize> {
    if entries.is_empty() {
        return Ok(0);
    }

    let mut tx = pool.begin().await?;
    let now = Utc::now().to_rfc3339();
    let mut count = 0usize;
    let is_user_override = source_channel == "user_override";
    let prune_superseded_variants = matches!(source_channel, "litellm" | "openrouter");
    let mut written_references: Vec<(String, String)> = Vec::new();

    // Prune historical zero-priced ghost entries without aliases or locks
    let _ = sqlx::query(
        r#"
        DELETE FROM model_pricing_catalog
        WHERE tenant_id = ?
          AND source_channel != 'user_override'
          AND price_locked = 0
          AND usage_aliases = '[]'
          AND input_per_million <= 0.0
          AND output_per_million <= 0.0
        "#,
    )
    .bind(tenant_id)
    .execute(&mut *tx)
    .await;

    for entry in entries {
        if !is_user_override && entry.input_per_million <= 0.0 && entry.output_per_million <= 0.0 {
            continue;
        }

        let patterns_to_write: Vec<&str> = if is_user_override {
            entry
                .model_patterns
                .iter()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            let is_valid_ref = |s: &&str| {
                !s.is_empty()
                    && !s.starts_with(OVERRIDE_MODEL_ID_PREFIX)
                    && !s.starts_with(IGNORE_MODEL_ID_PREFIX)
                    && !is_dirty_model_name(s)
            };
            entry
                .model_patterns
                .iter()
                .map(|s| s.trim())
                .find(|s| is_valid_ref(s) && !s.contains('*'))
                .or_else(|| {
                    entry
                        .model_patterns
                        .iter()
                        .map(|s| s.trim())
                        .find(is_valid_ref)
                })
                .into_iter()
                .collect()
        };

        for model_id in patterns_to_write {
            let res = if is_user_override {
                let locked_int = if entry.price_locked { 1i64 } else { 0i64 };
                let aliases_json = serde_json::to_string(&entry.usage_aliases)
                    .unwrap_or_else(|_| "[]".to_string());
                sqlx::query(
                    r#"
                    INSERT INTO model_pricing_catalog (
                        tenant_id, model_id, provider, currency,
                        input_per_million, cache_read_per_million, cache_write_per_million,
                        reasoning_per_million, output_per_million,
                        raw_currency, raw_input_per_million, raw_output_per_million,
                        source_channel, price_locked, usage_aliases, created_at, updated_at
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    ON CONFLICT(tenant_id, model_id) DO UPDATE SET
                        provider = excluded.provider,
                        currency = excluded.currency,
                        input_per_million = excluded.input_per_million,
                        cache_read_per_million = excluded.cache_read_per_million,
                        cache_write_per_million = excluded.cache_write_per_million,
                        reasoning_per_million = excluded.reasoning_per_million,
                        output_per_million = excluded.output_per_million,
                        raw_currency = excluded.raw_currency,
                        raw_input_per_million = excluded.raw_input_per_million,
                        raw_output_per_million = excluded.raw_output_per_million,
                        source_channel = excluded.source_channel,
                        price_locked = excluded.price_locked,
                        updated_at = excluded.updated_at
                    "#,
                )
                .bind(tenant_id)
                .bind(model_id)
                .bind(&entry.provider)
                .bind(&entry.currency)
                .bind(entry.input_per_million)
                .bind(entry.cache_read_per_million)
                .bind(entry.cache_write_per_million)
                .bind(entry.reasoning_per_million)
                .bind(entry.output_per_million)
                .bind(entry.raw_currency.as_deref().unwrap_or("USD"))
                .bind(entry.raw_input_per_million.unwrap_or(0.0))
                .bind(entry.raw_output_per_million.unwrap_or(0.0))
                .bind(source_channel)
                .bind(locked_int)
                .bind(&aliases_json)
                .bind(&now)
                .bind(&now)
                .execute(&mut *tx)
                .await?
            } else {
                sqlx::query(
                    r#"
                    INSERT INTO model_pricing_catalog (
                        tenant_id, model_id, provider, currency,
                        input_per_million, cache_read_per_million, cache_write_per_million,
                        reasoning_per_million, output_per_million,
                        raw_currency, raw_input_per_million, raw_output_per_million,
                        source_channel, price_locked, usage_aliases, created_at, updated_at
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, '[]', ?, ?)
                    ON CONFLICT(tenant_id, model_id) DO UPDATE SET
                        provider = excluded.provider,
                        currency = excluded.currency,
                        input_per_million = excluded.input_per_million,
                        cache_read_per_million = excluded.cache_read_per_million,
                        cache_write_per_million = excluded.cache_write_per_million,
                        reasoning_per_million = excluded.reasoning_per_million,
                        output_per_million = excluded.output_per_million,
                        raw_currency = excluded.raw_currency,
                        raw_input_per_million = excluded.raw_input_per_million,
                        raw_output_per_million = excluded.raw_output_per_million,
                        source_channel = excluded.source_channel,
                        updated_at = excluded.updated_at
                    WHERE model_pricing_catalog.source_channel != 'user_override'
                      AND model_pricing_catalog.price_locked = 0
                    "#,
                )
                .bind(tenant_id)
                .bind(model_id)
                .bind(&entry.provider)
                .bind(&entry.currency)
                .bind(entry.input_per_million)
                .bind(entry.cache_read_per_million)
                .bind(entry.cache_write_per_million)
                .bind(entry.reasoning_per_million)
                .bind(entry.output_per_million)
                .bind(entry.raw_currency.as_deref().unwrap_or("USD"))
                .bind(entry.raw_input_per_million.unwrap_or(0.0))
                .bind(entry.raw_output_per_million.unwrap_or(0.0))
                .bind(source_channel)
                .bind(&now)
                .bind(&now)
                .execute(&mut *tx)
                .await?
            };

            count += res.rows_affected() as usize;
            if prune_superseded_variants
                && res.rows_affected() > 0
                && !written_references
                    .iter()
                    .any(|(written_id, _)| written_id == model_id)
            {
                written_references.push((model_id.to_string(), entry.provider.clone()));
            }
        }
    }

    if prune_superseded_variants {
        prune_superseded_reference_rows(&mut tx, tenant_id, &written_references, &now).await?;
    }

    tx.commit().await?;
    Ok(count)
}

/// True when `stale_model_id` is a historical variant (dated / decorated display name)
/// of `base_model_id` rather than a distinct model. `candidate_model_names` already
/// normalizes case, date suffixes, gateway prefixes and alias suffixes, so distinct
/// siblings such as `gpt-4o-mini` never collapse into `gpt-4o`.
fn is_superseded_variant(stale_model_id: &str, base_model_id: &str) -> bool {
    let base = base_model_id.trim().to_ascii_lowercase();
    if base.is_empty() || stale_model_id.trim().eq_ignore_ascii_case(&base) {
        return false;
    }
    candidate_model_names(stale_model_id)
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(&base))
}

async fn merge_usage_aliases_into_row(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    base_model_id: &str,
    aliases: &[String],
    now: &str,
) -> StoreResult<()> {
    let Some(row) = sqlx::query(
        "SELECT id, usage_aliases FROM model_pricing_catalog WHERE tenant_id = ? AND model_id = ? LIMIT 1",
    )
    .bind(tenant_id)
    .bind(base_model_id)
    .fetch_optional(&mut **tx)
    .await?
    else {
        return Ok(());
    };

    let row_id: i64 = row.get("id");
    let existing_json: String = row.get("usage_aliases");
    let mut merged: Vec<String> = serde_json::from_str(&existing_json).unwrap_or_default();
    let mut changed = false;
    for alias in aliases {
        let trimmed = alias.trim();
        if trimmed.is_empty()
            || merged
                .iter()
                .any(|existing| existing.trim().eq_ignore_ascii_case(trimmed))
        {
            continue;
        }
        merged.push(trimmed.to_string());
        changed = true;
    }
    if !changed {
        return Ok(());
    }

    let merged_json = serde_json::to_string(&merged).unwrap_or_else(|_| "[]".to_string());
    sqlx::query("UPDATE model_pricing_catalog SET usage_aliases = ?, updated_at = ? WHERE id = ?")
        .bind(&merged_json)
        .bind(now)
        .bind(row_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Drops non-override, unlocked rows that are historical variants of a freshly synced
/// canonical base row, so their higher specificity score can no longer shadow the new price.
/// Their `usage_aliases` are merged onto the base row before deletion.
async fn prune_superseded_reference_rows(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    written_references: &[(String, String)],
    now: &str,
) -> StoreResult<()> {
    if written_references.is_empty() {
        return Ok(());
    }

    let rows = sqlx::query(
        r#"
        SELECT id, model_id, provider, usage_aliases
        FROM model_pricing_catalog
        WHERE tenant_id = ?
          AND source_channel != 'user_override'
          AND price_locked = 0
        "#,
    )
    .bind(tenant_id)
    .fetch_all(&mut **tx)
    .await?;

    for row in rows {
        let row_id: i64 = row.get("id");
        let model_id: String = row.get("model_id");
        let provider = row.get::<String, _>("provider").trim().to_ascii_lowercase();

        let superseded_by = written_references.iter().find(|(base_id, base_provider)| {
            let base_provider = base_provider.trim().to_ascii_lowercase();
            let provider_matches =
                base_provider.is_empty() || provider.is_empty() || base_provider == provider;
            provider_matches && is_superseded_variant(&model_id, base_id)
        });
        let Some((base_model_id, _)) = superseded_by else {
            continue;
        };

        let aliases_json: String = row.get("usage_aliases");
        let aliases: Vec<String> = serde_json::from_str(&aliases_json).unwrap_or_default();
        merge_usage_aliases_into_row(tx, tenant_id, base_model_id, &aliases, now).await?;

        sqlx::query("DELETE FROM model_pricing_catalog WHERE id = ?")
            .bind(row_id)
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}

pub async fn load_pricing_catalog_entries_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<Vec<ModelPriceEntry>> {
    let rows = sqlx::query(
        r#"
        SELECT model_id, provider, currency,
               input_per_million, cache_read_per_million, cache_write_per_million,
               reasoning_per_million, output_per_million,
               raw_currency, raw_input_per_million, raw_output_per_million,
               price_locked, usage_aliases, source_channel
        FROM model_pricing_catalog
        WHERE tenant_id = ?
          AND (
              source_channel = 'user_override'
              OR price_locked = 1
              OR input_per_million > 0.0
              OR output_per_million > 0.0
              OR usage_aliases != '[]'
          )
        "#,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let model_id: String = r.get("model_id");
            let price_locked: i64 = r.get("price_locked");
            let aliases_json: String = r.get("usage_aliases");
            let usage_aliases: Vec<String> =
                serde_json::from_str(&aliases_json).unwrap_or_default();
            let is_ignored = model_id.starts_with(IGNORE_MODEL_ID_PREFIX);
            ModelPriceEntry {
                provider: r.get("provider"),
                model_patterns: vec![model_id],
                currency: r.get("currency"),
                input_per_million: r.get("input_per_million"),
                cache_read_per_million: r.get("cache_read_per_million"),
                cache_write_per_million: r.get("cache_write_per_million"),
                reasoning_per_million: r.get("reasoning_per_million"),
                output_per_million: r.get("output_per_million"),
                raw_currency: r.get("raw_currency"),
                raw_input_per_million: r.get("raw_input_per_million"),
                raw_output_per_million: r.get("raw_output_per_million"),
                price_locked: price_locked != 0,
                usage_aliases,
                source_channel: r.get("source_channel"),
                is_ignored,
            }
        })
        .collect())
}

pub async fn get_pricing_metadata_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    key: &str,
) -> StoreResult<Option<String>> {
    let row =
        sqlx::query("SELECT value FROM model_pricing_metadata WHERE tenant_id = ? AND key = ?")
            .bind(tenant_id)
            .bind(key)
            .fetch_optional(pool)
            .await?;

    Ok(row.map(|r| r.get("value")))
}

pub async fn set_pricing_metadata_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    key: &str,
    value: &str,
) -> StoreResult<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO model_pricing_metadata (tenant_id, key, value, updated_at)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(tenant_id, key) DO UPDATE SET
            value = excluded.value,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(tenant_id)
    .bind(key)
    .bind(value)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn recalculate_usage_costs_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    catalog: &PriceCatalog,
) -> StoreResult<usize> {
    // Prune historical zero-priced ghost entries without aliases or locks
    let _ = sqlx::query(
        r#"
        DELETE FROM model_pricing_catalog
        WHERE tenant_id = ?
          AND source_channel != 'user_override'
          AND price_locked = 0
          AND usage_aliases = '[]'
          AND input_per_million <= 0.0
          AND output_per_million <= 0.0
        "#,
    )
    .bind(tenant_id)
    .execute(pool)
    .await;

    // 1. 获取所有需要核算费用的 DISTINCT (provider, model) 组合（避免全表逐行加载与十万次单条更新）
    let distinct_models = sqlx::query(
        r#"
        SELECT DISTINCT provider, model
        FROM conversation_usage_events
        WHERE tenant_id = ? AND (cost_basis != 'host_reported' OR host_reported_cost IS NULL OR host_reported_cost <= 0.0)
        "#,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;

    if distinct_models.is_empty() {
        return Ok(0);
    }

    let mut tx = pool.begin().await?;
    let now = Utc::now().to_rfc3339();
    let mut updated_count = 0usize;

    for row in distinct_models {
        let provider: String = row.get("provider");
        let model: String = row.get("model");

        if let Some(resolved) = catalog.resolve_model(&provider, &model) {
            let (in_rate, cr_rate, cw_rate, reas_rate, out_rate) = if resolved.is_ignored {
                (0.0, 0.0, 0.0, 0.0, 0.0)
            } else {
                let e = resolved.entry;
                (
                    e.input_per_million,
                    e.cache_read_per_million,
                    e.cache_write_per_million,
                    e.reasoning_per_million,
                    e.output_per_million,
                )
            };
            // 利用 SQLite 数学表达式批量计算该模型下所有事件的核算费用
            let res = sqlx::query(
                r#"
                UPDATE conversation_usage_events
                SET catalog_estimated_cost = ROUND(
                        (
                            MAX(0, input_tokens) * ? +
                            MAX(0, cache_read_tokens) * ? +
                            MAX(0, cache_write_tokens) * ? +
                            MAX(0, reasoning_tokens) * ? +
                            MAX(0, output_tokens) * ?
                        ) / 1000000.0,
                        6
                    ),
                    cost_basis = 'catalog_estimated',
                    currency = 'CNY',
                    price_catalog_version = ?,
                    updated_at = ?
                WHERE tenant_id = ?
                  AND provider = ?
                  AND model = ?
                  AND (cost_basis != 'host_reported' OR host_reported_cost IS NULL OR host_reported_cost <= 0.0)
                "#,
            )
            .bind(in_rate)
            .bind(cr_rate)
            .bind(cw_rate)
            .bind(reas_rate)
            .bind(out_rate)
            .bind(&catalog.catalog_version)
            .bind(&now)
            .bind(tenant_id)
            .bind(&provider)
            .bind(&model)
            .execute(&mut *tx)
            .await?;

            updated_count += res.rows_affected() as usize;
        } else {
            // 未匹配到定价模型的事件，批量置为未定价
            let res = sqlx::query(
                r#"
                UPDATE conversation_usage_events
                SET catalog_estimated_cost = NULL,
                    cost_basis = 'none',
                    currency = 'CNY',
                    price_catalog_version = NULL,
                    updated_at = ?
                WHERE tenant_id = ?
                  AND provider = ?
                  AND model = ?
                  AND (cost_basis != 'host_reported' OR host_reported_cost IS NULL OR host_reported_cost <= 0.0)
                "#,
            )
            .bind(&now)
            .bind(tenant_id)
            .bind(&provider)
            .bind(&model)
            .execute(&mut *tx)
            .await?;

            updated_count += res.rows_affected() as usize;
        }
    }

    tx.commit().await?;
    Ok(updated_count)
}

#[cfg(test)]
#[path = "pricing_repo_tests.rs"]
mod tests;
