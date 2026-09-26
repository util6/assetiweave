use super::*;

#[derive(Debug, FromRow)]
pub(crate) struct WebRecordSessionListItemRow {
    pub(crate) id: String,
    pub(crate) source_id: String,
    pub(crate) adapter_id: String,
    pub(crate) external_id: String,
    pub(crate) title: String,
    pub(crate) project_path: Option<String>,
    pub(crate) started_at: Option<String>,
    pub(crate) updated_at: Option<String>,
    pub(crate) source_locator: Option<String>,
    pub(crate) source_fingerprint: Option<String>,
    pub(crate) missing: i64,
    pub(crate) created_at: String,
    pub(crate) imported_at: String,
    pub(crate) question_count: i64,
    pub(crate) turn_count: i64,
}

pub(crate) struct QuestionAggregate {
    pub(crate) question_text: String,
    pub(crate) answer_text: String,
    pub(crate) code_text: String,
    pub(crate) command_text: String,
}

#[derive(Debug, FromRow)]
pub(crate) struct ExistingWebRecordSessionRow {
    pub(crate) title: String,
    pub(crate) started_at: Option<String>,
    pub(crate) updated_at: Option<String>,
    pub(crate) source_locator: Option<String>,
    pub(crate) source_fingerprint: Option<String>,
    pub(crate) missing: i64,
}

#[derive(Debug, FromRow)]
pub(crate) struct ExistingWebRecordTurnRow {
    pub(crate) external_id: String,
    pub(crate) fingerprint: String,
    pub(crate) missing: i64,
}

#[derive(Debug, FromRow)]
pub(crate) struct WebRecordPartTranslationRow {
    pub(crate) id: String,
    pub(crate) text: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) translated_text: Option<String>,
}

pub(crate) fn web_record_session_from_normalized(
    source: &ConversationSource,
    normalized: &NormalizedConversationSession,
    now: &str,
) -> ConversationSession {
    ConversationSession {
        id: stable_id("web-record-session", &[&source.id, &normalized.external_id]),
        source_id: source.id.clone(),
        adapter_id: source.adapter_id.clone(),
        external_id: normalized.external_id.clone(),
        title: normalized
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Untitled web conversation")
            .to_string(),
        project_path: None,
        started_at: normalized.started_at.clone(),
        updated_at: normalized.updated_at.clone(),
        source_locator: normalized.source_locator.clone(),
        source_fingerprint: normalized.source_fingerprint.clone(),
        missing: false,
        created_at: now.to_string(),
        imported_at: now.to_string(),
        execution_origin: "user".to_string(),
        execution_purpose: None,
        user_visible: true,
    }
}

pub(crate) fn web_record_turn_from_normalized(
    session_id: &str,
    normalized: &crate::backend::domain::NormalizedConversationTurn,
    now: &str,
) -> ConversationTurn {
    ConversationTurn {
        id: stable_id("web-record-turn", &[session_id, &normalized.external_id]),
        session_id: session_id.to_string(),
        external_id: normalized.external_id.clone(),
        turn_index: normalized.turn_index,
        user_text: normalized.user_text.trim().to_string(),
        title: normalized.title.clone(),
        started_at: normalized.started_at.clone(),
        ended_at: normalized.ended_at.clone(),
        fingerprint: conversation_turn_fingerprint(normalized),
        missing: false,
        imported_at: now.to_string(),
    }
}

pub(crate) fn normalize_query(query: Option<&str>) -> Option<String> {
    query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase)
}

pub(crate) fn first_line(text: &str) -> String {
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("Untitled question");
    let trimmed = line.trim();
    if trimmed.chars().count() > 96 {
        trimmed.chars().take(96).collect()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn stable_id(prefix: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(b"\0");
    }
    format!("{prefix}-{:x}", hasher.finalize())
}
