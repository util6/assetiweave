use crate::backend::domain::MemoryScope;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MemoryRecallSearchParams {
    pub query: String,
    #[serde(default)]
    pub scope: MemoryScope,
    pub since: Option<String>,
    pub until: Option<String>,
    #[serde(alias = "fileHint")]
    pub file: Option<String>,
    #[serde(alias = "commandHint")]
    pub command: Option<String>,
    #[serde(alias = "errorHint")]
    pub error: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct MemoryRecallSessionCreateParams {
    #[serde(default)]
    pub scope: MemoryScope,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MemoryRecallTurnSendParams {
    #[serde(alias = "sessionId")]
    pub session_id: String,
    pub query: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MemoryRecallTurnCancelParams {
    #[serde(alias = "turnId")]
    pub turn_id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MemoryRecallSessionGetParams {
    #[serde(alias = "sessionId")]
    pub session_id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MemoryContextResolveParams {
    #[serde(default, alias = "projectPath")]
    pub project_path: Option<String>,
    pub query: Option<String>,
    #[serde(default, alias = "tokenBudget")]
    pub token_budget: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MemoryProjectGetParams {
    #[serde(alias = "projectPath")]
    pub project_path: String,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct MemoryScopeRebuildParams {
    #[serde(default)]
    pub scope: MemoryScope,
    /// 新版入口优先使用显式 target；缺省时兼容旧 scope 语义。
    #[serde(default)]
    pub target: Option<MemoryRebuildTarget>,
    #[serde(default, alias = "projectPath")]
    pub project_path: Option<String>,
    #[serde(default)]
    pub reason: Option<MemoryRebuildReason>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRebuildTarget {
    Recent,
    Project,
    Global,
    All,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRebuildReason {
    Manual,
    Migration,
    ProjectionRepair,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct MemoryTaskListParams {
    #[serde(default, alias = "activeOnly")]
    pub active_only: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MemoryTaskGetParams {
    #[serde(alias = "taskId")]
    pub task_id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MemoryTaskRetryParams {
    #[serde(alias = "taskId")]
    pub task_id: String,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct MemoryReferenceRepairParams {
    /// 只回放审计与修复计划；缺省为 dry-run。
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
    /// 写入必须显式确认。
    #[serde(default)]
    pub yes: bool,
}

/// Memory 流水线执行参数
#[derive(Debug, Clone, Default)]
pub struct MemoryPipelineParams {
    pub force_rebuild: bool,
    pub target: Option<MemoryRebuildTarget>,
    pub project_path: Option<String>,
    pub reason: Option<MemoryRebuildReason>,
}

impl MemoryPipelineParams {
    pub fn background() -> Self {
        Self::default()
    }

    pub fn manual(force_rebuild: bool) -> Self {
        Self {
            force_rebuild,
            reason: Some(MemoryRebuildReason::Manual),
            ..Default::default()
        }
    }

    pub fn rebuild(
        target: MemoryRebuildTarget,
        project_path: Option<String>,
        reason: Option<MemoryRebuildReason>,
    ) -> Self {
        Self {
            force_rebuild: true,
            target: Some(target),
            project_path,
            reason,
        }
    }
}
