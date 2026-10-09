use crate::backend::{
    domain::conversations::pricing::{ModelPriceEntry, PriceCatalog},
    store::StoreResult,
};
use chrono::Utc;
use sqlx::{Row as SqlxRow, SqlitePool};

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

    // Prune historical zero-priced ghost entries to prevent them from poisoning matches
    let _ = sqlx::query(
        r#"
        DELETE FROM model_pricing_catalog
        WHERE tenant_id = ?
          AND source_channel != 'user_override'
          AND input_per_million <= 0.0
          AND output_per_million <= 0.0
        "#,
    )
    .bind(tenant_id)
    .execute(&mut *tx)
    .await;

    for entry in entries {
        if source_channel != "user_override"
            && entry.input_per_million <= 0.0
            && entry.output_per_million <= 0.0
        {
            continue;
        }

        for pattern in &entry.model_patterns {
            let model_id = pattern.trim();
            if model_id.is_empty() {
                continue;
            }

            sqlx::query(
                r#"
                INSERT INTO model_pricing_catalog (
                    tenant_id, model_id, provider, currency,
                    input_per_million, cache_read_per_million, cache_write_per_million,
                    reasoning_per_million, output_per_million,
                    raw_currency, raw_input_per_million, raw_output_per_million,
                    source_channel, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
            .await?;

            count += 1;
        }
    }

    tx.commit().await?;
    Ok(count)
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
               raw_currency, raw_input_per_million, raw_output_per_million
        FROM model_pricing_catalog
        WHERE tenant_id = ?
          AND (source_channel = 'user_override' OR input_per_million > 0.0 OR output_per_million > 0.0)
        "#,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| ModelPriceEntry {
            provider: r.get("provider"),
            model_patterns: vec![r.get("model_id")],
            currency: r.get("currency"),
            input_per_million: r.get("input_per_million"),
            cache_read_per_million: r.get("cache_read_per_million"),
            cache_write_per_million: r.get("cache_write_per_million"),
            reasoning_per_million: r.get("reasoning_per_million"),
            output_per_million: r.get("output_per_million"),
            raw_currency: r.get("raw_currency"),
            raw_input_per_million: r.get("raw_input_per_million"),
            raw_output_per_million: r.get("raw_output_per_million"),
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
    // Prune historical zero-priced ghost entries to avoid catalog contamination
    let _ = sqlx::query(
        r#"
        DELETE FROM model_pricing_catalog
        WHERE tenant_id = ?
          AND source_channel != 'user_override'
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

        if let Some(entry) = catalog.find_model(&provider, &model) {
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
            .bind(entry.input_per_million)
            .bind(entry.cache_read_per_million)
            .bind(entry.cache_write_per_million)
            .bind(entry.reasoning_per_million)
            .bind(entry.output_per_million)
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
