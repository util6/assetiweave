use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRecordKind {
    Session,
    Web,
}

impl MemoryRecordKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Web => "web",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryRecallQuestionRef {
    pub record_kind: MemoryRecordKind,
    pub source_id: String,
    pub session_id: String,
    pub session_title: String,
    pub project_path: Option<String>,
    pub question_id: String,
    pub question_index: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryRecallSearchHit {
    pub record_kind: MemoryRecordKind,
    pub source_id: String,
    pub session_id: String,
    pub session_title: String,
    pub project_path: Option<String>,
    pub question_id: String,
    pub question_index: i64,
    pub turn_id: Option<String>,
    pub part_id: Option<String>,
    pub block_id: String,
    pub card_type: String,
    pub snippet: String,
    pub lexical_score: u64,
    pub semantic_score: u64,
    pub score: u64,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryRecallSearchResult {
    pub query: String,
    pub backend: String,
    pub total_count: usize,
    pub hits: Vec<MemoryRecallSearchHit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRecallSessionStatus {
    Active,
    Completed,
    Failed,
    Cancelled,
    ResumeUnavailable,
}

impl MemoryRecallSessionStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::ResumeUnavailable => "resume_unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRecallTurnStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
    ResumeUnavailable,
}

impl MemoryRecallTurnStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::ResumeUnavailable => "resume_unavailable",
        }
    }

    pub(crate) fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::ResumeUnavailable
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallSessionReference {
    pub record_kind: MemoryRecordKind,
    pub session_id: String,
    pub question_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallContentReference {
    pub record_kind: MemoryRecordKind,
    pub session_id: String,
    pub question_id: String,
    pub turn_id: Option<String>,
    pub part_id: Option<String>,
    pub block_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryRecallStructuredOutput {
    pub answer: String,
    #[serde(alias = "session_references")]
    pub session_references: Vec<MemoryRecallSessionReference>,
    #[serde(alias = "content_references")]
    pub content_references: Vec<MemoryRecallContentReference>,
    #[serde(alias = "follow_up_suggestions")]
    pub follow_up_suggestions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallTurn {
    pub id: String,
    pub session_id: String,
    pub sequence: i64,
    pub conversation_session_id: String,
    pub conversation_turn_id: String,
    pub status: MemoryRecallTurnStatus,
    pub user_text: String,
    pub structured_output: Option<MemoryRecallStructuredOutput>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallSession {
    pub id: String,
    pub status: MemoryRecallSessionStatus,
    pub scope: MemoryScope,
    pub execution_context_key: String,
    pub agent_id: String,
    pub model: Option<String>,
    pub turn_count: i64,
    pub active_turn_id: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub turns: Vec<MemoryRecallTurn>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct MemoryScope {
    pub app_id: Option<String>,
    pub source_id: Option<String>,
    pub project_path: Option<String>,
    pub session_id: Option<String>,
}

impl MemoryScope {
    pub fn fingerprint(&self) -> Result<String, String> {
        let payload = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        Ok(format!("{:x}", Sha256::digest(payload)))
    }
}

#[cfg(test)]
mod tests {
    use super::MemoryScope;

    #[test]
    fn memory_scope_fingerprint_is_stable_and_scope_sensitive() {
        let scope = MemoryScope {
            app_id: Some("codex".to_string()),
            project_path: Some("~/project".to_string()),
            ..MemoryScope::default()
        };
        let mut other = scope.clone();
        other.project_path = Some("~/other".to_string());

        assert_eq!(scope.fingerprint().unwrap(), scope.fingerprint().unwrap());
        assert_ne!(scope.fingerprint().unwrap(), other.fingerprint().unwrap());
    }
}
