use super::prelude::*;
use crate::backend::models::ConversationPart;
use crate::backend::runtime::{AppError, AppResult};
use sha2::{Digest, Sha256};

const RECALL_EXACT_MAX_QUESTIONS: usize = 24;
const RECALL_FULL_PAGE_MAX: usize = 200;
const RECALL_MAX_EVIDENCE: usize = 512;
const RECALL_MAX_INPUT_CHARS: usize = 240_000;
const RECALL_SEARCH_MAX_CORPUS: usize = 512;
const RECALL_SEARCH_MAX_LIMIT: usize = 128;
const RECALL_LEXICAL_WEIGHT: u64 = 100;

impl AppService {
    pub(crate) fn search_memory_recall(
        &self,
        params: MemoryRecallSearchParams,
    ) -> AppResult<MemoryRecallSearchResult> {
        validate_recall_search_params(&params)?;
        let query = params.query.trim().to_string();
        let limit = params.limit.unwrap_or(24).clamp(1, RECALL_SEARCH_MAX_LIMIT);
        let corpus = self.load_recall_search_corpus(&params)?;
        let mut hits = BTreeMap::<String, MemoryRecallSearchHit>::new();
        let mut lexical_backends = BTreeSet::new();

        for (record_kind, label) in [
            (MemoryEvidenceRecordKind::Session, "session"),
            (MemoryEvidenceRecordKind::Web, "web"),
        ] {
            if record_kind == MemoryEvidenceRecordKind::Web && params.scope.project_path.is_some() {
                continue;
            }
            let result = self.search_conversation_records(ConversationSearchParams {
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
            })?;
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
                let Some(document) = self.load_recall_search_document(reference)? else {
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
            .map(
                |document| crate::backend::search::memory_semantic::SemanticDocument {
                    key: recall_question_key(&document.reference),
                    text: document.search_text.clone(),
                },
            )
            .collect::<Vec<_>>();
        let semantic_matches = crate::backend::search::memory_semantic::rank_documents(
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
                .map(
                    |part| crate::backend::search::memory_semantic::SemanticDocument {
                        key: part.block_id.clone(),
                        text: part.content.clone(),
                    },
                )
                .collect::<Vec<_>>();
            let best_part =
                crate::backend::search::memory_semantic::rank_documents(&query, &part_documents, 1)
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

    pub(crate) fn preview_memory_recall(
        &self,
        params: MemoryRecallPreviewParams,
    ) -> AppResult<MemoryRecallPreview> {
        validate_recall_params(&params)?;
        let source_revision = self
            .db
            .block_on(crate::backend::store::load_memory_source_revision_sqlx(
                self.db.pool(),
                self.tenant_id(),
            ))
            .map_err(AppError::external)?;
        let (backend, total, refs) = match params.mode {
            MemoryRecallMode::Exact => self.exact_recall_refs(&params)?,
            MemoryRecallMode::Full => {
                let limit = params.limit.unwrap_or(50).clamp(1, RECALL_FULL_PAGE_MAX);
                let offset = params.offset.unwrap_or(0);
                let (total, refs) = self
                    .db
                    .block_on(
                        crate::backend::store::list_memory_recall_question_refs_sqlx(
                            self.db.pool(),
                            self.tenant_id(),
                            &params.scope,
                            params.since.as_deref(),
                            params.until.as_deref(),
                            params.include_unavailable,
                            limit,
                            offset,
                        ),
                    )
                    .map_err(AppError::external)?;
                ("bounded_sql".to_string(), total, refs)
            }
        };
        let mut evidence = Vec::new();
        let mut questions = Vec::new();
        let mut input_chars = 0usize;
        let mut truncated = false;
        for question_ref in refs {
            let detail = self.load_recall_question(&question_ref)?;
            let mut ids = Vec::new();
            let mut question_chars = 0usize;
            for part in recall_evidence_parts(&detail) {
                if evidence.len() >= RECALL_MAX_EVIDENCE || input_chars >= RECALL_MAX_INPUT_CHARS {
                    truncated = true;
                    break;
                }
                let remaining = RECALL_MAX_INPUT_CHARS.saturating_sub(input_chars);
                let excerpt: String = part.content.chars().take(remaining.min(8_192)).collect();
                if excerpt.is_empty() {
                    continue;
                }
                let reference = format!("evidence-{}", evidence.len());
                let chars = excerpt.chars().count();
                input_chars += chars;
                question_chars += chars;
                ids.push(reference.clone());
                evidence.push(MemoryRecallEvidence {
                    reference,
                    card_type: part.card_type.to_string(),
                    snapshot: NewMemoryEvidenceSnapshot {
                        record_kind: question_ref.record_kind,
                        source_id: Some(question_ref.source_id.clone()),
                        session_id: question_ref.session_id.clone(),
                        question_id: Some(question_ref.question_id.clone()),
                        turn_id: part.turn_id,
                        part_id: part.part_id,
                        block_id: part.block_id,
                        content_hash: format!(
                            "sha256:{:x}",
                            Sha256::digest(part.content.as_bytes())
                        ),
                        excerpt,
                        translated_excerpt: None,
                        event_time: Some(detail.question.created_at.clone()),
                        source_revision,
                        source_unavailable: false,
                    },
                });
            }
            if !ids.is_empty() {
                questions.push(MemoryRecallQuestion {
                    record_kind: question_ref.record_kind,
                    source_id: question_ref.source_id,
                    session_id: question_ref.session_id,
                    session_title: question_ref.session_title,
                    project_path: question_ref.project_path,
                    question_id: question_ref.question_id,
                    question_index: question_ref.question_index,
                    question_title: detail.question.title.clone().unwrap_or_else(|| {
                        detail
                            .turns
                            .iter()
                            .map(|turn| turn.user_text.trim())
                            .find(|text| !text.is_empty())
                            .map(|text| text.chars().take(80).collect())
                            .unwrap_or_else(|| "Question".to_string())
                    }),
                    evidence_ids: ids,
                    input_char_count: question_chars,
                });
            }
            if truncated {
                break;
            }
        }
        let query = params
            .query
            .as_deref()
            .map(|value| value.trim())
            .filter(|v| !v.is_empty());
        let formal_matches = match query {
            Some(query) => self.recall_formal_matches(query, &params.scope)?,
            None => Vec::new(),
        };
        let dream_matches = match query {
            Some(query) => self.recall_dream_matches(query, &params.scope)?,
            None => Vec::new(),
        };
        Ok(MemoryRecallPreview {
            mode: params.mode,
            scope: params.scope,
            query: query.map(str::to_string),
            backend,
            source_revision,
            total_question_count: total,
            selected_question_count: questions.len(),
            skipped_question_count: total.saturating_sub(questions.len()),
            evidence_count: evidence.len(),
            input_char_count: input_chars,
            truncated: truncated
                || questions.len() < total.min(params.limit.unwrap_or(RECALL_EXACT_MAX_QUESTIONS)),
            include_unavailable: params.include_unavailable,
            questions,
            evidence,
            formal_matches,
            dream_matches,
        })
    }

    pub(crate) fn run_memory_recall(
        &self,
        params: MemoryRecallRunParams,
    ) -> AppResult<MemoryRecallRunResult> {
        self.run_memory_recall_with_control(params, None, |_, _, _, _| {})
    }

    pub(crate) fn run_memory_recall_with_control<F>(
        &self,
        params: MemoryRecallRunParams,
        cancellation: Option<crate::backend::ai_execution::AiExecutionCancellation>,
        mut progress: F,
    ) -> AppResult<MemoryRecallRunResult>
    where
        F: FnMut(&str, usize, usize, Option<&str>),
    {
        progress("context", 0, 0, None);
        let preview = self.preview_memory_recall(params.preview)?;
        if !params.synthesize || params.dry_run {
            return Ok(MemoryRecallRunResult {
                run_id: None,
                preview,
                synthesized: false,
                answer_markdown: None,
                claims: Vec::new(),
                memory_candidates: Vec::new(),
                conflicts: Vec::new(),
                insufficient_evidence: false,
                extractions: Vec::new(),
            });
        }
        self.synthesize_memory_recall(preview, cancellation, &mut progress)
    }

    fn exact_recall_refs(
        &self,
        params: &MemoryRecallPreviewParams,
    ) -> AppResult<(String, usize, Vec<MemoryRecallQuestionRef>)> {
        let query = params
            .query
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                AppError::Validation("exact Memory Recall requires a query".to_string())
            })?;
        let limit = params
            .limit
            .unwrap_or(RECALL_EXACT_MAX_QUESTIONS)
            .clamp(1, RECALL_EXACT_MAX_QUESTIONS);
        let result = self.search_memory_recall(MemoryRecallSearchParams {
            query: query.to_string(),
            scope: params.scope.clone(),
            since: params.since.clone(),
            until: params.until.clone(),
            file: params.file.clone(),
            command: params.command.clone(),
            error: params.error.clone(),
            limit: Some(limit),
            offset: params.offset,
        })?;
        let mut refs = Vec::new();
        for hit in &result.hits {
            if refs.iter().any(|item: &MemoryRecallQuestionRef| {
                item.question_id == hit.question_id && item.record_kind == hit.record_kind
            }) {
                continue;
            }
            refs.push(MemoryRecallQuestionRef {
                record_kind: hit.record_kind,
                source_id: hit.source_id.clone(),
                session_id: hit.session_id.clone(),
                session_title: hit.session_title.clone(),
                project_path: hit.project_path.clone(),
                question_id: hit.question_id.clone(),
                question_index: hit.question_index,
            });
        }
        Ok((result.backend, result.total_count, refs))
    }

    fn load_recall_search_corpus(
        &self,
        params: &MemoryRecallSearchParams,
    ) -> AppResult<BTreeMap<String, RecallSearchDocument>> {
        let (total, references) = self
            .db
            .block_on(
                crate::backend::store::list_memory_recall_question_refs_sqlx(
                    self.db.pool(),
                    self.tenant_id(),
                    &params.scope,
                    params.since.as_deref(),
                    params.until.as_deref(),
                    false,
                    RECALL_SEARCH_MAX_CORPUS,
                    0,
                ),
            )
            .map_err(AppError::external)?;
        let _ = total;
        let mut corpus = BTreeMap::new();
        for reference in references {
            if let Some(document) = self.load_recall_search_document(reference)? {
                if !document_matches_search_hints(&document, params) {
                    continue;
                }
                corpus.insert(recall_question_key(&document.reference), document);
            }
        }
        Ok(corpus)
    }

    fn load_recall_search_document(
        &self,
        reference: MemoryRecallQuestionRef,
    ) -> AppResult<Option<RecallSearchDocument>> {
        let available = match reference.record_kind {
            MemoryEvidenceRecordKind::Session => self.db.block_on(
                sqlx::query_scalar::<_, i64>(
                    "SELECT EXISTS(SELECT 1 FROM conversation_sessions s JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id WHERE s.tenant_id=?1 AND s.id=?2 AND s.missing=0 AND source.enabled=1)",
                )
                .bind(self.tenant_id())
                .bind(&reference.session_id)
                .fetch_one(self.db.pool()),
            ),
            MemoryEvidenceRecordKind::Web => self.db.block_on(
                sqlx::query_scalar::<_, i64>(
                    "SELECT EXISTS(SELECT 1 FROM web_record_sessions s JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id WHERE s.tenant_id=?1 AND s.id=?2 AND s.missing=0 AND source.enabled=1)",
                )
                .bind(self.tenant_id())
                .bind(&reference.session_id)
                .fetch_one(self.db.pool()),
            ),
        }
        .map_err(AppError::Db)?;
        if available != 1 {
            return Ok(None);
        }
        let detail = self.load_recall_question(&reference)?;
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

    fn load_recall_question(
        &self,
        reference: &MemoryRecallQuestionRef,
    ) -> AppResult<crate::backend::dto::ConversationQuestionDetail> {
        match reference.record_kind {
            MemoryEvidenceRecordKind::Session => self.get_conversation_question(
                crate::backend::application::ConversationQuestionGetParams {
                    question_id: reference.question_id.clone(),
                },
            ),
            MemoryEvidenceRecordKind::Web => self
                .get_web_record_session(
                    crate::backend::application::ConversationSessionGetParams {
                        session_id: reference.session_id.clone(),
                    },
                )?
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

    fn recall_formal_matches(
        &self,
        query: &str,
        scope: &MemoryScope,
    ) -> AppResult<Vec<MemoryItem>> {
        let needle = query.to_lowercase();
        Ok(self
            .list_memory_items(MemoryItemListParams {
                statuses: vec![MemoryItemStatus::Active, MemoryItemStatus::Completed],
                scope: (scope != &MemoryScope::default()).then_some(scope.clone()),
                limit: Some(200),
                ..Default::default()
            })?
            .items
            .into_iter()
            .filter(|item| {
                format!("{}\n{}", item.title, item.content_markdown)
                    .to_lowercase()
                    .contains(&needle)
            })
            .take(20)
            .collect())
    }

    fn recall_dream_matches(
        &self,
        query: &str,
        scope: &MemoryScope,
    ) -> AppResult<Vec<crate::backend::models::MemoryDreamNote>> {
        let needle = query.to_lowercase();
        Ok(self
            .list_memory_dream_notes(MemoryDreamListParams {
                statuses: vec![
                    MemoryDreamNoteStatus::Active,
                    MemoryDreamNoteStatus::Promoted,
                    MemoryDreamNoteStatus::Stale,
                ],
                scope: (scope != &MemoryScope::default()).then_some(scope.clone()),
                limit: Some(200),
                offset: Some(0),
            })?
            .items
            .into_iter()
            .filter(|note| note.markdown.to_lowercase().contains(&needle))
            .take(20)
            .collect())
    }
}

#[derive(Debug, Clone)]
struct RecallPart {
    turn_id: Option<String>,
    part_id: Option<String>,
    block_id: String,
    card_type: String,
    content: String,
}

#[derive(Debug, Clone)]
struct RecallSearchDocument {
    reference: MemoryRecallQuestionRef,
    search_text: String,
    parts: Vec<RecallPart>,
    source_parts: Vec<ConversationPart>,
}

fn recall_question_key(reference: &MemoryRecallQuestionRef) -> String {
    format!(
        "{}\0{}\0{}",
        reference.record_kind.as_str(),
        reference.session_id,
        reference.question_id
    )
}

fn recall_locator_key(
    record_kind: MemoryEvidenceRecordKind,
    session_id: &str,
    question_id: &str,
    turn_id: Option<&str>,
    part_id: Option<&str>,
    block_id: &str,
) -> String {
    format!(
        "{}\0{}\0{}\0{}\0{}\0{}",
        record_kind.as_str(),
        session_id,
        question_id,
        turn_id.unwrap_or_default(),
        part_id.unwrap_or_default(),
        block_id
    )
}

fn merge_recall_search_hit(
    hits: &mut BTreeMap<String, MemoryRecallSearchHit>,
    key: String,
    mut candidate: MemoryRecallSearchHit,
) {
    let Some(existing) = hits.get_mut(&key) else {
        hits.insert(key, candidate);
        return;
    };
    existing.lexical_score = existing.lexical_score.max(candidate.lexical_score);
    existing.semantic_score = existing.semantic_score.max(candidate.semantic_score);
    existing.score = existing
        .lexical_score
        .saturating_mul(RECALL_LEXICAL_WEIGHT)
        .saturating_add(existing.semantic_score);
    if candidate.snippet.len() < existing.snippet.len() {
        std::mem::swap(&mut existing.snippet, &mut candidate.snippet);
    }
    existing.sources.extend(candidate.sources);
    existing.sources.sort();
    existing.sources.dedup();
}

fn document_matches_search_hints(
    document: &RecallSearchDocument,
    params: &MemoryRecallSearchParams,
) -> bool {
    let contains_hint = |hint: Option<&String>, values: Vec<String>| {
        hint.map(|value| value.trim())
            .filter(|hint| !hint.is_empty())
            .is_none_or(|hint| {
                values
                    .iter()
                    .any(|value| value.to_lowercase().contains(&hint.to_lowercase()))
            })
    };
    let values = document
        .source_parts
        .iter()
        .flat_map(|part| {
            [
                part.text.clone(),
                part.command.clone(),
                part.cwd.clone(),
                part.command_label.clone(),
                part.metadata_json.clone(),
            ]
            .into_iter()
            .flatten()
        })
        .collect::<Vec<_>>();
    if !contains_hint(params.file.as_ref(), values.clone()) {
        return false;
    }
    if let Some(command) = params
        .command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let command = command.to_lowercase();
        if !document.source_parts.iter().any(|part| {
            part.kind == crate::backend::models::ConversationPartKind::Command
                || part.command.is_some()
        }) || !values
            .iter()
            .any(|value| value.to_lowercase().contains(&command))
        {
            return false;
        }
    }
    if let Some(error) = params
        .error
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let error = error.to_lowercase();
        if !document.source_parts.iter().any(|part| {
            part.exit_code.is_some_and(|code| code != 0)
                || part
                    .status
                    .as_deref()
                    .is_some_and(|status| status.to_lowercase().contains("error"))
                || values
                    .iter()
                    .any(|value| value.to_lowercase().contains(&error))
        }) {
            return false;
        }
    }
    true
}

fn leading_recall_snippet(content: &str) -> String {
    content.chars().take(320).collect()
}

fn recall_evidence_parts(
    detail: &crate::backend::dto::ConversationQuestionDetail,
) -> Vec<RecallPart> {
    let mut result = Vec::new();
    for turn in &detail.turns {
        if !turn.user_text.trim().is_empty() {
            result.push(RecallPart {
                turn_id: Some(turn.id.clone()),
                part_id: None,
                block_id: format!("{}-question", turn.id),
                card_type: "question".to_string(),
                content: turn.user_text.clone(),
            });
        }
    }
    for node in &detail.projected_content_nodes {
        result.push(RecallPart {
            turn_id: Some(node.turn_id.clone()),
            part_id: Some(node.part_id.clone()),
            block_id: node.node_id.clone(),
            card_type: node.node_type.clone(),
            content: node.content.clone(),
        });
    }
    result
}

#[cfg(test)]
pub(crate) fn recall_card_projection_for_test(
    detail: &crate::backend::dto::ConversationQuestionDetail,
) -> Vec<(String, String, String)> {
    recall_evidence_parts(detail)
        .into_iter()
        .filter_map(|part| {
            part.part_id
                .map(|part_id| (part_id, part.card_type, part.content))
        })
        .collect()
}

fn validate_recall_params(params: &MemoryRecallPreviewParams) -> AppResult<()> {
    if let Some(query) = &params.query {
        let count = query.trim().chars().count();
        if count > 512 {
            return Err(AppError::Validation(
                "Memory Recall query must not exceed 512 characters".to_string(),
            ));
        }
    }
    if params.mode == MemoryRecallMode::Exact
        && params
            .query
            .as_ref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err(AppError::Validation(
            "exact Memory Recall requires a query".to_string(),
        ));
    }
    if params.mode == MemoryRecallMode::Full && params.scope == MemoryScope::default() {
        return Err(AppError::Validation(
            "full Memory organize requires an explicit scope".to_string(),
        ));
    }
    Ok(())
}

fn validate_recall_search_params(params: &MemoryRecallSearchParams) -> AppResult<()> {
    let query = params.query.trim();
    if query.is_empty() {
        return Err(AppError::Validation(
            "Memory Recall search requires a query".to_string(),
        ));
    }
    if query.chars().count() > 512 {
        return Err(AppError::Validation(
            "Memory Recall search query must not exceed 512 characters".to_string(),
        ));
    }
    for hint in [
        params.file.as_deref(),
        params.command.as_deref(),
        params.error.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if hint.chars().count() > 512 {
            return Err(AppError::Validation(
                "Memory Recall search hints must not exceed 512 characters".to_string(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::dto::ConversationQuestionDetail;
    use crate::backend::models::{
        ConversationGroupingOrigin, ConversationPart, ConversationPartKind, ConversationPartRole,
        ConversationQuestion, ConversationQuestionTurn, ConversationTurn,
    };

    #[test]
    fn memory_recall_context_requires_query_or_explicit_full_scope() {
        let exact = MemoryRecallPreviewParams {
            mode: MemoryRecallMode::Exact,
            scope: MemoryScope::default(),
            query: None,
            since: None,
            until: None,
            include_unavailable: false,
            limit: None,
            offset: None,
            file: None,
            command: None,
            error: None,
        };
        let error = validate_recall_params(&exact).expect_err("exact query");
        assert!(matches!(error, AppError::Validation(_)));
        assert!(error.to_string().contains("requires a query"));
        let full = MemoryRecallPreviewParams {
            mode: MemoryRecallMode::Full,
            query: None,
            ..exact
        };
        let error = validate_recall_params(&full).expect_err("full scope");
        assert!(matches!(error, AppError::Validation(_)));
        assert!(error.to_string().contains("explicit scope"));
    }

    #[test]
    fn memory_recall_context_preserves_all_six_card_families() {
        let detail = test_question_detail();
        let types = recall_evidence_parts(&detail)
            .into_iter()
            .map(|part| part.card_type)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            types,
            ["answer", "code", "command", "question", "result", "tool"]
                .into_iter()
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
        );
    }

    fn test_question_detail() -> ConversationQuestionDetail {
        let turn = ConversationTurn {
            id: "turn-1".into(),
            session_id: "session-1".into(),
            external_id: "external".into(),
            turn_index: 0,
            user_text: "Why?".into(),
            title: None,
            started_at: None,
            ended_at: None,
            fingerprint: "fp".into(),
            missing: false,
            imported_at: "2026-01-01T00:00:00Z".into(),
        };
        let turn_id = turn.id.clone();
        let part =
            |id: &str, role, kind, text: Option<&str>, command: Option<&str>| ConversationPart {
                id: id.into(),
                turn_id: turn_id.clone(),
                part_index: 0,
                role,
                kind,
                text: text.map(str::to_string),
                language: None,
                command: command.map(str::to_string),
                cwd: None,
                status: None,
                exit_code: None,
                command_label: None,
                source_execution_id: None,
                content_card: None,
                metadata_json: None,
                translated_text: None,
            };
        let question_turns = vec![ConversationQuestionTurn {
            question_id: "q-1".into(),
            turn_id: "turn-1".into(),
            turn_order: 0,
            assignment_origin: ConversationGroupingOrigin::Imported,
            assigned_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }];
        let parts = vec![
            part(
                "answer",
                ConversationPartRole::Assistant,
                ConversationPartKind::Text,
                Some("answer"),
                None,
            ),
            part(
                "result",
                ConversationPartRole::Tool,
                ConversationPartKind::Text,
                Some("result"),
                None,
            ),
            part(
                "tool",
                ConversationPartRole::Assistant,
                ConversationPartKind::Tool,
                Some("tool"),
                None,
            ),
            part(
                "command",
                ConversationPartRole::Assistant,
                ConversationPartKind::Command,
                None,
                Some("pwd"),
            ),
            part(
                "code",
                ConversationPartRole::Assistant,
                ConversationPartKind::CodeBlock,
                Some("let x = 1;"),
                None,
            ),
        ];
        let projected_content_nodes = parts
            .iter()
            .flat_map(|part| {
                let candidate = crate::backend::projection::conversation_content_nodes::ConversationContentNodeCandidate {
                    node_type: part.id.clone(),
                    semantic_role: None,
                    renderer: crate::backend::dto::ConversationCardRenderer::Plain,
                    role: part.role,
                    content: part
                        .text
                        .clone()
                        .or_else(|| part.command.clone())
                        .unwrap_or_default(),
                    language: None,
                    cwd: None,
                    status: None,
                    exit_code: None,
                    source_execution_id: None,
                    command_label: None,
                    translated_content: None,
                    legacy_anchor_ids: Vec::new(),
                };
                crate::backend::projection::conversation_content_nodes::project_content_nodes_for_part(
                    "q-1",
                    0,
                    part,
                    &[candidate],
                )
            })
            .collect();
        ConversationQuestionDetail {
            question: ConversationQuestion {
                id: "q-1".into(),
                session_id: "session-1".into(),
                title: None,
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
            },
            question_turns,
            turns: vec![turn],
            parts,
            projected_content_nodes,
        }
    }
}
