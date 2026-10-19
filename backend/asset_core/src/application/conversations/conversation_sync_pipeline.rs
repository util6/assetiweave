use super::conversation_adapters::conversation_storage_error;
use super::conversation_sync_support::*;
use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::infrastructure::tasks::{
    StageStatus, TaskActivity, TaskFailure, TaskOutcome, TaskStage,
};
use serde_json::{json, Value};

impl AppService {
    pub async fn sync_conversations_with_control<F>(
        &self,
        params: ConversationSyncParams,
        cancellation: Option<&tokio_util::sync::CancellationToken>,
        task_id: Option<&str>,
        on_progress: &mut F,
    ) -> AppResult<Value>
    where
        F: FnMut(usize, usize, Option<String>, &[String]) + Send,
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
        let completed_adapter_ids =
            std::sync::Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));
        let on_progress_mutex = std::sync::Arc::new(tokio::sync::Mutex::new(on_progress));
        {
            let mut lock = on_progress_mutex.lock().await;
            (**lock)(0, total_source_count, None, &[]);
        }

        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
        let mut group_futures = Vec::new();

        for (adapter_id, group_sources) in adapter_groups {
            let semaphore = semaphore.clone();
            let completed_source_count = completed_source_count.clone();
            let completed_adapter_ids = completed_adapter_ids.clone();
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
                    let current_completed = {
                        let lock = completed_adapter_ids.lock().await;
                        lock.clone()
                    };
                    {
                        let mut lock = on_progress_mutex.lock().await;
                        (**lock)(completed, total_source_count, Some(source.name.clone()), &current_completed);
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
                    let completed_adapter_ids_detail = completed_adapter_ids.clone();
                    let progress_task_runtime_detail = task_runtime.clone();
                    let task_id_detail = task_id.map(|s| s.to_string());
                    let sync_stage_id_detail = sync_stage_id.clone();
                    let scan_stage_id_detail = scan_stage_id.clone();
                    let worker_id_detail = worker_id.clone();
                    let mut on_detail = move |msg: String| {
                        let completed = completed_detail.load(std::sync::atomic::Ordering::Relaxed);
                        let current_completed = {
                            let lock = completed_adapter_ids_detail.try_lock();
                            lock.map(|g| g.clone()).unwrap_or_default()
                        };
                        if let Ok(mut lock) = on_progress_mutex_detail.try_lock() {
                            (**lock)(completed, total_source_count, Some(msg.clone()), &current_completed);
                        }
                        if let Some(ref tid) = task_id_detail {
                            if msg.contains(
                                crate::backend::application::conversations::conversation_sync_support::CONVERSATION_SYNC_PERSISTING_PHASE,
                            ) {
                                let _ = progress_task_runtime_detail.remove_activity(tid, &sync_stage_id_detail, &worker_id_detail);
                                let _ = progress_task_runtime_detail.remove_activity(tid, &scan_stage_id_detail, &worker_id_detail);
                                let _ = progress_task_runtime_detail.set_stage_progress(
                                    tid,
                                    &sync_stage_id_detail,
                                    0,
                                    None,
                                    Some(msg),
                                );
                            }
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
                    let current_completed = {
                        let lock = completed_adapter_ids.lock().await;
                        lock.clone()
                    };
                    {
                        let mut lock = on_progress_mutex.lock().await;
                        (**lock)(completed, total_source_count, None, &current_completed);
                    }

                    match sync_result {
                        Ok(Some(result)) => {
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

                let mut task_failures = Vec::new();
                for err in &group_errors {
                    let msg = err.get("message").and_then(Value::as_str).unwrap_or("未知数据源错误");
                    let src_id = err.get("source_id").and_then(Value::as_str);
                    task_failures.push(TaskFailure {
                        code: "SOURCE_SYNC_FAILED".to_string(),
                        message: msg.to_string(),
                        stage: sync_stage_id.clone(),
                        identity: src_id.map(ToString::to_string),
                        retryable: true,
                        path: None,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    });
                }
                let mut session_failure_count = 0;
                for result in &group_results {
                    if let Some(failures) = result.get("session_failures").and_then(Value::as_array) {
                        for fail in failures {
                            session_failure_count += 1;
                            let ext_id = fail
                                .get("session_external_id")
                                .or_else(|| fail.get("external_id"))
                                .and_then(Value::as_str);
                            let msg = fail
                                .get("error_message")
                                .or_else(|| fail.get("error"))
                                .and_then(Value::as_str)
                                .unwrap_or("会话导入失败");
                            let code = fail
                                .get("error_code")
                                .and_then(Value::as_str)
                                .unwrap_or("SESSION_IMPORT_FAILED");
                            let retryable = fail
                                .get("retryable")
                                .and_then(Value::as_bool)
                                .unwrap_or(true);
                            task_failures.push(TaskFailure {
                                code: code.to_string(),
                                message: msg.to_string(),
                                stage: sync_stage_id.clone(),
                                identity: ext_id.map(ToString::to_string),
                                retryable,
                                path: None,
                                timestamp: chrono::Utc::now().to_rfc3339(),
                            });
                        }
                    }
                }

                let total_failures = group_errors.len() + session_failure_count;
                let has_success = !group_results.is_empty();
                let sync_status = if total_failures == 0 {
                    StageStatus::Succeeded
                } else if has_success {
                    StageStatus::PartialSuccess
                } else {
                    StageStatus::Failed
                };

                // 所有 source 处理完成后，按真实结果推进各阶段状态与错误记录
                if let Some(ref tid) = task_id {
                    let _ = task_runtime.update_stage_status(tid, &scan_stage_id, StageStatus::Succeeded);
                    let _ = task_runtime.finish_stage(
                        tid,
                        &sync_stage_id,
                        sync_status,
                        Vec::new(),
                        task_failures.clone(),
                        Vec::new(),
                    );

                    if has_success {
                        let _ = task_runtime.update_stage_status(tid, &persist_stage_id, StageStatus::Running);
                        let _ = task_runtime.record_stage_step(
                            tid,
                            &persist_stage_id,
                            "persist_completed",
                            Some(format!("共成功处理 {} 个数据源会话，{} 个失败", group_results.len(), total_failures)),
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
                        let persist_status = if total_failures == 0 {
                            StageStatus::Succeeded
                        } else {
                            StageStatus::PartialSuccess
                        };
                        let _ = task_runtime.finish_stage(
                            tid,
                            &persist_stage_id,
                            persist_status,
                            Vec::new(),
                            task_failures,
                            Vec::new(),
                        );
                    } else {
                        let _ = task_runtime.finish_stage(
                            tid,
                            &persist_stage_id,
                            StageStatus::Skipped,
                            Vec::new(),
                            Vec::new(),
                            Vec::new(),
                        );
                    }
                }

                // 该应用（adapter）已完成落库，加入 completed_adapter_ids 并立即通知外部触发前端增量刷新！
                let current_completed = {
                    let mut lock = completed_adapter_ids.lock().await;
                    if !lock.contains(&adapter_id) {
                        lock.push(adapter_id.clone());
                    }
                    lock.clone()
                };
                let completed = completed_source_count.load(std::sync::atomic::Ordering::Relaxed);
                {
                    let mut lock = on_progress_mutex.lock().await;
                    (**lock)(completed, total_source_count, None, &current_completed);
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

        if !params.dry_run && !results.is_empty() {
            self.runtime.notify_domain_events();
        }

        if results.is_empty() && errors.is_empty() {
            return Err(AppError::NotFound(
                "no matching conversation sources".to_string(),
            ));
        }

        let total_session_failures = results
            .iter()
            .filter_map(|r| r.get("session_failures").and_then(Value::as_array))
            .map(|arr| arr.len())
            .sum::<usize>();
        let total_failure_count = errors.len() + total_session_failures;

        if let Some(task_id) = task_id {
            if !results.is_empty() && total_failure_count == 0 {
                let _ = task_runtime.set_outcome(
                    task_id,
                    TaskOutcome::Success,
                    Some(format!("成功同步 {} 个数据源", results.len())),
                    None,
                );
            } else if !results.is_empty() && total_failure_count > 0 {
                let mut notes = Vec::new();
                if !errors.is_empty() {
                    notes.push(format!("{} 个数据源异常", errors.len()));
                }
                if total_session_failures > 0 {
                    notes.push(format!("{} 个会话导入失败", total_session_failures));
                }
                let _ = task_runtime.set_outcome(
                    task_id,
                    TaskOutcome::PartialSuccess,
                    Some(format!(
                        "部分成功：{} 个数据源处理完成，{}",
                        results.len(),
                        notes.join("，")
                    )),
                    Some(format!("同步过程中出现异常：{}", notes.join("，"))),
                );
            } else if results.is_empty() && total_failure_count > 0 {
                let _ = task_runtime.set_outcome(
                    task_id,
                    TaskOutcome::Failure,
                    None,
                    Some(format!(
                        "所有数据源同步均失败（{} 个错误）",
                        total_failure_count
                    )),
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
