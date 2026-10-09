use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceNodeKind {
    Question,
    UserIntent,
    UserCorrection,
    AgentProposal,
    ExecutionEvidence,
    VerificationEvidence,
    ContentOutline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceReadStatus {
    ReadInInitialPack,
    ReadByTool,
    IndexedOnly,
    Truncated,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShortEvidenceRef {
    pub ref_key: String,
    pub turn_id: String,
    pub turn_index: i64,
    pub part_index: usize,
    pub role: String,
    pub status: EvidenceReadStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoundedEvidenceNode {
    pub ref_key: String,
    pub role: String,
    pub kind: EvidenceNodeKind,
    pub title: String,
    pub text: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceIndexEntry {
    pub ref_key: String,
    pub turn_id: String,
    pub turn_index: i64,
    pub role: String,
    pub snippet: String,
    pub total_chars: usize,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceCoverage {
    pub total_turns: usize,
    pub total_nodes: usize,
    pub indexed_nodes: usize,
    pub read_nodes: usize,
    pub truncated_nodes: usize,
    pub unavailable_nodes: usize,
    pub is_fully_covered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoundedEvidenceInitialPack {
    pub work_order_id: String,
    pub session_id: String,
    pub source_revision: i64,
    pub task_boundary: String,
    pub intent_and_corrections: Vec<BoundedEvidenceNode>,
    pub outcomes_and_verifications: Vec<BoundedEvidenceNode>,
    pub index: Vec<EvidenceIndexEntry>,
    pub coverage: EvidenceCoverage,
    pub total_chars: usize,
    pub nodes_count: usize,
}

impl BoundedEvidenceInitialPack {
    pub fn all_nodes(&self) -> impl Iterator<Item = &BoundedEvidenceNode> {
        self.intent_and_corrections
            .iter()
            .chain(self.outcomes_and_verifications.iter())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EvidenceReadError {
    #[error("tool '{0}' is not authorized by the execution contract")]
    UnauthorizedTool(String),

    #[error("reference '{requested}' is out of scope or unknown: {reason}")]
    OutOfScope { requested: String, reason: String },

    #[error("tool reading budget exhausted: {reason}")]
    BudgetExhausted { reason: String },

    #[error("content for reference '{ref_key}' is unavailable or missing from source")]
    ContentUnavailable { ref_key: String },
}
