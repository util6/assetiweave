//! Application Service: Model Pricing Sync Pipeline

use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::conversations::pricing::{
    base_model_id_from_catalog_key, is_dirty_model_name, ModelPriceEntry, PriceCatalog,
    IGNORE_MODEL_ID_PREFIX, OVERRIDE_MODEL_ID_PREFIX,
};
use crate::backend::domain::conversations::usage::ModelPricingSyncResultDto;
use crate::backend::infrastructure::conversations::pricing_fetcher::{
    fetch_tiered_pricing_catalog, fetch_usd_to_cny_rate, PricingSourceChannel, TieredFetchOutcome,
};
use crate::backend::infrastructure::tasks::{
    stages, ConflictPolicy, InfraError, PipelineDescriptor, TaskCapabilities, TaskHandle, TaskKind,
    TaskOutput, TaskSpec,
};
use crate::backend::store::conversations::pricing_repo::{
    get_pricing_metadata_sqlx, get_rebind_target_model_ids_sqlx, load_pricing_catalog_entries_sqlx,
    recalculate_usage_costs_sqlx, set_pricing_metadata_sqlx, upsert_pricing_catalog_entries_sqlx,
};
use crate::backend::store::conversations::usage_repo::get_distinct_usage_models_sqlx;
use chrono::Utc;
use std::collections::HashSet;
use tokio_util::sync::CancellationToken;

impl AppService {
    /// 价格同步与重算流水线骨架（阶段 id 全部来自 `stages::*` 编译期常量）。
    fn pricing_sync_pipeline() -> PipelineDescriptor {
        PipelineDescriptor::builder()
            .stage(stages::FETCH_PRICING, "获取最新价格与外汇汇率")
            .stage(stages::PERSIST_CATALOG, "写入本地价格库持久化缓存")
            .stage(stages::RECALCULATE_HISTORY, "全量重新核算历史会话用量费用")
            .build()
    }

    /// 启动任务中心全量托管的价格同步与重算任务。
    pub fn start_model_pricing_sync(
        &self,
        force: bool,
        target_models: Option<Vec<String>>,
    ) -> AppResult<TaskHandle<ModelPricingSyncResultDto>> {
        self.start_model_pricing_sync_with(
            force,
            target_models,
            |cached_rate, cached_entries, targets| {
                let rate = fetch_usd_to_cny_rate().unwrap_or(cached_rate);
                let outcome =
                    fetch_tiered_pricing_catalog(rate, cached_entries, targets.as_deref());
                (rate, outcome)
            },
        )
    }

    #[cfg(test)]
    pub(crate) fn start_model_pricing_sync_with_fetcher<Fetch>(
        &self,
        force: bool,
        fetcher: Fetch,
    ) -> AppResult<TaskHandle<ModelPricingSyncResultDto>>
    where
        Fetch: FnOnce(f64, Vec<ModelPriceEntry>, Option<Vec<String>>) -> (f64, TieredFetchOutcome)
            + Send
            + 'static,
    {
        self.start_model_pricing_sync_with(force, None, fetcher)
    }

