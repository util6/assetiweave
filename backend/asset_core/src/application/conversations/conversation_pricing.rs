//! Application Service: Model Pricing Status, Catalog Queries & Mutations

#[path = "conversation_pricing_view.rs"]
mod view;

use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::conversations::pricing::{
    base_model_id_from_catalog_key, ModelPriceEntry, PriceCatalog,
};
use crate::backend::domain::conversations::usage::{
    ModelPriceEntryDto, ModelPricingCatalogDto, ModelPricingStatusDto,
};
use crate::backend::store::conversations::pricing_repo::{
    get_pricing_metadata_sqlx, load_pricing_catalog_entries_sqlx, rebind_model_pricing_alias_sqlx,
    recalculate_usage_costs_sqlx, save_locked_override_sqlx, set_model_pricing_ignored_sqlx,
    set_model_pricing_lock_sqlx, set_pricing_metadata_sqlx,
};
use crate::backend::store::conversations::usage_repo::get_distinct_usage_provider_models_sqlx;
use view::{
    build_local_pricing_entries, build_reference_catalog_dtos, find_reference_target_entry,
    ActivePricingContext,
};

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

    async fn recalculate_and_get_status(&self) -> AppResult<ModelPricingStatusDto> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let ctx = self.load_active_pricing_context().await?;
        let _ = recalculate_usage_costs_sqlx(pool, tenant_id, &ctx.catalog).await?;

        let distinct_local = get_distinct_usage_provider_models_sqlx(pool, tenant_id)
            .await
            .unwrap_or_default();
        let all_entries = build_reference_catalog_dtos(&ctx);
        let local_entries = build_local_pricing_entries(&ctx, distinct_local);

        Ok(ModelPricingStatusDto {
            models_count: all_entries.len(),
            local_models_count: local_entries.len(),
            exchange_rate: ctx.rate,
            active_channel: ctx.channel,
            catalog_version: ctx.version,
            last_updated_at: ctx.last_updated,
        })
    }

    pub async fn get_model_pricing_status(&self) -> AppResult<ModelPricingStatusDto> {
        let ctx = self.load_active_pricing_context().await?;
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();

        let distinct_local = get_distinct_usage_provider_models_sqlx(pool, tenant_id)
            .await
            .unwrap_or_default();
        let all_entries = build_reference_catalog_dtos(&ctx);
        let local_entries = build_local_pricing_entries(&ctx, distinct_local);

        Ok(ModelPricingStatusDto {
            models_count: all_entries.len(),
            local_models_count: local_entries.len(),
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
        let local_entries = build_local_pricing_entries(&ctx, distinct_local);
        let all_entries = build_reference_catalog_dtos(&ctx);

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
        self.recalculate_and_get_status().await
    }

    pub async fn save_model_price_entry(
        &self,
        entry: ModelPriceEntryDto,
    ) -> AppResult<ModelPricingStatusDto> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();

        let target_base_id = entry
            .matched_model_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                entry
                    .model_patterns
                    .first()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| {
                let raw = entry.raw_model.trim();
                if raw.is_empty() {
                    None
                } else {
                    Some(raw)
                }
            })
            .map(base_model_id_from_catalog_key)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::Validation("模型标识不能为空".to_string()))?
            .to_string();

        let currency = if entry.currency.trim().is_empty() {
            "CNY".to_string()
        } else {
            entry.currency.trim().to_string()
        };

        let domain_entry = ModelPriceEntry {
            provider: entry.provider,
            model_patterns: vec![target_base_id.clone()],
            currency: currency.clone(),
            input_per_million: entry.input_per_million,
            cache_read_per_million: entry.cache_read_per_million,
            cache_write_per_million: entry.cache_write_per_million,
            reasoning_per_million: entry.reasoning_per_million,
            output_per_million: entry.output_per_million,
            raw_currency: Some(currency),
            raw_input_per_million: Some(entry.input_per_million),
            raw_output_per_million: Some(entry.output_per_million),
            price_locked: true,
            usage_aliases: Vec::new(),
            source_channel: "user_override".to_string(),
            is_ignored: false,
        };

        save_locked_override_sqlx(pool, tenant_id, &target_base_id, &domain_entry).await?;
        self.recalculate_and_get_status().await
    }

    pub async fn rebind_model_pricing_alias(
        &self,
        raw_model: String,
        target_model_id: Option<String>,
    ) -> AppResult<ModelPricingStatusDto> {
        let trimmed_raw = raw_model.trim();
        if trimmed_raw.is_empty() {
            return Err(AppError::Validation("原始模型名称不能为空".to_string()));
        }

        let pool = self.db.pool();
        let tenant_id = self.tenant_id();

        let target_id_opt = target_model_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());

        if let Some(target_id) = target_id_opt {
            let ctx = self.load_active_pricing_context().await?;
            let target_entry = find_reference_target_entry(&ctx, target_id)
                .ok_or_else(|| AppError::NotFound(format!("未找到目标参考模型: {target_id}")))?;
            rebind_model_pricing_alias_sqlx(pool, tenant_id, trimmed_raw, Some(&target_entry))
                .await?;
        } else {
            rebind_model_pricing_alias_sqlx(pool, tenant_id, trimmed_raw, None).await?;
        }

        self.recalculate_and_get_status().await
    }

    pub async fn set_model_pricing_lock(
        &self,
        model_id: String,
        locked: bool,
    ) -> AppResult<ModelPricingStatusDto> {
        let clean_id = base_model_id_from_catalog_key(&model_id).trim().to_string();
        if clean_id.is_empty() {
            return Err(AppError::Validation("模型标识不能为空".to_string()));
        }

        let pool = self.db.pool();
        let tenant_id = self.tenant_id();

        let fallback_entry = if locked {
            let ctx = self.load_active_pricing_context().await?;
            find_reference_target_entry(&ctx, &clean_id)
        } else {
            None
        };

        set_model_pricing_lock_sqlx(pool, tenant_id, &clean_id, locked, fallback_entry.as_ref())
            .await?;

        self.recalculate_and_get_status().await
    }

    pub async fn set_model_pricing_ignored(
        &self,
        raw_model: String,
        provider: Option<String>,
        ignored: bool,
    ) -> AppResult<ModelPricingStatusDto> {
        let trimmed_raw = raw_model.trim();
        if trimmed_raw.is_empty() {
            return Err(AppError::Validation("原始模型名称不能为空".to_string()));
        }

        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        set_model_pricing_ignored_sqlx(pool, tenant_id, trimmed_raw, provider.as_deref(), ignored)
            .await?;

        self.recalculate_and_get_status().await
    }
}

#[cfg(test)]
#[path = "conversation_pricing_tests.rs"]
mod tests;
