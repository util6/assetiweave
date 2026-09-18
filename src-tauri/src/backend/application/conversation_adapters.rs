use super::prelude::*;
use crate::backend::runtime::tasks::{
    StageStatus, TaskActivity, TaskFailure, TaskOutcome, TaskStage,
};
use crate::backend::runtime::{AppError, AppResult};

fn conversation_storage_error(error: AppError) -> AppError {
    error
}

fn conversation_external_error(error: impl std::fmt::Display) -> AppError {
    AppError::external(error)
}

impl AppService {
    pub(crate) async fn conversation_payload_policy_reparse_required(&self) -> AppResult<bool> {
        crate::backend::store::conversation_payload_policy_reparse_required_sqlx(
            self.db.pool(),
            self.tenant_id(),
            crate::backend::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
        )
        .await
        .map_err(conversation_storage_error)
    }

    pub(crate) fn list_conversation_adapters(&self) -> AppResult<Vec<ConversationAdapter>> {
        let current = self.runtime.context();
        let catalog = if current.tenant.id == self.tenant_id() {
            current.conversation_adapter_catalog.clone()
        } else {
            self.conversation_adapter_catalog.clone()
        };
        Ok(catalog.adapters.clone())
    }

    pub(crate) fn scaffold_conversation_adapter(
        &self,
        params: crate::backend::conversations::ExternalAdapterScaffoldParams,
    ) -> AppResult<crate::backend::conversations::ExternalAdapterScaffoldResult> {
        crate::backend::conversations::scaffold_external_adapter(params)
            .map_err(conversation_external_error)
    }

    pub(crate) fn validate_conversation_adapter(
        &self,
        params: crate::backend::conversations::ExternalAdapterValidateParams,
    ) -> AppResult<crate::backend::conversations::ExternalAdapterValidationResult> {
        crate::backend::conversations::validate_external_adapter(params).map_err(|error| error)
    }

    pub(crate) async fn list_conversation_adapter_runtime_statuses(
        &self,
    ) -> AppResult<Vec<crate::backend::conversations::ConversationAdapterRuntimeStatus>> {
        let adapters = self.list_conversation_adapters()?;
        let sources = self.list_conversation_sources().await?;
        let settings = self.app_settings_value();
        crate::backend::conversations::list_conversation_adapter_runtime_statuses_with_settings(
            &adapters, &sources, &settings,
        )
        .await
        .map_err(conversation_external_error)
    }

