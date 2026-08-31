use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionMemoryJobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Skipped,
}

impl SessionMemoryJobStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionMemoryStatus {
    Active,
    Invalid,
    Failed,
}

impl SessionMemoryStatus {
    pub(crate) fn as_str(self) -> &'static str {
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
pub(crate) enum RecentMemoryEventCategory {
    Progress,
    Decision,
    Research,
    Verification,
    Blocker,
    FollowUp,
}

impl RecentMemoryEventCategory {
    pub(crate) const ALL: [Self; 6] = [
        Self::Progress,
        Self::Decision,
        Self::Research,
        Self::Verification,
        Self::Blocker,
        Self::FollowUp,
    ];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Progress => "progress",
            Self::Decision => "decision",
            Self::Research => "research",
            Self::Verification => "verification",
            Self::Blocker => "blocker",
            Self::FollowUp => "follow_up",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct SessionMemoryJob {
    pub(crate) tenant_id: String,
    pub(crate) id: String,
    pub(crate) session_id: String,
    pub(crate) source_id: String,
    pub(crate) source_revision: i64,
    pub(crate) source_fingerprint: String,
    pub(crate) contract_version: String,
    pub(crate) prompt_version: String,
    pub(crate) source_event_id: String,
    pub(crate) source_sync_run_id: String,
    pub(crate) status: SessionMemoryJobStatus,
    pub(crate) not_before: String,
    pub(crate) attempt_count: i64,
    pub(crate) last_error: Option<String>,
    pub(crate) started_at: Option<String>,
    pub(crate) finished_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct SessionMemory {
    pub(crate) tenant_id: String,
    pub(crate) id: String,
    pub(crate) session_id: String,
    pub(crate) source_id: String,
    pub(crate) source_revision: i64,
    pub(crate) source_fingerprint: String,
    pub(crate) contract_version: String,
    pub(crate) prompt_version: String,
    pub(crate) status: SessionMemoryStatus,
    pub(crate) project_path: Option<String>,
    pub(crate) summary: String,
    pub(crate) goal: String,
    pub(crate) result: String,
    pub(crate) decisions: Vec<String>,
    pub(crate) verification: Vec<String>,
    pub(crate) blockers: Vec<String>,
    pub(crate) follow_up: Vec<String>,
    pub(crate) topics: Vec<String>,
    pub(crate) generated_at: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct SessionMemorySourceReference {
    pub(crate) tenant_id: String,
    pub(crate) id: String,
    pub(crate) memory_id: String,
    pub(crate) source_id: String,
    pub(crate) session_id: String,
    pub(crate) question_id: Option<String>,
    pub(crate) turn_id: Option<String>,
    pub(crate) part_id: Option<String>,
    pub(crate) node_id: Option<String>,
    pub(crate) node_order: Option<usize>,
    pub(crate) reference_key: String,
    pub(crate) source_revision: i64,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct RecentMemoryEvent {
    pub(crate) tenant_id: String,
    pub(crate) id: String,
    pub(crate) memory_id: String,
    pub(crate) session_id: String,
    pub(crate) category: RecentMemoryEventCategory,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) occurred_at: String,
    pub(crate) source_reference_id: Option<String>,
    pub(crate) fingerprint: String,
    pub(crate) created_at: String,
}
