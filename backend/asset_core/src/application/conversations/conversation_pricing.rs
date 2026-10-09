//! Application Service: Model Pricing Status & Catalog Queries

use crate::backend::application::prelude::*;
use crate::backend::application::AppResult;
use crate::backend::domain::conversations::pricing::{ModelPriceEntry, PriceCatalog};
use crate::backend::domain::conversations::usage::{
    ModelPriceEntryDto, ModelPricingCatalogDto, ModelPricingStatusDto,
};
use crate::backend::store::conversations::pricing_repo::{
    get_pricing_metadata_sqlx, load_pricing_catalog_entries_sqlx, recalculate_usage_costs_sqlx,
    set_pricing_metadata_sqlx, upsert_pricing_catalog_entries_sqlx,
};
use crate::backend::store::conversations::usage_repo::get_distinct_usage_provider_models_sqlx;
use chrono::Utc;

struct ActivePricingContext {
    rate: f64,
    channel: String,
    version: String,
    last_updated: String,
    cached_models: Vec<ModelPriceEntry>,
    catalog: PriceCatalog,
}

impl AppService {
    async fn load_active_pricing_context(&self) -> AppResult<ActivePricingContext> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();

        let cached_models = load_pricing_catalog_entries_sqlx(pool, tenant_id)
            .await
            .unwrap_or_default();
        let rate_str = get_pricing_metadata_sqlx(pool, tenant_id, "usd_to_cny_rate")
            .await
            .ok()
            .flatten();
        let rate = rate_str.and_then(|s| s.parse::<f64>().ok()).unwrap_or(7.25);
        let channel = get_pricing_metadata_sqlx(pool, tenant_id, "active_source_channel")
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| "bundled".to_string());
        let version = get_pricing_metadata_sqlx(pool, tenant_id, "catalog_version")
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| "bundled".to_string());
        let last_updated = get_pricing_metadata_sqlx(pool, tenant_id, "last_synced_at")
            .await
            .ok()
            .flatten()
            .unwrap_or_default();

        let catalog = PriceCatalog::new(
            version.clone(),
            last_updated.clone(),
            rate,
            cached_models.clone(),
        );

        Ok(ActivePricingContext {
            rate,
            channel,
            version,
            last_updated,
            cached_models,
            catalog,
        })
    }

    pub async fn get_model_pricing_status(&self) -> AppResult<ModelPricingStatusDto> {
        let ctx = self.load_active_pricing_context().await?;
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();

        let distinct_local = get_distinct_usage_provider_models_sqlx(pool, tenant_id)
            .await
            .unwrap_or_default();
        let mut seen = std::collections::HashSet::new();
        for m in distinct_local {
            seen.insert(format!(
                "{}::{}",
                m.provider.to_ascii_lowercase(),
                m.model.to_ascii_lowercase()
            ));
        }
        for cached in &ctx.cached_models {
            for p in &cached.model_patterns {
                seen.insert(format!(
                    "{}::{}",
                    cached.provider.to_ascii_lowercase(),
                    p.to_ascii_lowercase()
                ));
            }
        }

        let count = if ctx.cached_models.is_empty() {
            PriceCatalog::bundled().models.len()
        } else {
            ctx.cached_models.len()
        };

        Ok(ModelPricingStatusDto {
            models_count: count,
            local_models_count: seen.len(),
            exchange_rate: ctx.rate,
            active_channel: ctx.channel,
            catalog_version: ctx.version,
            last_updated_at: ctx.last_updated,
        })
    }

    pub async fn get_model_pricing_catalog(&self) -> AppResult<ModelPricingCatalogDto> {
        let ctx = self.load_active_pricing_context().await?;
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();

        let distinct_local = get_distinct_usage_provider_models_sqlx(pool, tenant_id)
            .await
            .unwrap_or_default();

        let mut local_entries = Vec::new();
        let mut seen_local = std::collections::HashSet::new();

        for item in distinct_local {
            let key = format!(
                "{}::{}",
                item.provider.to_ascii_lowercase(),
                item.model.to_ascii_lowercase()
            );
            if !seen_local.insert(key) {
                continue;
            }

            if let Some(entry) = ctx.catalog.find_model(&item.provider, &item.model) {
                local_entries.push(ModelPriceEntryDto {
                    provider: item.provider,
                    model_patterns: vec![item.model],
                    currency: "CNY".to_string(),
                    input_per_million: (entry.input_per_million * 100.0).round() / 100.0,
                    cache_read_per_million: (entry.cache_read_per_million * 100.0).round() / 100.0,
                    cache_write_per_million: (entry.cache_write_per_million * 100.0).round()
                        / 100.0,
                    output_per_million: (entry.output_per_million * 100.0).round() / 100.0,
                    reasoning_per_million: (entry.reasoning_per_million * 100.0).round() / 100.0,
                    raw_currency: entry.raw_currency.clone(),
                    raw_input_per_million: entry.raw_input_per_million,
                    raw_output_per_million: entry.raw_output_per_million,
                });
            } else {
                local_entries.push(ModelPriceEntryDto {
                    provider: item.provider,
                    model_patterns: vec![item.model],
                    currency: "CNY".to_string(),
                    input_per_million: 0.0,
                    cache_read_per_million: 0.0,
                    cache_write_per_million: 0.0,
                    output_per_million: 0.0,
                    reasoning_per_million: 0.0,
                    raw_currency: Some("CNY".to_string()),
                    raw_input_per_million: Some(0.0),
                    raw_output_per_million: Some(0.0),
                });
            }
        }

        for cached in &ctx.cached_models {
            for pattern in &cached.model_patterns {
                let key = format!(
                    "{}::{}",
                    cached.provider.to_ascii_lowercase(),
                    pattern.to_ascii_lowercase()
                );
                if seen_local.insert(key) {
                    local_entries.push(ModelPriceEntryDto {
                        provider: cached.provider.clone(),
                        model_patterns: vec![pattern.clone()],
                        currency: cached.currency.clone(),
                        input_per_million: (cached.input_per_million * 100.0).round() / 100.0,
                        cache_read_per_million: (cached.cache_read_per_million * 100.0).round()
                            / 100.0,
                        cache_write_per_million: (cached.cache_write_per_million * 100.0).round()
                            / 100.0,
                        output_per_million: (cached.output_per_million * 100.0).round() / 100.0,
                        reasoning_per_million: (cached.reasoning_per_million * 100.0).round()
                            / 100.0,
                        raw_currency: cached.raw_currency.clone(),
                        raw_input_per_million: cached.raw_input_per_million,
                        raw_output_per_million: cached.raw_output_per_million,
                    });
                }
            }
        }

        local_entries.sort_by(|a, b| {
            a.provider
                .cmp(&b.provider)
                .then_with(|| a.model_patterns.cmp(&b.model_patterns))
        });

        let mut all_entries = ctx
            .catalog
            .models
            .into_iter()
            .map(|m| ModelPriceEntryDto {
                provider: m.provider,
                model_patterns: m.model_patterns,
                currency: m.currency,
                input_per_million: (m.input_per_million * 100.0).round() / 100.0,
                cache_read_per_million: (m.cache_read_per_million * 100.0).round() / 100.0,
                cache_write_per_million: (m.cache_write_per_million * 100.0).round() / 100.0,
                output_per_million: (m.output_per_million * 100.0).round() / 100.0,
                reasoning_per_million: (m.reasoning_per_million * 100.0).round() / 100.0,
                raw_currency: m.raw_currency,
                raw_input_per_million: m.raw_input_per_million,
                raw_output_per_million: m.raw_output_per_million,
            })
            .collect::<Vec<_>>();

        all_entries.sort_by(|a, b| {
            a.provider
                .cmp(&b.provider)
                .then_with(|| a.model_patterns.cmp(&b.model_patterns))
        });

        Ok(ModelPricingCatalogDto {
            catalog_version: ctx.version,
            updated_at: ctx.last_updated,
            usd_to_cny_rate: ctx.rate,
            active_channel: ctx.channel,
            models: all_entries,
            local_models: local_entries,
        })
    }

    pub async fn set_model_pricing_exchange_rate(
        &self,
        rate: f64,
    ) -> AppResult<ModelPricingStatusDto> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        set_pricing_metadata_sqlx(pool, tenant_id, "usd_to_cny_rate", &rate.to_string()).await?;

        let cached_models = load_pricing_catalog_entries_sqlx(pool, tenant_id)
            .await
            .unwrap_or_default();
        let catalog = PriceCatalog::new(
            "custom".to_string(),
            Utc::now().to_rfc3339(),
            rate,
            cached_models,
        );
        let _ = recalculate_usage_costs_sqlx(pool, tenant_id, &catalog).await?;
        self.get_model_pricing_status().await
    }

    pub async fn save_model_price_entry(
        &self,
        entry: ModelPriceEntryDto,
    ) -> AppResult<ModelPricingStatusDto> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let domain_entry = ModelPriceEntry {
            provider: entry.provider,
            model_patterns: entry.model_patterns,
            currency: entry.currency,
            input_per_million: entry.input_per_million,
            cache_read_per_million: entry.cache_read_per_million,
            cache_write_per_million: entry.cache_write_per_million,
            reasoning_per_million: entry.reasoning_per_million,
            output_per_million: entry.output_per_million,
            raw_currency: entry.raw_currency,
            raw_input_per_million: entry.raw_input_per_million,
            raw_output_per_million: entry.raw_output_per_million,
        };
        upsert_pricing_catalog_entries_sqlx(pool, tenant_id, &[domain_entry], "user_override")
            .await?;

        let status = self.get_model_pricing_status().await?;
        let cached_models = load_pricing_catalog_entries_sqlx(pool, tenant_id)
            .await
            .unwrap_or_default();
        let catalog = PriceCatalog::new(
            "custom".to_string(),
            Utc::now().to_rfc3339(),
            status.exchange_rate,
            cached_models,
        );
        let _ = recalculate_usage_costs_sqlx(pool, tenant_id, &catalog).await?;
        Ok(status)
    }
}

#[cfg(test)]
#[path = "conversation_pricing_tests.rs"]
mod tests;
