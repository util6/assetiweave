use crate::backend::{
    domain::conversations::pricing::{
        base_model_id_from_catalog_key, is_dirty_model_name, ModelPriceEntry,
        IGNORE_MODEL_ID_PREFIX, OVERRIDE_MODEL_ID_PREFIX,
    },
    store::{StoreError, StoreResult},
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::{Row as SqlxRow, Sqlite, SqlitePool, Transaction};
use std::collections::BTreeSet;

/// Persists a user-locked override row (`__override__::<base_model_id>`) with `price_locked = 1`
/// and `source_channel = 'user_override'`, leaving any native reference row `<base_model_id>` untouched.
pub async fn save_locked_override_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    base_model_id: &str,
    entry: &ModelPriceEntry,
) -> StoreResult<()> {
    let clean_base_id = base_model_id_from_catalog_key(base_model_id).trim();
    if clean_base_id.is_empty() {
        return Err(StoreError::Validation(
            "base_model_id cannot be empty".to_string(),
        ));
    }

    let override_model_id = format!("{OVERRIDE_MODEL_ID_PREFIX}{clean_base_id}");
    let now = Utc::now().to_rfc3339();
    let currency = if entry.currency.trim().is_empty() {
        "CNY"
    } else {
        entry.currency.trim()
    };
    let raw_currency = entry.raw_currency.as_deref().unwrap_or(currency);
    let raw_input = entry
        .raw_input_per_million
        .unwrap_or(entry.input_per_million);
    let raw_output = entry
        .raw_output_per_million
        .unwrap_or(entry.output_per_million);

    sqlx::query(
        r#"
        INSERT INTO model_pricing_catalog (
            tenant_id, model_id, provider, currency,
            input_per_million, cache_read_per_million, cache_write_per_million,
            reasoning_per_million, output_per_million,
            raw_currency, raw_input_per_million, raw_output_per_million,
            source_channel, price_locked, usage_aliases, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'user_override', 1, '[]', ?, ?)
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
            source_channel = 'user_override',
            price_locked = 1,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(tenant_id)
    .bind(&override_model_id)
    .bind(entry.provider.trim())
    .bind(currency)
    .bind(entry.input_per_million)
    .bind(entry.cache_read_per_million)
    .bind(entry.cache_write_per_million)
    .bind(entry.reasoning_per_million)
    .bind(entry.output_per_million)
    .bind(raw_currency)
    .bind(raw_input)
    .bind(raw_output)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(())
}

/// Locks or unlocks a base model's pricing override.
///
/// - When `locked == false`: removes `__override__::<base_model_id>` and clears `price_locked = 0`
///   on any legacy unprefixed row for `<base_model_id>`.
/// - When `locked == true`: ensures `__override__::<base_model_id>` exists with `price_locked = 1`,
///   copying from an existing DB row for `<base_model_id>` or `fallback_entry` when needed.
pub async fn set_model_pricing_lock_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    base_model_id: &str,
    locked: bool,
    fallback_entry: Option<&ModelPriceEntry>,
) -> StoreResult<()> {
    let clean_base_id = base_model_id_from_catalog_key(base_model_id).trim();
    if clean_base_id.is_empty() {
        return Err(StoreError::Validation(
            "base_model_id cannot be empty".to_string(),
        ));
    }

    let override_model_id = format!("{OVERRIDE_MODEL_ID_PREFIX}{clean_base_id}");
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;

    if !locked {
        sqlx::query("DELETE FROM model_pricing_catalog WHERE tenant_id = ? AND model_id = ?")
            .bind(tenant_id)
            .bind(&override_model_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query(
            r#"
            UPDATE model_pricing_catalog
            SET price_locked = 0, updated_at = ?
            WHERE tenant_id = ? AND model_id = ? AND price_locked != 0
            "#,
        )
        .bind(&now)
        .bind(tenant_id)
        .bind(clean_base_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        return Ok(());
    }

    let updated = sqlx::query(
        r#"
        UPDATE model_pricing_catalog
        SET price_locked = 1, source_channel = 'user_override', updated_at = ?
        WHERE tenant_id = ? AND model_id = ?
        "#,
    )
    .bind(&now)
    .bind(tenant_id)
    .bind(&override_model_id)
    .execute(&mut *tx)
    .await?;

    if updated.rows_affected() > 0 {
        tx.commit().await?;
        return Ok(());
    }

    let db_ref = sqlx::query(
        r#"
        SELECT provider, currency,
               input_per_million, cache_read_per_million, cache_write_per_million,
               reasoning_per_million, output_per_million,
               raw_currency, raw_input_per_million, raw_output_per_million
        FROM model_pricing_catalog
        WHERE tenant_id = ? AND model_id = ?
        LIMIT 1
        "#,
    )
    .bind(tenant_id)
    .bind(clean_base_id)
    .fetch_optional(&mut *tx)
    .await?;

    let snapshot = if let Some(r) = db_ref {
        ModelPriceEntry {
            provider: r.get("provider"),
            model_patterns: vec![clean_base_id.to_string()],
            currency: r.get("currency"),
            input_per_million: r.get("input_per_million"),
            cache_read_per_million: r.get("cache_read_per_million"),
            cache_write_per_million: r.get("cache_write_per_million"),
            reasoning_per_million: r.get("reasoning_per_million"),
            output_per_million: r.get("output_per_million"),
            raw_currency: r.get("raw_currency"),
            raw_input_per_million: r.get("raw_input_per_million"),
            raw_output_per_million: r.get("raw_output_per_million"),
            ..Default::default()
        }
    } else if let Some(fb) = fallback_entry {
        fb.clone()
    } else {
        return Err(StoreError::NotFound(format!(
            "pricing entry not found for model: {clean_base_id}"
        )));
    };

    let currency = if snapshot.currency.trim().is_empty() {
        "CNY"
    } else {
        snapshot.currency.trim()
    };
    let raw_currency = snapshot.raw_currency.as_deref().unwrap_or(currency);
    let raw_input = snapshot
        .raw_input_per_million
        .unwrap_or(snapshot.input_per_million);
    let raw_output = snapshot
        .raw_output_per_million
        .unwrap_or(snapshot.output_per_million);

    sqlx::query(
        r#"
        INSERT INTO model_pricing_catalog (
            tenant_id, model_id, provider, currency,
            input_per_million, cache_read_per_million, cache_write_per_million,
            reasoning_per_million, output_per_million,
            raw_currency, raw_input_per_million, raw_output_per_million,
            source_channel, price_locked, usage_aliases, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'user_override', 1, '[]', ?, ?)
        "#,
    )
    .bind(tenant_id)
    .bind(&override_model_id)
    .bind(snapshot.provider.trim())
    .bind(currency)
    .bind(snapshot.input_per_million)
    .bind(snapshot.cache_read_per_million)
    .bind(snapshot.cache_write_per_million)
    .bind(snapshot.reasoning_per_million)
    .bind(snapshot.output_per_million)
    .bind(raw_currency)
    .bind(raw_input)
    .bind(raw_output)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Removes `trimmed_raw` (case-insensitive trimmed) from `usage_aliases` across matching catalog rows.
/// When `include_ignored` is true, also prunes matching aliases from `__ignore__::` rows and deletes
/// any `__ignore__::` row whose `usage_aliases` becomes empty.
async fn remove_usage_alias_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    trimmed_raw: &str,
    include_ignored: bool,
    now: &str,
) -> StoreResult<()> {
    let rows = sqlx::query(
        r#"
        SELECT id, model_id, usage_aliases
        FROM model_pricing_catalog
        WHERE tenant_id = ?
          AND usage_aliases IS NOT NULL
          AND TRIM(usage_aliases) != ''
          AND TRIM(usage_aliases) != '[]'
        "#,
    )
    .bind(tenant_id)
    .fetch_all(&mut **tx)
    .await?;

    for row in rows {
        let row_id: i64 = row.get("id");
        let model_id: String = row.get("model_id");
        let is_ignore_row = model_id.starts_with(IGNORE_MODEL_ID_PREFIX);
        if is_ignore_row && !include_ignored {
            continue;
        }
        let aliases_json: String = row.get("usage_aliases");
        let mut aliases: Vec<String> = serde_json::from_str(&aliases_json).unwrap_or_default();
        let before_len = aliases.len();
        aliases.retain(|a| !a.trim().eq_ignore_ascii_case(trimmed_raw));
        if aliases.len() == before_len {
            continue;
        }
        if is_ignore_row && aliases.is_empty() {
            sqlx::query("DELETE FROM model_pricing_catalog WHERE id = ?")
                .bind(row_id)
                .execute(&mut **tx)
                .await?;
        } else {
            let updated_json = serde_json::to_string(&aliases).unwrap_or_else(|_| "[]".to_string());
            sqlx::query(
                "UPDATE model_pricing_catalog SET usage_aliases = ?, updated_at = ? WHERE id = ?",
            )
            .bind(&updated_json)
            .bind(now)
            .bind(row_id)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

fn resolve_target_base_id(entry: &ModelPriceEntry) -> Option<String> {
    let is_valid = |s: &&str| {
        let trimmed = s.trim();
        !trimmed.is_empty()
            && !trimmed.starts_with(IGNORE_MODEL_ID_PREFIX)
            && !is_dirty_model_name(trimmed)
    };
    let chosen = entry
        .model_patterns
        .iter()
        .map(|s| s.trim())
        .find(|s| is_valid(s) && !s.contains('*'))
        .or_else(|| entry.model_patterns.iter().map(|s| s.trim()).find(is_valid))?;
    let base = base_model_id_from_catalog_key(chosen)
        .trim_matches('*')
        .trim()
        .to_string();
    if base.is_empty() {
        None
    } else {
        Some(base)
    }
}

fn ignore_row_model_id(trimmed_raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(trimmed_raw.to_ascii_lowercase().as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(16);
    for byte in &digest[..8] {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    format!("{IGNORE_MODEL_ID_PREFIX}{hex}")
}

/// Binds `raw_model` as a manual `usage_aliases` entry on `target_entry`'s base model,
/// or unbinds `raw_model` from all rows when `target_entry` is `None`.
pub async fn rebind_model_pricing_alias_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    raw_model: &str,
    target_entry: Option<&ModelPriceEntry>,
) -> StoreResult<()> {
    let trimmed_raw = raw_model.trim();
    if trimmed_raw.is_empty() {
        return Err(StoreError::Validation(
            "raw_model cannot be empty".to_string(),
        ));
    }

    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;

    remove_usage_alias_in_tx(&mut tx, tenant_id, trimmed_raw, true, &now).await?;

    if let Some(target) = target_entry {
        let target_base_id = resolve_target_base_id(target).ok_or_else(|| {
            StoreError::Validation("target_entry has no valid base model_id".to_string())
        })?;

        let existing = sqlx::query(
            r#"
            SELECT id, usage_aliases
            FROM model_pricing_catalog
            WHERE tenant_id = ? AND model_id = ?
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(&target_base_id)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(row) = existing {
            let row_id: i64 = row.get("id");
            let aliases_json: String = row.get("usage_aliases");
            let mut aliases: Vec<String> = serde_json::from_str(&aliases_json).unwrap_or_default();
            if !aliases
                .iter()
                .any(|a| a.trim().eq_ignore_ascii_case(trimmed_raw))
            {
                aliases.push(trimmed_raw.to_string());
            }
            let updated_json = serde_json::to_string(&aliases).unwrap_or_else(|_| "[]".to_string());
            sqlx::query(
                "UPDATE model_pricing_catalog SET usage_aliases = ?, updated_at = ? WHERE id = ?",
            )
            .bind(&updated_json)
            .bind(&now)
            .bind(row_id)
            .execute(&mut *tx)
            .await?;
        } else {
            let currency = if target.currency.trim().is_empty() {
                "CNY"
            } else {
                target.currency.trim()
            };
            let raw_currency = target.raw_currency.as_deref().unwrap_or(currency);
            let raw_input = target
                .raw_input_per_million
                .unwrap_or(target.input_per_million);
            let raw_output = target
                .raw_output_per_million
                .unwrap_or(target.output_per_million);
            let aliases_json =
                serde_json::to_string(&vec![trimmed_raw]).unwrap_or_else(|_| "[]".to_string());

            sqlx::query(
                r#"
                INSERT INTO model_pricing_catalog (
                    tenant_id, model_id, provider, currency,
                    input_per_million, cache_read_per_million, cache_write_per_million,
                    reasoning_per_million, output_per_million,
                    raw_currency, raw_input_per_million, raw_output_per_million,
                    source_channel, price_locked, usage_aliases, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'bundled', 0, ?, ?, ?)
                "#,
            )
            .bind(tenant_id)
            .bind(&target_base_id)
            .bind(target.provider.trim())
            .bind(currency)
            .bind(target.input_per_million)
            .bind(target.cache_read_per_million)
            .bind(target.cache_write_per_million)
            .bind(target.reasoning_per_million)
            .bind(target.output_per_million)
            .bind(raw_currency)
            .bind(raw_input)
            .bind(raw_output)
            .bind(&aliases_json)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

/// Marks or unmarks `raw_model` as explicitly ignored (`__ignore__::<hash>`).
/// When `ignored == true`, also strips `raw_model` from any manual rebind `usage_aliases`.
pub async fn set_model_pricing_ignored_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    raw_model: &str,
    provider: Option<&str>,
    ignored: bool,
) -> StoreResult<()> {
    let trimmed_raw = raw_model.trim();
    if trimmed_raw.is_empty() {
        return Err(StoreError::Validation(
            "raw_model cannot be empty".to_string(),
        ));
    }

    let now = Utc::now().to_rfc3339();
    let ignore_key = ignore_row_model_id(trimmed_raw);
    let mut tx = pool.begin().await?;

    if !ignored {
        sqlx::query("DELETE FROM model_pricing_catalog WHERE tenant_id = ? AND model_id = ?")
            .bind(tenant_id)
            .bind(&ignore_key)
            .execute(&mut *tx)
            .await?;

        let ignore_rows = sqlx::query(
            r#"
            SELECT id, usage_aliases
            FROM model_pricing_catalog
            WHERE tenant_id = ? AND model_id LIKE ?
            "#,
        )
        .bind(tenant_id)
        .bind(format!("{IGNORE_MODEL_ID_PREFIX}%"))
        .fetch_all(&mut *tx)
        .await?;

        for row in ignore_rows {
            let row_id: i64 = row.get("id");
            let aliases_json: String = row.get("usage_aliases");
            let mut aliases: Vec<String> = serde_json::from_str(&aliases_json).unwrap_or_default();
            let before_len = aliases.len();
            aliases.retain(|a| !a.trim().eq_ignore_ascii_case(trimmed_raw));
            if aliases.len() == before_len {
                continue;
            }
            if aliases.is_empty() {
                sqlx::query("DELETE FROM model_pricing_catalog WHERE id = ?")
                    .bind(row_id)
                    .execute(&mut *tx)
                    .await?;
            } else {
                let updated_json =
                    serde_json::to_string(&aliases).unwrap_or_else(|_| "[]".to_string());
                sqlx::query(
                    "UPDATE model_pricing_catalog SET usage_aliases = ?, updated_at = ? WHERE id = ?",
                )
                .bind(&updated_json)
                .bind(&now)
                .bind(row_id)
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await?;
        return Ok(());
    }

    remove_usage_alias_in_tx(&mut tx, tenant_id, trimmed_raw, false, &now).await?;

    let aliases_json =
        serde_json::to_string(&vec![trimmed_raw]).unwrap_or_else(|_| "[]".to_string());
    let provider_str = provider.unwrap_or("").trim();

    sqlx::query(
        r#"
        INSERT INTO model_pricing_catalog (
            tenant_id, model_id, provider, currency,
            input_per_million, cache_read_per_million, cache_write_per_million,
            reasoning_per_million, output_per_million,
            raw_currency, raw_input_per_million, raw_output_per_million,
            source_channel, price_locked, usage_aliases, created_at, updated_at
        ) VALUES (?, ?, ?, 'CNY', 0.0, 0.0, 0.0, 0.0, 0.0, 'CNY', 0.0, 0.0, 'user_override', 1, ?, ?, ?)
        ON CONFLICT(tenant_id, model_id) DO UPDATE SET
            provider = excluded.provider,
            input_per_million = 0.0,
            cache_read_per_million = 0.0,
            cache_write_per_million = 0.0,
            reasoning_per_million = 0.0,
            output_per_million = 0.0,
            raw_currency = 'CNY',
            raw_input_per_million = 0.0,
            raw_output_per_million = 0.0,
            source_channel = 'user_override',
            price_locked = 1,
            usage_aliases = excluded.usage_aliases,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(tenant_id)
    .bind(&ignore_key)
    .bind(provider_str)
    .bind(&aliases_json)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Returns distinct base `model_id`s that have non-empty `usage_aliases` (on non-ignored rows)
/// or active `price_locked = 1` overrides (stripped of `__override__::`).
pub async fn get_rebind_target_model_ids_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<Vec<String>> {
    let rows = sqlx::query(
        r#"
        SELECT model_id, usage_aliases, price_locked
        FROM model_pricing_catalog
        WHERE tenant_id = ?
          AND model_id NOT LIKE ?
          AND (
              (usage_aliases IS NOT NULL AND TRIM(usage_aliases) != '' AND TRIM(usage_aliases) != '[]')
              OR price_locked = 1
          )
        "#,
    )
    .bind(tenant_id)
    .bind(format!("{IGNORE_MODEL_ID_PREFIX}%"))
    .fetch_all(pool)
    .await?;

    let mut targets = BTreeSet::new();
    for row in rows {
        let model_id: String = row.get("model_id");
        if model_id.starts_with(IGNORE_MODEL_ID_PREFIX) {
            continue;
        }
        let price_locked: i64 = row.get("price_locked");
        let aliases_json: String = row.get("usage_aliases");
        let aliases: Vec<String> = serde_json::from_str(&aliases_json).unwrap_or_default();
        if price_locked == 0 && aliases.is_empty() {
            continue;
        }
        let base_id = base_model_id_from_catalog_key(&model_id).trim();
        if !base_id.is_empty() {
            targets.insert(base_id.to_string());
        }
    }

    Ok(targets.into_iter().collect())
}
