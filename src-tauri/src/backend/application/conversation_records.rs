use super::prelude::*;
use crate::backend::runtime::{AppError, AppResult};

impl AppService {
    pub(crate) fn list_conversation_sessions(
        &self,
        params: ConversationSessionListParams,
    ) -> AppResult<Vec<crate::backend::dto::ConversationSessionListItem>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let adapter_id = params.adapter_id;
        let source_id = params.source_id;
        let query = params.query;
        let limit = params.limit.unwrap_or(50).clamp(1, 500);
        let offset = params.offset.unwrap_or(0);
        let direct_id_query = query.as_deref().is_some_and(|value| {
            value.trim().len() == 8
                && crate::backend::models::conversation_id_search_term(value).is_some()
        });
        Ok(self.db.block_on(async move {
            if direct_id_query {
                crate::backend::store::list_conversation_sessions_by_id_fragment_sqlx(
                    &pool,
                    &tenant_id,
                    crate::backend::dto::ConversationRecordKind::Session,
                    adapter_id.as_deref(),
                    source_id.as_deref(),
                    query.as_deref().unwrap_or_default(),
                    limit,
                    offset,
                )
                .await
                .map_err(|error| error)
            } else {
                crate::backend::store::list_conversation_sessions_sqlx(
                    &pool,
                    &tenant_id,
                    adapter_id.as_deref(),
                    source_id.as_deref(),
                    query.as_deref(),
                    limit,
                    offset,
                )
                .await
                .map_err(|error| error)
            }
        })?)
    }

    pub(crate) fn get_conversation_session(
        &self,
        params: ConversationSessionGetParams,
    ) -> AppResult<crate::backend::dto::ConversationSessionDetail> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        Ok(self
            .db
            .block_on(async move {
                let session_id =
                    crate::backend::store::resolve_conversation_session_id_prefix_sqlx(
                        &pool,
                        &tenant_id,
                        &params.session_id,
                    )
                    .await?;
                crate::backend::store::load_conversation_session_detail_sqlx(
                    &pool,
                    &tenant_id,
                    &session_id,
                )
                .await
            })
            .map_err(AppError::external)?)
    }

    pub(crate) fn list_web_record_sessions(
        &self,
        params: ConversationSessionListParams,
    ) -> AppResult<Vec<crate::backend::dto::ConversationSessionListItem>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let adapter_id = params.adapter_id;
        let source_id = params.source_id;
        let query = params.query;
        let limit = params.limit.unwrap_or(50).clamp(1, 500);
        let offset = params.offset.unwrap_or(0);
        let direct_id_query = query.as_deref().is_some_and(|value| {
            value.trim().len() == 8
                && crate::backend::models::conversation_id_search_term(value).is_some()
        });
        Ok(self.db.block_on(async move {
            if direct_id_query {
                crate::backend::store::list_conversation_sessions_by_id_fragment_sqlx(
                    &pool,
                    &tenant_id,
                    crate::backend::dto::ConversationRecordKind::Web,
                    adapter_id.as_deref(),
                    source_id.as_deref(),
                    query.as_deref().unwrap_or_default(),
                    limit,
                    offset,
                )
                .await
                .map_err(|error| error)
            } else {
                crate::backend::store::list_web_record_sessions_sqlx(
                    &pool,
                    &tenant_id,
                    adapter_id.as_deref(),
                    source_id.as_deref(),
                    query.as_deref(),
                    limit,
                    offset,
                )
                .await
            }
        })?)
    }

    pub(crate) fn get_web_record_session(
        &self,
        params: ConversationSessionGetParams,
    ) -> AppResult<crate::backend::dto::ConversationSessionDetail> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        Ok(self.db.block_on(async move {
            let session_id = crate::backend::store::resolve_web_record_session_id_prefix_sqlx(
                &pool,
                &tenant_id,
                &params.session_id,
            )
            .await?;
            crate::backend::store::load_web_record_session_detail_sqlx(
                &pool,
                &tenant_id,
                &session_id,
            )
            .await
        })?)
    }

    pub(crate) fn search_conversation_records(
        &self,
        params: ConversationSearchParams,
    ) -> AppResult<ConversationSearchResult> {
        self.search_conversation_records_with_recent_deltas(params, None)
    }

    pub(crate) fn search_recent_incremental_conversation_records(
        &self,
        params: ConversationIncrementalSearchParams,
    ) -> AppResult<ConversationSearchResult> {
        let recent_runs = params.recent_runs.unwrap_or(3).clamp(1, 20);
        self.search_conversation_records_with_recent_deltas(
            params.into_search_params(),
            Some(recent_runs),
        )
    }

    fn search_conversation_records_with_recent_deltas(
        &self,
        params: ConversationSearchParams,
        recent_run_limit: Option<usize>,
    ) -> AppResult<ConversationSearchResult> {
        let query = params.query.trim();
        if query.is_empty() {
            return Err(AppError::Validation(
                "conversation search query is required".to_string(),
            ));
        }
        if query.chars().count() > 512 {
            return Err(AppError::Validation(
                "conversation search query must not exceed 512 characters".to_string(),
            ));
        }
        let direct_id_query = crate::backend::models::conversation_id_search_term(query).is_some();
        if let Some(mode) = params
            .search_options
            .as_ref()
            .and_then(|options| options.retrieval_mode)
        {
            if mode != crate::backend::dto::SearchRetrievalMode::Lexical {
                return Err(AppError::Validation(format!(
                    "conversation search retrieval mode {} is not supported; supported modes: lexical",
                    match mode {
                        crate::backend::dto::SearchRetrievalMode::Lexical => "lexical",
                        crate::backend::dto::SearchRetrievalMode::Semantic => "semantic",
                        crate::backend::dto::SearchRetrievalMode::Hybrid => "hybrid",
                    }
                )));
            }
        }
        let (record_kind_label, record_kind) =
            normalize_conversation_record_kind(params.record_kind.as_deref())?;
        let limit = params.limit.unwrap_or(50).clamp(1, 500);
        let offset = params.offset.unwrap_or(0);
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let adapter_id = params.adapter_id.clone();
        let source_id = params.source_id.clone();
        let recent_deltas = if let Some(recent_run_limit) = recent_run_limit {
            let delta_pool = pool.clone();
            let delta_tenant_id = tenant_id.clone();
            let delta_adapter_id = adapter_id.clone();
            let delta_source_id = source_id.clone();
            self.db
                .block_on(async move {
                    crate::backend::store::load_recent_conversation_sync_deltas_sqlx(
                        &delta_pool,
                        &delta_tenant_id,
                        record_kind,
                        delta_source_id.as_deref(),
                        delta_adapter_id.as_deref(),
                        recent_run_limit,
                    )
                    .await
                })
                .map_err(AppError::external)?
        } else {
            Vec::new()
        };
        let incremental_scope = recent_run_limit.map(|recent_runs| {
            let included_run_count = recent_deltas
                .iter()
                .map(|delta| delta.sync_run_id.as_str())
                .collect::<BTreeSet<_>>()
                .len();
            ConversationSearchIncrementalScope {
                recent_runs,
                included_run_count,
                changed_session_count: recent_deltas
                    .iter()
                    .map(|delta| delta.session_id.as_str())
                    .collect::<BTreeSet<_>>()
                    .len(),
            }
        });
        let allowed_session_ids = recent_run_limit.map(|_| {
            recent_deltas
                .iter()
                .map(|delta| delta.session_id.clone())
                .collect::<BTreeSet<_>>()
        });
        let incremental_match_by_session =
            recent_deltas
                .into_iter()
                .fold(BTreeMap::new(), |mut matches, delta| {
                    matches.entry(delta.session_id).or_insert_with(|| {
                        crate::backend::dto::ConversationSearchIncrementalMatch {
                            sync_run_id: delta.sync_run_id,
                            change_kind: delta.change_kind,
                            observed_at: delta.observed_at,
                        }
                    });
                    matches
                });
        let project_path = if record_kind == crate::backend::dto::ConversationRecordKind::Web {
            None
        } else {
            params.project_path.clone()
        };
        let query = query.to_string();
        let search_query = query.clone();
        let content_types = params.content_types.clone();
        let legacy_semantic_roles = ["answer", "tool", "command", "code", "result"];
        let mut card_kinds = params.card_kinds.clone();
        let mut semantic_roles = params.semantic_roles.clone();
        for content_type in &content_types {
            let value = content_type.as_str();
            if legacy_semantic_roles.contains(&value) {
                if !semantic_roles.iter().any(|role| role == value) {
                    semantic_roles.push(value.to_string());
                }
            } else if value != "question" && !card_kinds.iter().any(|kind| kind == value) {
                card_kinds.push(value.to_string());
            }
        }
        let mut scan_content_types = content_types.clone();
        for kind in &card_kinds {
            let kind = crate::backend::dto::ConversationSearchCardType::new(kind);
            if !scan_content_types.contains(&kind) {
                scan_content_types.push(kind);
            }
        }
        let include_questions = params.include_questions.unwrap_or_else(|| {
            (content_types.is_empty() && card_kinds.is_empty() && semantic_roles.is_empty())
                || content_types.iter().any(|kind| kind.as_str() == "question")
        });
        let include_cards = params.include_cards.unwrap_or_else(|| {
            content_types.is_empty() || !card_kinds.is_empty() || !semantic_roles.is_empty()
        });
        let since = params.since.clone();
        let until = params.until.clone();
        let timeline = params.timeline;
        let search_project_path = project_path.clone();
        let indexed_page =
            if allowed_session_ids.is_none() && since.is_none() && until.is_none() && !timeline {
                crate::backend::search::conversation::search_ready_conversation_index(
                    &self.db,
                    &self.db_path,
                    &tenant_id,
                    search_query.clone(),
                    record_kind_label.clone(),
                    card_kinds.clone(),
                    semantic_roles.clone(),
                    include_questions,
                    include_cards,
                    adapter_id.clone(),
                    source_id.clone(),
                    search_project_path.clone(),
                    limit,
                    offset,
                )
                .ok()
                .flatten()
            } else {
                None
            };
        let fallback_backend = if incremental_scope.is_some() {
            "incremental_delta_scan"
        } else {
            "legacy_scan"
        };
        let (mut page, backend, content_type_counts, semantic_role_counts) =
            if let Some(matches) = indexed_page {
                let facet_counts = matches.content_type_counts.clone();
                let semantic_counts = matches.semantic_role_counts.clone();
                let hydrate_pool = pool.clone();
                let hydrate_tenant = tenant_id.clone();
                let hydrate_adapter = adapter_id.clone();
                let hydrate_source = source_id.clone();
                let hydrate_query = search_query.clone();
                match self.db.block_on(async move {
                    crate::backend::store::hydrate_conversation_search_matches_sqlx(
                        &hydrate_pool,
                        &hydrate_tenant,
                        record_kind,
                        hydrate_adapter.as_deref(),
                        hydrate_source.as_deref(),
                        &hydrate_query,
                        matches,
                    )
                    .await
                }) {
                    Ok(page) => (page, "tantivy", Some(facet_counts), Some(semantic_counts)),
                    Err(_) => {
                        let page = self
                            .db
                            .block_on(async move {
                                crate::backend::store::search_conversation_cards_sqlx(
                                    &pool,
                                    &tenant_id,
                                    record_kind,
                                    adapter_id.as_deref(),
                                    source_id.as_deref(),
                                    search_project_path.as_deref(),
                                    &search_query,
                                    &scan_content_types,
                                    &semantic_roles,
                                    include_questions,
                                    include_cards,
                                    since.as_deref(),
                                    until.as_deref(),
                                    timeline,
                                    limit,
                                    offset,
                                    None,
                                )
                                .await
                            })
                            .map_err(AppError::external)?;
                        (page, "legacy_scan", None, None)
                    }
                }
            } else {
                let allowed_session_ids = allowed_session_ids.clone();
                let page = self
                    .db
                    .block_on(async move {
                        crate::backend::store::search_conversation_cards_sqlx(
                            &pool,
                            &tenant_id,
                            record_kind,
                            adapter_id.as_deref(),
                            source_id.as_deref(),
                            search_project_path.as_deref(),
                            &search_query,
                            &scan_content_types,
                            &semantic_roles,
                            include_questions,
                            include_cards,
                            since.as_deref(),
                            until.as_deref(),
                            timeline,
                            limit,
                            offset,
                            allowed_session_ids.as_ref(),
                        )
                        .await
                    })
                    .map_err(AppError::external)?;
                (page, fallback_backend, None, None)
            };
        if incremental_scope.is_some() {
            for hit in &mut page.hits {
                hit.incremental = incremental_match_by_session
                    .get(&hit.session.session.id)
                    .cloned();
            }
        }
        Ok(ConversationSearchResult {
            query: query.to_string(),
            record_kind: record_kind_label.clone(),
            scope: ConversationSearchScope {
                record_kind: record_kind_label,
                adapter_id: params.adapter_id,
                source_id: params.source_id,
                project_path,
                query: query.to_string(),
                content_types: params.content_types,
                card_kinds: params.card_kinds,
                semantic_roles: params.semantic_roles,
                include_questions,
                include_cards,
                since: params.since,
                until: params.until,
                timeline: params.timeline,
                limit,
                offset,
            },
            total_count: page.total_count,
            hits: page.hits,
            backend: if direct_id_query {
                "id_lookup".to_string()
            } else {
                backend.to_string()
            },
            incremental: incremental_scope,
            content_type_counts,
            semantic_role_counts,
        })
    }

    pub(crate) fn export_conversation_session(
        &self,
        params: ConversationSessionExportParams,
    ) -> AppResult<Value> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let input_session_id = params.session_id.clone();
        let (detail, adapter, source) = self.db.block_on(async move {
            let session_id = crate::backend::store::resolve_conversation_session_id_prefix_sqlx(
                &pool,
                &tenant_id,
                &input_session_id,
            )
            .await
            .map_err(AppError::external)?;
            let detail = crate::backend::store::load_conversation_session_detail_sqlx(
                &pool,
                &tenant_id,
                &session_id,
            )
            .await
            .map_err(AppError::external)?;
            let adapter = load_export_adapter_for_detail(&pool, &tenant_id, &detail).await?;
            let source = load_export_source_for_detail(&pool, &tenant_id, &detail).await?;
            AppResult::Ok((detail, adapter, source))
        })?;
        if matches!(params.format, ConversationExportFormat::Rendered) {
            self.ensure_conversation_adapter_package_runtime_ready(&adapter)?;
        }
        let settings =
            crate::backend::app_settings::read_app_settings_value_for_database(&self.db)?;
        export_loaded_conversation_markdown(
            detail,
            adapter,
            source,
            params,
            "session",
            "unknown-project",
            &settings,
        )
    }

    pub(crate) fn export_web_record_session(
        &self,
        params: ConversationSessionExportParams,
    ) -> AppResult<Value> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let input_session_id = params.session_id.clone();
        let (detail, adapter, source) = self.db.block_on(async move {
            let session_id = crate::backend::store::resolve_web_record_session_id_prefix_sqlx(
                &pool,
                &tenant_id,
                &input_session_id,
            )
            .await?;
            let detail = crate::backend::store::load_web_record_session_detail_sqlx(
                &pool,
                &tenant_id,
                &session_id,
            )
            .await?;
            let adapter = load_export_adapter_for_detail(&pool, &tenant_id, &detail).await?;
            let source = load_export_source_for_detail(&pool, &tenant_id, &detail).await?;
            AppResult::Ok((detail, adapter, source))
        })?;
        if matches!(params.format, ConversationExportFormat::Rendered) {
            self.ensure_conversation_adapter_package_runtime_ready(&adapter)?;
        }
        let settings =
            crate::backend::app_settings::read_app_settings_value_for_database(&self.db)?;
        export_loaded_conversation_markdown(
            detail, adapter, source, params, "web", "web", &settings,
        )
    }

    pub(crate) fn list_conversation_questions(
        &self,
        params: ConversationQuestionListParams,
    ) -> AppResult<Vec<crate::backend::dto::ConversationQuestionDetail>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let input_session_id = params.session_id;
        let query = params.query;
        let limit = params.limit.unwrap_or(100).clamp(1, 500);
        let offset = params.offset.unwrap_or(0);
        Ok(self
            .db
            .block_on(async move {
                let session_id =
                    crate::backend::store::resolve_conversation_session_id_prefix_sqlx(
                        &pool,
                        &tenant_id,
                        &input_session_id,
                    )
                    .await?;
                crate::backend::store::list_conversation_question_details_sqlx(
                    &pool,
                    &tenant_id,
                    &session_id,
                    query.as_deref(),
                    limit,
                    offset,
                )
                .await
            })
            .map_err(AppError::external)?)
    }

    pub(crate) fn get_conversation_question(
        &self,
        params: ConversationQuestionGetParams,
    ) -> AppResult<crate::backend::dto::ConversationQuestionDetail> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        Ok(self
            .db
            .block_on(async move {
                let question_id =
                    crate::backend::store::resolve_conversation_question_id_prefix_sqlx(
                        &pool,
                        &tenant_id,
                        &params.question_id,
                    )
                    .await?;
                crate::backend::store::load_conversation_question_detail_sqlx(
                    &pool,
                    &tenant_id,
                    &question_id,
                )
                .await
            })
            .map_err(AppError::external)?)
    }

    pub(crate) fn list_conversation_blocks(
        &self,
        params: ConversationBlockListParams,
    ) -> AppResult<Vec<crate::backend::dto::ConversationBlockLocator>> {
        let record_kind = conversation_record_kind_from_locator(&params.question_id)?;
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        Ok(self
            .db
            .block_on(async move {
                crate::backend::store::list_conversation_block_locators_sqlx(
                    &pool,
                    &tenant_id,
                    record_kind,
                    &params.question_id,
                )
                .await
            })
            .map_err(AppError::external)?)
    }

    pub(crate) fn get_conversation_block(
        &self,
        params: ConversationBlockGetParams,
    ) -> AppResult<crate::backend::dto::ConversationBlockDetail> {
        let record_kind = conversation_record_kind_from_locator(&params.block_id)?;
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        Ok(self
            .db
            .block_on(async move {
                crate::backend::store::load_conversation_block_detail_sqlx(
                    &pool,
                    &tenant_id,
                    record_kind,
                    &params.block_id,
                )
                .await
            })
            .map_err(AppError::external)?)
    }

    pub(crate) fn merge_conversation_questions(
        &self,
        params: ConversationQuestionMergeParams,
    ) -> AppResult<crate::backend::dto::ConversationMutationResult> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        Ok(self
            .db
            .block_on(async move {
                let mut resolved_question_ids = Vec::with_capacity(params.question_ids.len());
                for q_id in params.question_ids {
                    resolved_question_ids.push(
                        crate::backend::store::resolve_conversation_question_id_prefix_sqlx(
                            &pool, &tenant_id, &q_id,
                        )
                        .await?,
                    );
                }
                crate::backend::store::merge_conversation_questions_sqlx(
                    &pool,
                    &tenant_id,
                    &resolved_question_ids,
                    params.dry_run,
                )
                .await
            })
            .map_err(AppError::external)?)
    }

    pub(crate) fn split_conversation_question(
        &self,
        params: ConversationQuestionSplitParams,
    ) -> AppResult<crate::backend::dto::ConversationMutationResult> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        Ok(self
            .db
            .block_on(async move {
                let question_id =
                    crate::backend::store::resolve_conversation_question_id_prefix_sqlx(
                        &pool,
                        &tenant_id,
                        &params.question_id,
                    )
                    .await?;
                let before_turn_id =
                    crate::backend::store::resolve_conversation_turn_id_prefix_sqlx(
                        &pool,
                        &tenant_id,
                        &params.before_turn_id,
                    )
                    .await?;
                crate::backend::store::split_conversation_question_sqlx(
                    &pool,
                    &tenant_id,
                    &question_id,
                    &before_turn_id,
                    params.dry_run,
                )
                .await
            })
            .map_err(AppError::external)?)
    }

    pub(crate) fn update_conversation_part_translation(
        &self,
        params: ConversationPartTranslationUpdateParams,
    ) -> AppResult<()> {
        let part_id = params.part_id.trim();
        if part_id.is_empty() {
            return Err(AppError::Validation(
                "conversation part id is required".to_string(),
            ));
        }
        if params.translated_text.len() > 200_000 {
            return Err(AppError::Validation(
                "conversation part translation is too large".to_string(),
            ));
        }

        let (_, record_kind) = normalize_conversation_record_kind(params.record_kind.as_deref())?;
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let translated_text = params.translated_text;
        Ok(self.db.block_on(async move {
            match record_kind {
                crate::backend::dto::ConversationRecordKind::Session => {
                    let part_id = crate::backend::store::resolve_conversation_part_id_prefix_sqlx(
                        &pool,
                        &tenant_id,
                        &params.part_id,
                    )
                    .await
                    .map_err(AppError::external)?;
                    crate::backend::store::update_conversation_part_translation_sqlx(
                        &pool,
                        &tenant_id,
                        &part_id,
                        &translated_text,
                    )
                    .await
                    .map_err(|error| error)
                }
                crate::backend::dto::ConversationRecordKind::Web => {
                    let part_id = crate::backend::store::resolve_web_record_part_id_prefix_sqlx(
                        &pool,
                        &tenant_id,
                        &params.part_id,
                    )
                    .await?;
                    crate::backend::store::update_web_record_part_translation_sqlx(
                        &pool,
                        &tenant_id,
                        &part_id,
                        &translated_text,
                    )
                    .await
                }
            }
        })?)
    }
}

