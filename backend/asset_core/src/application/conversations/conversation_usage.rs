use super::usage_dto::{
    UsageDashboardDto, UsageDashboardFilter, UsageScanOptions, UsageScanStatusDto,
};
use crate::backend::application::prelude::*;
use crate::backend::infrastructure::tasks::{
    stages, InfraError, PipelineDescriptor, StageStatus, TaskCapabilities, TaskFailure, TaskFilter,
    TaskHandle, TaskKind, TaskOutcome, TaskOutput, TaskSkippedGroup, TaskSpec,
};
use crate::backend::{
    domain::{conversations::pricing::PriceCatalog, ConversationAdapter, ConversationSource},
    infrastructure::conversations::{
        adapter_supports_usage, read_external_adapter_usage_with_settings,
    },
    store::conversations::{
        count_usage_events_for_tenant_sqlx, get_pricing_metadata_sqlx,
        load_all_usage_source_states_sqlx, load_pricing_catalog_entries_sqlx,
        load_usage_source_state_sqlx, prune_unseen_usage_events_sqlx,
        query_usage_dashboard_sqlx, recalculate_usage_costs_sqlx, save_usage_source_state_sqlx,
        summarize_usage_source_states, upsert_usage_events_sqlx,
        usage_repo::get_distinct_usage_models_sqlx, UsageSourceState,
    },
};
use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

impl AppService {
    /// 用量统计扫描流水线骨架（阶段 id 全部来自 `stages::*` 编译期常量）。
    fn usage_scan_pipeline() -> PipelineDescriptor {
        PipelineDescriptor::builder()
            .stage(stages::PREPARE_SOURCES, "检索匹配会话数据源")
            .stage(stages::COLLECT_USAGE, "采集与解码用量记录")
            .stage(stages::PERSIST_EVENTS, "入库用量事件与游标")
            .stage(stages::RESOLVE_PRICING, "加载模型价格与计费规则")
            .stage(stages::CALCULATE_COSTS, "核算会话用量费用")
            .build()
    }

    /// 启动任务中心全量托管的用量统计扫描任务。
    pub fn start_conversation_usage_scan(
        &self,
        options: UsageScanOptions,
    ) -> AppResult<TaskHandle<Value>> {
        let tenant_id = self.tenant_id().to_string();
        let source_key = options
            .source_id
            .clone()
            .unwrap_or_else(|| "all".to_string());
        let spec = TaskSpec::new(
            TaskKind::ConversationUsageScan,
            Some(format!("{tenant_id}:conversation_usage_scan:{source_key}")),
        )
        .with_tenant_id(tenant_id.clone())
        .with_title("用量统计扫描")
        .with_capabilities(TaskCapabilities {
            cancellable: true,
            retryable: false,
            clearable: true,
        })
        .with_conflict_key(format!("{tenant_id}:conversation_usage_scan"))
        .with_pipeline(Self::usage_scan_pipeline());

        let service = self.clone();
        self.spawn_pipeline_task(spec, move |cx| async move {
            let cancellation = cx.cancellation();
            let task_id = cx.task_id().to_string();
            let mut noop_progress =
                |_completed: usize, _total: usize, _name: Option<String>, _events: usize| {};
            let result = service
                .scan_conversation_usage_with_control(
                    options,
                    Some(&cancellation),
                    Some(&task_id),
                    &mut noop_progress,
                )
                .await
                .map_err(InfraError::external)?;
            let summary = format!(
                "用量扫描完成：共 {} 个来源（{} 个失败），写入 {} 条用量事件（模式: {}）",
                result["total_sources"],
                result["failed_sources"],
                result["total_events_ingested"],
                result["mode"]
            );
            Ok(TaskOutput::with_summary(result, summary))
        })
    }

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
        let scanned_sources_count = states.len();
        let (last_scanned_at, source_diagnostics) =
            summarize_usage_source_states(&states);
        let total_events_count = count_usage_events_for_tenant_sqlx(pool, tenant_id).await?;

