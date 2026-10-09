use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::projection::{
    ConversationCardRenderer, ConversationContentNode, ConversationContentNodeLocator,
};
use super::{
    ConversationAdapter, ConversationPart, ConversationPartRole, ConversationQuestion,
    ConversationQuestionTurn, ConversationSession, ConversationTurn,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationRecordKind {
    Session,
    Web,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct ConversationSearchCardType(String);

impl ConversationSearchCardType {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn question() -> Self {
        Self::new("question")
    }

    #[cfg(test)]
    pub fn answer() -> Self {
        Self::new("answer")
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct ConversationExportContentFilter(BTreeMap<String, bool>);

impl ConversationExportContentFilter {
    pub fn is_visible(&self, kind: &str) -> bool {
        self.0.get(kind).copied().unwrap_or(true)
    }

    pub fn is_visible_node(&self, kind: &str, semantic_role: Option<&str>) -> bool {
        if let Some(visible) = self.0.get(kind) {
            return *visible;
        }
        if let Some(role) = semantic_role {
            if let Some(visible) = self.0.get(role) {
                return *visible;
            }
        }
        self.0
            .get(kind.rsplit('.').next().unwrap_or(kind))
            .copied()
            .unwrap_or(true)
    }
}

impl Default for ConversationExportContentFilter {
    fn default() -> Self {
        Self(BTreeMap::from([
            ("answer".to_string(), true),
            ("tool".to_string(), true),
            ("command".to_string(), true),
            ("code".to_string(), true),
            ("result".to_string(), true),
        ]))
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationExportFormat {
    #[default]
    Rendered,
    Raw,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationSessionListItem {
    #[serde(flatten)]
    pub session: ConversationSession,
    pub question_count: usize,
    pub turn_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationQuestionDetail {
    pub question: ConversationQuestion,
    pub question_turns: Vec<ConversationQuestionTurn>,
    pub turns: Vec<ConversationTurn>,
    pub parts: Vec<ConversationPart>,
    pub projected_content_nodes: Vec<ConversationContentNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationBlockLocator {
    pub record_kind: String,
    pub session_id: String,
    pub question_id: String,
    pub turn_id: String,
    pub block_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_id: Option<String>,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_role: Option<String>,
    pub renderer: ConversationCardRenderer,
    pub role: ConversationPartRole,
    pub content_length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationBlockDetail {
    #[serde(flatten)]
    pub locator: ConversationBlockLocator,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translated_content: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationSessionDetail {
    pub session: ConversationSession,
    pub questions: Vec<ConversationQuestionDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ConversationSessionOutline {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub turns: Vec<ConversationTurnOutline>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ConversationTurnOutline {
    pub turn_id: String,
    pub turn_index: usize,
    pub user_question: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub cards: Vec<ConversationCardRunGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ConversationCardRunGroup {
    pub card_kind: String,
    pub count: usize,
    pub card_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchRetrievalMode {
    Lexical,
    Semantic,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ConversationSearchIndexStatus {
    pub health: String,
    pub schema_version: i64,
    pub tokenizer_version: String,
    pub source_revision: i64,
    pub indexed_revision: Option<i64>,
    pub active_generation: Option<String>,
    pub document_count: i64,
    pub size_bytes: i64,
    pub last_built_at: Option<String>,
    pub last_error: Option<String>,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<String>,
    pub is_rebuilding: bool,
    pub updated_at: String,
    pub supported_modes: Vec<SearchRetrievalMode>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ConversationSearchIndexRebuildReport {
    pub generation: String,
    pub indexed_revision: i64,
    pub document_count: i64,
    pub size_bytes: i64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationSearchHit {
    pub session: ConversationSessionListItem,
    pub question_id: String,
    pub question_index: i64,
    pub question_title: String,
    pub turn_id: Option<String>,
    pub part_id: Option<String>,
    pub block_id: String,
    pub card_type: ConversationSearchCardType,
    pub snippet: String,
    pub score: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incremental: Option<ConversationSearchIncrementalMatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlight_segments: Option<Vec<ConversationSearchHighlightSegment>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationSearchIncrementalMatch {
    pub sync_run_id: String,
    pub change_kind: String,
    pub observed_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationSearchHighlightSegment {
    pub text: String,
    pub matched: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationSearchPage {
    pub total_count: usize,
    pub hits: Vec<ConversationSearchHit>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationMutationResult {
    pub dry_run: bool,
    pub session_id: String,
    pub affected_question_ids: Vec<String>,
    pub questions: Vec<ConversationQuestionDetail>,
}

/// Immutable domain catalog published through the shared kernel snapshot.
/// Package manifests remain domain-owned; this catalog only provides the
/// complete adapter registration view consumed by conversation workflows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConversationAdapterCatalog {
    pub adapters: Vec<ConversationAdapter>,
}

impl ConversationAdapterCatalog {
    pub fn new(adapters: Vec<ConversationAdapter>) -> Self {
        Self { adapters }
    }
}