async fn load_export_adapter_for_detail(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    detail: &crate::backend::dto::ConversationSessionDetail,
) -> AppResult<ConversationAdapter> {
    crate::backend::store::load_conversation_adapter_sqlx(
        pool,
        tenant_id,
        &detail.session.adapter_id,
    )
    .await
    .map_err(AppError::external)?
    .ok_or_else(|| {
        AppError::NotFound(format!(
            "conversation adapter not found: {}",
            detail.session.adapter_id
        ))
    })
}

async fn load_export_source_for_detail(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    detail: &crate::backend::dto::ConversationSessionDetail,
) -> AppResult<ConversationSource> {
    crate::backend::store::load_conversation_source_sqlx(pool, tenant_id, &detail.session.source_id)
        .await
        .map_err(AppError::external)?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "conversation source not found: {}",
                detail.session.source_id
            ))
        })
}

fn export_loaded_conversation_markdown(
    detail: crate::backend::dto::ConversationSessionDetail,
    adapter: ConversationAdapter,
    source: ConversationSource,
    params: ConversationSessionExportParams,
    record_kind: &str,
    fallback_project_segment: &str,
    settings: &Value,
) -> AppResult<Value> {
    validate_export_question_ids(&detail, &params.question_ids)?;
    let output_root = crate::backend::path_utils::expand_path(&params.output_root)?;
    let default_relative_path = default_export_relative_path(
        &detail,
        &params.question_ids,
        fallback_project_segment,
        params.format,
    );
    let default_relative_path_text = relative_path_text(&default_relative_path);
    let use_legacy_adapter_exporter = matches!(params.format, ConversationExportFormat::Rendered)
        && adapter.card_contract_version != Some(1)
        && adapter
            .capabilities
            .iter()
            .any(|capability| capability == "export_markdown");
    let (content, relative_path_text) = match params.format {
        ConversationExportFormat::Raw => (
            export_conversation_raw_json(&detail, &source, &params.question_ids, record_kind)?,
            default_relative_path_text,
        ),
        ConversationExportFormat::Rendered if use_legacy_adapter_exporter => {
            let export =
                crate::backend::conversations::export_external_adapter_markdown_with_settings(
                    &adapter,
                    &source,
                    &detail,
                    &params.question_ids,
                    &params.content_filter,
                    record_kind,
                    &default_relative_path_text,
                    settings,
                )
                .map_err(AppError::external)?;
            (export.content, export.relative_path)
        }
        ConversationExportFormat::Rendered => (
            export_conversation_rendered_markdown(
                &detail,
                &params.question_ids,
                &params.content_filter,
            ),
            default_relative_path_text,
        ),
    };
    let relative_path = validate_export_relative_path(&relative_path_text)?;
    let target_path = output_root.join(&relative_path);
    let question_count = params.question_ids.len();
    if params.dry_run {
        record_conversation_export_observation(
            &adapter.id,
            record_kind,
            true,
            params.format,
            use_legacy_adapter_exporter,
        );
        return Ok(json!({
            "dry_run": true,
            "written": false,
            "path": target_path,
            "bytes": content.len(),
            "question_ids": params.question_ids,
            "question_count": question_count,
            "format": export_format_label(params.format),
            "legacy_adapter_exporter_used": use_legacy_adapter_exporter
        }));
    }
    write_export_content(&output_root, &relative_path, &content)?;
    record_conversation_export_observation(
        &adapter.id,
        record_kind,
        false,
        params.format,
        use_legacy_adapter_exporter,
    );
    Ok(json!({
        "dry_run": false,
        "written": true,
        "path": target_path,
        "bytes": content.len(),
        "question_ids": params.question_ids,
        "question_count": question_count,
        "format": export_format_label(params.format),
        "legacy_adapter_exporter_used": use_legacy_adapter_exporter
    }))
}

