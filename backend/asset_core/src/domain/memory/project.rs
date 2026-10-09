use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectMemoryJobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

impl ProjectMemoryJobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectMemoryVersionStatus {
    Running,
    Succeeded,
    Failed,
    Invalid,
}

impl ProjectMemoryVersionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Invalid => "invalid",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectMemoryJob {
    pub tenant_id: String,
    pub id: String,
    pub project_id: String,
    pub project_path: String,
    pub target_watermark: i64,
    pub input_fingerprint: String,
    pub status: ProjectMemoryJobStatus,
    pub attempt_count: i64,
    pub retry_count: i64,
    pub retry_at: Option<String>,
    pub last_error: Option<String>,
    pub ownership_token: Option<String>,
    pub lease_expires_at: Option<String>,
    pub heartbeat_at: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectMemory {
    pub tenant_id: String,
    pub id: String,
    pub project_path: String,
    pub last_successful_version_id: Option<String>,
    pub last_successful_at: Option<String>,
    pub last_successful_watermark: i64,
    pub last_successful_input_fingerprint: Option<String>,
    pub document_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectMemoryVersion {
    pub tenant_id: String,
    pub id: String,
    pub project_id: String,
    pub version_number: i64,
    pub status: ProjectMemoryVersionStatus,
    pub input_fingerprint: String,
    pub source_watermark: i64,
    pub content_markdown: Option<String>,
    pub raw_output_json: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectMemorySource {
    pub session_memory_id: String,
    pub source_revision: i64,
    pub sort_order: i64,
}
