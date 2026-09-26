pub(crate) use super::memory_search_matching::*;
use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::ConversationPart;

const RECALL_SEARCH_MAX_CORPUS: usize = 512;
const RECALL_SEARCH_MAX_LIMIT: usize = 128;

impl AppService {
    pub(crate) async fn search_memory_recall(
        &self,
        params: MemoryRecallSearchParams,
    ) -> AppResult<MemoryRecallSearchResult> {
        validate_recall_search_params(&params)?;
        let query = params.query.trim().to_string();
        let limit = params.limit.unwrap_or(24).clamp(1, RECALL_SEARCH_MAX_LIMIT);
        let corpus = self.load_recall_search_corpus(&params).await?;
        let mut hits = BTreeMap::<String, MemoryRecallSearchHit>::new();
        let mut lexical_backends = BTreeSet::new();

        for (record_kind, label) in [
            (MemoryRecordKind::Session, "session"),
            (MemoryRecordKind::Web, "web"),
        ] {
            if record_kind == MemoryRecordKind::Web && params.scope.project_path.is_some() {
                continue;
            }
            let result = self
                .search_conversation_records(ConversationSearchParams {
                    record_kind: Some(label.to_string()),
                    adapter_id: params.scope.app_id.clone(),
                    source_id: params.scope.source_id.clone(),
                    project_path: params.scope.project_path.clone(),
                    query: query.clone(),
                    content_types: Vec::new(),
                    card_kinds: Vec::new(),
                    semantic_roles: Vec::new(),
                    include_questions: None,
                    include_cards: None,
                    since: params.since.clone(),
                    until: params.until.clone(),
                    timeline: false,
                    limit: Some(RECALL_SEARCH_MAX_LIMIT),
                    offset: Some(0),
                    search_options: None,
                })
                .await?;
            lexical_backends.insert(result.backend);
            for hit in result.hits {
                let reference = MemoryRecallQuestionRef {
                    record_kind,
                    source_id: hit.session.session.source_id.clone(),
                    session_id: hit.session.session.id.clone(),
                    session_title: hit.session.session.title.clone(),
                    project_path: hit.session.session.project_path.clone(),
                    question_id: hit.question_id.clone(),
                    question_index: hit.question_index,
                };
                let Some(document) = self.load_recall_search_document(reference).await? else {
                    continue;
                };
                let key = recall_locator_key(
                    record_kind,
                    &hit.session.session.id,
                    &hit.question_id,
                    hit.turn_id.as_deref(),
                    hit.part_id.as_deref(),
                    &hit.block_id,
                );
                if !document_matches_search_hints(&document, &params) {
                    continue;
                }
                merge_recall_search_hit(
                    &mut hits,
                    key,
                    MemoryRecallSearchHit {
                        record_kind,
                        source_id: hit.session.session.source_id,
                        session_id: hit.session.session.id,
                        session_title: hit.session.session.title,
                        project_path: hit.session.session.project_path,
                        question_id: hit.question_id,
                        question_index: hit.question_index,
                        turn_id: hit.turn_id,
                        part_id: hit.part_id,
                        block_id: hit.block_id,
                        card_type: hit.card_type.as_str().to_string(),
                        snippet: hit.snippet,
                        lexical_score: hit.score as u64,
                        semantic_score: 0,
                        score: (hit.score as u64).saturating_mul(RECALL_LEXICAL_WEIGHT),
                        sources: vec!["lexical".to_string()],
                    },
                );
            }
        }

        let semantic_documents = corpus
            .values()
            .map(|document| {
                crate::backend::infrastructure::search::memory_semantic::SemanticDocument {
                    key: recall_question_key(&document.reference),
                    text: document.search_text.clone(),
                }
            })
            .collect::<Vec<_>>();
        let semantic_matches =
            crate::backend::infrastructure::search::memory_semantic::rank_documents(
                &query,
                &semantic_documents,
                RECALL_SEARCH_MAX_CORPUS,
            );
        for semantic_match in semantic_matches {
            let Some(document) = corpus.get(&semantic_match.key) else {
                continue;
            };
            let part_documents = document
                .parts
                .iter()
                .map(|part| {
                    crate::backend::infrastructure::search::memory_semantic::SemanticDocument {
                        key: part.block_id.clone(),
                        text: part.content.clone(),
                    }
                })
                .collect::<Vec<_>>();
            let best_part =
                crate::backend::infrastructure::search::memory_semantic::rank_documents(
                    &query,
                    &part_documents,
                    1,
                )
                .into_iter()
                .next()
                .and_then(|matched| {
                    document
                        .parts
                        .iter()
                        .find(|part| part.block_id == matched.key)
                        .map(|part| (part, matched.score))
                });
            let Some((part, part_score)) = best_part else {
                continue;
            };
            let key = recall_locator_key(
                document.reference.record_kind,
                &document.reference.session_id,
                &document.reference.question_id,
                part.turn_id.as_deref(),
                part.part_id.as_deref(),
                &part.block_id,
            );
            merge_recall_search_hit(
                &mut hits,
                key,
                MemoryRecallSearchHit {
                    record_kind: document.reference.record_kind,
                    source_id: document.reference.source_id.clone(),
                    session_id: document.reference.session_id.clone(),
                    session_title: document.reference.session_title.clone(),
                    project_path: document.reference.project_path.clone(),
                    question_id: document.reference.question_id.clone(),
                    question_index: document.reference.question_index,
                    turn_id: part.turn_id.clone(),
                    part_id: part.part_id.clone(),
                    block_id: part.block_id.clone(),
                    card_type: part.card_type.clone(),
                    snippet: leading_recall_snippet(&part.content),
                    lexical_score: 0,
                    semantic_score: part_score.max(semantic_match.score),
                    score: part_score.max(semantic_match.score),
                    sources: vec!["semantic".to_string()],
                },
            );
        }

        let mut hits = hits.into_values().collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.record_kind.as_str().cmp(right.record_kind.as_str()))
                .then_with(|| left.session_id.cmp(&right.session_id))
                .then_with(|| left.question_id.cmp(&right.question_id))
                .then_with(|| left.block_id.cmp(&right.block_id))
        });
        let total_count = hits.len();
        let offset = params.offset.unwrap_or(0);
        hits = hits.into_iter().skip(offset).take(limit).collect();
        let backend = match lexical_backends.into_iter().collect::<Vec<_>>().as_slice() {
            [] => "deterministic_semantic".to_string(),
            backends => format!("hybrid({})+deterministic_semantic", backends.join("+")),
        };
        Ok(MemoryRecallSearchResult {
            query,
            backend,
            total_count,
            hits,
        })
    }

    async fn load_recall_search_corpus(
        &self,
        params: &MemoryRecallSearchParams,
    ) -> AppResult<BTreeMap<String, RecallSearchDocument>> {
        let (total, references) = crate::backend::store::list_memory_recall_question_refs_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &params.scope,
            params.since.as_deref(),
            params.until.as_deref(),
            false,
            RECALL_SEARCH_MAX_CORPUS,
            0,
        )
        .await
        .map_err(AppError::external)?;
        let _ = total;
        let mut corpus = BTreeMap::new();
        for reference in references {
            if let Some(document) = self.load_recall_search_document(reference).await? {
                if !document_matches_search_hints(&document, params) {
                    continue;
                }
                corpus.insert(recall_question_key(&document.reference), document);
            }
        }
        Ok(corpus)
    }

    async fn load_recall_search_document(
        &self,
        reference: MemoryRecallQuestionRef,
    ) -> AppResult<Option<RecallSearchDocument>> {
        let available = match reference.record_kind {
            MemoryRecordKind::Session => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT EXISTS(SELECT 1 FROM conversation_sessions s JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id WHERE s.tenant_id=?1 AND s.id=?2 AND s.missing=0 AND source.enabled=1 AND source.adapter_id <> 'assetiweave-memory-recall')",
                )
                .bind(self.tenant_id())
                .bind(&reference.session_id)
                .fetch_one(self.db.pool())
                .await
            }
            MemoryRecordKind::Web => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT EXISTS(SELECT 1 FROM web_record_sessions s JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id WHERE s.tenant_id=?1 AND s.id=?2 AND s.missing=0 AND source.enabled=1 AND source.adapter_id <> 'assetiweave-memory-recall')",
                )
                .bind(self.tenant_id())
                .bind(&reference.session_id)
                .fetch_one(self.db.pool())
                .await
            }
        }
        .map_err(AppError::Db)?;
        if available != 1 {
            return Ok(None);
        }
        let detail = self.load_recall_question(&reference).await?;
        if detail.turns.iter().all(|turn| turn.missing) {
            return Ok(None);
        }
        let parts = recall_evidence_parts(&detail);
        let search_text = format!(
            "{}\n{}\n{}",
            reference.session_title,
            detail.question.title.as_deref().unwrap_or_default(),
            parts
                .iter()
                .map(|part| part.content.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
        Ok(Some(RecallSearchDocument {
            reference,
            search_text,
            parts,
            source_parts: detail.parts,
        }))
    }

    async fn load_recall_question(
        &self,
        reference: &MemoryRecallQuestionRef,
    ) -> AppResult<crate::backend::domain::conversations::ConversationQuestionDetail> {
        match reference.record_kind {
            MemoryRecordKind::Session => {
                self.get_conversation_question(
                    crate::backend::application::ConversationQuestionGetParams {
                        question_id: reference.question_id.clone(),
                    },
                )
                .await
            }
            MemoryRecordKind::Web => self
                .get_web_record_session(crate::backend::application::ConversationSessionGetParams {
                    session_id: reference.session_id.clone(),
                })
                .await?
                .questions
                .into_iter()
                .find(|detail| detail.question.id == reference.question_id)
                .ok_or_else(|| {
                    AppError::NotFound(format!(
                        "web Recall question {} was not found",
                        reference.question_id
                    ))
                }),
        }
    }
}

#[cfg(test)]
#[path = "memory_search_tests.rs"]
mod tests;
