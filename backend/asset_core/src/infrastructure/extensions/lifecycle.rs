use std::fmt;

use super::{PackageIdentity, PackageKind};
use crate::backend::{
    infrastructure::tasks::{
        stages, CancelOutcome, ExternalRegistrationOutcome, PipelineDescriptor, TaskFn, TaskKind,
        TaskRuntime, TaskSnapshot, TaskSpec, TaskState,
    },
    infrastructure::InfraError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LifecycleOp {
    Install,
    Upgrade,
    Remove,
    Enable,
    Disable,
    Probe,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceKey {
    pub identity: PackageIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LifecycleRequestKey {
    pub resource: ResourceKey,
    pub operation: LifecycleOp,
}

impl ResourceKey {
    pub fn new(identity: PackageIdentity) -> Self {
        Self { identity }
    }

    pub fn dedup_key(&self, operation: LifecycleOp) -> String {
        format!("{operation:?}:{}", self)
    }
}

impl fmt::Display for ResourceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.identity.kind {
            PackageKind::ConversationAdapter => "conversation_adapter",
            PackageKind::Agent => "agent",
        };
        write!(
            f,
            "{kind}:{}@{}",
            self.identity.package_id, self.identity.version
        )
    }
}

impl LifecycleOp {
    #[cfg(test)]
    pub fn conflicts_with(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Install | Self::Upgrade, Self::Remove)
                | (Self::Remove, Self::Install | Self::Upgrade)
                | (Self::Enable | Self::Disable, Self::Remove)
                | (Self::Remove, Self::Enable | Self::Disable)
        )
    }
}

impl LifecycleRequestKey {
    pub fn dedup_key(&self) -> String {
        self.resource.dedup_key(self.operation)
    }

    pub fn conflict_key(&self) -> String {
        let kind = match self.resource.identity.kind {
            PackageKind::ConversationAdapter => "conversation_adapter",
            PackageKind::Agent => "agent",
        };
        format!(
            "extension-lifecycle:{}:{}",
            kind, self.resource.identity.package_id
        )
    }

    #[cfg(test)]
    pub fn conflicts_with(&self, other: &Self) -> bool {
        self.resource.same_package(&other.resource)
            && self.operation.conflicts_with(other.operation)
    }
}

impl ResourceKey {
    #[cfg(test)]
    fn same_package(&self, other: &Self) -> bool {
        self.identity.kind == other.identity.kind
            && self.identity.package_id == other.identity.package_id
    }
}

/// Shared lifecycle seam used by every extension domain.
///
/// Domain adapters own their public task projections, while this coordinator
/// only translates lifecycle requests into the canonical `TaskRuntime` task.
/// This prevents a domain from silently re-creating its own lifecycle
/// scheduler.
#[derive(Clone)]
pub struct LifecycleTaskCoordinator {
    runtime: TaskRuntime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleReservationOutcome {
    Started,
    Existing(String),
}

impl LifecycleTaskCoordinator {
    pub fn new(runtime: TaskRuntime) -> Self {
        Self { runtime }
    }

    pub fn reserve(
        &self,
        task_id: String,
        key: LifecycleRequestKey,
    ) -> Result<LifecycleReservationOutcome, InfraError> {
        let (title, pipeline) = match key.operation {
            LifecycleOp::Install | LifecycleOp::Upgrade => {
                let action_name = if matches!(key.operation, LifecycleOp::Upgrade) {
                    "更新"
                } else {
                    "安装"
                };
                let pipeline = PipelineDescriptor::builder()
                    .stage(stages::AGENT_DOWNLOAD, "下载分发包")
                    .stage(stages::AGENT_INSTALL, "安装运行时")
                    .stage(stages::AGENT_VERIFY, "验证 ACP 协议")
                    .build();
                (
                    format!(
                        "{} Agent: {}",
                        action_name, key.resource.identity.package_id
                    ),
                    pipeline,
                )
            }
            LifecycleOp::Remove => {
                let pipeline = PipelineDescriptor::builder()
                    .stage(stages::PREPARE, "检查依赖并解绑")
                    .stage(stages::AGENT_CLEANUP, "清理文件与注销")
                    .build();
                (
                    format!("卸载 Agent: {}", key.resource.identity.package_id),
                    pipeline,
                )
            }
            _ => {
                let pipeline = PipelineDescriptor::builder()
                    .stage(stages::PREPARE, "准备")
                    .stage(stages::VERIFY, "验证")
                    .build();
                (
                    format!("管理 Agent: {}", key.resource.identity.package_id),
                    pipeline,
                )
            }
        };

        let task_spec = TaskSpec::global(TaskKind::ExtensionLifecycle, Some(key.dedup_key()))
            .with_task_id(task_id.clone())
            .with_conflict_key(key.conflict_key())
            .with_pipeline(pipeline)
            .with_category("agent/lifecycle")
            .with_title(title);
        match self.runtime.register_external(task_spec)? {
            ExternalRegistrationOutcome::Started(_) => Ok(LifecycleReservationOutcome::Started),
            ExternalRegistrationOutcome::Existing(existing) => {
                Ok(LifecycleReservationOutcome::Existing(existing.task_id))
            }
            ExternalRegistrationOutcome::Conflict(existing) => Err(InfraError::Conflict(format!(
                "{} conflicts with {}",
                key.dedup_key(),
                existing
                    .dedup_key
                    .as_deref()
                    .unwrap_or("an active lifecycle task")
            ))),
        }
    }

    pub fn spawn(
        &self,
        task_id: &str,
        detail: serde_json::Value,
        task: TaskFn,
    ) -> Result<TaskSnapshot, InfraError> {
        let snapshot = self.runtime.start_external_with(task_id, detail, task)?;
        match snapshot.state {
            TaskState::Running => Ok(snapshot),
            TaskState::Pending => Err(InfraError::Conflict(format!(
                "扩展生命周期任务未进入运行态: {}",
                snapshot.task_id
            ))),
            TaskState::Cancelling
            | TaskState::Succeeded
            | TaskState::Failed
            | TaskState::Canceled => Ok(snapshot),
        }
    }

    pub fn cancel(&self, task_id: &str) -> CancelOutcome {
        self.runtime.cancel(task_id)
    }
}