fn record_conversation_export_observation(
    adapter_id: &str,
    record_kind: &str,
    dry_run: bool,
    format: ConversationExportFormat,
    legacy_adapter_exporter_used: bool,
) {
    crate::backend::logs::record_info(
        "conversation.export",
        "Conversation export completed",
        &[
            ("adapter_id", adapter_id.to_string()),
            ("record_kind", record_kind.to_string()),
            ("dry_run", dry_run.to_string()),
            ("format", export_format_label(format).to_string()),
            (
                "legacy_adapter_exporter_used",
                legacy_adapter_exporter_used.to_string(),
            ),
        ],
    );
}

fn export_conversation_rendered_markdown(
    detail: &crate::backend::dto::ConversationSessionDetail,
    question_ids: &[String],
    content_filter: &crate::backend::dto::ConversationExportContentFilter,
) -> String {
    let selected = question_ids.iter().collect::<BTreeSet<_>>();
    let mut output = format!("# {}\n", detail.session.title.trim());
    for (question_index, question) in detail.questions.iter().enumerate() {
        if !selected.is_empty() && !selected.contains(&question.question.id) {
            continue;
        }
        let prompt = question
            .turns
            .iter()
            .map(|turn| turn.user_text.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        output.push_str(&format!(
            "\n## {}. {}\n\n{}\n",
            question_index + 1,
            export_question_title(question),
            prompt,
        ));
        for node in &question.projected_content_nodes {
            if !content_filter.is_visible_node(&node.node_type, node.semantic_role.as_deref()) {
                continue;
            }
            output.push_str(&format!(
                "\n### {}\n\n",
                humanize_card_kind(&node.node_type)
            ));
            match node.renderer {
                crate::backend::dto::ConversationCardRenderer::Markdown => {
                    output.push_str(node.content.trim());
                    output.push('\n');
                }
                crate::backend::dto::ConversationCardRenderer::Code => {
                    output.push_str(&format!(
                        "```{}\n{}\n```\n",
                        node.language.as_deref().unwrap_or(""),
                        node.content.trim_end()
                    ));
                }
                crate::backend::dto::ConversationCardRenderer::Json => {
                    output.push_str(&format!("```json\n{}\n```\n", node.content.trim_end()));
                }
                crate::backend::dto::ConversationCardRenderer::Command => {
                    output.push_str(&format!("```sh\n{}\n```\n", node.content.trim_end()));
                }
                crate::backend::dto::ConversationCardRenderer::Plain
                | crate::backend::dto::ConversationCardRenderer::Path
                | crate::backend::dto::ConversationCardRenderer::TerminalOutput
                | crate::backend::dto::ConversationCardRenderer::Diff => {
                    output.push_str(&format!("```text\n{}\n```\n", node.content.trim_end()));
                }
            }
        }
    }
    output
}

#[derive(Debug, Serialize)]
struct ConversationRawQuestion {
    id: String,
    session_id: String,
    title: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
struct ConversationRawExport<'a> {
    schema_version: u32,
    format: &'static str,
    record_kind: &'a str,
    source: &'a ConversationSource,
    session: &'a crate::backend::models::ConversationSession,
    questions: Vec<ConversationRawQuestion>,
    question_turns: Vec<crate::backend::models::ConversationQuestionTurn>,
    turns: Vec<crate::backend::models::ConversationTurn>,
    parts: Vec<crate::backend::models::ConversationPart>,
}

fn export_conversation_raw_json(
    detail: &crate::backend::dto::ConversationSessionDetail,
    source: &ConversationSource,
    question_ids: &[String],
    record_kind: &str,
) -> AppResult<String> {
    let selected = question_ids.iter().collect::<BTreeSet<_>>();
    let selected_questions = detail
        .questions
        .iter()
        .filter(|question| selected.is_empty() || selected.contains(&question.question.id))
        .collect::<Vec<_>>();
    let questions = selected_questions
        .iter()
        .map(|question| ConversationRawQuestion {
            id: question.question.id.clone(),
            session_id: question.question.session_id.clone(),
            title: question.question.title.clone(),
            created_at: question.question.created_at.clone(),
            updated_at: question.question.updated_at.clone(),
        })
        .collect();
    let question_turns = detail
        .questions
        .iter()
        .filter(|question| selected.is_empty() || selected.contains(&question.question.id))
        .flat_map(|question| question.question_turns.iter().cloned())
        .collect();
    let turns = detail
        .questions
        .iter()
        .filter(|question| selected.is_empty() || selected.contains(&question.question.id))
        .flat_map(|question| question.turns.iter().cloned())
        .collect();
    let parts = detail
        .questions
        .iter()
        .filter(|question| selected.is_empty() || selected.contains(&question.question.id))
        .flat_map(|question| question.parts.iter().cloned())
        .collect();
    serde_json::to_string_pretty(&ConversationRawExport {
        schema_version: 1,
        format: "raw",
        record_kind,
        source,
        session: &detail.session,
        questions,
        question_turns,
        turns,
        parts,
    })
    .map_err(AppError::external)
}

fn export_question_title(question: &crate::backend::dto::ConversationQuestionDetail) -> String {
    question
        .question
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            question
                .turns
                .iter()
                .map(|turn| turn.user_text.trim())
                .find(|text| !text.is_empty())
                .map(|text| text.chars().take(96).collect())
        })
        .unwrap_or_else(|| "Question".to_string())
}

