use std::fmt;

use super::{PackageIdentity, PackageKind};
use crate::backend::runtime::{
    tasks::{
        CancelOutcome, ExternalRegistrationOutcome, TaskFn, TaskKind, TaskRuntime, TaskSnapshot,
        TaskSpec, TaskState,
    },
    AppError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LifecycleOp {
    Install,
    Upgrade,
    Remove,
    Enable,
    Disable,
    Probe,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ResourceKey {
    pub(crate) identity: PackageIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct LifecycleRequestKey {
    pub(crate) resource: ResourceKey,
    pub(crate) operation: LifecycleOp,
}

impl ResourceKey {
    pub(crate) fn new(identity: PackageIdentity) -> Self {
        Self { identity }
    }

    pub(crate) fn dedup_key(&self, operation: LifecycleOp) -> String {
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
    pub(crate) fn conflicts_with(self, other: Self) -> bool {
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
    pub(crate) fn dedup_key(&self) -> String {
        self.resource.dedup_key(self.operation)
    }

    pub(crate) fn conflict_key(&self) -> String {
        let kind = match self.resource.identity.kind {
            PackageKind::ConversationAdapter => "conversation_adapter",
            PackageKind::Agent => "agent",
        };
        format!(
            "extension-lifecycle:{}:{}",
            kind, self.resource.identity.package_id
        )
    }

    pub(crate) fn conflicts_with(&self, other: &Self) -> bool {
        self.resource.same_package(&other.resource)
            && self.operation.conflicts_with(other.operation)
    }
}

impl ResourceKey {
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
pub(crate) struct LifecycleTaskCoordinator {
    runtime: TaskRuntime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LifecycleReservationOutcome {
    Started,
    Existing(String),
}

impl LifecycleTaskCoordinator {
    pub(crate) fn new(runtime: TaskRuntime) -> Self {
        Self { runtime }
    }

    pub(crate) fn reserve(
        &self,
        task_id: String,
        key: LifecycleRequestKey,
    ) -> Result<LifecycleReservationOutcome, AppError> {
        let task_spec = TaskSpec::new(TaskKind::ExtensionLifecycle, Some(key.dedup_key()))
            .with_task_id(task_id.clone())
            .with_conflict_key(key.conflict_key());
        match self.runtime.register_external(task_spec)? {
            ExternalRegistrationOutcome::Started(_) => Ok(LifecycleReservationOutcome::Started),
            ExternalRegistrationOutcome::Existing(existing) => {
                Ok(LifecycleReservationOutcome::Existing(existing.task_id))
            }
            ExternalRegistrationOutcome::Conflict(existing) => Err(AppError::Extension(
                super::ExtensionError::Conflict(format!(
                    "{} conflicts with {}",
                    key.dedup_key(),
                    existing
                        .dedup_key
                        .as_deref()
                        .unwrap_or("an active lifecycle task")
                ))
                .to_string(),
            )),
        }
    }

    pub(crate) fn spawn(
        &self,
        task_id: &str,
        detail: serde_json::Value,
        task: TaskFn,
    ) -> Result<TaskSnapshot, AppError> {
        let snapshot = self.runtime.start_external_with(task_id, detail, task)?;
        match snapshot.state {
            TaskState::Running => Ok(snapshot),
            TaskState::Pending => Err(AppError::Conflict(format!(
                "扩展生命周期任务未进入运行态: {}",
                snapshot.task_id
            ))),
            TaskState::Cancelling
            | TaskState::Succeeded
            | TaskState::Failed
            | TaskState::Canceled => Ok(snapshot),
        }
    }

    pub(crate) fn cancel(&self, task_id: &str) -> CancelOutcome {
        self.runtime.cancel(task_id)
    }
}