    pub(crate) async fn register_conversation_adapter(
        &self,
        params: crate::backend::conversations::ExternalAdapterRegisterParams,
    ) -> AppResult<Value> {
        let dry_run = params.dry_run;
        let settings = self.app_settings_value();
        let preview = crate::backend::conversations::register_external_adapter_with_settings(
            params, &settings,
        )
        .await
        .map_err(conversation_external_error)?;
        let mut adapter =
            crate::backend::conversations::adapter_from_registration_preview(preview.clone())
                .map_err(|error| error)?;
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let existing =
            crate::backend::store::load_conversation_adapter_sqlx(pool, tenant_id, &adapter.id)
                .await
                .map_err(conversation_storage_error)?;
        let reactivating_builtin = existing.as_ref().is_some_and(|existing| {
            existing.trust_state == crate::backend::models::ConversationAdapterTrustState::BuiltIn
        });
        if reactivating_builtin {
            adapter.trust_state = crate::backend::models::ConversationAdapterTrustState::BuiltIn;
            adapter.enabled = true;
        }
        let preflight = self
            .prepare_conversation_adapter_package_change(ConversationAdapterPackageChangeParams {
                action: crate::backend::models::ConversationAdapterPackageChangeAction::Register,
                package_id: None,
                adapter_id: Some(adapter.id.clone()),
            })
            .await?;
        if !preflight.task_conflicts.is_empty() {
            return Err(AppError::Conflict(format!(
                "conversation adapter registration conflicts with running tasks: {}",
                preflight.task_conflicts.join(", ")
            )));
        }
        if !dry_run {
            crate::backend::store::upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter)
                .await
                .map_err(conversation_storage_error)?;
            if reactivating_builtin {
                crate::backend::store::enable_conversation_sources_by_adapter_sqlx(
                    pool,
                    tenant_id,
                    &adapter.id,
                )
                .await
                .map_err(conversation_storage_error)?;
            }
            self.runtime.refresh_conversation_adapter_catalog().await?;
        }
        Ok(preview)
    }

    pub(crate) async fn unregister_conversation_adapter(
        &self,
        params: ConversationAdapterUnregisterParams,
    ) -> AppResult<Value> {
        let preflight = self
            .prepare_conversation_adapter_package_change(ConversationAdapterPackageChangeParams {
                action: crate::backend::models::ConversationAdapterPackageChangeAction::Unregister,
                package_id: None,
                adapter_id: Some(params.adapter_id.clone()),
            })
            .await?;
        if !preflight.task_conflicts.is_empty() {
            return Err(AppError::Conflict(format!(
                "conversation adapter unregister conflicts with running tasks: {}",
                preflight.task_conflicts.join(", ")
            )));
        }
        if !params.dry_run && !params.yes {
            return Err(AppError::Validation(
                "conversation.adapter.unregister requires --yes".to_string(),
            ));
        }
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let adapter = crate::backend::store::load_conversation_adapter_sqlx(
            pool,
            tenant_id,
            &params.adapter_id,
        )
        .await
        .map_err(conversation_storage_error)?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "conversation adapter not found: {}",
                params.adapter_id
            ))
        })?;
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "unregistered": false,
                "adapter": adapter,
                "preflight": preflight
            }));
        }
        if adapter.trust_state == crate::backend::models::ConversationAdapterTrustState::BuiltIn {
            let adapter = crate::backend::store::disable_builtin_conversation_adapter_sqlx(
                pool,
                tenant_id,
                &params.adapter_id,
            )
            .await
            .map_err(conversation_storage_error)?;
            self.runtime.refresh_conversation_adapter_catalog().await?;
            return Ok(json!({
                "dry_run": false,
                "unregistered": false,
                "disabled": true,
                "adapter": adapter
            }));
        }
        let adapter = crate::backend::store::delete_conversation_adapter_registration_sqlx(
            pool,
            tenant_id,
            &params.adapter_id,
            preflight.package_id.as_deref(),
        )
        .await
        .map_err(conversation_storage_error)?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "conversation adapter not found: {}",
                params.adapter_id
            ))
        })?;
        self.runtime.refresh_conversation_adapter_catalog().await?;
        Ok(json!({
            "dry_run": false,
            "unregistered": true,
            "adapter": adapter
        }))
    }

    pub(crate) async fn try_run_conversation_adapter(
        &self,
        params: crate::backend::conversations::ExternalAdapterTryRunParams,
    ) -> AppResult<crate::backend::conversations::ExternalAdapterRunResult> {
        let settings = self.app_settings_value();
        crate::backend::conversations::try_run_external_adapter_with_settings(params, &settings)
            .await
            .map_err(conversation_external_error)
    }

    pub(crate) async fn project_conversation_command_parts(
        &self,
        params: crate::backend::conversations::ConversationCommandProjectionParams,
    ) -> AppResult<Vec<crate::backend::conversations::ConversationCommandProjection>> {
        let adapter = self
            .list_conversation_adapters()?
            .into_iter()
            .find(|adapter| adapter.id == params.adapter_id)
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "conversation adapter not found: {}",
                    params.adapter_id
                ))
            })?;
        let supports_adapter_projection = adapter
            .capabilities
            .iter()
            .any(|capability| capability == "project_command_parts");
        let settings = self.app_settings_value();
        if supports_adapter_projection {
            match crate::backend::conversations::project_external_adapter_command_parts_with_settings(
                &adapter,
                &params.parts,
                &settings,
            )
            .await
            {
                Ok(projections) => return Ok(projections),
                Err(error) => tracing::warn!(
                    action = "conversation.command_projection.fallback",
                    adapter_id = %adapter.id,
                    error = %error,
                    "adapter command projector failed; using the core projector"
                ),
            }
        }

        let core_projector = crate::backend::conversations::ensure_shell_command_projector()?;
        crate::backend::conversations::project_external_adapter_command_parts_with_settings(
            &core_projector,
            &params.parts,
            &settings,
        )
        .await
        .map_err(conversation_external_error)
    }

    pub(crate) async fn list_conversation_sources(&self) -> AppResult<Vec<ConversationSource>> {
        crate::backend::store::list_conversation_sources_sqlx(self.db.pool(), self.tenant_id())
            .await
            .map_err(conversation_storage_error)
    }

    pub(crate) async fn upsert_conversation_source(
        &self,
        params: ConversationSourceUpsertParams,
    ) -> AppResult<Value> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        if crate::backend::store::load_conversation_adapter_sqlx(
            pool,
            tenant_id,
            &params.source.adapter_id,
        )
        .await
        .map_err(conversation_storage_error)?
        .is_none()
        {
            return Err(AppError::NotFound(format!(
                "conversation adapter not found: {}",
                params.source.adapter_id
            )));
        }
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "source": params.source
            }));
        }
        crate::backend::store::upsert_conversation_source_sqlx(pool, tenant_id, &params.source)
            .await
            .map_err(conversation_storage_error)?;
        Ok(json!({
            "dry_run": false,
            "source": params.source
        }))
    }

    pub(crate) async fn disable_conversation_source(
        &self,
        params: ConversationSourceDisableParams,
    ) -> AppResult<Value> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let source =
            crate::backend::store::load_conversation_source_sqlx(pool, tenant_id, &params.id)
                .await
                .map_err(conversation_storage_error)?
                .ok_or_else(|| {
                    AppError::NotFound(format!("conversation source not found: {}", params.id))
                })?;
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "disabled": false,
                "source": source
            }));
        }
        let source =
            crate::backend::store::disable_conversation_source_sqlx(pool, tenant_id, &params.id)
                .await
                .map_err(conversation_storage_error)?;
        Ok(json!({
            "dry_run": false,
            "disabled": true,
            "source": source
        }))
    }

    pub(crate) async fn sync_conversations(
        &self,
        params: ConversationSyncParams,
    ) -> AppResult<Value> {
        self.sync_conversations_with_progress(params, |_, _, _| {})
            .await
    }

    pub(crate) async fn sync_conversations_with_progress<F>(
        &self,
        params: ConversationSyncParams,
        mut on_progress: F,
    ) -> AppResult<Value>
    where
        F: FnMut(usize, usize, Option<String>) + Send,
    {
        self.sync_conversations_with_progress_and_cancellation(params, None, &mut on_progress)
            .await
    }

    pub(crate) async fn sync_conversations_with_progress_and_cancellation<F>(
        &self,
        params: ConversationSyncParams,
        cancellation: Option<&tokio_util::sync::CancellationToken>,
        on_progress: &mut F,
    ) -> AppResult<Value>
    where
        F: FnMut(usize, usize, Option<String>) + Send,
    {
        self.sync_conversations_with_control(params, cancellation, None, on_progress)
            .await
    }

    async fn sync_single_conversation_source(
        &self,
        source: &ConversationSource,
        params: &ConversationSyncParams,
        record_kind: Option<crate::backend::dto::ConversationRecordKind>,
        settings: &Value,
        cancellation: Option<&tokio_util::sync::CancellationToken>,
        progress_listener: Option<crate::backend::conversations::ExternalAdapterProgressListener>,
        on_progress_detail: &mut (dyn FnMut(String) + Send),
    ) -> AppResult<Option<Value>> {
        ensure_conversation_sync_not_cancelled(cancellation)?;
        let adapter = crate::backend::store::load_conversation_adapter_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &source.adapter_id,
        )
        .await
        .map_err(conversation_storage_error)?;
        if !sync_source_matches_record_kind(adapter.as_ref(), &source.adapter_id, record_kind) {
            return Ok(None);
        }
        let web_record_source = is_web_record_adapter(adapter.as_ref(), &source.adapter_id);
        let source_record_kind = if web_record_source {
            crate::backend::dto::ConversationRecordKind::Web
        } else {
            crate::backend::dto::ConversationRecordKind::Session
        };
        let adapter_content_hash = adapter
            .as_ref()
            .and_then(|adapter| adapter.content_hash.clone());
        let card_contract_version = adapter
            .as_ref()
            .and_then(|adapter| adapter.card_contract_version);
        let payload_policy_version =
            crate::backend::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION;
        let known_versions = if params.mode.uses_known_versions() {
            crate::backend::store::load_conversation_session_versions_sqlx(
                self.db.pool(),
                self.tenant_id(),
                &source.id,
                source_record_kind,
                adapter_content_hash.as_deref(),
                card_contract_version,
                payload_policy_version,
            )
            .await
            .map_err(conversation_storage_error)?
        } else {
            std::collections::BTreeMap::new()
        };
        if !params.dry_run && web_record_source {
            on_progress_detail(format!("{} · 采集网页记录", source.name));
        }
        let ready_check = match adapter.as_ref() {
            Some(adapter) => self
                .ensure_conversation_adapter_package_runtime_ready(adapter)
                .await
                .map_err(|error| AppError::External(error.to_string())),
            None => Ok(()),
        };
        let mut on_read_progress = |done: usize, total: usize| {
            on_progress_detail(format!("{} · 读取会话 {done}/{total}", source.name));
        };
        let read_result = match ready_check {
            Ok(()) => {
                if !params.dry_run && web_record_source {
                    crate::backend::conversations::run_conversation_harvester_with_control(
                        adapter.as_ref(),
                        source,
                        matches!(params.mode, ConversationSyncMode::Full),
                        settings,
                        cancellation,
                    )
                    .await
                    .map_err(conversation_external_error)?;
                }
                crate::backend::conversations::read_source_sessions_with_progress_listener(
                    adapter.as_ref(),
                    source,
                    &known_versions,
                    settings,
                    cancellation,
                    &mut on_read_progress,
                    progress_listener,
                )
                .await
                .map_err(conversation_external_error)
            }
            Err(error) => Err(error),
        };
        ensure_conversation_sync_not_cancelled(cancellation)?;
        let sync_result = match read_result {
            Ok(read) if web_record_source => {
                let pool = self.db.pool();
                let tenant_id = self.tenant_id();
                let result = crate::backend::store::import_web_record_sessions_sqlx(
                    pool,
                    tenant_id,
                    source,
                    &read.sessions,
                    params.dry_run,
                )
                .await
                .map_err(conversation_storage_error)?;
                let retained_session_count = persist_successful_conversation_observation(
                    pool,
                    tenant_id,
                    &source.id,
                    source_record_kind,
                    &read,
                    params.dry_run,
                    adapter_content_hash.as_deref(),
                    card_contract_version,
                    payload_policy_version,
                )
                .await
                .map_err(conversation_storage_error)?;
                Ok(conversation_sync_result_value(
                    result,
                    &read,
                    retained_session_count,
                    params.mode,
                ))
            }
            Ok(read) => {
                let pool = self.db.pool();
                let tenant_id = self.tenant_id();
                let discovered_external_ids = read.incremental.then(|| {
                    read.session_descriptors
                        .iter()
                        .map(|descriptor| descriptor.external_id.clone())
                        .collect::<std::collections::BTreeSet<_>>()
                });
                let descriptor_versions = {
                    let mut map = std::collections::BTreeMap::new();
                    for descriptor in &read.session_descriptors {
                        map.insert(
                            descriptor.external_id.clone(),
                            descriptor.version_token.clone(),
                        );
                    }
                    map
                };
                let mut on_import_progress = |done: usize, total: usize| {
                    on_progress_detail(format!("{} · 写入会话 {done}/{total}", source.name));
                };
                let result = crate::backend::store::import_conversation_sessions_advanced_sqlx(
                    pool,
                    tenant_id,
                    source,
                    &read.sessions,
                    discovered_external_ids.as_ref(),
                    Some(&descriptor_versions),
                    read.session_failures.clone(),
                    read.session_warnings.clone(),
                    adapter_content_hash.as_deref(),
                    card_contract_version,
                    payload_policy_version,
                    params.dry_run,
                    cancellation,
                    &mut on_import_progress,
                )
                .await
                .map_err(conversation_storage_error)?;
                let retained_session_count = persist_successful_conversation_observation(
                    pool,
                    tenant_id,
                    &source.id,
                    source_record_kind,
                    &read,
                    params.dry_run,
                    adapter_content_hash.as_deref(),
                    card_contract_version,
                    payload_policy_version,
                )
                .await
                .map_err(conversation_storage_error)?;
                Ok(conversation_sync_result_value(
                    result,
                    &read,
                    retained_session_count,
                    params.mode,
                ))
            }
            Err(error) => Err(error),
        };
        sync_result.map(Some)
    }

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
        let sources =
            crate::backend::store::list_conversation_sources_sqlx(self.db.pool(), self.tenant_id())
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
            let stages = adapter_groups
                .keys()
                .map(|adapter_id| TaskStage {
                    id: format!("adapter:{adapter_id}"),
                    name: format!("Adapter: {adapter_id}"),
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
                })
                .collect::<Vec<_>>();
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
                let stage_id = format!("adapter:{adapter_id}");
                if let Some(task_id) = task_id {
                    let _ = task_runtime.update_stage_status(task_id, &stage_id, StageStatus::Running);
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

                    let task_id_string = task_id.map(|s| s.to_string());
                    let stage_id_clone = stage_id.clone();
                    let worker_id = format!("{}:{}", adapter_id, source.id);
                    let worker_id_clone = worker_id.clone();
                    let progress_task_runtime = task_runtime.clone();

                    let progress_listener: crate::backend::conversations::ExternalAdapterProgressListener =
                        std::sync::Arc::new(move |progress| {
                            if let Some(ref tid) = task_id_string {
                                let activity = TaskActivity {
                                    stage_id: stage_id_clone.clone(),
                                    worker_id: progress
                                        .worker
                                        .clone()
                                        .unwrap_or_else(|| worker_id_clone.clone()),
                                    operation: progress
                                        .operation
                                        .clone()
                                        .unwrap_or_else(|| "processing".to_string()),
                                    path: progress.path.clone(),
                                    display_path: None,
                                    started_at: chrono::Utc::now().to_rfc3339(),
                                    current: progress.current,
                                    total: progress.total,
                                };
                                let _ = progress_task_runtime.record_activity(tid, activity);
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

                    if let Some(task_id) = task_id {
                        let _ = task_runtime.remove_activity(task_id, &stage_id, &worker_id);
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
                            if params.source_id.is_some() {
                                if let Some(task_id) = task_id {
                                    let _ = task_runtime.finish_stage(
                                        task_id,
                                        &stage_id,
                                        StageStatus::Failed,
                                        Vec::new(),
                                        vec![TaskFailure {
                                            code: "ADAPTER_SOURCE_ERROR".to_string(),
                                            message: error.to_string(),
                                            stage: stage_id.clone(),
                                            identity: Some(format!("{}:{}", adapter_id, source.id)),
                                            retryable: false,
                                            path: None,
                                            timestamp: chrono::Utc::now().to_rfc3339(),
                                        }],
                                        Vec::new(),
                                    );
                                }
                                return Err(error);
                            }
                            group_errors.push(json!({
                                "source_id": source.id,
                                "adapter_id": source.adapter_id,
                                "message": error.to_string()
                            }));
                        }
                    }
                }

                if let Some(task_id) = task_id {
                    let (status, failures) = if group_errors.is_empty() {
                        (StageStatus::Succeeded, Vec::new())
                    } else {
                        let failures = group_errors
                            .iter()
                            .map(|err| TaskFailure {
                                code: "ADAPTER_SOURCE_ERROR".to_string(),
                                message: err
                                    .get("message")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("未知错误")
                                    .to_string(),
                                stage: stage_id.clone(),
                                identity: Some(adapter_id.clone()),
                                retryable: false,
                                path: None,
                                timestamp: chrono::Utc::now().to_rfc3339(),
                            })
                            .collect::<Vec<_>>();
                        (StageStatus::Failed, failures)
                    };
                    let _ = task_runtime.finish_stage(
                        task_id,
                        &stage_id,
                        status,
                        Vec::new(),
                        failures,
                        Vec::new(),
                    );
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
                crate::backend::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
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
                crate::backend::dto::ConversationRecordKind::Session => "session",
                crate::backend::dto::ConversationRecordKind::Web => "web",
            }),
            "results": results,
            "errors": errors,
            "legacy_cards_upgraded": legacy_cards_upgraded
        }))
    }
}

