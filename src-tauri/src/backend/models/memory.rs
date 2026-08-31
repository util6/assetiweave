use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRunKind {
    AutoDream,
    DeepRecall,
    FullOrganize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRunTrigger {
    Automatic,
    Manual,
    UserQuestion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryDreamNoteStatus {
    Active,
    Promoted,
    Archived,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryExtractionValidationStatus {
    Pending,
    Valid,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryItemKind {
    Preference,
    Decision,
    Method,
    Context,
    FollowUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryItemStatus {
    Candidate,
    Active,
    Completed,
    Superseded,
    Archived,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryItemOrigin {
    Manual,
    AutoDream,
    DeepRecall,
    FullOrganize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStaleReason {
    EvidenceChanged,
    EvidenceMissing,
    SourceUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRevisionChangeKind {
    Create,
    Accept,
    Update,
    Status,
    Supersedes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryEvidenceRecordKind {
    Session,
    Web,
}

impl MemoryEvidenceRecordKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Web => "web",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRecallMode {
    #[default]
    Exact,
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryRecallEvidence {
    pub reference: String,
    pub card_type: String,
    pub snapshot: NewMemoryEvidenceSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryRecallQuestion {
    pub record_kind: MemoryEvidenceRecordKind,
    pub source_id: String,
    pub session_id: String,
    pub session_title: String,
    pub project_path: Option<String>,
    pub question_id: String,
    pub question_index: i64,
    pub question_title: String,
    pub evidence_ids: Vec<String>,
    pub input_char_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryRecallQuestionRef {
    pub record_kind: MemoryEvidenceRecordKind,
    pub source_id: String,
    pub session_id: String,
    pub session_title: String,
    pub project_path: Option<String>,
    pub question_id: String,
    pub question_index: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryRecallSearchHit {
    pub record_kind: MemoryEvidenceRecordKind,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryRawMemory {
    pub kind: MemoryItemKind,
    pub text: String,
    pub evidence_ids: Vec<String>,
    pub confidence: Option<f64>,
    pub uncertainty: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryExtraction {
    pub id: String,
    pub run_id: String,
    pub batch_index: usize,
    pub raw_memories: Vec<MemoryRawMemory>,
    pub session_summary: String,
    pub question_count: usize,
    pub input_char_count: usize,
    pub evidence_count: usize,
    pub validation_status: MemoryExtractionValidationStatus,
    pub attempt_count: usize,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryRecallClaim {
    pub text: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryRecallCandidate {
    pub kind: MemoryItemKind,
    pub title: String,
    pub content_markdown: String,
    pub evidence_ids: Vec<String>,
    pub confidence: Option<f64>,
    pub supersedes_item_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryRecallConflict {
    pub description: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryDreamTrigger {
    Automatic,
    #[default]
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryDreamGateKind {
    Enabled,
    Runtime,
    Time,
    Sessions,
    Lock,
    Budget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryDreamGateResult {
    pub gate: MemoryDreamGateKind,
    pub passed: bool,
    pub reason_code: String,
    pub message: String,
    pub actual: Option<i64>,
    pub required: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryDreamCursor {
    pub session_sort_key: String,
    pub question_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryDreamDeltaQuestion {
    pub id: String,
    pub question_index: i64,
    pub input_char_count: usize,
    pub input_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryDreamDeltaSession {
    pub record_kind: MemoryEvidenceRecordKind,
    pub session_id: String,
    pub source_id: String,
    pub adapter_id: String,
    pub project_path: Option<String>,
    pub title: String,
    pub imported_at: String,
    pub session_sort_key: String,
    pub available_question_count: usize,
    pub questions: Vec<MemoryDreamDeltaQuestion>,
    pub input_char_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryDreamState {
    pub scope: MemoryScope,
    pub scope_fingerprint: String,
    pub last_successful_run_id: Option<String>,
    pub last_successful_at: Option<String>,
    pub source_revision_cursor: i64,
    pub session_cursor: Option<MemoryDreamCursor>,
    pub next_gate_at: Option<String>,
    pub last_error_kind: Option<String>,
    pub last_error_message: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryDreamQuestionDeltaRow {
    pub record_kind: MemoryEvidenceRecordKind,
    pub session_id: String,
    pub source_id: String,
    pub adapter_id: String,
    pub project_path: Option<String>,
    pub title: String,
    pub imported_at: String,
    pub session_sort_key: String,
    pub question_id: String,
    pub question_index: i64,
    pub input_char_count: usize,
    pub available_question_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryDreamNote {
    pub id: String,
    pub run_id: String,
    pub scope: MemoryScope,
    pub scope_fingerprint: String,
    pub markdown: String,
    pub session_count: usize,
    pub question_count: usize,
    pub evidence_count: usize,
    pub source_revision: i64,
    pub status: MemoryDreamNoteStatus,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryDreamNoteDetail {
    pub note: MemoryDreamNote,
    pub evidence: Vec<MemoryEvidenceSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryDreamCandidateDraft {
    pub kind: MemoryItemKind,
    pub title: String,
    pub content_markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryDreamBullet {
    pub text: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryDreamSection {
    pub heading: String,
    pub bullets: Vec<MemoryDreamBullet>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryDreamOutput {
    pub sections: Vec<MemoryDreamSection>,
}

#[derive(Debug, Clone)]
pub(crate) struct MemoryDreamEvidenceDraft {
    pub reference: String,
    pub draft: NewMemoryEvidenceSnapshot,
}

#[derive(Debug, Clone)]
pub(crate) struct MemoryDreamPersistInput {
    pub run_id: String,
    pub note_id: String,
    pub scope: MemoryScope,
    pub source_revision_end: i64,
    pub processed_count: usize,
    pub total_count: usize,
    pub markdown: String,
    pub output: Value,
    pub session_count: usize,
    pub question_count: usize,
    pub cursor_end: MemoryDreamCursor,
    pub next_gate_at: String,
    pub evidence: Vec<MemoryDreamEvidenceDraft>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryItem {
    pub id: String,
    pub kind: MemoryItemKind,
    pub status: MemoryItemStatus,
    pub title: String,
    pub content_markdown: String,
    pub scope: MemoryScope,
    pub scope_fingerprint: String,
    pub origin: MemoryItemOrigin,
    pub origin_run_id: Option<String>,
    pub origin_dream_note_id: Option<String>,
    pub origin_extraction_id: Option<String>,
    pub confidence: Option<f64>,
    pub supersedes_item_id: Option<String>,
    pub source_revision: i64,
    pub verified_revision: i64,
    pub stale_reason: Option<MemoryStaleReason>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NewMemoryItem {
    pub kind: MemoryItemKind,
    pub status: MemoryItemStatus,
    pub title: String,
    pub content_markdown: String,
    pub scope: MemoryScope,
    pub origin: MemoryItemOrigin,
    pub origin_run_id: Option<String>,
    pub origin_dream_note_id: Option<String>,
    pub origin_extraction_id: Option<String>,
    pub confidence: Option<f64>,
    pub supersedes_item_id: Option<String>,
    pub source_revision: i64,
    pub verified_revision: i64,
    pub stale_reason: Option<MemoryStaleReason>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryItemRevision {
    pub id: String,
    pub item_id: String,
    pub revision_number: i64,
    pub change_kind: MemoryRevisionChangeKind,
    pub kind: MemoryItemKind,
    pub status: MemoryItemStatus,
    pub title: String,
    pub content_markdown: String,
    pub scope: MemoryScope,
    pub scope_fingerprint: String,
    pub origin: MemoryItemOrigin,
    pub confidence: Option<f64>,
    pub supersedes_item_id: Option<String>,
    pub source_revision: i64,
    pub verified_revision: i64,
    pub stale_reason: Option<MemoryStaleReason>,
    pub changed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryEvidenceSnapshot {
    pub id: String,
    pub record_kind: MemoryEvidenceRecordKind,
    pub source_id: Option<String>,
    pub session_id: String,
    pub question_id: Option<String>,
    pub turn_id: Option<String>,
    pub part_id: Option<String>,
    pub block_id: String,
    pub content_hash: String,
    pub excerpt: String,
    pub translated_excerpt: Option<String>,
    pub event_time: Option<String>,
    pub source_revision: i64,
    pub source_unavailable: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NewMemoryEvidenceSnapshot {
    pub record_kind: MemoryEvidenceRecordKind,
    pub source_id: Option<String>,
    pub session_id: String,
    pub question_id: Option<String>,
    pub turn_id: Option<String>,
    pub part_id: Option<String>,
    pub block_id: String,
    pub content_hash: String,
    pub excerpt: String,
    pub translated_excerpt: Option<String>,
    pub event_time: Option<String>,
    pub source_revision: i64,
    pub source_unavailable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryItemDetail {
    pub item: MemoryItem,
    pub evidence: Vec<MemoryEvidenceSnapshot>,
    pub revisions: Vec<MemoryItemRevision>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryItemFilter {
    pub kinds: Vec<MemoryItemKind>,
    pub statuses: Vec<MemoryItemStatus>,
    pub origins: Vec<MemoryItemOrigin>,
    pub scope_fingerprint: Option<String>,
    pub stale_only: bool,
    pub limit: usize,
    pub offset: usize,
}

impl Default for MemoryItemFilter {
    fn default() -> Self {
        Self {
            kinds: Vec::new(),
            statuses: Vec::new(),
            origins: Vec::new(),
            scope_fingerprint: None,
            stale_only: false,
            limit: 50,
            offset: 0,
        }
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
