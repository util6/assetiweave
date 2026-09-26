use crate::backend::domain::MemoryScope;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct MemoryRecallSearchParams {
    pub(crate) query: String,
    #[serde(default)]
    pub(crate) scope: MemoryScope,
    pub(crate) since: Option<String>,
    pub(crate) until: Option<String>,
    #[serde(alias = "fileHint")]
    pub(crate) file: Option<String>,
    #[serde(alias = "commandHint")]
    pub(crate) command: Option<String>,
    #[serde(alias = "errorHint")]
    pub(crate) error: Option<String>,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub(crate) struct MemoryRecallSessionCreateParams {
    #[serde(default)]
    pub(crate) scope: MemoryScope,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct MemoryRecallTurnSendParams {
    #[serde(alias = "sessionId")]
    pub(crate) session_id: String,
    pub(crate) query: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct MemoryRecallTurnCancelParams {
    #[serde(alias = "turnId")]
    pub(crate) turn_id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct MemoryRecallSessionGetParams {
    #[serde(alias = "sessionId")]
    pub(crate) session_id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct MemoryContextResolveParams {
    #[serde(default, alias = "projectPath")]
    pub(crate) project_path: Option<String>,
    pub(crate) query: Option<String>,
    #[serde(default, alias = "tokenBudget")]
    pub(crate) token_budget: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct MemoryProjectGetParams {
    #[serde(alias = "projectPath")]
    pub(crate) project_path: String,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub(crate) struct MemoryScopeRebuildParams {
    #[serde(default)]
    pub(crate) scope: MemoryScope,
    /// 新版入口优先使用显式 target；缺省时兼容旧 scope 语义。
    #[serde(default)]
    pub(crate) target: Option<MemoryRebuildTarget>,
    #[serde(default, alias = "projectPath")]
    pub(crate) project_path: Option<String>,
    #[serde(default)]
    pub(crate) reason: Option<MemoryRebuildReason>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MemoryRebuildTarget {
    Recent,
    Project,
    Global,
    All,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MemoryRebuildReason {
    Manual,
    Migration,
    ProjectionRepair,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub(crate) struct MemoryTaskListParams {
    #[serde(default, alias = "activeOnly")]
    pub(crate) active_only: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct MemoryTaskGetParams {
    #[serde(alias = "taskId")]
    pub(crate) task_id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct MemoryTaskRetryParams {
    #[serde(alias = "taskId")]
    pub(crate) task_id: String,
}
