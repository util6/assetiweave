use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::SourceAvailability;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecentMemoryStatus {
    Empty,
    Generating,
    Ready,
    UpdateFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecentMemoryErrorView {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecentMemoryStateView {
    pub status: RecentMemoryStatus,
    pub snapshot: Option<RecentMemorySnapshotView>,
    pub latest_attempt_task_id: Option<String>,
    pub latest_attempt_error: Option<RecentMemoryErrorView>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecentSnapshotPublicationKind {
    Generated,
    Reused,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecentMemorySnapshotView {
    pub snapshot_id: String,
    pub sequence: i64,
    pub target_watermark: String,
    pub window_start: String,
    pub window_end: String,
    pub window_hours: i64,
    pub publication_kind: RecentSnapshotPublicationKind,
    pub reused_from_snapshot_id: Option<String>,
    pub content_generated_at: String,
    pub published_at: String,
    pub projects: Vec<RecentProjectView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecentProjectView {
    pub project_key: String,
    pub project_title: String,
    pub project_path: Option<String>,
    pub summary: String,
    pub no_material_change: bool,
    pub latest_activity_at: String,
    pub source_session_count: i64,
    pub items: Vec<RecentMemoryItemView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecentMemoryItemView {
    pub item_id: String,
    pub revision_id: String,
    pub category: String,
    pub status: String,
    pub title: String,
    pub summary: String,
    pub rationale: String,
    pub occurred_at: String,
    pub recommendation_rank: Option<i64>,
    pub source_availability: SourceAvailability,
    pub session_references: Vec<RecentSessionReferenceView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecentSessionReferenceView {
    pub source_id: String,
    pub session_id: String,
    pub session_title: String,
    pub source_agent: String,
    pub last_activity_at: String,
    pub available: bool,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RecentMemoryEventTarget {
    pub record_kind: String,
    pub session_id: String,
    pub question_id: Option<String>,
    pub turn_id: Option<String>,
    pub block_id: Option<String>,
}
