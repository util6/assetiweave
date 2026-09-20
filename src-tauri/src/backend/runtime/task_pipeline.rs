use super::tasks::{StageStatus, TaskKind, TaskStage};
use serde::{Deserialize, Serialize};

/// 开放任务分类。
///
/// 既可以承接系统内置的 `TaskKind` 命名空间，也可以承接第三方或动态扩展的自由命名空间。
/// 例如：`conversation/sync`、`system/backup`、`custom/my_flow`。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskCategory(pub String);

impl TaskCategory {
    pub fn new(category: impl Into<String>) -> Self {
        Self(category.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for TaskCategory {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for TaskCategory {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<TaskKind> for TaskCategory {
    fn from(kind: TaskKind) -> Self {
        Self(kind.default_category_string().to_string())
    }
}

impl std::fmt::Display for TaskCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 流水线阶段描述符。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageDescriptor {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub user_visible: bool,
}

fn default_true() -> bool {
    true
}

impl StageDescriptor {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            user_visible: true,
        }
    }

    pub fn with_user_visible(mut self, user_visible: bool) -> Self {
        self.user_visible = user_visible;
        self
    }
}

/// 声明式流水线骨架描述符。
///
/// 任务提交时提供该骨架，TaskRuntime 将在任务初始化阶段预置全量 stages（状态为 Pending），
/// 确保前端/CLI 一次性感知完整执行计划，杜绝运行时突兀跳出新阶段。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineDescriptor {
    pub stages: Vec<StageDescriptor>,
}

impl PipelineDescriptor {
    pub fn new(stages: impl IntoIterator<Item = StageDescriptor>) -> Self {
        Self {
            stages: stages.into_iter().collect(),
        }
    }

    pub fn builder() -> PipelineDescriptorBuilder {
        PipelineDescriptorBuilder::default()
    }

    /// 将描述符转换为初始 TaskStage 列表（全部预置为 Pending）。
    pub(crate) fn to_initial_stages(&self) -> Vec<TaskStage> {
        self.stages
            .iter()
            .map(|desc| TaskStage {
                id: desc.id.clone(),
                name: desc.name.clone(),
                status: StageStatus::Pending,
                started_at: None,
                finished_at: None,
                duration_ms: None,
                progress: None,
                current_activities: Vec::new(),
                metrics: Vec::new(),
                failures: Vec::new(),
                skipped: Vec::new(),
                agent_session_ref: None,
            })
            .collect()
    }
}

/// 流水线描述符构建器。
#[derive(Debug, Default)]
pub struct PipelineDescriptorBuilder {
    stages: Vec<StageDescriptor>,
}

impl PipelineDescriptorBuilder {
    pub fn stage(mut self, id: impl Into<String>, name: impl Into<String>) -> Self {
        self.stages.push(StageDescriptor::new(id, name));
        self
    }

    pub fn stage_with_visibility(
        mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        user_visible: bool,
    ) -> Self {
        self.stages
            .push(StageDescriptor::new(id, name).with_user_visible(user_visible));
        self
    }

    pub fn build(self) -> PipelineDescriptor {
        PipelineDescriptor {
            stages: self.stages,
        }
    }
}

#[cfg(test)]
#[path = "task_pipeline_tests.rs"]
mod tests;
