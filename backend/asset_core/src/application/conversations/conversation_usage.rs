use super::usage_dto::{
    UsageDashboardDto, UsageDashboardFilter, UsageScanOptions, UsageScanStatusDto,
    UsageSourceDiagnosticDto,
};
use crate::backend::application::prelude::*;
use crate::backend::infrastructure::tasks::{StageStatus, TaskStage};
use crate::backend::{
    domain::{conversations::pricing::PriceCatalog, ConversationAdapter, ConversationSource},
    infrastructure::conversations::{
        adapter_supports_usage, read_external_adapter_usage_with_settings,
    },
    store::conversations::{
        get_pricing_metadata_sqlx, load_all_usage_source_states_sqlx,
        load_pricing_catalog_entries_sqlx, load_usage_source_state_sqlx,
        prune_unseen_usage_events_sqlx, query_usage_dashboard_sqlx, recalculate_usage_costs_sqlx,
        save_usage_source_state_sqlx, upsert_usage_events_sqlx,
        usage_repo::get_distinct_usage_models_sqlx, UsageSourceState,
    },
};
use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

impl AppService {
    pub async fn get_conversation_usage_dashboard(
        &self,
        filter: UsageDashboardFilter,
    ) -> AppResult<UsageDashboardDto> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        Ok(query_usage_dashboard_sqlx(pool, tenant_id, &filter).await?)
    }

    pub async fn get_conversation_usage_scan_status(&self) -> AppResult<UsageScanStatusDto> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let states = load_all_usage_source_states_sqlx(pool, tenant_id).await?;

        let mut last_scanned_at: Option<String> = None;
        let mut source_diagnostics = Vec::new();
        let scanned_sources_count = states.len();

        for st in states {
            if let Some(ref scan_time) = st.last_success_scan_at {
                if last_scanned_at.as_ref().is_none_or(|curr| scan_time > curr) {
                    last_scanned_at = Some(scan_time.clone());
                }
            }

            let timestamp = st
                .last_success_scan_at
                .clone()
                .unwrap_or_else(|| st.updated_at.clone());
            if let Some(ref diag_json) = st.diagnostics_json {
                let msgs = serde_json::from_str::<Vec<String>>(diag_json)
                    .unwrap_or_else(|_| vec![diag_json.clone()]);
                let level = if st.status == "error" {
                    "error"
                } else {
                    "info"
                }
                .to_string();
                for msg in msgs {
                    source_diagnostics.push(UsageSourceDiagnosticDto {
                        source_id: st.source_id.clone(),
                        adapter_id: st.adapter_id.clone(),
                        level: level.clone(),
                        message: msg,
                        timestamp: timestamp.clone(),
                    });
                }
            }
        }

        let total_events_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM conversation_usage_events WHERE tenant_id = ?",
        )
        .bind(tenant_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0) as usize;

        Ok(UsageScanStatusDto {
            scanned_sources_count,
            total_events_count,
            last_scanned_at,
            active_scan_task_id: None,
            source_diagnostics,
        })
    }

    pub async fn scan_conversation_usage_with_control<F>(
        &self,
        options: UsageScanOptions,
        cancellation: Option<&tokio_util::sync::CancellationToken>,
        task_id: Option<&str>,
        on_progress: &mut F,
    ) -> AppResult<Value>
    where
        F: FnMut(usize, usize, Option<String>, usize) + Send,
    {
        if let Some(token) = cancellation {
            if token.is_cancelled() {
                return Err(AppError::Cancelled("usage scan cancelled".to_string()));
            }
        }

        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let settings = self.app_settings_value();
        let mode = options.mode.unwrap_or_else(|| "incremental".to_string());
        let is_full_mode = mode == "full";

        let task_runtime = self.runtime.task_runtime();
        if let Some(tid) = task_id {
            let stages = vec![
                make_stage("prepare_sources", "检索匹配会话数据源"),
                make_stage("collect_usage", "采集与解码用量记录"),
                make_stage("persist_events", "入库用量事件与游标"),
                make_stage("resolve_pricing", "加载模型价格与计费规则"),
                make_stage("calculate_costs", "核算会话用量费用"),
            ];
            let _ = task_runtime.set_stages(tid, stages);
        }

        let task_ctx = task_id.and_then(|tid| task_runtime.task_context(tid).ok());

        // Stage 1: 发现与匹配数据源
        let (target_sources, adapter_map) = {
            let stage_guard = task_ctx
                .as_ref()
                .map(|ctx| ctx.enter_stage("prepare_sources"));
            if let Some(ref guard) = stage_guard {
                guard.record_step(
                    "find_sources",
                    Some("正在扫描启用的对话数据源与适配器".to_string()),
                    None,
                    None,
                );
            }

            let all_sources = super::conversation_sources::list_sources(pool, tenant_id)
                .await
                .map_err(|e| {
                    AppError::Storage(format!("failed to list conversation sources: {e}"))
                })?;

            let matching_sources: Vec<ConversationSource> = all_sources
                .into_iter()
                .filter(|s| s.enabled)
                .filter(|s| options.source_id.as_deref().is_none_or(|id| id == s.id))
                .filter(|s| {
                    options
                        .adapter_id
                        .as_deref()
                        .is_none_or(|id| id == s.adapter_id)
                })
                .collect();

            let all_adapters = super::conversation_storage::list_adapters(pool, tenant_id)
                .await
                .map_err(|e| {
                    AppError::Storage(format!("failed to list conversation adapters: {e}"))
                })?;

            let adapter_map: std::collections::HashMap<String, ConversationAdapter> = all_adapters
                .into_iter()
                .map(|a| (a.id.clone(), a))
                .collect();

            let target_sources: Vec<ConversationSource> = matching_sources
                .into_iter()
                .filter(|s| {
                    if let Some(adapter) = adapter_map.get(&s.adapter_id) {
                        adapter_supports_usage(adapter)
                    } else {
                        false
                    }
                })
                .collect();

            if let Some(ref guard) = stage_guard {
                guard.record_step(
                    "match_complete",
                    Some(format!(
                        "共发现 {} 个支持用量统计的就绪数据源",
                        target_sources.len()
                    )),
                    None,
                    None,
                );
            }

            (target_sources, adapter_map)
        };

        let total_sources = target_sources.len();
        let observation_run_id = Uuid::new_v4().to_string();
        let mut total_events_ingested = 0usize;
        let mut completed_source_count = 0usize;

        // Stage 2 & Stage 3: 采集解码与入库
        for source in &target_sources {
            if let Some(token) = cancellation {
                if token.is_cancelled() {
                    return Err(AppError::Cancelled("usage scan cancelled".to_string()));
                }
            }

            on_progress(
                completed_source_count,
                total_sources,
                Some(source.name.clone()),
                total_events_ingested,
            );

            let adapter = match adapter_map.get(&source.adapter_id) {
                Some(a) => a,
                None => continue,
            };

            let previous_state = load_usage_source_state_sqlx(pool, tenant_id, &source.id).await?;
            let cursor = if is_full_mode {
                None
            } else {
                previous_state
                    .as_ref()
                    .and_then(|s| s.opaque_cursor.as_deref())
            };

            let scan_result = {
                let stage_guard = task_ctx
                    .as_ref()
                    .map(|ctx| ctx.enter_stage("collect_usage"));
                if let Some(ref guard) = stage_guard {
                    guard.record_step(
                        "read_external",
                        Some(format!("正在读取并解码数据源【{}】的用量事件", source.name)),
                        None,
                        None,
                    );
                }
                read_external_adapter_usage_with_settings(adapter, source, cursor, &settings).await
            };

            let now = Utc::now().to_rfc3339();
            match scan_result {
                Ok(run_result) => {
                    let stage_guard = task_ctx
                        .as_ref()
                        .map(|ctx| ctx.enter_stage("persist_events"));
                    let events_count = run_result.usage_events.len();
                    if events_count > 0 {
                        let inserted = upsert_usage_events_sqlx(
                            pool,
                            tenant_id,
                            &source.id,
                            &source.adapter_id,
                            &run_result.usage_events,
                            Some(&observation_run_id),
                        )
                        .await?;
                        total_events_ingested += inserted;

                        if let Some(ref guard) = stage_guard {
                            guard.record_step(
                                "persist_events",
                                Some(format!(
                                    "数据源【{}】写入 {} 条用量记录",
                                    source.name, inserted
                                )),
                                None,
                                None,
                            );
                        }
                    }

                    if is_full_mode && run_result.snapshot_complete {
                        let _ = prune_unseen_usage_events_sqlx(
                            pool,
                            tenant_id,
                            &source.id,
                            &observation_run_id,
                        )
                        .await;
                    }

                    let diag_json = if !run_result.diagnostics.is_empty() {
                        Some(serde_json::to_string(&run_result.diagnostics).unwrap_or_default())
                    } else {
                        None
                    };

                    let new_state = build_usage_source_state(
                        tenant_id,
                        source,
                        previous_state.as_ref(),
                        "active",
                        run_result.decoder_profile,
                        run_result.next_cursor,
                        is_full_mode,
                        diag_json,
                        &now,
                    );
                    save_usage_source_state_sqlx(pool, &new_state).await?;
                }
                Err(err) => {
                    tracing::error!(
                        action = "conversation.usage.scan",
                        source_id = %source.id,
                        error = %err,
                        "数据源用量扫描失败"
                    );

                    let failed_state = build_usage_source_state(
                        tenant_id,
                        source,
                        previous_state.as_ref(),
                        "error",
                        None,
                        None,
                        is_full_mode,
                        Some(serde_json::to_string(&vec![err.to_string()]).unwrap_or_default()),
                        &now,
                    );
                    let _ = save_usage_source_state_sqlx(pool, &failed_state).await;
                }
            }

            completed_source_count += 1;
            on_progress(
                completed_source_count,
                total_sources,
                Some(source.name.clone()),
                total_events_ingested,
            );
        }

        // Stage 4: 加载模型价格与计费规则 (100% 优先本地缓存/内置，零网络开销)
        let catalog = {
            let stage_guard = task_ctx
                .as_ref()
                .map(|ctx| ctx.enter_stage("resolve_pricing"));
            if let Some(ref guard) = stage_guard {
                guard.record_step(
                    "load_pricing",
                    Some("正在基于本地缓存与内置规则匹配模型价格".to_string()),
                    None,
                    None,
                );
            }

            let distinct_models = get_distinct_usage_models_sqlx(pool, tenant_id)
                .await
                .unwrap_or_default();

            let rate_str = get_pricing_metadata_sqlx(pool, tenant_id, "usd_to_cny_rate")
                .await
                .ok()
                .flatten();
            let rate = rate_str.and_then(|s| s.parse::<f64>().ok()).unwrap_or(7.25);

            let cached_models = load_pricing_catalog_entries_sqlx(pool, tenant_id)
                .await
                .unwrap_or_default();

            let catalog = PriceCatalog::new(
                "local".to_string(),
                Utc::now().to_rfc3339(),
                rate,
                cached_models,
            );

            if let Some(ref guard) = stage_guard {
                guard.record_step(
                    "pricing_ready",
                    Some(format!(
                        "计费规则就绪: 参考汇率 1 USD = ¥{:.2}，涵盖 {} 个已用模型",
                        rate,
                        distinct_models.len()
                    )),
                    None,
                    None,
                );
            }

            catalog
        };

        // Stage 5: 核算会话用量费用 (即刻出账)
        let recalculated_events_count = {
            let stage_guard = task_ctx
                .as_ref()
                .map(|ctx| ctx.enter_stage("calculate_costs"));
            if let Some(ref guard) = stage_guard {
                guard.record_step(
                    "calculate_costs",
                    Some("正在根据模型单价与汇率自动核算历史与新增用量费用".to_string()),
                    None,
                    None,
                );
            }

            let recalculated = recalculate_usage_costs_sqlx(pool, tenant_id, &catalog).await?;

            if let Some(ref guard) = stage_guard {
                guard.record_step(
                    "costs_done",
                    Some(format!("费用核算完毕，已更新 {} 条记录费用", recalculated)),
                    None,
                    None,
                );
            }

            recalculated
        };

        Ok(json!({
            "total_sources": total_sources,
            "completed_sources": completed_source_count,
            "total_events_ingested": total_events_ingested,
            "recalculated_events_count": recalculated_events_count,
            "mode": mode,
        }))
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

fn build_usage_source_state(
    tenant_id: &str,
    source: &ConversationSource,
    previous: Option<&UsageSourceState>,
    status: &str,
    decoder_profile: Option<String>,
    next_cursor: Option<String>,
    is_full_mode: bool,
    diagnostics: Option<String>,
    now: &str,
) -> UsageSourceState {
    UsageSourceState {
        tenant_id: tenant_id.to_string(),
        source_id: source.id.clone(),
        adapter_id: source.adapter_id.clone(),
        decoder_profile: decoder_profile
            .or_else(|| previous.and_then(|s| s.decoder_profile.clone())),
        schema_fingerprint: None,
        opaque_cursor: next_cursor.or_else(|| {
            if is_full_mode {
                None
            } else {
                previous.and_then(|s| s.opaque_cursor.clone())
            }
        }),
        last_success_scan_at: if status == "active" {
            Some(now.to_string())
        } else {
            previous.and_then(|s| s.last_success_scan_at.clone())
        },
        last_full_scan_at: if status == "active" && is_full_mode {
            Some(now.to_string())
        } else {
            previous.and_then(|s| s.last_full_scan_at.clone())
        },
        status: status.to_string(),
        diagnostics_json: diagnostics,
        created_at: previous
            .map(|s| s.created_at.clone())
            .unwrap_or_else(|| now.to_string()),
        updated_at: now.to_string(),
    }
}