fn humanize_card_kind(kind: &str) -> String {
    kind.rsplit('.')
        .next()
        .unwrap_or(kind)
        .replace(['-', '_'], " ")
}

fn validate_export_question_ids(
    detail: &crate::backend::dto::ConversationSessionDetail,
    question_ids: &[String],
) -> AppResult<()> {
    if question_ids.is_empty() {
        return Ok(());
    }
    let available = detail
        .questions
        .iter()
        .map(|question| &question.question.id)
        .collect::<BTreeSet<_>>();
    if let Some(missing_id) = question_ids
        .iter()
        .find(|question_id| !available.contains(question_id))
    {
        return Err(AppError::NotFound(format!(
            "conversation question not found in session: {missing_id}"
        )));
    }
    Ok(())
}

fn default_export_relative_path(
    detail: &crate::backend::dto::ConversationSessionDetail,
    question_ids: &[String],
    fallback_project_segment: &str,
    format: ConversationExportFormat,
) -> PathBuf {
    let project_segment = detail
        .session
        .project_path
        .as_deref()
        .and_then(|path| Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .unwrap_or(fallback_project_segment);
    let short_id = detail
        .session
        .id
        .chars()
        .rev()
        .take(8)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    let question_count = question_ids.len();
    let file_stem = if question_count == 0 {
        sanitize_path_segment(&detail.session.title)
    } else {
        format!(
            "{}-selected-{question_count}",
            sanitize_path_segment(&detail.session.title)
        )
    };
    let extension = match format {
        ConversationExportFormat::Rendered => "md",
        ConversationExportFormat::Raw => "json",
    };
    PathBuf::from(sanitize_path_segment(&detail.session.adapter_id))
        .join(sanitize_path_segment(project_segment))
        .join(format!("{file_stem}-{short_id}.{extension}"))
}

fn export_format_label(format: ConversationExportFormat) -> &'static str {
    match format {
        ConversationExportFormat::Rendered => "rendered",
        ConversationExportFormat::Raw => "raw",
    }
}

