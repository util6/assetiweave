use super::prelude::*;

impl AppService {
    pub(crate) fn list_conversation_adapters(&self) -> AppResult<Vec<ConversationAdapter>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            crate::backend::store::list_conversation_adapters_sqlx(&pool, &tenant_id).await
        })
    }

    pub(crate) fn scaffold_conversation_adapter(
        &self,
        params: crate::backend::conversations::ExternalAdapterScaffoldParams,
    ) -> AppResult<crate::backend::conversations::ExternalAdapterScaffoldResult> {
        crate::backend::conversations::scaffold_external_adapter(params)
    }

    pub(crate) fn validate_conversation_adapter(
        &self,
        params: crate::backend::conversations::ExternalAdapterValidateParams,
    ) -> AppResult<crate::backend::conversations::ExternalAdapterValidationResult> {
        crate::backend::conversations::validate_external_adapter(params)
    }

    pub(crate) fn list_conversation_adapter_runtime_statuses(
        &self,
    ) -> AppResult<Vec<crate::backend::conversations::ConversationAdapterRuntimeStatus>> {
        let adapters = self.list_conversation_adapters()?;
        let sources = self.list_conversation_sources()?;
        crate::backend::conversations::list_conversation_adapter_runtime_statuses(
            &adapters, &sources,
        )
    }

    pub(crate) fn register_conversation_adapter(
        &self,
        params: crate::backend::conversations::ExternalAdapterRegisterParams,
    ) -> AppResult<Value> {
        let dry_run = params.dry_run;
        let preview = crate::backend::conversations::register_external_adapter(params)?;
        let mut adapter =
            crate::backend::conversations::adapter_from_registration_preview(preview.clone())?;
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let adapter_id = adapter.id.clone();
        let existing = self.db.block_on(async move {
            crate::backend::store::load_conversation_adapter_sqlx(&pool, &tenant_id, &adapter_id)
                .await
        })?;
        let reactivating_builtin = existing.as_ref().is_some_and(|existing| {
            existing.trust_state == crate::backend::models::ConversationAdapterTrustState::BuiltIn
        });
        if reactivating_builtin {
            adapter.trust_state = crate::backend::models::ConversationAdapterTrustState::BuiltIn;
            adapter.enabled = true;
        }
        let preflight = self.prepare_conversation_adapter_package_change(
            ConversationAdapterPackageChangeParams {
                action: crate::backend::models::ConversationAdapterPackageChangeAction::Register,
                package_id: None,
                adapter_id: Some(adapter.id.clone()),
            },
        )?;
        if !preflight.task_conflicts.is_empty() {
            return Err(format!(
                "conversation adapter registration conflicts with running tasks: {}",
                preflight.task_conflicts.join(", ")
            ));
        }
        if !dry_run {
            let pool = self.db.pool().clone();
            let tenant_id = self.tenant_id().to_string();
            self.db.block_on(async move {
                crate::backend::store::upsert_conversation_adapter_sqlx(
                    &pool, &tenant_id, &adapter,
                )
                .await?;
                if reactivating_builtin {
                    crate::backend::store::enable_conversation_sources_by_adapter_sqlx(
                        &pool,
                        &tenant_id,
                        &adapter.id,
                    )
                    .await?;
                }
                AppResult::Ok(())
            })?;
        }
        Ok(preview)
    }

    pub(crate) fn unregister_conversation_adapter(
        &self,
        params: ConversationAdapterUnregisterParams,
    ) -> AppResult<Value> {
        let preflight = self.prepare_conversation_adapter_package_change(
            ConversationAdapterPackageChangeParams {
                action: crate::backend::models::ConversationAdapterPackageChangeAction::Unregister,
                package_id: None,
                adapter_id: Some(params.adapter_id.clone()),
            },
        )?;
        if !preflight.task_conflicts.is_empty() {
            return Err(format!(
                "conversation adapter unregister conflicts with running tasks: {}",
                preflight.task_conflicts.join(", ")
            ));
        }
        if !params.dry_run && !params.yes {
            return Err("conversation.adapter.unregister requires --yes".to_string());
        }
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let adapter_id = params.adapter_id.clone();
        let adapter = self
            .db
            .block_on(async move {
                crate::backend::store::load_conversation_adapter_sqlx(
                    &pool,
                    &tenant_id,
                    &adapter_id,
                )
                .await
            })?
            .ok_or_else(|| format!("conversation adapter not found: {}", params.adapter_id))?;
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "unregistered": false,
                "adapter": adapter,
                "preflight": preflight
            }));
        }
        if adapter.trust_state == crate::backend::models::ConversationAdapterTrustState::BuiltIn {
            let pool = self.db.pool().clone();
            let tenant_id = self.tenant_id().to_string();
            let adapter_id = params.adapter_id.clone();
            let adapter = self.db.block_on(async move {
                crate::backend::store::disable_builtin_conversation_adapter_sqlx(
                    &pool,
                    &tenant_id,
                    &adapter_id,
                )
                .await
            })?;
            return Ok(json!({
                "dry_run": false,
                "unregistered": false,
                "disabled": true,
                "adapter": adapter
            }));
        }
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let adapter_id = params.adapter_id.clone();
        let package_id = preflight.package_id.clone();
        let adapter = self
            .db
            .block_on(async move {
                crate::backend::store::delete_conversation_adapter_registration_sqlx(
                    &pool,
                    &tenant_id,
                    &adapter_id,
                    package_id.as_deref(),
                )
                .await
            })?
            .ok_or_else(|| format!("conversation adapter not found: {}", params.adapter_id))?;
        Ok(json!({
            "dry_run": false,
            "unregistered": true,
            "adapter": adapter
        }))
    }

    pub(crate) fn try_run_conversation_adapter(
        &self,
        params: crate::backend::conversations::ExternalAdapterTryRunParams,
    ) -> AppResult<crate::backend::conversations::ExternalAdapterRunResult> {
        crate::backend::conversations::try_run_external_adapter(params)
    }

    pub(crate) fn list_conversation_sources(&self) -> AppResult<Vec<ConversationSource>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            crate::backend::store::list_conversation_sources_sqlx(&pool, &tenant_id).await
        })
    }

    pub(crate) fn upsert_conversation_source(
        &self,
        params: ConversationSourceUpsertParams,
    ) -> AppResult<Value> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let adapter_id = params.source.adapter_id.clone();
        if self
            .db
            .block_on(async move {
                crate::backend::store::load_conversation_adapter_sqlx(
                    &pool,
                    &tenant_id,
                    &adapter_id,
                )
                .await
            })?
            .is_none()
        {
            return Err(format!(
                "conversation adapter not found: {}",
                params.source.adapter_id
            ));
        }
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "source": params.source
            }));
        }
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let source = params.source.clone();
        self.db.block_on(async move {
            crate::backend::store::upsert_conversation_source_sqlx(&pool, &tenant_id, &source).await
        })?;
        Ok(json!({
            "dry_run": false,
            "source": params.source
        }))
    }

    pub(crate) fn disable_conversation_source(
        &self,
        params: ConversationSourceDisableParams,
    ) -> AppResult<Value> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let source_id = params.id.clone();
        let source = self
            .db
            .block_on(async move {
                crate::backend::store::load_conversation_source_sqlx(&pool, &tenant_id, &source_id)
                    .await
            })?
            .ok_or_else(|| format!("conversation source not found: {}", params.id))?;
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "disabled": false,
                "source": source
            }));
        }
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let source_id = params.id.clone();
        let source = self.db.block_on(async move {
            crate::backend::store::disable_conversation_source_sqlx(&pool, &tenant_id, &source_id)
                .await
        })?;
        Ok(json!({
            "dry_run": false,
            "disabled": true,
            "source": source
        }))
    }

    pub(crate) fn sync_conversations(&self, params: ConversationSyncParams) -> AppResult<Value> {
        let record_kind = normalize_sync_record_kind(params.record_kind.as_deref())?;
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let sources = self
            .db
            .block_on(async move {
                crate::backend::store::list_conversation_sources_sqlx(&pool, &tenant_id).await
            })?
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
            return Err("no matching conversation sources".to_string());
        }

        let mut results = Vec::new();
        let mut errors = Vec::new();
        for source in sources {
            let pool = self.db.pool().clone();
            let tenant_id = self.tenant_id().to_string();
            let adapter_id = source.adapter_id.clone();
            let adapter = self.db.block_on(async move {
                crate::backend::store::load_conversation_adapter_sqlx(
                    &pool,
                    &tenant_id,
                    &adapter_id,
                )
                .await
            })?;
            if !sync_source_matches_record_kind(adapter.as_ref(), &source.adapter_id, record_kind) {
                continue;
            }
            let web_record_source = is_web_record_adapter(adapter.as_ref(), &source.adapter_id);
            let source_record_kind = if web_record_source {
                crate::backend::dto::ConversationRecordKind::Web
            } else {
                crate::backend::dto::ConversationRecordKind::Session
            };
            let pool = self.db.pool().clone();
            let tenant_id = self.tenant_id().to_string();
            let source_id = source.id.clone();
            let known_versions = self.db.block_on(async move {
                crate::backend::store::load_conversation_session_versions_sqlx(
                    &pool,
                    &tenant_id,
                    &source_id,
                    source_record_kind,
                )
                .await
            })?;
            let read_result = adapter
                .as_ref()
                .map(|adapter| self.ensure_conversation_adapter_package_runtime_ready(adapter))
                .unwrap_or(Ok(()))
                .and_then(|_| {
                    if !params.dry_run && web_record_source {
                        crate::backend::conversations::run_conversation_harvester_for_adapter_source(
                            adapter.as_ref(),
                            &source,
                        )
                        .and_then(|_| {
                            crate::backend::conversations::read_source_sessions_incrementally_with_adapter(
                                adapter.as_ref(),
                                &source,
                                &known_versions,
                            )
                        })
                    } else {
                        crate::backend::conversations::read_source_sessions_incrementally_with_adapter(
                            adapter.as_ref(),
                            &source,
                            &known_versions,
                        )
                    }
                });
            let sync_result = match read_result {
                Ok(read) if web_record_source => {
                    let pool = self.db.pool().clone();
                    let tenant_id = self.tenant_id().to_string();
                    let import_source = source.clone();
                    self.db.block_on(async move {
                        let result = crate::backend::store::import_web_record_sessions_sqlx(
                            &pool,
                            &tenant_id,
                            &import_source,
                            &read.sessions,
                            params.dry_run,
                        )
                        .await?;
                        let retained_session_count = persist_successful_conversation_observation(
                            &pool,
                            &tenant_id,
                            &import_source.id,
                            source_record_kind,
                            &read,
                            params.dry_run,
                        )
                        .await?;
                        Ok(conversation_sync_result_value(
                            result,
                            &read,
                            retained_session_count,
                        ))
                    })
                }
                Ok(read) => {
                    let pool = self.db.pool().clone();
                    let tenant_id = self.tenant_id().to_string();
                    let import_source = source.clone();
                    self.db.block_on(async move {
                        let result = crate::backend::store::import_conversation_sessions_sqlx(
                            &pool,
                            &tenant_id,
                            &import_source,
                            &read.sessions,
                            params.dry_run,
                        )
                        .await?;
                        let retained_session_count = persist_successful_conversation_observation(
                            &pool,
                            &tenant_id,
                            &import_source.id,
                            source_record_kind,
                            &read,
                            params.dry_run,
                        )
                        .await?;
                        Ok(conversation_sync_result_value(
                            result,
                            &read,
                            retained_session_count,
                        ))
                    })
                }
                Err(error) => Err(error),
            };
            match sync_result {
                Ok(result) => results.push(result),
                Err(error) if params.source_id.is_some() => return Err(error),
                Err(error) => errors.push(json!({
                    "source_id": source.id,
                    "adapter_id": source.adapter_id,
                    "message": error
                })),
            }
        }
        if results.is_empty() && errors.is_empty() {
            return Err("no matching conversation sources".to_string());
        }
        Ok(json!({
            "dry_run": params.dry_run,
            "record_kind": record_kind.map(|kind| match kind {
                crate::backend::dto::ConversationRecordKind::Session => "session",
                crate::backend::dto::ConversationRecordKind::Web => "web",
            }),
            "results": results,
            "errors": errors
        }))
    }
}

