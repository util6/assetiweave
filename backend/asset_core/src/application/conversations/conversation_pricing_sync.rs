//! Application Service: Model Pricing Sync Pipeline

use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::conversations::pricing::PriceCatalog;
use crate::backend::domain::conversations::usage::ModelPricingSyncResultDto;
use crate::backend::infrastructure::conversations::pricing_fetcher::{
    fetch_tiered_pricing_catalog, fetch_usd_to_cny_rate,
};
use crate::backend::infrastructure::tasks::{StageStatus, TaskStage};
use crate::backend::store::conversations::pricing_repo::{
    get_pricing_metadata_sqlx, load_pricing_catalog_entries_sqlx, recalculate_usage_costs_sqlx,
    set_pricing_metadata_sqlx, upsert_pricing_catalog_entries_sqlx,
};
use crate::backend::store::conversations::usage_repo::get_distinct_usage_models_sqlx;
use chrono::Utc;
use tokio_util::sync::CancellationToken;

impl AppService {
    pub async fn sync_model_pricing_catalog(
        &self,
        force: bool,
    ) -> AppResult<ModelPricingSyncResultDto> {
        let mut noop_progress = |_phase: &str, _msg: &str| {};
        self.sync_model_pricing_catalog_with_control(force, None, None, None, &mut noop_progress)
            .await
    }