fn relative_path_text(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::Normal(segment) => Some(segment.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn validate_export_relative_path(value: &str) -> AppResult<PathBuf> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::Validation(
            "markdown_export relative_path is required".to_string(),
        ));
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(AppError::Validation(
            "markdown_export relative_path must be relative".to_string(),
        ));
    }
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(segment) => relative.push(segment),
            _ => {
                return Err(AppError::Validation(
                    "markdown_export relative_path cannot contain root, prefix, '.', or '..'"
                        .to_string(),
                ))
            }
        }
    }
    if relative.as_os_str().is_empty() {
        return Err(AppError::Validation(
            "markdown_export relative_path is required".to_string(),
        ));
    }
    Ok(relative)
}

fn write_export_content(output_root: &Path, relative_path: &Path, content: &str) -> AppResult<()> {
    fs::create_dir_all(output_root)?;
    let relative_parent = relative_path.parent().unwrap_or_else(|| Path::new(""));
    let parent = prepare_export_parent(output_root, relative_parent)?;
    let target_path = output_root.join(relative_path);
    if let Ok(metadata) = fs::symlink_metadata(&target_path) {
        if metadata.file_type().is_symlink() {
            return Err(AppError::Conflict(format!(
                "markdown_export relative_path points to a symlink: {}",
                relative_path.display()
            )));
        }
        if metadata.is_dir() {
            return Err(AppError::Conflict(format!(
                "markdown_export relative_path points to a directory: {}",
                relative_path.display()
            )));
        }
    }
    ensure_export_parent_stays_in_root(output_root, &parent)?;
    fs::write(&target_path, content).map_err(AppError::from)
}

