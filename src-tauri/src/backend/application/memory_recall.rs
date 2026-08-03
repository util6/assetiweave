use super::prelude::*;
use sha2::{Digest, Sha256};

const RECALL_EXACT_MAX_QUESTIONS: usize = 24;
const RECALL_FULL_PAGE_MAX: usize = 200;
const RECALL_MAX_EVIDENCE: usize = 512;
const RECALL_MAX_INPUT_CHARS: usize = 240_000;

impl AppService {
    pub(crate) fn preview_memory_recall(
        &self,
        params: MemoryRecallPreviewParams,
    ) -> AppResult<MemoryRecallPreview> {
        validate_recall_params(&params)?;
        let source_revision =
            self.db
                .block_on(crate::backend::store::load_memory_source_revision_sqlx(
                    self.db.pool(),
                    self.tenant_id(),
                ))?;
        let (backend, total, refs) = match params.mode {
            MemoryRecallMode::Exact => self.exact_recall_refs(&params)?,
            MemoryRecallMode::Full => {
                let limit = params.limit.unwrap_or(50).clamp(1, RECALL_FULL_PAGE_MAX);
                let offset = params.offset.unwrap_or(0);
                let (total, refs) = self.db.block_on(
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
                )?;
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
                        detail.question.question_text.chars().take(80).collect()
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
            .map(str::trim)
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
            .ok_or_else(|| "exact Memory Recall requires a query".to_string())?;
        let limit = params
            .limit
            .unwrap_or(RECALL_EXACT_MAX_QUESTIONS)
            .clamp(1, RECALL_EXACT_MAX_QUESTIONS);
        let mut refs = Vec::new();
        let mut backends = BTreeSet::new();
        let mut total = 0usize;
        for (record_kind, label) in [
            (MemoryEvidenceRecordKind::Session, "session"),
            (MemoryEvidenceRecordKind::Web, "web"),
        ] {
            if record_kind == MemoryEvidenceRecordKind::Web && params.scope.project_path.is_some() {
                continue;
            }
            let result = self.search_conversation_records(
                crate::backend::application::ConversationSearchParams {
                    record_kind: Some(label.to_string()),
                    adapter_id: params.scope.app_id.clone(),
                    source_id: params.scope.source_id.clone(),
                    project_path: params.scope.project_path.clone(),
                    query: query.to_string(),
                    content_types: Vec::new(),
                    card_kinds: Vec::new(),
                    semantic_roles: Vec::new(),
                    include_questions: None,
                    include_cards: None,
                    since: params.since.clone(),
                    until: params.until.clone(),
                    timeline: false,
                    limit: Some(limit),
                    offset: params.offset,
                    search_options: None,
                },
            )?;
            total = total.saturating_add(result.total_count);
            backends.insert(result.backend);
            for hit in result.hits {
                if params
                    .scope
                    .session_id
                    .as_ref()
                    .is_some_and(|id| id != &hit.session.session.id)
                {
                    continue;
                }
                if refs.iter().any(|item: &MemoryRecallQuestionRef| {
                    item.question_id == hit.question_id && item.record_kind == record_kind
                }) {
                    continue;
                }
                refs.push(MemoryRecallQuestionRef {
                    record_kind,
                    source_id: hit.session.session.source_id.clone(),
                    session_id: hit.session.session.id.clone(),
                    session_title: hit.session.session.title.clone(),
                    project_path: hit.session.session.project_path.clone(),
                    question_id: hit.question_id,
                    question_index: hit.question_index,
                });
                if refs.len() >= limit {
                    break;
                }
            }
            if refs.len() >= limit {
                break;
            }
        }
        Ok((
            backends.into_iter().collect::<Vec<_>>().join("+"),
            total,
            refs,
        ))
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
                    format!(
                        "web Recall question {} was not found",
                        reference.question_id
                    )
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

struct RecallPart {
    turn_id: Option<String>,
    part_id: Option<String>,
    block_id: String,
    card_type: String,
    content: String,
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
    if result.is_empty() && !detail.question.question_text.trim().is_empty() {
        result.push(RecallPart {
            turn_id: None,
            part_id: None,
            block_id: format!("{}-question", detail.question.id),
            card_type: "question".to_string(),
            content: detail.question.question_text.clone(),
        });
    }
    for card in &detail.cards {
        let turn_id = detail
            .parts
            .iter()
            .find(|part| part.id == card.part_id)
            .map(|part| part.turn_id.clone());
        result.push(RecallPart {
            turn_id,
            part_id: Some(card.part_id.clone()),
            block_id: card.card_id.clone(),
            card_type: card.kind.clone(),
            content: card.body.clone(),
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
            return Err("Memory Recall query must not exceed 512 characters".to_string());
        }
    }
    if params.mode == MemoryRecallMode::Exact
        && params
            .query
            .as_ref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err("exact Memory Recall requires a query".to_string());
    }
    if params.mode == MemoryRecallMode::Full && params.scope == MemoryScope::default() {
        return Err("full Memory organize requires an explicit scope".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::dto::ConversationQuestionDetail;
    use crate::backend::models::{
        ConversationGroupingOrigin, ConversationPart, ConversationPartKind, ConversationPartRole,
        ConversationQuestion, ConversationTurn,
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
        };
        assert!(validate_recall_params(&exact)
            .expect_err("exact query")
            .contains("requires a query"));
        let full = MemoryRecallPreviewParams {
            mode: MemoryRecallMode::Full,
            query: None,
            ..exact
        };
        assert!(validate_recall_params(&full)
            .expect_err("full scope")
            .contains("explicit scope"));
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
                source_execution_id: None,
                content_card: None,
                metadata_json: None,
                translated_text: None,
            };
        let mut detail = ConversationQuestionDetail {
            question: ConversationQuestion {
                id: "q-1".into(),
                session_id: "session-1".into(),
                question_index: 0,
                title: None,
                question_text: "Why?".into(),
                answer_text: String::new(),
                code_text: String::new(),
                command_text: String::new(),
                grouping_origin: ConversationGroupingOrigin::Imported,
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
            },
            turns: vec![turn],
            parts: vec![
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
            ],
            cards: Vec::new(),
            content_nodes: Vec::new(),
        };
        detail.cards = detail
            .parts
            .iter()
            .map(|part| crate::backend::dto::ConversationCard {
                card_id: part.id.clone(),
                part_id: part.id.clone(),
                adapter_id: "fixture".to_string(),
                kind: part.id.clone(),
                semantic_role: None,
                renderer: crate::backend::dto::ConversationCardRenderer::Plain,
                role: part.role,
                body: part
                    .text
                    .clone()
                    .or_else(|| part.command.clone())
                    .unwrap_or_default(),
                language: None,
                cwd: None,
                status: None,
                exit_code: None,
                source_execution_id: None,
                translated_body: None,
                legacy_anchor_ids: Vec::new(),
            })
            .collect();
        detail
    }
}
