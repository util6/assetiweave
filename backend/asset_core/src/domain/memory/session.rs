use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionMemoryJobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Skipped,
    Canceled,
}

impl SessionMemoryJobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Canceled => "canceled",
        }
    }
}

impl Default for SessionMemoryJobStatus {
    fn default() -> Self {
        Self::Queued
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionMemoryStatus {
    Active,
    Invalid,
    Failed,
}

impl Default for SessionMemoryStatus {
    fn default() -> Self {
        Self::Active
    }
}

impl SessionMemoryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Invalid => "invalid",
            Self::Failed => "failed",
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Ord, PartialOrd,
)]
#[serde(rename_all = "snake_case")]
pub enum RecentMemoryEventCategory {
    Progress,
    Decision,
    Research,
    Verification,
    Blocker,
    FollowUp,
}

impl RecentMemoryEventCategory {
    pub const ALL: [Self; 6] = [
        Self::Progress,
        Self::Decision,
        Self::Research,
        Self::Verification,
        Self::Blocker,
        Self::FollowUp,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Progress => "progress",
            Self::Decision => "decision",
            Self::Research => "research",
            Self::Verification => "verification",
            Self::Blocker => "blocker",
            Self::FollowUp => "follow_up",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "progress" => Some(Self::Progress),
            "decision" => Some(Self::Decision),
            "research" => Some(Self::Research),
            "verification" => Some(Self::Verification),
            "blocker" => Some(Self::Blocker),
            "follow_up" => Some(Self::FollowUp),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct SessionMemoryJob {
    pub tenant_id: String,
    pub id: String,
    pub session_id: String,
    pub source_id: String,
    pub source_revision: i64,
    pub source_fingerprint: String,
    pub contract_version: String,
    pub prompt_version: String,
    pub source_event_id: String,
    pub source_sync_run_id: String,
    pub status: SessionMemoryJobStatus,
    pub not_before: String,
    pub attempt_count: i64,
    pub last_error: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub ownership_token: Option<String>,
    pub lease_expires_at: Option<String>,
    pub heartbeat_at: Option<String>,
    pub retry_count: i64,
    pub retry_at: Option<String>,
    pub watermark: Option<i64>,
    pub recipe_id: Option<String>,
    pub recipe_revision: Option<i64>,
    pub recipe_content_hash: Option<String>,
    pub budget_policy_version: Option<String>,
    pub work_order_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct SessionMemory {
    pub tenant_id: String,
    pub id: String,
    pub session_id: String,
    pub source_id: String,
    pub source_revision: i64,
    pub source_fingerprint: String,
    pub contract_version: String,
    pub prompt_version: String,
    pub status: SessionMemoryStatus,
    pub project_path: Option<String>,
    pub summary: String,
    pub goal: String,
    pub result: String,
    pub decisions: Vec<String>,
    pub verification: Vec<String>,
    pub blockers: Vec<String>,
    pub follow_up: Vec<String>,
    pub topics: Vec<String>,
    pub generated_at: String,
    pub created_at: String,
    pub updated_at: String,
    pub recipe_id: Option<String>,
    pub recipe_content_hash: Option<String>,
    pub work_order_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionMemorySourceReference {
    pub tenant_id: String,
    pub id: String,
    pub memory_id: String,
    pub source_id: String,
    pub session_id: String,
    pub question_id: Option<String>,
    pub turn_id: Option<String>,
    pub part_id: Option<String>,
    pub node_id: Option<String>,
    pub node_order: Option<usize>,
    pub reference_key: String,
    pub source_revision: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecentMemoryEvent {
    pub tenant_id: String,
    pub id: String,
    pub memory_id: String,
    pub session_id: String,
    pub category: RecentMemoryEventCategory,
    pub title: String,
    pub summary: String,
    pub occurred_at: String,
    pub source_reference_id: Option<String>,
    pub fingerprint: String,
    pub created_at: String,
}
