use super::memory_recall_workflow::{
    MAX_RECALL_ANSWER_CHARS, MAX_RECALL_QUERY_CHARS, MAX_RECALL_REFERENCES, RECALL_SOURCE_ID,
};
use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::{ConversationPartKind, ConversationPartRole, ConversationSourceKind};
use sha2::{Digest, Sha256};

pub(crate) fn build_recall_prompt(
    session: &MemoryRecallSession,
    turn: &MemoryRecallTurn,
) -> String {
    let history = session
        .turns
        .iter()
        .filter(|item| item.id != turn.id)
        .map(|item| {
            let answer = item
                .structured_output
                .as_ref()
                .map(|output| output.answer.as_str())
                .unwrap_or("");
            format!("prior clue: {}\nprior answer: {answer}", item.user_text)
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        "You are AssetIWeave's Recall Agent. Use only the read-only Recall tools exposed by the host. Never mutate data. Return JSON only with exactly these fields: answer (string), sessionReferences (array), contentReferences (array), followUpSuggestions (array of strings). Do not put internal IDs in answer. Current clue: {}\nScope: {}\n{}",
        turn.user_text,
        serde_json::to_string(&session.scope).unwrap_or_else(|_| "{}".to_string()),
        history
    )
}

pub(crate) async fn parse_and_validate_recall_output(
    service: &AppService,
    tenant_id: &str,
    scope: &MemoryScope,
    raw: &str,
) -> AppResult<MemoryRecallStructuredOutput> {
    let redacted = crate::backend::domain::memory::evidence::redact_memory_text(raw).text;
    let value = strip_json_fence(&redacted);
    let mut output: MemoryRecallStructuredOutput =
        serde_json::from_str(value).map_err(|error| {
            AppError::Validation(format!("Recall output schema is invalid: {error}"))
        })?;
    output.answer = output.answer.trim().to_string();
    if output.answer.is_empty() || output.answer.chars().count() > MAX_RECALL_ANSWER_CHARS {
        return Err(AppError::Validation(
            "Recall answer is empty or too long".to_string(),
        ));
    }
    if output.session_references.len() > MAX_RECALL_REFERENCES
        || output.content_references.len() > MAX_RECALL_REFERENCES
        || output.follow_up_suggestions.len() > MAX_RECALL_REFERENCES
    {
        return Err(AppError::Validation(
            "Recall output contains too many references".to_string(),
        ));
    }
    let mut valid_sessions = Vec::new();
    let mut session_keys = BTreeSet::new();
    for reference in output.session_references {
        let key = format!(
            "{}:{}:{}",
            reference.record_kind.as_str(),
            reference.session_id,
            reference.question_id.as_deref().unwrap_or_default()
        );
        if !session_keys.insert(key) {
            continue;
        }
        if !service
            .recall_session_reference_exists_for_scope(tenant_id, scope, &reference)
            .await?
        {
            return Err(AppError::Validation(
                "Recall output contains an invalid or out-of-scope session reference".to_string(),
            ));
        }
        valid_sessions.push(reference);
    }
    let mut valid_content = Vec::new();
    let mut content_keys = BTreeSet::new();
    for reference in output.content_references {
        let key = format!(
            "{}:{}:{}:{}",
            reference.record_kind.as_str(),
            reference.question_id,
            reference.turn_id.as_deref().unwrap_or_default(),
            reference.block_id
        );
        if !content_keys.insert(key) {
            continue;
        }
        if !service
            .recall_content_reference_exists_for_scope(tenant_id, scope, &reference)
            .await?
        {
            return Err(AppError::Validation(
                "Recall output contains an invalid or out-of-scope content reference".to_string(),
            ));
        }
        valid_content.push(reference);
    }
    let mut suggestions = output
        .follow_up_suggestions
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .take(MAX_RECALL_REFERENCES)
        .collect::<Vec<_>>();
    suggestions.dedup();
    let referenced_ids = valid_sessions
        .iter()
        .flat_map(|reference| {
            std::iter::once(reference.session_id.as_str()).chain(reference.question_id.as_deref())
        })
        .chain(valid_content.iter().flat_map(|reference| {
            std::iter::once(reference.session_id.as_str())
                .chain(std::iter::once(reference.question_id.as_str()))
                .chain(reference.turn_id.as_deref())
                .chain(std::iter::once(reference.block_id.as_str()))
        }));
    for id in referenced_ids {
        if !id.is_empty() && output.answer.contains(id) {
            return Err(AppError::Validation(
                "Recall answer must not contain internal locator IDs".to_string(),
            ));
        }
    }
    output.session_references = valid_sessions;
    output.content_references = valid_content;
    output.follow_up_suggestions = suggestions;
    Ok(output)
}

pub(crate) fn redact_recall_query(query: &str) -> AppResult<String> {
    let query = crate::backend::domain::memory::evidence::redact_memory_text(query).text;
    let query = query.trim();
    if query.is_empty() {
        return Err(AppError::Validation("Recall clue is required".to_string()));
    }
    if query.chars().count() > MAX_RECALL_QUERY_CHARS {
        return Err(AppError::Validation("Recall clue is too long".to_string()));
    }
    Ok(query.to_string())
}

pub(crate) fn normalize_recall_id(value: &str, kind: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 160 || value.contains(['\n', '\r', '\0']) {
        return Err(AppError::Validation(format!("Recall {kind} id is invalid")));
    }
    Ok(value.to_string())
}

pub(crate) fn recall_conversation_session_id(session_id: &str) -> String {
    stable_recall_id("conversation-session", &[RECALL_SOURCE_ID, session_id])
}

pub(crate) fn recall_conversation_turn_id(session_id: &str, turn_id: &str) -> String {
    stable_recall_id("conversation-turn", &[session_id, turn_id])
}

pub(crate) fn stable_recall_id(prefix: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(b"\0");
    }
    format!("{prefix}-{:x}", hasher.finalize())
}

pub(crate) fn fingerprint_turns(
    turns: &[crate::backend::domain::NormalizedConversationTurn],
) -> String {
    let mut hasher = Sha256::new();
    for turn in turns {
        hasher.update(turn.external_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(turn.user_text.as_bytes());
        for part in &turn.parts {
            hasher.update(format!("{:?}:{:?}", part.role, part.kind).as_bytes());
            hasher.update(b"\0");
            hasher.update(part.text.as_deref().unwrap_or_default().as_bytes());
        }
    }
    format!("{:x}", hasher.finalize())
}

pub(crate) fn strip_json_fence(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.starts_with("```") && trimmed.ends_with("```") {
        let body = trimmed
            .trim_start_matches('`')
            .strip_prefix("json")
            .unwrap_or_else(|| trimmed.trim_start_matches('`'));
        return body.trim_end_matches('`').trim();
    }
    trimmed
}
