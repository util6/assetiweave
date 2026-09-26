use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GlobalMemoryJobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

impl GlobalMemoryJobStatus {
    pub(crate) fn as_str(self) -> &'static str {
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
pub(crate) enum GlobalMemoryVersionStatus {
    Running,
    Succeeded,
    Failed,
    Invalid,
}

impl GlobalMemoryVersionStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Invalid => "invalid",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct GlobalMemoryJob {
    pub(crate) tenant_id: String,
    pub(crate) id: String,
    pub(crate) target_watermark: i64,
    pub(crate) input_fingerprint: String,
    pub(crate) status: GlobalMemoryJobStatus,
    pub(crate) attempt_count: i64,
    pub(crate) retry_count: i64,
    pub(crate) retry_at: Option<String>,
    pub(crate) last_error: Option<String>,
    pub(crate) ownership_token: Option<String>,
    pub(crate) lease_expires_at: Option<String>,
    pub(crate) heartbeat_at: Option<String>,
    pub(crate) started_at: Option<String>,
    pub(crate) finished_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct GlobalMemory {
    pub(crate) tenant_id: String,
    pub(crate) id: String,
    pub(crate) last_successful_version_id: Option<String>,
    pub(crate) last_successful_at: Option<String>,
    pub(crate) last_successful_watermark: i64,
    pub(crate) last_successful_input_fingerprint: Option<String>,
    pub(crate) summary_document_path: Option<String>,
    pub(crate) memory_document_path: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct GlobalMemoryVersion {
    pub(crate) tenant_id: String,
    pub(crate) id: String,
    pub(crate) version_number: i64,
    pub(crate) status: GlobalMemoryVersionStatus,
    pub(crate) input_fingerprint: String,
    pub(crate) source_watermark: i64,
    pub(crate) summary_markdown: Option<String>,
    pub(crate) memory_markdown: Option<String>,
    pub(crate) raw_output_json: Option<String>,
    pub(crate) error_message: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct GlobalMemorySource {
    pub(crate) project_id: String,
    pub(crate) project_path: String,
    pub(crate) project_version_id: String,
    pub(crate) project_watermark: i64,
    pub(crate) sort_order: i64,
}