fn prepare_export_parent(output_root: &Path, relative_parent: &Path) -> AppResult<PathBuf> {
    let mut current = output_root.to_path_buf();
    for component in relative_parent.components() {
        let std::path::Component::Normal(segment) = component else {
            return Err(AppError::Validation(
                "markdown_export relative_path cannot contain root, prefix, '.', or '..'"
                    .to_string(),
            ));
        };
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(AppError::Conflict(format!(
                    "markdown_export relative_path cannot traverse symlink: {}",
                    current.display()
                )));
            }
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return Err(AppError::Conflict(format!(
                    "markdown_export relative_path parent is not a directory: {}",
                    current.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current)?;
            }
            Err(error) => return Err(AppError::from(error)),
        }
    }
    Ok(current)
}

fn ensure_export_parent_stays_in_root(output_root: &Path, parent: &Path) -> AppResult<()> {
    let canonical_root = output_root.canonicalize()?;
    let canonical_parent = parent.canonicalize()?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(AppError::Conflict(
            "markdown_export relative_path resolves outside output_root".to_string(),
        ));
    }
    Ok(())
}

fn normalize_conversation_record_kind(
    record_kind: Option<&str>,
) -> AppResult<(String, crate::backend::dto::ConversationRecordKind)> {
    let record_kind = record_kind
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("session");
    match record_kind {
        "session" | "sessions" => Ok((
            "session".to_string(),
            crate::backend::dto::ConversationRecordKind::Session,
        )),
        "web" | "web-record" | "web_record" | "web-records" | "web_records" => Ok((
            "web".to_string(),
            crate::backend::dto::ConversationRecordKind::Web,
        )),
        other => Err(AppError::Validation(format!(
            "unsupported conversation record kind: {other}"
        ))),
    }
}

