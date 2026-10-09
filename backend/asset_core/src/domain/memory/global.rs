use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GlobalMemoryJobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

impl GlobalMemoryJobStatus {
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
pub enum GlobalMemoryVersionStatus {
    Running,
    Succeeded,
    Failed,
    Invalid,
}

impl GlobalMemoryVersionStatus {
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
pub struct GlobalMemoryJob {
    pub tenant_id: String,
    pub id: String,
    pub target_watermark: i64,
    pub input_fingerprint: String,
    pub status: GlobalMemoryJobStatus,
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
pub struct GlobalMemory {
    pub tenant_id: String,
    pub id: String,
    pub last_successful_version_id: Option<String>,
    pub last_successful_at: Option<String>,
    pub last_successful_watermark: i64,
    pub last_successful_input_fingerprint: Option<String>,
    pub summary_document_path: Option<String>,
    pub memory_document_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GlobalMemoryVersion {
    pub tenant_id: String,
    pub id: String,
    pub version_number: i64,
    pub status: GlobalMemoryVersionStatus,
    pub input_fingerprint: String,
    pub source_watermark: i64,
    pub summary_markdown: Option<String>,
    pub memory_markdown: Option<String>,
    pub raw_output_json: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GlobalMemorySource {
    pub project_id: String,
    pub project_path: String,
    pub project_version_id: String,
    pub project_watermark: i64,
    pub sort_order: i64,
}
