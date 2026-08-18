use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
};

use super::{PackageIdentity, PackageKind};
use crate::backend::runtime::{
    tasks::{
        CancelOutcome, SpawnOutcome, TaskFn, TaskKind, TaskRuntime, TaskSnapshot, TaskSpec,
        TaskState,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct LifecycleReservation {
    key: LifecycleRequestKey,
    spawned: bool,
}

/// Shared lifecycle seam used by every extension domain.
///
/// Domain adapters own their public task projections, while this coordinator
/// owns the cross-domain reservation, exact-operation deduplication and the
/// actual `TaskRuntime` task. This prevents a domain from silently re-creating
/// its own lifecycle scheduler.
#[derive(Clone)]
pub(crate) struct LifecycleTaskCoordinator {
    runtime: TaskRuntime,
    reservations: Arc<Mutex<HashMap<String, LifecycleReservation>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LifecycleReservationOutcome {
    Started,
    Existing(String),
}

impl LifecycleTaskCoordinator {
    pub(crate) fn new(runtime: TaskRuntime) -> Self {
        Self {
            runtime,
            reservations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn reserve(
        &self,
        task_id: String,
        key: LifecycleRequestKey,
    ) -> Result<LifecycleReservationOutcome, AppError> {
        let mut reservations = self
            .reservations
            .lock()
            .map_err(|_| AppError::Conflict("扩展生命周期注册表不可用".to_string()))?;
        self.prune_finished(&mut reservations);

        if let Some((existing_id, _existing)) = reservations
            .iter()
            .find(|(_, reservation)| reservation.key == key)
        {
            return Ok(LifecycleReservationOutcome::Existing(existing_id.clone()));
        }
        if let Some((_, conflicting)) = reservations
            .iter()
            .find(|(_, reservation)| key.conflicts_with(&reservation.key))
        {
            return Err(AppError::Extension(
                super::ExtensionError::Conflict(format!(
                    "{} conflicts with {}",
                    key.dedup_key(),
                    conflicting.key.dedup_key()
                ))
                .to_string(),
            ));
        }

        reservations.insert(
            task_id,
            LifecycleReservation {
                key,
                spawned: false,
            },
        );
        Ok(LifecycleReservationOutcome::Started)
    }

    pub(crate) fn spawn(
        &self,
        task_id: &str,
        detail: serde_json::Value,
        task: TaskFn,
    ) -> Result<TaskSnapshot, AppError> {
        let key = {
            let reservations = self
                .reservations
                .lock()
                .map_err(|_| AppError::Conflict("扩展生命周期注册表不可用".to_string()))?;
            reservations
                .get(task_id)
                .map(|reservation| reservation.key.clone())
                .ok_or_else(|| AppError::Conflict(format!("扩展生命周期预留不存在: {task_id}")))?
        };

        let spec = TaskSpec::new(TaskKind::ExtensionLifecycle, Some(key.dedup_key()))
            .with_task_id(task_id.to_string());
        let outcome = self.runtime.spawn(TaskSpec { detail, ..spec }, task);
        match outcome {
            Ok(SpawnOutcome::Started(snapshot)) => {
                if let Ok(mut reservations) = self.reservations.lock() {
                    if let Some(reservation) = reservations.get_mut(task_id) {
                        reservation.spawned = true;
                    }
                }
                Ok(snapshot)
            }
            Ok(SpawnOutcome::Existing(snapshot)) => {
                self.release(task_id);
                Err(AppError::Conflict(format!(
                    "扩展生命周期任务已由 TaskRuntime 占用: {}",
                    snapshot.task_id
                )))
            }
            Err(error) => {
                self.release(task_id);
                Err(error)
            }
        }
    }

    pub(crate) fn cancel(&self, task_id: &str) -> CancelOutcome {
        self.runtime.cancel(task_id)
    }

    pub(crate) fn task(&self, task_id: &str) -> Option<TaskSnapshot> {
        self.runtime.get(task_id)
    }

    pub(crate) fn release(&self, task_id: &str) {
        if let Ok(mut reservations) = self.reservations.lock() {
            reservations.remove(task_id);
        }
    }

    /// Drop a projection-only reservation immediately. A reservation that is
    /// already backed by TaskRuntime stays until the runtime observes the
    /// terminal state; releasing it from inside the task would allow a second
    /// request to race with the still-running first task.
    pub(crate) fn finish_projection(&self, task_id: &str) {
        if let Ok(mut reservations) = self.reservations.lock() {
            if reservations
                .get(task_id)
                .is_some_and(|reservation| !reservation.spawned)
            {
                reservations.remove(task_id);
            }
        }
    }

    pub(crate) fn runtime(&self) -> TaskRuntime {
        self.runtime.clone()
    }

    fn prune_finished(&self, reservations: &mut HashMap<String, LifecycleReservation>) {
        reservations.retain(|task_id, reservation| {
            !reservation.spawned
                || self.runtime.get(task_id).is_some_and(|snapshot| {
                    matches!(snapshot.state, TaskState::Pending | TaskState::Running)
                })
        });
    }
}
