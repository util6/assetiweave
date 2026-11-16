use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
impl AppService {
    pub async fn search_recent_incremental_conversation_records(
        &self,
        params: ConversationIncrementalSearchParams,
    ) -> AppResult<ConversationSearchResult> {
        let recent_runs = params.recent_runs.unwrap_or(3).clamp(1, 20);
        self.search_conversation_records_with_recent_deltas(
            params.into_search_params(),
            Some(recent_runs),
        )
        .await
    }

    pub async fn search_conversation_records_with_recent_deltas(
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
        let direct_id_query = crate::backend::domain::conversation_id_search_term(query).is_some();
        if let Some(mode) = params
            .search_options
            .as_ref()
            .and_then(|options| options.retrieval_mode)
        {
            if mode != crate::backend::domain::conversations::SearchRetrievalMode::Lexical {
                return Err(AppError::Validation(format!(
                    "conversation search retrieval mode {} is not supported; supported modes: lexical",
                    match mode {
                        crate::backend::domain::conversations::SearchRetrievalMode::Lexical =>
                            "lexical",
                        crate::backend::domain::conversations::SearchRetrievalMode::Semantic =>
                            "semantic",
                        crate::backend::domain::conversations::SearchRetrievalMode::Hybrid =>
                            "hybrid",
                    }
                )));
            }
        }
        let (record_kind_label, record_kind) =
            normalize_conversation_record_kind(params.record_kind.as_deref())?;
        let limit = params.limit.unwrap_or(50).clamp(1, 500);
        let offset = params.offset.unwrap_or(0);
        let pool = self.pool();
        let tenant_id = self.tenant_id();
        let adapter_id = params.adapter_id.clone();
        let source_id = params.source_id.clone();
        let (allowed_session_ids, incremental_match_by_session, incremental_scope) =
            if let Some(recent_runs) = recent_run_limit {
                let recent_sessions: Vec<(String, String, String)> = sqlx::query_as(
                    r#"
                    SELECT id, created_at, imported_at
                    FROM conversation_sessions
                    WHERE tenant_id = ?1
                      AND (?2 IS NULL OR adapter_id = ?2)
                      AND (?3 IS NULL OR source_id = ?3)
                    ORDER BY imported_at DESC
                    "#,
                )
                .bind(tenant_id)
                .bind(adapter_id.as_deref())
                .bind(source_id.as_deref())
                .fetch_all(pool)
                .await
                .map_err(AppError::external)?;

                let mut distinct_run_timestamps = Vec::new();
                for (_, _, imported_at) in &recent_sessions {
                    if !distinct_run_timestamps.contains(imported_at) {
                        distinct_run_timestamps.push(imported_at.clone());
                        if distinct_run_timestamps.len() >= recent_runs {
                            break;
                        }
                    }
                }

                let mut allowed = BTreeSet::new();
                let mut matches = BTreeMap::new();
                for (id, created_at, imported_at) in &recent_sessions {
                    if distinct_run_timestamps.contains(imported_at) {
                        allowed.insert(id.clone());
                        let change_kind = if created_at == imported_at {
                            "new".to_string()
                        } else {
                            "updated".to_string()
                        };
                        matches.insert(
                        id.clone(),
                        crate::backend::domain::conversations::ConversationSearchIncrementalMatch {
                            sync_run_id: format!("run-{}", imported_at),
                            change_kind,
                            observed_at: imported_at.clone(),
                        },
                    );
                    }
                }

                let scope = ConversationSearchIncrementalScope {
                    recent_runs,
                    included_run_count: distinct_run_timestamps.len(),
                    changed_session_count: allowed.len(),
                };

                (Some(allowed), matches, Some(scope))
            } else {
                (None, BTreeMap::new(), None)
            };
        let project_path =
            if record_kind == crate::backend::domain::conversations::ConversationRecordKind::Web {
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
            let kind = crate::backend::domain::conversations::ConversationSearchCardType::new(kind);
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
        let indexed_page = if allowed_session_ids.is_none()
            && since.is_none()
            && until.is_none()
            && !timeline
        {
            crate::backend::application::conversations::conversation_search::search_ready_conversation_index(
                pool,
                &self.db_path,
                tenant_id,
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
            .await
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
                match crate::backend::store::hydrate_conversation_search_matches_sqlx(
                    pool,
                    tenant_id,
                    record_kind,
                    adapter_id.as_deref(),
                    source_id.as_deref(),
                    &search_query,
                    matches,
                )
                .await
                {
                    Ok(page) => (page, "tantivy", Some(facet_counts), Some(semantic_counts)),
                    Err(_) => {
                        let page = crate::backend::store::search_conversation_cards_sqlx(
                            pool,
                            tenant_id,
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
                        .map_err(AppError::external)?;
                        (page, "legacy_scan", None, None)
                    }
                }
            } else {
                let allowed_session_ids = allowed_session_ids.clone();
                let page = crate::backend::store::search_conversation_cards_sqlx(
                    pool,
                    tenant_id,
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

    pub fn start_conversation_content_search_task(
        &self,
        params: ConversationSearchParams,
    ) -> AppResult<crate::backend::infrastructure::tasks::TaskHandle<ConversationSearchTaskSummary>>
    {
        use crate::backend::infrastructure::error::InfraError;
        use crate::backend::infrastructure::tasks;

        let trimmed_query = params.query.trim().to_string();
        if trimmed_query.is_empty() {
            return Err(AppError::Validation(
                "conversation search query is required".to_string(),
            ));
        }
        if trimmed_query.chars().count() > 512 {
            return Err(AppError::Validation(
                "conversation search query must not exceed 512 characters".to_string(),
            ));
        }
        let (record_kind_label, _) =
            normalize_conversation_record_kind(params.record_kind.as_deref())?;

        let pipeline = tasks::PipelineDescriptor::builder()
            .stage(tasks::stages::PREPARE, "校验参数与检查全文索引")
            .stage(tasks::stages::SCAN_FILES, "检索对话内容与水合命中卡片")
            .stage(tasks::stages::FINALIZE, "缓存搜索结果并生成摘要")
            .build();

        let spec = tasks::TaskSpec::new(tasks::TaskKind::ConversationContentSearch, None)
            .with_tenant_id(self.tenant_id())
            .with_pipeline(pipeline)
            .with_title("搜索对话内容");

        let service = self.clone();
        self.spawn_pipeline_task(spec, move |cx| async move {
            let task_id = cx.progress.task_id.clone();

            {
                let stage = cx.enter_stage(tasks::stages::PREPARE);
                stage.progress(
                    0,
                    Some(1),
                    Some(format!("准备检索「{}」({})", trimmed_query, record_kind_label)),
                );
                if cx.is_cancelled() {
                    return Err(InfraError::external("cancelled".to_string()));
                }
                let _ = service.get_conversation_search_index_status().await;
                stage.progress(1, Some(1), Some("参数与索引检查完成".to_string()));
            }

            if cx.is_cancelled() {
                return Err(InfraError::external("cancelled".to_string()));
            }

            let result = {
                let mut stage = cx.enter_stage(tasks::stages::SCAN_FILES);
                stage.progress(
                    0,
                    None,
                    Some(format!("正在检索「{}」…", trimmed_query)),
                );
                let cancellation = cx.cancellation();
                let search_fut = service.search_conversation_records(params);
                let search_result = tokio::select! {
                    biased;
                    _ = cancellation.cancelled() => {
                        return Err(InfraError::external("cancelled".to_string()));
                    }
                    res = search_fut => res.map_err(|error| InfraError::external(error.to_string()))?,
                };
                stage.record_metric(
                    "total_count",
                    u64::try_from(search_result.total_count).unwrap_or(0),
                );
                stage.record_metric(
                    "hit_count",
                    u64::try_from(search_result.hits.len()).unwrap_or(0),
                );
                stage.progress(
                    1,
                    Some(1),
                    Some(format!("命中 {} 个结果", search_result.total_count)),
                );
                search_result
            };

            if cx.is_cancelled() {
                return Err(InfraError::external("cancelled".to_string()));
            }

            let (summary_payload, summary_text) = {
                let stage = cx.enter_stage(tasks::stages::FINALIZE);
                let payload = ConversationSearchTaskSummary {
                    query: result.query.clone(),
                    record_kind: result.record_kind.clone(),
                    total_count: result.total_count,
                    hit_count: result.hits.len(),
                };
                let summary_text = if result.total_count > 0 {
                    format!("「{}」共找到 {} 个结果", result.query, result.total_count)
                } else {
                    format!("未找到与「{}」匹配的对话内容", result.query)
                };
                service
                    .runtime
                    .store_conversation_search_result(&task_id, result);
                stage.progress(1, Some(1), Some(summary_text.clone()));
                (payload, summary_text)
            };

            Ok(tasks::TaskOutput::with_summary(summary_payload, summary_text))
        })
    }

    pub fn get_conversation_content_search_task_result(
        &self,
        params: ConversationSearchTaskResultGetParams,
    ) -> AppResult<Option<ConversationSearchResult>> {
        let task_id = params.task_id.trim();
        if task_id.is_empty() {
            return Err(AppError::Validation("task_id is required".to_string()));
        }
        Ok(self.runtime.get_conversation_search_result(task_id))
    }
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
