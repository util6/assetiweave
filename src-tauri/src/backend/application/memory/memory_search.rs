pub(crate) use super::memory_search_matching::*;
use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};

const RECALL_SEARCH_CANDIDATE_LIMIT: usize = 32;
const RECALL_SEARCH_MAX_LIMIT: usize = 128;

impl AppService {
    pub(crate) async fn search_memory_recall(
        &self,
        params: MemoryRecallSearchParams,
    ) -> AppResult<MemoryRecallSearchResult> {
        match tokio::time::timeout(
            std::time::Duration::from_secs(2),
            self.search_memory_recall_inner(params),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(AppError::Timeout(
                "Memory recall search timed out after 2s".to_string(),
            )),
        }
    }

    async fn search_memory_recall_inner(
        &self,
        params: MemoryRecallSearchParams,
    ) -> AppResult<MemoryRecallSearchResult> {
        validate_recall_search_params(&params)?;
        let query = params.query.trim().to_string();
        let limit = params.limit.unwrap_or(24).clamp(1, RECALL_SEARCH_MAX_LIMIT);
        let candidate_limit = limit.max(20).min(RECALL_SEARCH_CANDIDATE_LIMIT);

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
                    limit: Some(candidate_limit),
                    offset: Some(0),
                    search_options: None,
                })
                .await?;

            if result.hits.is_empty() {
                continue;
            }
            lexical_backends.insert(result.backend);

            // Batch validate candidate sessions (missing = 0, source enabled = 1, adapter != assetiweave-memory-recall)
            let session_ids = result
                .hits
                .iter()
                .map(|hit| hit.session.session.id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();

            let valid_session_ids = crate::backend::store::filter_valid_recall_sessions_sqlx(
                self.db.pool(),
                self.tenant_id(),
                record_kind,
                &session_ids,
            )
            .await
            .map_err(AppError::Store)?;

            let candidate_hits: Vec<_> = result
                .hits
                .into_iter()
                .filter(|hit| valid_session_ids.contains(&hit.session.session.id))
                .collect();

            if candidate_hits.is_empty() {
                continue;
            }

            // Batch filter by hints (file, command, error) if present
            let has_hints =
                params.file.is_some() || params.command.is_some() || params.error.is_some();
            let matching_question_ids = if has_hints {
                let candidate_question_ids = candidate_hits
                    .iter()
                    .map(|h| h.question_id.clone())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                let parts = crate::backend::store::load_recall_parts_for_questions_sqlx(
                    self.db.pool(),
                    self.tenant_id(),
                    record_kind,
                    &candidate_question_ids,
                )
                .await
                .map_err(AppError::Store)?;

                let mut parts_by_question =
                    BTreeMap::<String, Vec<crate::backend::store::RecallPartFacts>>::new();
                for part in parts {
                    parts_by_question
                        .entry(part.question_id.clone())
                        .or_default()
                        .push(part);
                }
                let mut matched = BTreeSet::new();
                for (q_id, q_parts) in parts_by_question {
                    if part_facts_match_search_hints(&q_parts, &params) {
                        matched.insert(q_id);
                    }
                }
                Some(matched)
            } else {
                None
            };

            for hit in candidate_hits {
                if let Some(ref matched_ids) = matching_question_ids {
                    if !matched_ids.contains(&hit.question_id) {
                        continue;
                    }
                }

                let key = recall_locator_key(
                    record_kind,
                    &hit.session.session.id,
                    &hit.question_id,
                    hit.turn_id.as_deref(),
                    hit.part_id.as_deref(),
                    &hit.block_id,
                );

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
                        sources: vec!["tantivy_bm25".to_string()],
                    },
                );
            }
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
        let hits = hits.into_iter().skip(offset).take(limit).collect();
        let backend = match lexical_backends.into_iter().collect::<Vec<_>>().as_slice() {
            [] => "tantivy_bm25".to_string(),
            backends => format!("tantivy_bm25({})", backends.join("+")),
        };

        Ok(MemoryRecallSearchResult {
            query,
            backend,
            total_count,
            hits,
        })
    }
}

#[cfg(test)]
#[path = "memory_search_tests.rs"]
mod tests;