    fn start_model_pricing_sync_with<Fetch>(
        &self,
        force: bool,
        target_models: Option<Vec<String>>,
        fetcher: Fetch,
    ) -> AppResult<TaskHandle<ModelPricingSyncResultDto>>
    where
        Fetch: FnOnce(f64, Vec<ModelPriceEntry>, Option<Vec<String>>) -> (f64, TieredFetchOutcome)
            + Send
            + 'static,
    {
        let tenant_id = self.tenant_id().to_string();
        let mut spec = TaskSpec::new(
            TaskKind::ModelPricingSync,
            Some(format!("{tenant_id}:model_pricing_sync")),
        )
        .with_tenant_id(tenant_id.clone())
        .with_title("模型价格同步与重算")
        .with_capabilities(TaskCapabilities {
            cancellable: true,
            retryable: false,
            clearable: true,
        })
        .with_conflict_key(format!("{tenant_id}:model_pricing_sync"))
        .with_pipeline(Self::pricing_sync_pipeline());

        if force {
            // 显式强制刷新：取消同去重键下仍在运行的历史任务并启动新一轮同步，
            // 避免一次异常卡死的旧任务让同步入口永久被去重拒绝。
            spec = spec.with_conflict_policy(ConflictPolicy::CancelExisting);
        }

        let service = self.clone();
        self.spawn_pipeline_task(spec, move |cx| async move {
            let cancellation = cx.cancellation();
            let task_id = cx.task_id().to_string();
            let mut noop_progress = |_phase: &str, _message: &str| {};
            let result = service
                .sync_model_pricing_catalog_inner(
                    target_models,
                    Some(&cancellation),
                    Some(&task_id),
                    &mut noop_progress,
                    fetcher,
                )
                .await
                .map_err(InfraError::external)?;
            let mut summary = format!(
                "价格同步完成：渠道 {}，更新 {} 款模型，重算 {} 条记录（汇率 {:.4}）",
                result.channel_used,
                result.models_count,
                result.recalculated_events_count,
                result.exchange_rate
            );
            if let Some(reason) = result.warning.as_deref() {
                summary.push_str(&format!("；{reason}"));
            }
            Ok(TaskOutput::with_summary(result, summary))
        })
    }
}

impl AppService {
    #[cfg(test)]
    pub(crate) async fn sync_model_pricing_catalog_with_fetcher<Fetch>(
        &self,
        _force: bool,
        target_models: Option<Vec<String>>,
        cancellation: Option<&CancellationToken>,
        task_id: Option<&str>,
        fetcher: Fetch,
    ) -> AppResult<ModelPricingSyncResultDto>
    where
        Fetch: FnOnce(f64, Vec<ModelPriceEntry>, Option<Vec<String>>) -> (f64, TieredFetchOutcome)
            + Send
            + 'static,
    {
        let mut noop_progress = |_phase: &str, _msg: &str| {};
        self.sync_model_pricing_catalog_inner(
            target_models,
            cancellation,
            task_id,
            &mut noop_progress,
            fetcher,
        )
        .await
    }

