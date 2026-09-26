use super::conversation_adapters::conversation_storage_error;
use super::conversation_sync_support::*;
use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::infrastructure::tasks::{StageStatus, TaskActivity, TaskOutcome, TaskStage};
use serde_json::{json, Value};

impl AppService {
    pub(crate) async fn sync_conversations_with_control<F>(
        &self,
        params: ConversationSyncParams,
        cancellation: Option<&tokio_util::sync::CancellationToken>,
        task_id: Option<&str>,
        on_progress: &mut F,
    ) -> AppResult<Value>
    where
        F: FnMut(usize, usize, Option<String>) + Send,
    {
        ensure_conversation_sync_not_cancelled(cancellation)?;
        let record_kind = normalize_sync_record_kind(params.record_kind.as_deref())?;
        let settings = self.app_settings_value();
        let adapters = super::conversation_storage::list_adapters(self.db.pool(), self.tenant_id())
            .await
            .map_err(conversation_storage_error)?;
        let adapter_map: std::collections::BTreeMap<String, ConversationAdapter> = adapters
            .into_iter()
            .map(|adapter| (adapter.id.clone(), adapter))
            .collect();
        let sources = super::conversation_sources::list_sources(self.db.pool(), self.tenant_id())
            .await
            .map_err(conversation_storage_error)?
            .into_iter()
            .filter(|source| params.source_id.as_deref().is_none_or(|id| id == source.id))
            .filter(|source| {
                params
                    .adapter_id
                    .as_deref()
                    .is_none_or(|id| id == source.adapter_id)
            })
            .filter(|source| source.enabled)
            .filter(|source| {
                sync_source_matches_record_kind(
                    adapter_map.get(&source.adapter_id),
                    &source.adapter_id,
                    record_kind,
                )
            })
            .collect::<Vec<_>>();
        if sources.is_empty() {
            return Err(AppError::NotFound(
                "no matching conversation sources".to_string(),
            ));
        }

        let mut adapter_groups: std::collections::BTreeMap<String, Vec<ConversationSource>> =
            std::collections::BTreeMap::new();
        for source in sources {
            adapter_groups
                .entry(source.adapter_id.clone())
                .or_default()
                .push(source);
        }

        let task_runtime = self.runtime.task_runtime();
        if let Some(task_id) = task_id {
            let is_single_adapter = adapter_groups.len() == 1;
            let mut stages = Vec::new();
            for adapter_id in adapter_groups.keys() {
                let prefix = if is_single_adapter {
                    String::new()
                } else {
                    format!("[{adapter_id}] ")
                };
                stages.push(TaskStage {
                    id: format!("adapter:{adapter_id}:scan"),
                    name: format!("{prefix}扫描待更新会话"),
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
                });
                stages.push(TaskStage {
                    id: format!("adapter:{adapter_id}:sync"),
                    name: format!("{prefix}解析与同步会话记录"),
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
                });
                stages.push(TaskStage {
                    id: format!("adapter:{adapter_id}:persist"),
                    name: format!("{prefix}数据入库与索引"),
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
                });
            }
            let _ = task_runtime.set_stages(task_id, stages);
        }

        let total_source_count = adapter_groups.values().map(|g| g.len()).sum::<usize>();
        let completed_source_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let on_progress_mutex = std::sync::Arc::new(tokio::sync::Mutex::new(on_progress));
        {
            let mut lock = on_progress_mutex.lock().await;
            (**lock)(0, total_source_count, None);
        }

        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
        let mut group_futures = Vec::new();

        for (adapter_id, group_sources) in adapter_groups {
            let semaphore = semaphore.clone();
            let completed_source_count = completed_source_count.clone();
            let on_progress_mutex = on_progress_mutex.clone();
            let params = &params;
            let settings = &settings;
            let cancellation = cancellation;
            let task_runtime = task_runtime.clone();

            group_futures.push(async move {
                let _permit = semaphore
                    .acquire()
                    .await
                    .map_err(|_| AppError::Cancelled("同步并发信号量已关闭".to_string()))?;

                ensure_conversation_sync_not_cancelled(cancellation)?;
                let scan_stage_id = format!("adapter:{adapter_id}:scan");
                let sync_stage_id = format!("adapter:{adapter_id}:sync");
                let persist_stage_id = format!("adapter:{adapter_id}:persist");

                let task_ctx = task_id.and_then(|tid| task_runtime.task_context(tid).ok());

                if let Some(ref ctx) = task_ctx {
                    let scan_guard = ctx.enter_stage(&scan_stage_id);
                    let source_names = group_sources.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ");
                    scan_guard.record_step(
                        "start_scan",
                        Some(format!("正在探测会话源: {source_names}")),
                        None,
                        None,
                    );
                }

                let mut group_results = Vec::new();
                let mut group_errors = Vec::new();

                for source in group_sources {
                    ensure_conversation_sync_not_cancelled(cancellation)?;
                    let completed = completed_source_count.load(std::sync::atomic::Ordering::Relaxed);
                    {
                        let mut lock = on_progress_mutex.lock().await;
                        (**lock)(completed, total_source_count, Some(source.name.clone()));
                    }

                    let worker_id = format!("{}:{}", adapter_id, source.id);
                    let progress_task_runtime = task_runtime.clone();
                    let task_id_string = task_id.map(|s| s.to_string());
                    let scan_stage_id_clone = scan_stage_id.clone();
                    let sync_stage_id_clone = sync_stage_id.clone();
                    let default_worker = worker_id.clone();

                    let progress_listener: crate::backend::infrastructure::conversations::ExternalAdapterProgressListener =
                        std::sync::Arc::new(move |progress| {
                            if let Some(ref tid) = task_id_string {
                                let is_scan = progress.stage.as_deref() == Some("scan");
                                let target_stage = if is_scan {
                                    &scan_stage_id_clone
                                } else {
                                    // 当出现 parse/sync 事件时，确保 scan_stage 已标记为 Succeeded，sync_stage 变为 Running
                                    let _ = progress_task_runtime.update_stage_status(tid, &scan_stage_id_clone, StageStatus::Succeeded);
                                    let _ = progress_task_runtime.update_stage_status(tid, &sync_stage_id_clone, StageStatus::Running);
                                    &sync_stage_id_clone
                                };

                                let worker_name = progress.worker.clone().unwrap_or_else(|| default_worker.clone());
                                let op = progress.operation.clone().unwrap_or_else(|| "processing".to_string());

                                // 1. 内存中持久维护活跃 worker，不随每次回调丢弃，杜绝闪烁
                                let activity = TaskActivity {
                                    stage_id: target_stage.clone(),
                                    worker_id: worker_name,
                                    operation: op.clone(),
                                    path: None,
                                    display_path: progress.path.clone(),
                                    started_at: chrono::Utc::now().to_rfc3339(),
                                    current: progress.current,
                                    total: progress.total,
                                };
                                let _ = progress_task_runtime.record_activity_silent(tid, activity);

                                // 2. 沉淀为常驻历史步骤
                                let _ = progress_task_runtime.record_stage_step(
                                    tid,
                                    target_stage,
                                    op,
                                    progress.path.clone(),
                                    progress.current,
                                    progress.total,
                                );

                                // 3. 阶段数值与 note 更新
                                let note = match (&progress.path, &progress.operation) {
                                    (Some(p), Some(o)) => Some(format!("{o}: {p}")),
                                    (Some(p), None) => Some(p.clone()),
                                    (None, Some(o)) => Some(o.clone()),
                                    (None, None) => None,
                                };
                                if let (Some(curr), total_opt) = (progress.current, progress.total) {
                                    let _ = progress_task_runtime.set_stage_progress(
                                        tid,
                                        target_stage,
                                        curr,
                                        total_opt,
                                        note,
                                    );
                                } else if let Some(note_str) = note {
                                    let _ = progress_task_runtime.set_stage_progress(
                                        tid,
                                        target_stage,
                                        0,
                                        None,
                                        Some(note_str),
                                    );
                                }
                            }
                        });

                    let on_progress_mutex_detail = on_progress_mutex.clone();
                    let completed_detail = completed_source_count.clone();
                    let mut on_detail = move |msg: String| {
                        let completed = completed_detail.load(std::sync::atomic::Ordering::Relaxed);
                        if let Ok(mut lock) = on_progress_mutex_detail.try_lock() {
                            (**lock)(completed, total_source_count, Some(msg));
                        }
                    };

                    let sync_result = self
                        .sync_single_conversation_source(
                            &source,
                            params,
                            record_kind,
                            settings,
                            cancellation,
                            Some(progress_listener),
                            &mut on_detail,
                        )
                        .await;

                    // source 完成后平滑清理 worker activity
                    if let Some(ref tid) = task_id {
                        let _ = task_runtime.remove_activity(tid, &sync_stage_id, &worker_id);
                        let _ = task_runtime.remove_activity(tid, &scan_stage_id, &worker_id);
                    }

                    let completed =
                        completed_source_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    {
                        let mut lock = on_progress_mutex.lock().await;
                        (**lock)(completed, total_source_count, None);
                    }

                    match sync_result {
                        Ok(Some(result)) => {
                            if !params.dry_run {
                                self.runtime.notify_domain_events();
                            }
                            group_results.push(result);
                        }
                        Ok(None) => {}
                        Err(error) => {
                            if let Some(ref tid) = task_id {
                                let _ = task_runtime.update_stage_status(tid, &sync_stage_id, StageStatus::Failed);
                            }
                            if params.source_id.is_some() {
                                return Err(error);
                            }
                            group_errors.push(serde_json::json!({
                                "source_id": source.id,
                                "adapter_id": source.adapter_id,
                                "message": error.to_string()
                            }));
                        }
                    }
                }

                // 所有 source 处理完成后，将 scan 与 sync 阶段平滑标记为完成，并推进 persist 阶段
                if let Some(ref tid) = task_id {
                    let _ = task_runtime.update_stage_status(tid, &scan_stage_id, StageStatus::Succeeded);
                    let _ = task_runtime.update_stage_status(tid, &sync_stage_id, StageStatus::Succeeded);

                    // 推进入库与索引阶段
                    let _ = task_runtime.update_stage_status(tid, &persist_stage_id, StageStatus::Running);
                    let _ = task_runtime.record_stage_step(
                        tid,
                        &persist_stage_id,
                        "persist_completed",
                        Some(format!("共成功处理 {} 个数据源会话，索引已落库", group_results.len())),
                        Some(group_results.len() as u64),
                        Some(group_results.len() as u64),
                    );
                    let _ = task_runtime.set_stage_progress(
                        tid,
                        &persist_stage_id,
                        1,
                        Some(1),
                        Some("会话记录已安全入库，搜索索引更新完毕".to_string()),
                    );
                    let _ = task_runtime.update_stage_status(tid, &persist_stage_id, StageStatus::Succeeded);
                }

                Ok((group_results, group_errors))
            });
        }

        let group_outcomes = futures::future::join_all(group_futures).await;
        let mut results = Vec::new();
        let mut errors = Vec::new();

        for outcome in group_outcomes {
            match outcome {
                Ok((group_results, group_errors)) => {
                    results.extend(group_results);
                    errors.extend(group_errors);
                }
                Err(error) => {
                    return Err(error);
                }
            }
        }

        if results.is_empty() && errors.is_empty() {
            return Err(AppError::NotFound(
                "no matching conversation sources".to_string(),
            ));
        }

        if let Some(task_id) = task_id {
            if !results.is_empty() && errors.is_empty() {
                let _ = task_runtime.set_outcome(
                    task_id,
                    TaskOutcome::Success,
                    Some(format!("成功同步 {} 个数据源", results.len())),
                    None,
                );
            } else if !results.is_empty() && !errors.is_empty() {
                let _ = task_runtime.set_outcome(
                    task_id,
                    TaskOutcome::PartialSuccess,
                    Some(format!(
                        "部分成功：{} 成功，{} 失败",
                        results.len(),
                        errors.len()
                    )),
                    Some(format!("{} 个数据源同步发生异常", errors.len())),
                );
            } else if results.is_empty() && !errors.is_empty() {
                let _ = task_runtime.set_outcome(
                    task_id,
                    TaskOutcome::Failure,
                    None,
                    Some(format!("所有数据源同步均失败（共 {} 个）", errors.len())),
                );
            }
        }

        if !params.dry_run
            && params.source_id.is_none()
            && params.adapter_id.is_none()
            && record_kind.is_none()
            && errors.is_empty()
        {
            crate::backend::store::mark_conversation_payload_policy_applied_sqlx(
                self.db.pool(),
                self.tenant_id(),
                crate::backend::infrastructure::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
            )
            .await
            .map_err(conversation_storage_error)?;
        }
        let legacy_cards_upgraded = results
            .iter()
            .filter_map(|result| result.get("legacy_cards_upgraded").and_then(Value::as_u64))
            .sum::<u64>();
        tracing::info!(
            action = "conversation.sync",
            legacy_cards_upgraded = %legacy_cards_upgraded,
            source_count = %results.len(),
            error_count = %errors.len(),
            dry_run = %params.dry_run,
            mode = %format!("{:?}", params.mode).to_ascii_lowercase(),
            "Conversation sync completed"
        );
        Ok(json!({
            "dry_run": params.dry_run,
            "mode": params.mode,
            "record_kind": record_kind.map(|kind| match kind {
                crate::backend::domain::conversations::ConversationRecordKind::Session => "session",
                crate::backend::domain::conversations::ConversationRecordKind::Web => "web",
            }),
            "results": results,
            "errors": errors,
            "legacy_cards_upgraded": legacy_cards_upgraded
        }))
    }
}
