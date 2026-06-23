use super::prelude::*;

impl AppService {
    pub(crate) fn list_conversation_adapters(&self) -> AppResult<Vec<ConversationAdapter>> {
        let pool = self.db.pool().clone();
        self.db.block_on(async move {
            crate::backend::store::list_conversation_adapters_sqlx(&pool).await
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

    pub(crate) fn register_conversation_adapter(
        &self,
        params: crate::backend::conversations::ExternalAdapterRegisterParams,
    ) -> AppResult<Value> {
        let dry_run = params.dry_run;
        let preview = crate::backend::conversations::register_external_adapter(params)?;
        if !dry_run {
            let adapter =
                crate::backend::conversations::adapter_from_registration_preview(preview.clone())?;
            let pool = self.db.pool().clone();
            self.db.block_on(async move {
                crate::backend::store::upsert_conversation_adapter_sqlx(&pool, &adapter).await
            })?;
        }
        Ok(preview)
    }

    pub(crate) fn unregister_conversation_adapter(
        &self,
        params: ConversationAdapterUnregisterParams,
    ) -> AppResult<Value> {
        if !params.dry_run && !params.yes {
            return Err("conversation.adapter.unregister requires --yes".to_string());
        }
        let pool = self.db.pool().clone();
        let adapter_id = params.adapter_id.clone();
        let adapter = self
            .db
            .block_on(async move {
                crate::backend::store::load_conversation_adapter_sqlx(&pool, &adapter_id).await
            })?
            .ok_or_else(|| format!("conversation adapter not found: {}", params.adapter_id))?;
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "unregistered": false,
                "adapter": adapter
            }));
        }
        let pool = self.db.pool().clone();
        let adapter_id = params.adapter_id.clone();
        let adapter = self.db.block_on(async move {
            crate::backend::store::delete_conversation_adapter_sqlx(&pool, &adapter_id).await
        })?;
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
        self.db.block_on(async move {
            crate::backend::store::list_conversation_sources_sqlx(&pool).await
        })
    }

    pub(crate) fn upsert_conversation_source(
        &self,
        params: ConversationSourceUpsertParams,
    ) -> AppResult<Value> {
        let pool = self.db.pool().clone();
        let adapter_id = params.source.adapter_id.clone();
        if self
            .db
            .block_on(async move {
                crate::backend::store::load_conversation_adapter_sqlx(&pool, &adapter_id).await
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
        let source = params.source.clone();
        self.db.block_on(async move {
            crate::backend::store::upsert_conversation_source_sqlx(&pool, &source).await
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
        let source_id = params.id.clone();
        let source = self
            .db
            .block_on(async move {
                crate::backend::store::load_conversation_source_sqlx(&pool, &source_id).await
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
        let source_id = params.id.clone();
        let source = self.db.block_on(async move {
            crate::backend::store::disable_conversation_source_sqlx(&pool, &source_id).await
        })?;
        Ok(json!({
            "dry_run": false,
            "disabled": true,
            "source": source
        }))
    }

    pub(crate) fn sync_conversations(&self, params: ConversationSyncParams) -> AppResult<Value> {
        let pool = self.db.pool().clone();
        let sources = self
            .db
            .block_on(
                async move { crate::backend::store::list_conversation_sources_sqlx(&pool).await },
            )?
            .into_iter()
            .filter(|source| params.source_id.as_deref().is_none_or(|id| id == source.id))
            .filter(|source| {
                params
                    .adapter_id
                    .as_deref()
                    .is_none_or(|id| id == source.adapter_id)
            })
            .filter(|source| {
                source.enabled || params.source_id.as_deref() == Some(source.id.as_str())
            })
            .collect::<Vec<_>>();
        if sources.is_empty() {
            return Err("no matching conversation sources".to_string());
        }

        let mut results = Vec::new();
        let mut errors = Vec::new();
        for source in sources {
            let pool = self.db.pool().clone();
            let adapter_id = source.adapter_id.clone();
            let adapter = self.db.block_on(async move {
                crate::backend::store::load_conversation_adapter_sqlx(&pool, &adapter_id).await
            })?;
            let sync_result = match crate::backend::conversations::read_source_sessions_with_adapter(
                adapter.as_ref(),
                &source,
            ) {
                Ok(sessions) if is_web_record_adapter(adapter.as_ref(), &source.adapter_id) => {
                    let pool = self.db.pool().clone();
                    let import_source = source.clone();
                    self.db.block_on(async move {
                        crate::backend::store::import_web_record_sessions_sqlx(
                            &pool,
                            &import_source,
                            &sessions,
                            params.dry_run,
                        )
                        .await
                    })
                }
                Ok(sessions) => {
                    let pool = self.db.pool().clone();
                    let import_source = source.clone();
                    self.db.block_on(async move {
                        crate::backend::store::import_conversation_sessions_sqlx(
                            &pool,
                            &import_source,
                            &sessions,
                            params.dry_run,
                        )
                        .await
                    })
                }
                Err(error) => Err(error),
            };
            match sync_result {
                Ok(result) => results.push(json!(result)),
                Err(error) if params.source_id.is_some() => return Err(error),
                Err(error) => errors.push(json!({
                    "source_id": source.id,
                    "adapter_id": source.adapter_id,
                    "message": error
                })),
            }
        }
        Ok(json!({
            "dry_run": params.dry_run,
            "results": results,
            "errors": errors
        }))
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