fn conversation_record_kind_from_locator(
    locator: &str,
) -> AppResult<crate::backend::dto::ConversationRecordKind> {
    let locator = locator.trim();
    if locator.starts_with("web-record-question-")
        || locator.starts_with("web-record-turn-")
        || locator.starts_with("web-record-part-")
    {
        return Ok(crate::backend::dto::ConversationRecordKind::Web);
    }
    if locator.starts_with("conversation-question-")
        || locator.starts_with("conversation-turn-")
        || locator.starts_with("conversation-part-")
    {
        return Ok(crate::backend::dto::ConversationRecordKind::Session);
    }
    Err(AppError::Validation(format!(
        "conversation locator must use a full conversation-* or web-record-* identifier: {locator}"
    )))
}

fn sanitize_path_segment(value: &str) -> String {
    let mut segment = String::new();
    let mut last_was_separator = false;
    for character in value.trim().chars() {
        if character.is_alphanumeric() || matches!(character, '_' | '.') {
            segment.push(character);
            last_was_separator = false;
        } else if !last_was_separator && !segment.is_empty() {
            segment.push('-');
            last_was_separator = true;
        }
        if segment.chars().count() >= 96 {
            break;
        }
    }
    let segment = segment.trim_matches(['-', '.']).to_string();
    if segment.is_empty() {
        "untitled".to_string()
    } else {
        segment
    }
}