    async fn sync_model_pricing_catalog_inner<F, Fetch>(
        &self,
        target_models: Option<Vec<String>>,
        cancellation: Option<&CancellationToken>,
        task_id: Option<&str>,
        on_progress: &mut F,
        fetcher: Fetch,
    ) -> AppResult<ModelPricingSyncResultDto>
    where
        F: FnMut(&str, &str) + Send,
        Fetch: FnOnce(f64, Vec<ModelPriceEntry>, Option<Vec<String>>) -> (f64, TieredFetchOutcome)
            + Send
            + 'static,
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
            // 托管流水线在提交时已前置声明阶段骨架，此处仅补齐外部注册路径的阶段名称。
            let already_declared = task_runtime
                .get(tid)
                .is_some_and(|snapshot| !snapshot.stages.is_empty());
            if !already_declared {
                let _ =
                    task_runtime.set_stages(tid, Self::pricing_sync_pipeline().to_initial_stages());
            }
        }

        let task_ctx = task_id.and_then(|tid| task_runtime.task_context(tid).ok());

        // Stage 1: Fetch pricing
        on_progress("fetching", "正在获取最新模型价格及汇率...");
        let fetch_outcome = {
            let stage_guard = task_ctx
                .as_ref()
                .map(|ctx| ctx.enter_stage(stages::FETCH_PRICING));
            if let Some(ref guard) = stage_guard {
                guard.record_step(
                    "fetch_rate",
                    Some("正在获取 USD/CNY 实时外汇汇率".to_string()),
                    None,
                    None,
                );
            }

            // Determine active/target models on demand (union usage models + rebind/locked base IDs)
            let used_models = match target_models {
                Some(models) if !models.is_empty() => models,
                _ => {
                    let usage_models = get_distinct_usage_models_sqlx(pool, tenant_id)
                        .await
                        .unwrap_or_default();
                    let rebind_targets = get_rebind_target_model_ids_sqlx(pool, tenant_id)
                        .await
                        .unwrap_or_default();
                    let mut seen = HashSet::new();
                    let mut combined = Vec::new();
                    for model in usage_models.into_iter().chain(rebind_targets.into_iter()) {
                        let trimmed = model.trim().to_string();
                        if !trimmed.is_empty() && seen.insert(trimmed.to_ascii_lowercase()) {
                            combined.push(trimmed);
                        }
                    }
                    combined
                }
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

            let active_locked_base_ids: HashSet<String> = cached_entries
                .iter()
                .filter(|e| e.price_locked && !e.is_ignored)
                .filter_map(|e| e.model_patterns.first())
                .filter_map(|p| {
                    let trimmed = p.trim();
                    if trimmed.is_empty() || trimmed.starts_with(IGNORE_MODEL_ID_PREFIX) {
                        return None;
                    }
                    let base = base_model_id_from_catalog_key(trimmed)
                        .trim_matches('*')
                        .trim()
                        .to_ascii_lowercase();
                    if base.is_empty() {
                        None
                    } else {
                        Some(base)
                    }
                })
                .collect();

            if let Some(c) = cancellation {
                if c.is_cancelled() {
                    return Err(AppError::Cancelled("模型价格同步已取消".to_string()));
                }
            }

            let cached_entries_clone = cached_entries.clone();
            let used_models_clone = used_models.clone();
            let fetch_res = tokio::task::spawn_blocking(move || {
                let target_slice = if used_models_clone.is_empty() {
                    None
                } else {
                    Some(used_models_clone)
                };
                fetcher(cached_rate, cached_entries_clone, target_slice)
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

            let mut unlocked_entries = Vec::with_capacity(outcome.entries.len());
            let mut locked_ref_entries = Vec::new();
            for entry in outcome.entries {
                let is_locked_base = canonical_outcome_base_id(&entry)
                    .is_some_and(|base_id| active_locked_base_ids.contains(&base_id));
                if is_locked_base {
                    locked_ref_entries.push(entry);
                } else {
                    unlocked_entries.push(entry);
                }
            }
            let unlocked_match_count = unlocked_entries
                .iter()
                .filter(|e| e.input_per_million > 0.0 || e.output_per_million > 0.0)
                .count();

            let downgraded = outcome.downgraded;
            let warning = outcome.warning.clone();

            if let Some(ref guard) = stage_guard {
                guard.record_step(
                    "rate_done",
                    Some(format!("参考汇率: 1 USD = ¥{live_rate:.4}")),
                    None,
                    None,
                );
                let mut message = if locked_ref_entries.is_empty() {
                    format!(
                        "按需获取完成: 渠道 [{}] 共匹配 {} 款实际模型价格",
                        outcome.channel.as_str(),
                        unlocked_match_count
                    )
                } else {
                    format!(
                        "按需获取完成: 渠道 [{}] 共匹配 {} 款可更新模型价格（已排除 {} 款锁定模型）",
                        outcome.channel.as_str(),
                        unlocked_match_count,
                        locked_ref_entries.len()
                    )
                };
                if let Some(reason) = warning.as_deref() {
                    message.push_str(&format!("；{reason}"));
                }
                guard.record_step("fetch_done", Some(message), None, None);
            }
            FetchPhaseOutcome {
                channel: outcome.channel,
                rate: outcome.rate,
                catalog_version: outcome.catalog_version,
                unlocked_entries,
                locked_ref_entries,
                downgraded,
                warning,
            }
        };

        let FetchPhaseOutcome {
            channel,
            rate,
            catalog_version,
            unlocked_entries,
            locked_ref_entries,
            downgraded,
            warning,
        } = fetch_outcome;

        // Stage 2: Persist catalog
        on_progress("persisting", "正在写入本地价格库持久化缓存...");
        let (channel_str, synced_models_count, now) = {
            let stage_guard = task_ctx
                .as_ref()
                .map(|ctx| ctx.enter_stage(stages::PERSIST_CATALOG));
            if let Some(ref guard) = stage_guard {
                guard.record_step(
                    "upsert_sqlite",
                    Some(if locked_ref_entries.is_empty() {
                        format!(
                            "正在批量更新数据库，入库 {} 条模型价格记录",
                            unlocked_entries.len()
                        )
                    } else {
                        format!(
                            "正在批量更新数据库，入库 {} 条可更新模型价格记录（另有 {} 款锁定模型仅刷新底层参考价）",
                            unlocked_entries.len(),
                            locked_ref_entries.len()
                        )
                    }),
                    None,
                    None,
                );
            }

            if let Some(c) = cancellation {
                if c.is_cancelled() {
                    return Err(AppError::Cancelled("模型价格同步已取消".to_string()));
                }
            }

            let channel_str = channel.as_str().to_string();
            let synced_models_count = upsert_pricing_catalog_entries_sqlx(
                pool,
                tenant_id,
                &unlocked_entries,
                &channel_str,
            )
            .await?;
            if !locked_ref_entries.is_empty() {
                // Keep underlying native <base> reference rows fresh so unlocking later
                // falls back to current reference pricing, without counting them as updated.
                let _ = upsert_pricing_catalog_entries_sqlx(
                    pool,
                    tenant_id,
                    &locked_ref_entries,
                    &channel_str,
                )
                .await?;
            }

            let now = Utc::now().to_rfc3339();
            set_pricing_metadata_sqlx(pool, tenant_id, "usd_to_cny_rate", &rate.to_string())
                .await?;
            set_pricing_metadata_sqlx(pool, tenant_id, "catalog_version", &catalog_version).await?;
            set_pricing_metadata_sqlx(pool, tenant_id, "active_source_channel", &channel_str)
                .await?;
            set_pricing_metadata_sqlx(pool, tenant_id, "last_synced_at", &now).await?;

            if let Some(ref guard) = stage_guard {
                guard.record_step(
                    "persist_done",
                    Some(format!("价格库已保存: 更新模型数 {synced_models_count}")),
                    None,
                    None,
                );
            }
            (channel_str, synced_models_count, now)
        };

        // Stage 3: Recalculate historical usage costs
        on_progress("recalculating", "正在基于最新价格库重新核算历史记录费用...");
        let recalculated_count = {
            let stage_guard = task_ctx
                .as_ref()
                .map(|ctx| ctx.enter_stage(stages::RECALCULATE_HISTORY));
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

            let effective_entries = load_pricing_catalog_entries_sqlx(pool, tenant_id).await?;
            let catalog = PriceCatalog::new(
                catalog_version.clone(),
                now.clone(),
                rate,
                effective_entries,
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
            models_count: synced_models_count,
            exchange_rate: rate,
            recalculated_events_count: recalculated_count,
            catalog_version,
            updated_at: now,
            downgraded,
            warning,
        })
    }
}

/// 抓取阶段产出：锁定模型参考行已从可更新条目中拆出，并携带降级标记。
struct FetchPhaseOutcome {
    channel: PricingSourceChannel,
    rate: f64,
    catalog_version: String,
    unlocked_entries: Vec<ModelPriceEntry>,
    locked_ref_entries: Vec<ModelPriceEntry>,
    downgraded: bool,
    warning: Option<String>,
}

fn canonical_outcome_base_id(entry: &ModelPriceEntry) -> Option<String> {
    let is_valid_ref = |s: &&str| {
        !s.is_empty()
            && !s.starts_with(OVERRIDE_MODEL_ID_PREFIX)
            && !s.starts_with(IGNORE_MODEL_ID_PREFIX)
            && !is_dirty_model_name(s)
    };
    let chosen = entry
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
        })?;
    let base = base_model_id_from_catalog_key(chosen)
        .trim_matches('*')
        .trim()
        .to_ascii_lowercase();
    if base.is_empty() {
        None
    } else {
        Some(base)
    }
}

#[cfg(test)]
#[path = "conversation_pricing_sync_tests.rs"]
mod tests;