fn ensure_conversation_sync_not_cancelled(
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> AppResult<()> {
    if cancellation.is_some_and(tokio_util::sync::CancellationToken::is_cancelled) {
        return Err(AppError::Cancelled(
            "conversation sync cancelled".to_string(),
        ));
    }
    Ok(())
}

async fn persist_successful_conversation_observation(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    source_id: &str,
    record_kind: crate::backend::dto::ConversationRecordKind,
    read: &crate::backend::conversations::ConversationSourceReadResult,
    dry_run: bool,
    adapter_content_hash: Option<&str>,
    card_contract_version: Option<u32>,
    payload_policy_version: u32,
) -> AppResult<usize> {
    if dry_run || !read.incremental {
        return Ok(0);
    }
    let hydrated_external_ids = read
        .sessions
        .iter()
        .map(|session| session.external_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    crate::backend::store::persist_conversation_session_observations_sqlx(
        pool,
        tenant_id,
        source_id,
        record_kind,
        &read.session_descriptors,
        &hydrated_external_ids,
        adapter_content_hash,
        card_contract_version,
        payload_policy_version,
    )
    .await
    .map_err(conversation_storage_error)
}

fn conversation_sync_result_value(
    result: crate::backend::store::ConversationImportResult,
    read: &crate::backend::conversations::ConversationSourceReadResult,
    retained_session_count: usize,
    mode: ConversationSyncMode,
) -> Value {
    let mut value = json!(result);
    let Some(object) = value.as_object_mut() else {
        return value;
    };
    let store_skipped = object
        .get("skipped_session_count")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    object.insert(
        "session_count".to_string(),
        json!(read.discovered_session_count),
    );
    object.insert(
        "active_session_count".to_string(),
        json!(read.active_session_count),
    );
    object.insert(
        "skipped_session_count".to_string(),
        json!(read.skipped_session_count + store_skipped),
    );
    object.insert("incremental".to_string(), json!(read.incremental));
    object.insert("mode".to_string(), json!(mode));
    object.insert(
        "legacy_cards_upgraded".to_string(),
        json!(read.legacy_cards_upgraded),
    );
    object.insert(
        "retained_session_count".to_string(),
        json!(retained_session_count),
    );
    value
}

fn normalize_sync_record_kind(
    record_kind: Option<&str>,
) -> AppResult<Option<crate::backend::dto::ConversationRecordKind>> {
    let Some(record_kind) = record_kind.and_then(clean_non_empty_string) else {
        return Ok(None);
    };
    match record_kind.as_str() {
        "session" | "sessions" | "conversation" | "conversations" => {
            Ok(Some(crate::backend::dto::ConversationRecordKind::Session))
        }
        "web" | "web-record" | "web_record" | "web-records" | "web_records" => {
            Ok(Some(crate::backend::dto::ConversationRecordKind::Web))
        }
        _ => Err(AppError::Validation(format!(
            "unsupported conversation record kind: {record_kind}"
        ))),
    }
}

fn clean_non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_ascii_lowercase())
}

fn sync_source_matches_record_kind(
    adapter: Option<&ConversationAdapter>,
    adapter_id: &str,
    record_kind: Option<crate::backend::dto::ConversationRecordKind>,
) -> bool {
    match record_kind {
        Some(crate::backend::dto::ConversationRecordKind::Session) => {
            !is_web_record_adapter(adapter, adapter_id)
        }
        Some(crate::backend::dto::ConversationRecordKind::Web) => {
            is_web_record_adapter(adapter, adapter_id)
        }
        None => true,
    }
}

fn is_web_record_adapter(adapter: Option<&ConversationAdapter>, adapter_id: &str) -> bool {
    adapter.is_some_and(|adapter| {
        adapter
            .capabilities
            .iter()
            .any(|capability| capability == "web_records")
    }) || adapter_id.ends_with("-web")
}

#[cfg(test)]
#[path = "conversation_adapters_tests.rs"]
mod tests;
