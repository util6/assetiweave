use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
pub enum MemoryRunPhase {
    Queued,
    Gates,
    Context,
    Phase1,
    Phase2,
    Finalizing,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRunStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Interrupted,
    Cancelled,
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
