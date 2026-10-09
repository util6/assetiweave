//! Background Tasks: Mounting Domain

use super::BackgroundTaskRegistry;
use super::{
    background_task_status, dedupe_non_empty, extension_lifecycle_key, runtime_error_message,
    BackgroundTaskProjection, BackgroundTaskStatus,
};
use crate::backend::{
    application::{
        AgentMarketRefreshResult, AppResult, ConversationAdapterPackageInstallParams,
        ConversationAdapterPackageUninstallParams, ConversationScriptInstallParams,
        ConversationSyncMode, ConversationSyncParams, SkillAcquireParams,
    },
    domain::{AppErrorView, CatalogAsset},
    infrastructure::agent_execution::{
        AiExecutionCancellation, AiExecutionCleanupReport, AiExecutionError, AiExecutionErrorView,
        AiExecutionPhase, AiExecutionPurpose, AiExecutionResult,
    },
    infrastructure::agent_market::{
        AgentLifecycleTaskSnapshot, AgentMarketError, LifecycleTaskPhase, LifecycleTaskState,
        ProgressSnapshot,
    },
    infrastructure::extensions::{
        LifecycleOp, LifecycleRequestKey, LifecycleReservationOutcome, LifecycleTaskCoordinator,
        PackageIdentity, PackageKind, ResourceKey,
    },
    infrastructure::tasks::{
        ExternalRegistrationOutcome, TaskFn, TaskKind, TaskRuntime, TaskSnapshot, TaskSpec,
        TaskState,
    },
};
use chrono::Utc;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use uuid::Uuid;

use super::impl_basic_projection;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct BatchMountTaskProgress {
    pub(crate) phase: String,
    pub(crate) completed: u64,
    pub(crate) total: Option<u64>,
    pub(crate) current_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct BatchMountTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) mode: String,
    pub(crate) profile_id: String,
    pub(crate) progress: BatchMountTaskProgress,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<Value>,
    pub(crate) error: Option<AppErrorView>,
}

impl BackgroundTaskRegistry {
    pub(crate) fn begin_batch_mount(
        &self,
        tenant_id: &str,
        mode: &str,
        profile_id: &str,
        dedup_suffix: &str,
    ) -> AppResult<(BatchMountTaskSnapshot, bool)> {
        let id = Uuid::new_v4().to_string();
        let snapshot = BatchMountTaskSnapshot {
            id: id.clone(),
            status: BackgroundTaskStatus::Running,
            mode: mode.to_string(),
            profile_id: profile_id.to_string(),
            progress: BatchMountTaskProgress {
                phase: "preparing".to_string(),
                completed: 0,
                total: None,
                current_id: None,
            },
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        let registration = self.register_projection_for_tenant(
            Some(tenant_id),
            TaskKind::BatchMount,
            &id,
            Some(format!(
                "mount:{tenant_id}:{mode}:{profile_id}:{dedup_suffix}"
            )),
            [format!("mount-profile:{tenant_id}:{profile_id}")],
            &snapshot,
        )?;
        match registration {
            ExternalRegistrationOutcome::Started(ref runtime)
            | ExternalRegistrationOutcome::Existing(ref runtime)
            | ExternalRegistrationOutcome::Conflict(ref runtime) => Ok((
                self.projection_from_runtime(&runtime)?,
                matches!(&registration, ExternalRegistrationOutcome::Started(_)),
            )),
        }
    }

    pub(crate) fn update_batch_mount_progress(
        &self,
        task_id: &str,
        completed: u64,
        total: Option<u64>,
        current_id: Option<&str>,
    ) -> AppResult<BatchMountTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: BatchMountTaskSnapshot = self.decode(&runtime)?;
        if runtime.state.is_active() {
            snapshot.progress.completed = completed;
            snapshot.progress.total = total;
            snapshot.progress.current_id = current_id.map(str::to_string);
        }
        self.task_runtime
            .set_progress(task_id, completed, total, current_id)?;
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn finish_batch_mount(
        &self,
        task_id: &str,
        result: AppResult<Value>,
    ) -> AppResult<BatchMountTaskSnapshot> {
        let runtime = self.finish_external_result(task_id, result)?;
        let mut snapshot: BatchMountTaskSnapshot = self.decode(&runtime)?;
        snapshot.result = runtime.result.clone();
        self.write_projection(task_id, &snapshot)?;
        self.projection_from_runtime(&self.external_task_snapshot(task_id)?)
    }

    #[allow(dead_code)]

    pub(crate) fn batch_mount_snapshot(&self, task_id: &str) -> AppResult<BatchMountTaskSnapshot> {
        self.projection(task_id)
    }

    pub(crate) fn batch_mount_snapshot_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<BatchMountTaskSnapshot> {
        self.projection_for_tenant(tenant_id, task_id)
    }

    #[allow(dead_code)]

    pub(crate) fn batch_mount_snapshots(&self) -> AppResult<Vec<BatchMountTaskSnapshot>> {
        let mut snapshots =
            self.list_projections::<BatchMountTaskSnapshot>(TaskKind::BatchMount)?;
        snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        Ok(snapshots)
    }

    pub(crate) fn batch_mount_snapshots_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Vec<BatchMountTaskSnapshot>> {
        let mut snapshots = self.list_projections_for_tenant::<BatchMountTaskSnapshot>(
            tenant_id,
            TaskKind::BatchMount,
        )?;
        snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        Ok(snapshots)
    }

    #[allow(dead_code)]

    pub(crate) fn cancel_batch_mount(&self, task_id: &str) -> AppResult<BatchMountTaskSnapshot> {
        self.cancel_external_task(task_id)?;
        self.projection(task_id)
    }

    pub(crate) fn cancel_batch_mount_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<BatchMountTaskSnapshot> {
        self.cancel_external_task_for_tenant(tenant_id, task_id)?;
        self.projection_for_tenant(tenant_id, task_id)
    }
}

impl_basic_projection!(BatchMountTaskSnapshot);
