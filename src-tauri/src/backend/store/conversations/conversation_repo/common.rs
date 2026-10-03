use super::*;

pub(crate) const CONVERSATION_IMPORT_BATCH_SIZE: usize = 8;

#[derive(Debug, Clone)]
pub(crate) struct RecentConversationSessionRecord {
    pub(crate) session: ConversationSessionListItem,
    pub(crate) last_activity_at: String,
    pub(crate) cwd: Option<String>,
    pub(crate) source_agent: String,
    pub(crate) recent_events: Vec<crate::backend::domain::RecentMemoryEvent>,
}

pub(super) fn builtin_sources(now: &str) -> Vec<ConversationSource> {
    vec![
        ConversationSource {
            id: "codex-live".to_string(),
            adapter_id: "codex".to_string(),
            name: "Codex local sessions".to_string(),
            kind: ConversationSourceKind::Live,
            location: "~/.codex".to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        },
        ConversationSource {
            id: "claude-code-live".to_string(),
            adapter_id: "claude-code".to_string(),
            name: "Claude Code local sessions".to_string(),
            kind: ConversationSourceKind::Live,
            location: "~/.claude/projects".to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        },
        ConversationSource {
            id: "opencode-live".to_string(),
            adapter_id: "opencode".to_string(),
            name: "OpenCode local sessions".to_string(),
            kind: ConversationSourceKind::Live,
            location: "~/.local/share/opencode/opencode.db".to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        },
        ConversationSource {
            id: "codebuddy-live".to_string(),
            adapter_id: "codebuddy".to_string(),
            name: "CodeBuddy local sessions".to_string(),
            kind: ConversationSourceKind::Live,
            location: "~/.codebuddy".to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        },
    ]
}

pub(crate) use super::row_mappers::*;

pub(super) fn conversation_session_from_normalized(
    source: &ConversationSource,
    normalized: &NormalizedConversationSession,
    now: &str,
) -> ConversationSession {
    let is_agent_workspace = normalized
        .project_path
        .as_deref()
        .map(|p| p.contains("agent-executions"))
        .unwrap_or(false);
    let execution_origin = normalized.execution_origin.clone().unwrap_or_else(|| {
        if is_agent_workspace {
            "internal_agent".to_string()
        } else {
            "user".to_string()
        }
    });
    let execution_purpose = normalized.execution_purpose.clone();
    let user_visible = normalized
        .user_visible
        .unwrap_or_else(|| !execution_origin.starts_with("internal_"));

    ConversationSession {
        id: stable_id(
            "conversation-session",
            &[&source.id, &normalized.external_id],
        ),
        source_id: source.id.clone(),
        adapter_id: source.adapter_id.clone(),
        external_id: normalized.external_id.clone(),
        title: normalized
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Untitled session")
            .to_string(),
        project_path: normalized.project_path.clone(),
        started_at: normalized.started_at.clone(),
        updated_at: normalized.updated_at.clone(),
        source_locator: normalized.source_locator.clone(),
        source_fingerprint: normalized.source_fingerprint.clone(),
        missing: false,
        created_at: now.to_string(),
        imported_at: now.to_string(),
        execution_origin,
        execution_purpose,
        user_visible,
    }
}

pub(super) fn conversation_turn_from_normalized(
    session_id: &str,
    normalized: &crate::backend::domain::NormalizedConversationTurn,
    now: &str,
) -> ConversationTurn {
    ConversationTurn {
        id: stable_id("conversation-turn", &[session_id, &normalized.external_id]),
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
        model: normalized.model.clone(),
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ConversationRecordTables {
    pub(super) sessions: &'static str,
    pub(super) session_project_path_expr: &'static str,
    pub(super) turns: &'static str,
    pub(super) parts: &'static str,
    pub(super) questions: &'static str,
    pub(super) question_turns: &'static str,
    pub(super) session_id_prefix: &'static str,
    pub(super) question_id_prefix: &'static str,
    pub(super) turn_id_prefix: &'static str,
    pub(super) part_id_prefix: &'static str,
}

impl ConversationRecordKind {
    pub(super) fn tables(self) -> ConversationRecordTables {
        match self {
            ConversationRecordKind::Session => ConversationRecordTables {
                sessions: "conversation_sessions",
                session_project_path_expr: "s.project_path",
                turns: "conversation_turns",
                parts: "conversation_parts",
                questions: "conversation_questions",
                question_turns: "conversation_question_turns",
                session_id_prefix: "conversation-session-",
                question_id_prefix: "conversation-question-",
                turn_id_prefix: "conversation-turn-",
                part_id_prefix: "conversation-part-",
            },
            ConversationRecordKind::Web => ConversationRecordTables {
                sessions: "web_record_sessions",
                session_project_path_expr: "NULL",
                turns: "web_record_turns",
                parts: "web_record_parts",
                questions: "web_record_questions",
                question_turns: "web_record_question_turns",
                session_id_prefix: "web-record-session-",
                question_id_prefix: "web-record-question-",
                turn_id_prefix: "web-record-turn-",
                part_id_prefix: "web-record-part-",
            },
        }
    }
}

pub(super) fn compact_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn normalize_query(query: Option<&str>) -> Option<String> {
    query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase)
}

pub(super) fn normalize_project_path(project_path: Option<&str>) -> Option<String> {
    project_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn first_line(text: &str) -> String {
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

pub(super) fn search_question_title_from_turns(
    question: &ConversationQuestion,
    turns: &[ConversationTurn],
) -> String {
    question
        .title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .or_else(|| {
            turns
                .iter()
                .map(|turn| turn.user_text.as_str())
                .find(|text| !text.trim().is_empty())
                .map(first_line)
        })
        .unwrap_or_else(|| "Untitled question".to_string())
}

pub(super) fn stable_id(prefix: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(b"\0");
    }
    format!("{prefix}-{:x}", hasher.finalize())
}
