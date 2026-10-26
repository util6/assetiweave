use std::collections::BTreeMap;

use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};

pub(super) const RECALL_LEXICAL_WEIGHT: u64 = 100;

#[cfg(test)]
#[derive(Debug, Clone)]
pub(super) struct RecallPart {
    pub(super) turn_id: Option<String>,
    pub(super) part_id: Option<String>,
    pub(super) block_id: String,
    pub(super) card_type: String,
    pub(super) content: String,
}

pub(super) fn recall_locator_key(
    record_kind: MemoryRecordKind,
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

pub(super) fn merge_recall_search_hit(
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

pub(super) fn part_facts_match_search_hints(
    parts: &[crate::backend::store::RecallPartFacts],
    params: &MemoryRecallSearchParams,
) -> bool {
    let contains_hint = |hint: Option<&String>, values: &[String]| {
        hint.map(|value| value.trim())
            .filter(|hint| !hint.is_empty())
            .is_none_or(|hint| {
                values
                    .iter()
                    .any(|value| value.to_lowercase().contains(&hint.to_lowercase()))
            })
    };
    let values = parts
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
    if !contains_hint(params.file.as_ref(), &values) {
        return false;
    }
    if let Some(command) = params
        .command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let command = command.to_lowercase();
        if !parts
            .iter()
            .any(|part| part.kind == "command" || part.command.is_some())
            || !values
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
        if !parts.iter().any(|part| {
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

#[cfg(test)]
pub(super) fn recall_evidence_parts(
    detail: &crate::backend::domain::conversations::ConversationQuestionDetail,
) -> Vec<RecallPart> {
    let mut result = Vec::new();
    for turn in &detail.turns {
        if !turn.prompt.trim().is_empty() {
            result.push(RecallPart {
                turn_id: Some(turn.id.clone()),
                part_id: None,
                block_id: format!("{}-question", turn.id),
                card_type: "question".to_string(),
                content: turn.prompt.clone(),
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
pub fn recall_card_projection_for_test(
    detail: &crate::backend::domain::conversations::ConversationQuestionDetail,
) -> Vec<(String, String, String)> {
    recall_evidence_parts(detail)
        .into_iter()
        .filter_map(|part| {
            part.part_id
                .map(|part_id| (part_id, part.card_type, part.content))
        })
        .collect()
}

pub(super) fn validate_recall_search_params(params: &MemoryRecallSearchParams) -> AppResult<()> {
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
