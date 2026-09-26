use crate::backend::domain::{
    GlobalMemoryVersion, L2ProjectMemoryView, ProjectMemorySource, ProjectMemoryVersion,
};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct MemoryContextReference {
    pub kind: String,
    pub id: String,
    pub source_revision: Option<i64>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct MemoryContextResult {
    pub text: String,
    pub revision: String,
    pub generated_at: Option<String>,
    pub estimated_tokens: usize,
    pub token_budget: usize,
    pub references: Vec<MemoryContextReference>,
    pub global_version: Option<GlobalMemoryVersion>,
    pub project_version: Option<ProjectMemoryVersion>,
    pub project_sources: Vec<ProjectMemorySource>,
}

pub type MemoryProjectView = L2ProjectMemoryView;

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRebuildResult {
    pub accepted: bool,
    pub scheduled_task_ids: Vec<String>,
    pub target_watermark: Option<String>,
    pub reused: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct MemoryTaskView {
    pub id: String,
    pub status: String,
    pub kind: String,
    pub progress: Option<crate::backend::infrastructure::tasks::TaskProgress>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub result: Option<Value>,
    pub error: Option<crate::backend::domain::AppErrorView>,
    pub detail: Value,
}
