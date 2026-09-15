use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RecentMemoryStatus {
    Empty,
    Generating,
    Ready,
    UpdateFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecentMemoryErrorView {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecentMemoryStateView {
    pub(crate) status: RecentMemoryStatus,
    pub(crate) snapshot: Option<RecentMemorySnapshotView>,
    pub(crate) latest_attempt_task_id: Option<String>,
    pub(crate) latest_attempt_error: Option<RecentMemoryErrorView>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RecentSnapshotPublicationKind {
    Generated,
    Reused,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecentMemorySnapshotView {
    pub(crate) snapshot_id: String,
    pub(crate) sequence: i64,
    pub(crate) target_watermark: String,
    pub(crate) window_start: String,
    pub(crate) window_end: String,
    pub(crate) window_hours: i64,
    pub(crate) publication_kind: RecentSnapshotPublicationKind,
    pub(crate) reused_from_snapshot_id: Option<String>,
    pub(crate) content_generated_at: String,
    pub(crate) published_at: String,
    pub(crate) projects: Vec<RecentProjectView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecentProjectView {
    pub(crate) project_key: String,
    pub(crate) project_title: String,
    pub(crate) project_path: Option<String>,
    pub(crate) summary: String,
    pub(crate) no_material_change: bool,
    pub(crate) latest_activity_at: String,
    pub(crate) source_session_count: i64,
    pub(crate) items: Vec<RecentMemoryItemView>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceAvailability {
    Available,
    PartiallyUnavailable,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecentMemoryItemView {
    pub(crate) item_id: String,
    pub(crate) revision_id: String,
    pub(crate) category: String,
    pub(crate) status: String,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) rationale: String,
    pub(crate) occurred_at: String,
    pub(crate) recommendation_rank: Option<i64>,
    pub(crate) source_availability: SourceAvailability,
    pub(crate) session_references: Vec<RecentSessionReferenceView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecentSessionReferenceView {
    pub(crate) source_id: String,
    pub(crate) session_id: String,
    pub(crate) session_title: String,
    pub(crate) source_agent: String,
    pub(crate) last_activity_at: String,
    pub(crate) available: bool,
    pub(crate) unavailable_reason: Option<String>,
}
