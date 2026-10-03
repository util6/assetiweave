use super::conversation_adapters::{conversation_external_error, conversation_storage_error};
use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use serde_json::{json, Value};

impl AppService {
    pub(crate) async fn sync_conversations(
        &self,
        params: ConversationSyncParams,
    ) -> AppResult<Value> {
        self.sync_conversations_with_progress(params, |_, _, _, _| {})
            .await
    }

    pub(crate) async fn sync_conversations_with_progress<F>(
        &self,
        params: ConversationSyncParams,
        mut on_progress: F,
    ) -> AppResult<Value>
    where
        F: FnMut(usize, usize, Option<String>, &[String]) + Send,
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
        F: FnMut(usize, usize, Option<String>, &[String]) + Send,
    {
        self.sync_conversations_with_control(params, cancellation, None, on_progress)
            .await
    }

    pub(crate) async fn sync_single_conversation_source(
        &self,
        source: &ConversationSource,
        params: &ConversationSyncParams,
        record_kind: Option<crate::backend::domain::conversations::ConversationRecordKind>,
        settings: &Value,
        cancellation: Option<&tokio_util::sync::CancellationToken>,
        progress_listener: Option<
            crate::backend::infrastructure::conversations::ExternalAdapterProgressListener,
        >,
        on_progress_detail: &mut (dyn FnMut(String) + Send),
    ) -> AppResult<Option<Value>> {
        ensure_conversation_sync_not_cancelled(cancellation)?;
        let adapter = super::conversation_storage::load_adapter(
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
            crate::backend::domain::conversations::ConversationRecordKind::Web
        } else {
            crate::backend::domain::conversations::ConversationRecordKind::Session
        };
        let adapter_content_hash = adapter
            .as_ref()
            .and_then(|adapter| adapter.content_hash.clone());
        let card_contract_version = adapter
            .as_ref()
            .and_then(|adapter| adapter.card_contract_version);
        let projection_version = adapter
            .as_ref()
            .and_then(|adapter| adapter.projection_version);
        let payload_policy_version =
            crate::backend::infrastructure::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION;
        let known_versions = if params.mode.uses_known_versions() {
            crate::backend::store::load_conversation_session_versions_sqlx(
                self.db.pool(),
                self.tenant_id(),
                &source.id,
                source_record_kind,
                adapter_content_hash.as_deref(),
                card_contract_version,
                payload_policy_version,
                projection_version,
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
                    crate::backend::infrastructure::conversations::run_conversation_harvester_with_control(
                        adapter.as_ref(),
                        source,
                        matches!(params.mode, ConversationSyncMode::Full),
                        settings,
                        cancellation,
                    )
                    .await
                    .map_err(conversation_external_error)?;
                }
                crate::backend::infrastructure::conversations::read_source_sessions_with_progress_listener(
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
                    projection_version,
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
                    projection_version,
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
                    projection_version,
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
}

pub(crate) fn ensure_conversation_sync_not_cancelled(
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
    record_kind: crate::backend::domain::conversations::ConversationRecordKind,
    read: &crate::backend::infrastructure::conversations::ConversationSourceReadResult,
    dry_run: bool,
    adapter_content_hash: Option<&str>,
    card_contract_version: Option<u32>,
    payload_policy_version: u32,
    projection_version: Option<u32>,
) -> AppResult<usize> {
    if dry_run || !read.incremental {
        return Ok(0);
    }
    let hydrated_external_ids = read
        .sessions
        .iter()
        .map(|session| session.external_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let observations = persistable_conversation_session_observations(&read.session_descriptors);
    crate::backend::store::persist_conversation_session_observations_sqlx(
        pool,
        tenant_id,
        source_id,
        record_kind,
        &observations,
        &hydrated_external_ids,
        adapter_content_hash,
        card_contract_version,
        payload_policy_version,
        projection_version,
    )
    .await
    .map_err(conversation_storage_error)
}

pub(crate) fn persistable_conversation_session_observations(
    descriptors: &[crate::backend::infrastructure::conversations::ConversationSessionDescriptor],
) -> Vec<crate::backend::domain::conversations::ConversationSessionObservation> {
    descriptors
        .iter()
        .map(
            |descriptor| crate::backend::domain::conversations::ConversationSessionObservation {
                external_id: descriptor.external_id.clone(),
                updated_at: descriptor.updated_at.clone(),
                source_locator: descriptor.source_locator.clone(),
                version_token: descriptor.version_token.clone(),
            },
        )
        .collect()
}

pub(crate) fn conversation_sync_result_value(
    result: crate::backend::store::ConversationImportResult,
    read: &crate::backend::infrastructure::conversations::ConversationSourceReadResult,
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

pub(crate) fn normalize_sync_record_kind(
    record_kind: Option<&str>,
) -> AppResult<Option<crate::backend::domain::conversations::ConversationRecordKind>> {
    let Some(record_kind) = record_kind.and_then(clean_non_empty_string) else {
        return Ok(None);
    };
    match record_kind.as_str() {
        "session" | "sessions" | "conversation" | "conversations" => Ok(Some(
            crate::backend::domain::conversations::ConversationRecordKind::Session,
        )),
        "web" | "web-record" | "web_record" | "web-records" | "web_records" => Ok(Some(
            crate::backend::domain::conversations::ConversationRecordKind::Web,
        )),
        _ => Err(AppError::Validation(format!(
            "unsupported conversation record kind: {record_kind}"
        ))),
    }
}

pub(crate) fn clean_non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_ascii_lowercase())
}

pub(crate) fn sync_source_matches_record_kind(
    adapter: Option<&ConversationAdapter>,
    adapter_id: &str,
    record_kind: Option<crate::backend::domain::conversations::ConversationRecordKind>,
) -> bool {
    match record_kind {
        Some(crate::backend::domain::conversations::ConversationRecordKind::Session) => {
            !is_web_record_adapter(adapter, adapter_id)
        }
        Some(crate::backend::domain::conversations::ConversationRecordKind::Web) => {
            is_web_record_adapter(adapter, adapter_id)
        }
        None => true,
    }
}

pub(crate) fn is_web_record_adapter(
    adapter: Option<&ConversationAdapter>,
    adapter_id: &str,
) -> bool {
    adapter.is_some_and(|adapter| {
        adapter
            .capabilities
            .iter()
            .any(|capability| capability == "web_records")
    }) || adapter_id.ends_with("-web")
}
