use super::prelude::*;
use crate::backend::{
    conversations::{
        adapter_supports_usage, read_external_adapter_usage_with_settings,
        usage_repo::{
            load_all_usage_source_states_sqlx, load_usage_source_state_sqlx,
            prune_unseen_usage_events_sqlx, query_usage_dashboard_sqlx,
            save_usage_source_state_sqlx, upsert_usage_events_sqlx, UsageSourceState,
        },
    },
    dto::{
        UsageDashboardDto, UsageDashboardFilter, UsageScanOptions, UsageScanStatusDto,
        UsageSourceDiagnosticDto,
    },
    models::{ConversationAdapter, ConversationSource},
    runtime::{AppError, AppResult},
};
use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

impl AppService {
    pub(crate) async fn get_conversation_usage_dashboard(
        &self,
        filter: UsageDashboardFilter,
    ) -> AppResult<UsageDashboardDto> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        query_usage_dashboard_sqlx(pool, tenant_id, &filter).await
    }

    pub(crate) async fn get_conversation_usage_scan_status(&self) -> AppResult<UsageScanStatusDto> {
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
                if let Ok(parsed) = serde_json::from_str::<Vec<String>>(diag_json) {
                    for msg in parsed {
                        source_diagnostics.push(UsageSourceDiagnosticDto {
                            source_id: st.source_id.clone(),
                            adapter_id: st.adapter_id.clone(),
                            level: if st.status == "error" {
                                "error".to_string()
                            } else {
                                "info".to_string()
                            },
                            message: msg,
                            timestamp: timestamp.clone(),
                        });
                    }
                } else {
                    source_diagnostics.push(UsageSourceDiagnosticDto {
                        source_id: st.source_id.clone(),
                        adapter_id: st.adapter_id.clone(),
                        level: if st.status == "error" {
                            "error".to_string()
                        } else {
                            "info".to_string()
                        },
                        message: diag_json.clone(),
                        timestamp,
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

    pub(crate) async fn scan_conversation_usage_with_control<F>(
        &self,
        options: UsageScanOptions,
        cancellation: Option<&tokio_util::sync::CancellationToken>,
        _task_id: Option<&str>,
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

        // 1. Fetch enabled sources
        let all_sources = crate::backend::store::list_conversation_sources_sqlx(pool, tenant_id)
            .await
            .map_err(|e| AppError::Storage(format!("failed to list conversation sources: {e}")))?;

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

        // 2. Fetch all adapters
        let all_adapters = crate::backend::store::list_conversation_adapters_sqlx(pool, tenant_id)
            .await
            .map_err(|e| AppError::Storage(format!("failed to list conversation adapters: {e}")))?;

        let adapter_map: std::collections::HashMap<String, ConversationAdapter> = all_adapters
            .into_iter()
            .map(|a| (a.id.clone(), a))
            .collect();

        // 3. Filter sources whose adapter supports `read_usage`
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

        let total_sources = target_sources.len();
        let observation_run_id = Uuid::new_v4().to_string();
        let mut total_events_ingested = 0usize;
        let mut completed_source_count = 0usize;

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

            let scan_result =
                read_external_adapter_usage_with_settings(adapter, source, cursor, &settings).await;

            let now = Utc::now().to_rfc3339();
            match scan_result {
                Ok(run_result) => {
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
                    }

                    // If full mode was requested and completed successfully, prune stale events
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

                    let new_state = UsageSourceState {
                        tenant_id: tenant_id.to_string(),
                        source_id: source.id.clone(),
                        adapter_id: source.adapter_id.clone(),
                        decoder_profile: run_result.decoder_profile,
                        schema_fingerprint: None,
                        opaque_cursor: run_result.next_cursor.or_else(|| {
                            if is_full_mode {
                                None
                            } else {
                                previous_state
                                    .as_ref()
                                    .and_then(|s| s.opaque_cursor.clone())
                            }
                        }),
                        last_success_scan_at: Some(now.clone()),
                        last_full_scan_at: if is_full_mode {
                            Some(now.clone())
                        } else {
                            previous_state
                                .as_ref()
                                .and_then(|s| s.last_full_scan_at.clone())
                        },
                        status: "active".to_string(),
                        diagnostics_json: diag_json,
                        created_at: previous_state
                            .as_ref()
                            .map(|s| s.created_at.clone())
                            .unwrap_or_else(|| now.clone()),
                        updated_at: now,
                    };

                    save_usage_source_state_sqlx(pool, &new_state).await?;
                }
                Err(err) => {
                    tracing::error!(
                        action = "conversation.usage.scan",
                        source_id = %source.id,
                        error = %err,
                        "数据源用量扫描失败"
                    );

                    let failed_state = UsageSourceState {
                        tenant_id: tenant_id.to_string(),
                        source_id: source.id.clone(),
                        adapter_id: source.adapter_id.clone(),
                        decoder_profile: previous_state
                            .as_ref()
                            .and_then(|s| s.decoder_profile.clone()),
                        schema_fingerprint: None,
                        opaque_cursor: previous_state
                            .as_ref()
                            .and_then(|s| s.opaque_cursor.clone()),
                        last_success_scan_at: previous_state
                            .as_ref()
                            .and_then(|s| s.last_success_scan_at.clone()),
                        last_full_scan_at: previous_state
                            .as_ref()
                            .and_then(|s| s.last_full_scan_at.clone()),
                        status: "error".to_string(),
                        diagnostics_json: Some(
                            serde_json::to_string(&vec![err.to_string()]).unwrap_or_default(),
                        ),
                        created_at: previous_state
                            .as_ref()
                            .map(|s| s.created_at.clone())
                            .unwrap_or_else(|| now.clone()),
                        updated_at: now,
                    };

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

        Ok(json!({
            "total_sources": total_sources,
            "completed_sources": completed_source_count,
            "total_events_ingested": total_events_ingested,
            "mode": mode,
        }))
    }
}