    pub async fn sync_model_pricing_catalog_with_control<F>(
        &self,
        _force: bool,
        target_models: Option<Vec<String>>,
        cancellation: Option<&CancellationToken>,
        task_id: Option<&str>,
        on_progress: &mut F,
    ) -> AppResult<ModelPricingSyncResultDto>
    where
        F: FnMut(&str, &str) + Send,
    {
        if let Some(c) = cancellation {
            if c.is_cancelled() {
                return Err(AppError::Cancelled("模型价格同步已取消".to_string()));
            }
        }

        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let task_runtime = self.runtime.task_runtime();

        // Register 3 pipeline stages if task_id is present
        if let Some(tid) = task_id {
            let stages = vec![
                make_stage("fetch_pricing", "获取最新价格与外汇汇率"),
                make_stage("persist_catalog", "写入本地价格库持久化缓存"),
                make_stage("recalculate_history", "全量重新核算历史会话用量费用"),
            ];
            let _ = task_runtime.set_stages(tid, stages);
        }

        let task_ctx = task_id.and_then(|tid| task_runtime.task_context(tid).ok());

        // Stage 1: Fetch pricing
        on_progress("fetching", "正在获取最新模型价格及汇率...");
        let (_live_rate, outcome) = {
            let stage_guard = task_ctx
                .as_ref()
                .map(|ctx| ctx.enter_stage("fetch_pricing"));
            if let Some(ref guard) = stage_guard {
                guard.record_step(
                    "fetch_rate",
                    Some("正在获取 USD/CNY 实时外汇汇率".to_string()),
                    None,
                    None,
                );
            }

            // Determine active/target models on demand
            let used_models = match target_models {
                Some(models) if !models.is_empty() => models,
                _ => get_distinct_usage_models_sqlx(pool, tenant_id)
                    .await
                    .unwrap_or_default(),
            };

            if let Some(ref guard) = stage_guard {
                if used_models.is_empty() {
                    guard.record_step(
                        "detect_models",
                        Some("当前尚无已用模型记录，默认就绪".to_string()),
                        None,
                        None,
                    );
                } else {
                    let preview = if used_models.len() <= 6 {
                        used_models.join(", ")
                    } else {
                        format!(
                            "{} 等共 {} 款",
                            used_models[..6].join(", "),
                            used_models.len()
                        )
                    };
                    guard.record_step(
                        "detect_models",
                        Some(format!("按需检索实际使用的模型: {}", preview)),
                        None,
                        None,
                    );
                }
            }

            let cached_rate_str = get_pricing_metadata_sqlx(pool, tenant_id, "usd_to_cny_rate")
                .await
                .ok()
                .flatten();
            let cached_rate = cached_rate_str
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(7.25);

            let cached_entries = load_pricing_catalog_entries_sqlx(pool, tenant_id)
                .await
                .unwrap_or_default();

            if let Some(c) = cancellation {
                if c.is_cancelled() {
                    return Err(AppError::Cancelled("模型价格同步已取消".to_string()));
                }
            }

            let cached_entries_clone = cached_entries.clone();
            let used_models_clone = used_models.clone();
            let fetch_res = tokio::task::spawn_blocking(move || {
                let rate = fetch_usd_to_cny_rate().unwrap_or(cached_rate);
                let target_slice = if used_models_clone.is_empty() {
                    None
                } else {
                    Some(used_models_clone.as_slice())
                };
                let outcome =
                    fetch_tiered_pricing_catalog(rate, cached_entries_clone, target_slice);
                (rate, outcome)
            })
            .await;

            let (live_rate, outcome) = match fetch_res {
                Ok((r, o)) => (r, o),
                Err(e) => {
                    tracing::warn!("pricing fetch failed: {e:?}, using bundled fallback");
                    let target_slice = if used_models.is_empty() {
                        None
                    } else {
                        Some(used_models.as_slice())
                    };
                    let outcome = fetch_tiered_pricing_catalog(cached_rate, vec![], target_slice);
                    (cached_rate, outcome)
                }
            };

            if let Some(ref guard) = stage_guard {
                guard.record_step(
                    "rate_done",
                    Some(format!("参考汇率: 1 USD = ¥{live_rate:.4}")),
                    None,
                    None,
                );
                guard.record_step(
                    "fetch_done",
                    Some(format!(
                        "按需获取完成: 渠道 [{}] 共匹配 {} 款实际模型价格",
                        outcome.channel.as_str(),
                        outcome.entries.len()
                    )),
                    None,
                    None,
                );
            }
            (live_rate, outcome)
        };

        // Stage 2: Persist catalog
        on_progress("persisting", "正在写入本地价格库持久化缓存...");
        let (channel_str, models_count, now) = {
            let stage_guard = task_ctx
                .as_ref()
                .map(|ctx| ctx.enter_stage("persist_catalog"));
            if let Some(ref guard) = stage_guard {
                guard.record_step(
                    "upsert_sqlite",
                    Some(format!(
                        "正在批量更新数据库，入库 {} 条模型价格记录",
                        outcome.entries.len()
                    )),
                    None,
                    None,
                );
            }

            if let Some(c) = cancellation {
                if c.is_cancelled() {
                    return Err(AppError::Cancelled("模型价格同步已取消".to_string()));
                }
            }

            let channel_str = outcome.channel.as_str().to_string();
            let models_count = upsert_pricing_catalog_entries_sqlx(
                pool,
                tenant_id,
                &outcome.entries,
                &channel_str,
            )
            .await?;

            let now = Utc::now().to_rfc3339();
            set_pricing_metadata_sqlx(
                pool,
                tenant_id,
                "usd_to_cny_rate",
                &outcome.rate.to_string(),
            )
            .await?;
            set_pricing_metadata_sqlx(pool, tenant_id, "catalog_version", &outcome.catalog_version)
                .await?;
            set_pricing_metadata_sqlx(pool, tenant_id, "active_source_channel", &channel_str)
                .await?;
            set_pricing_metadata_sqlx(pool, tenant_id, "last_synced_at", &now).await?;

            if let Some(ref guard) = stage_guard {
                guard.record_step(
                    "persist_done",
                    Some(format!("价格库已保存: 活跃模型数 {models_count}")),
                    None,
                    None,
                );
            }
            (channel_str, models_count, now)
        };

        // Stage 3: Recalculate historical usage costs
        on_progress("recalculating", "正在基于最新价格库重新核算历史记录费用...");
        let recalculated_count = {
            let stage_guard = task_ctx
                .as_ref()
                .map(|ctx| ctx.enter_stage("recalculate_history"));
            if let Some(ref guard) = stage_guard {
                guard.record_step(
                    "recalc_batch",
                    Some("开始批量核算 conversation_usage_events 历史条目费用".to_string()),
                    None,
                    None,
                );
            }

            if let Some(c) = cancellation {
                if c.is_cancelled() {
                    return Err(AppError::Cancelled("模型价格同步已取消".to_string()));
                }
            }

            let catalog = PriceCatalog::new(
                outcome.catalog_version.clone(),
                now.clone(),
                outcome.rate,
                outcome.entries,
            );
            let recalculated_count =
                recalculate_usage_costs_sqlx(pool, tenant_id, &catalog).await?;

            if let Some(ref guard) = stage_guard {
                guard.record_step(
                    "recalc_done",
                    Some(format!("费用核算完毕，已更新 {recalculated_count} 条记录")),
                    None,
                    None,
                );
            }
            recalculated_count
        };

        on_progress("completed", "模型价格库更新与用量重算已完成");

        Ok(ModelPricingSyncResultDto {
            channel_used: channel_str,
            models_count,
            exchange_rate: outcome.rate,
            recalculated_events_count: recalculated_count,
            catalog_version: outcome.catalog_version,
            updated_at: now,
        })
    }
}

fn make_stage(id: &str, name: &str) -> TaskStage {
    TaskStage {
        id: id.to_string(),
        name: name.to_string(),
        status: StageStatus::Pending,
        started_at: None,
        finished_at: None,
        duration_ms: None,
        progress: None,
        current_activities: Vec::new(),
        metrics: Vec::new(),
        failures: Vec::new(),
        skipped: Vec::new(),
        agent_session_ref: None,
        steps: Vec::new(),
    }
}