async fn persist_successful_conversation_observation(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    source_id: &str,
    record_kind: crate::backend::dto::ConversationRecordKind,
    read: &crate::backend::conversations::ConversationSourceReadResult,
    dry_run: bool,
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
    )
    .await
}

fn conversation_sync_result_value(
    result: crate::backend::store::ConversationImportResult,
    read: &crate::backend::conversations::ConversationSourceReadResult,
    retained_session_count: usize,
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
        _ => Err(format!(
            "unsupported conversation record kind: {record_kind}"
        )),
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
mod tests {
    use super::*;

    fn adapter(id: &str, capabilities: &[&str]) -> ConversationAdapter {
        ConversationAdapter {
            id: id.to_string(),
            name: id.to_string(),
            kind: crate::backend::models::ConversationAdapterKind::External,
            version: "0.1.0".to_string(),
            enabled: true,
            manifest_path: None,
            executable_path: None,
            content_hash: None,
            trusted_hash: None,
            trust_state: crate::backend::models::ConversationAdapterTrustState::Trusted,
            protocol_version: Some(1),
            capabilities: capabilities.iter().map(|value| value.to_string()).collect(),
            input_kinds: vec![crate::backend::models::ConversationSourceKind::Directory],
            created_at: "2026-06-23T00:00:00Z".to_string(),
            updated_at: "2026-06-23T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn sync_record_kind_filters_session_and_web_sources_by_adapter_capability() {
        let session_adapter = adapter("codex", &["read_session"]);
        let web_adapter = adapter("qwen-web", &["read_session", "web_records"]);

        assert!(sync_source_matches_record_kind(
            Some(&session_adapter),
            "codex",
            normalize_sync_record_kind(Some("session")).unwrap(),
        ));
        assert!(!sync_source_matches_record_kind(
            Some(&web_adapter),
            "qwen-web",
            normalize_sync_record_kind(Some("session")).unwrap(),
        ));
        assert!(!sync_source_matches_record_kind(
            Some(&session_adapter),
            "codex",
            normalize_sync_record_kind(Some("web_records")).unwrap(),
        ));
        assert!(sync_source_matches_record_kind(
            Some(&web_adapter),
            "qwen-web",
            normalize_sync_record_kind(Some("web")).unwrap(),
        ));
        assert!(normalize_sync_record_kind(Some("assets")).is_err());
    }
}