        Ok(UsageScanStatusDto {
            scanned_sources_count,
            total_events_count,
            last_scanned_at,
            active_scan_task_id: self.active_usage_scan_task_id(tenant_id),
            source_diagnostics,
        })
    }

    /// 当前租户正在运行的用量扫描任务 id（供前端恢复运行态）。
    fn active_usage_scan_task_id(&self, tenant_id: &str) -> Option<String> {
        self.runtime
            .task_runtime()
            .list_for_tenant(
                tenant_id,
                TaskFilter {
                    kind: Some(TaskKind::ConversationUsageScan),
                    active_only: true,
                    ..Default::default()
                },
            )
            .into_iter()
            .next()
            .map(|snapshot| snapshot.task_id)
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
            // 托管流水线在提交时已前置声明阶段骨架，此处仅补齐外部注册路径的阶段名称。
            let already_declared = task_runtime
                .get(tid)
                .is_some_and(|snapshot| !snapshot.stages.is_empty());
            if !already_declared {
                let _ =
                    task_runtime.set_stages(tid, Self::usage_scan_pipeline().to_initial_stages());
            }
        }

        let task_ctx = task_id.and_then(|tid| task_runtime.task_context(tid).ok());

        // Stage 1: 发现与匹配数据源
        let (target_sources, adapter_map) = {
            let stage_guard = task_ctx
                .as_ref()
                .map(|ctx| ctx.enter_stage(stages::PREPARE_SOURCES));
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
        let mut successful_source_count = 0usize;
        let mut source_failures: Vec<TaskFailure> = Vec::new();

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
                    .map(|ctx| ctx.enter_stage(stages::COLLECT_USAGE));
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
                        .map(|ctx| ctx.enter_stage(stages::PERSIST_EVENTS));
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

                    if (is_full_mode || cursor.is_none()) && run_result.snapshot_complete {
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
                    successful_source_count += 1;
                }
                Err(err) => {
                    tracing::error!(
                        action = "conversation.usage.scan",
                        source_id = %source.id,
                        error = %err,
                        "数据源用量扫描失败"
                    );

                    source_failures.push(TaskFailure {
                        code: "usage_source_read_failed".to_string(),
                        message: format!("数据源【{}】读取用量失败：{err}", source.name),
                        stage: stages::COLLECT_USAGE.to_string(),
                        identity: Some(source.id.clone()),
                        retryable: true,
                        path: None,
                        timestamp: now.clone(),
                    });
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

        let failed_source_count = source_failures.len();

        // 单源读取失败必须成为当前阶段的可见故障，而不是被静默吞掉。
        if let Some(tid) = task_id {
            if failed_source_count > 0 {
                let collect_status = if successful_source_count > 0 {
                    StageStatus::PartialSuccess
                } else {
                    StageStatus::Failed
                };
                let _ = task_runtime.finish_stage(
                    tid,
                    stages::COLLECT_USAGE,
                    collect_status,
                    Vec::new(),
                    source_failures.clone(),
                    Vec::new(),
                );
            }

            if total_sources > 0 && successful_source_count == 0 {
                // 没有任何来源可入库时，不去假装“持久化”阶段仍在运行。
                let _ = task_runtime.finish_stage(
                    tid,
                    stages::PERSIST_EVENTS,
                    StageStatus::Skipped,
                    Vec::new(),
                    Vec::new(),
                    vec![TaskSkippedGroup {
                        reason_code: "no_successful_sources".to_string(),
                        count: total_sources as u64,
                        samples: source_failures
                            .iter()
                            .filter_map(|failure| failure.identity.clone())
                            .take(5)
                            .collect(),
                    }],
                );
            }
        }

        // Stage 4: 加载模型价格与计费规则 (100% 优先本地缓存/内置，零网络开销)
        let catalog = {
            let stage_guard = task_ctx
                .as_ref()
                .map(|ctx| ctx.enter_stage(stages::RESOLVE_PRICING));
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
                .map(|ctx| ctx.enter_stage(stages::CALCULATE_COSTS));
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

        if let Some(ctx) = task_ctx.as_ref() {
            if failed_source_count == 0 {
                ctx.set_outcome(
                    TaskOutcome::Success,
                    Some(format!(
                        "成功扫描 {successful_source_count} 个数据源，写入 {total_events_ingested} 条用量事件"
                    )),
                    None,
                );
            } else if successful_source_count > 0 {
                ctx.set_outcome(
                    TaskOutcome::PartialSuccess,
                    Some(format!(
                        "扫描完成 {successful_source_count} 个数据源，写入 {total_events_ingested} 条用量事件；{failed_source_count} 个数据源读取失败"
                    )),
                    Some(failure_summary(&source_failures)),
                );
            } else {
                ctx.set_outcome(
                    TaskOutcome::Failure,
                    None,
                    Some(failure_summary(&source_failures)),
                );
            }
        }

        if total_sources > 0 && successful_source_count == 0 && failed_source_count > 0 {
            return Err(AppError::External(format!(
                "所有数据源用量扫描均失败（{failed_source_count} 个）"
            )));
        }

        Ok(json!({
            "total_sources": total_sources,
            "completed_sources": completed_source_count,
            "successful_sources": successful_source_count,
            "failed_sources": failed_source_count,
            "total_events_ingested": total_events_ingested,
            "recalculated_events_count": recalculated_events_count,
            "mode": mode,
        }))
    }
}

fn failure_summary(failures: &[TaskFailure]) -> String {
    let names: Vec<&str> = failures
        .iter()
        .map(|failure| failure.identity.as_deref().unwrap_or("unknown-source"))
        .collect();
    format!("{} 个数据源读取失败：{}", failures.len(), names.join("、"))
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

#[cfg(test)]
#[path = "conversation_usage_tests.rs"]
mod tests;
