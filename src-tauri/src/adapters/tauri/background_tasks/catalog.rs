//! Background Tasks: Catalog Domain

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceScanScope {
    All,
    Skills,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceScanProgressPhase {
    Preparing,
    Scanning,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct SourceScanTaskProgress {
    pub(crate) phase: SourceScanProgressPhase,
    pub(crate) completed_source_count: u64,
    pub(crate) total_source_count: Option<u64>,
    pub(crate) current_source_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct SourceScanTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) scope: SourceScanScope,
    pub(crate) kind: Option<crate::backend::domain::AssetKind>,
    pub(crate) progress: SourceScanTaskProgress,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<Vec<CatalogAsset>>,
    pub(crate) error: Option<AppErrorView>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct SkillBackupTaskError {
    pub(crate) asset_id: Option<String>,
    pub(crate) error: AppErrorView,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct SkillBackupTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) asset_ids: Vec<String>,
    pub(crate) total_count: usize,
    pub(crate) completed_count: usize,
    pub(crate) failed_count: usize,
    pub(crate) current_asset_id: Option<String>,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    #[serde(default)]
    pub(crate) assets: Vec<CatalogAsset>,
    pub(crate) errors: Vec<SkillBackupTaskError>,
    pub(crate) error: Option<AppErrorView>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct RemoteSkillAcquireTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) url: String,
    pub(crate) branch: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) dry_run: bool,
    pub(crate) phase: String,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<Value>,
    pub(crate) error: Option<AppErrorView>,
}

impl BackgroundTaskRegistry {
    pub(crate) fn begin_source_scan(
        &self,
        tenant_id: &str,
        scope: SourceScanScope,
        kind: Option<crate::backend::domain::AssetKind>,
    ) -> AppResult<(SourceScanTaskSnapshot, bool)> {
        let id = Uuid::new_v4().to_string();
        let snapshot = SourceScanTaskSnapshot {
            id: id.clone(),
            status: BackgroundTaskStatus::Running,
            scope,
            kind,
            progress: SourceScanTaskProgress {
                phase: SourceScanProgressPhase::Preparing,
                completed_source_count: 0,
                total_source_count: None,
                current_source_name: None,
            },
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        let dedup_key = format!(
            "scan:{tenant_id}:{}:{}",
            match scope {
                SourceScanScope::All => "all",
                SourceScanScope::Skills => "skills",
            },
            kind.map(|value| format!("{value:?}"))
                .unwrap_or_else(|| "all".to_string())
        );
        let registration = self.register_projection_for_tenant(
            Some(tenant_id),
            TaskKind::Scan,
            &id,
            Some(dedup_key),
            [format!("catalog-write:{tenant_id}")],
            &snapshot,
        )?;
        match registration {
            ExternalRegistrationOutcome::Started(runtime) => {
                Ok((self.projection_from_runtime(&runtime)?, true))
            }
            ExternalRegistrationOutcome::Existing(runtime)
            | ExternalRegistrationOutcome::Conflict(runtime) => {
                Ok((self.projection_from_runtime(&runtime)?, false))
            }
        }
    }

    pub(crate) fn finish_source_scan(
        &self,
        task_id: &str,
        result: AppResult<Vec<CatalogAsset>>,
    ) -> AppResult<SourceScanTaskSnapshot> {
        let runtime_result = match result.as_ref() {
            Ok(value) => {
                serde_json::to_value(value).map_err(crate::backend::application::AppError::external)
            }
            Err(error) => Err(crate::backend::application::AppError::from(error.view())),
        };
        let runtime = self.finish_external_result(task_id, runtime_result)?;
        let mut snapshot: SourceScanTaskSnapshot = self.decode(&runtime)?;
        snapshot.result = runtime
            .result
            .clone()
            .map(serde_json::from_value)
            .transpose()
            .map_err(crate::backend::application::AppError::external)?;
        self.write_projection(task_id, &snapshot)?;
        self.projection_from_runtime(&self.external_task_snapshot(task_id)?)
    }

    #[allow(dead_code)]

    pub(crate) fn source_scan_snapshot(&self, task_id: &str) -> AppResult<SourceScanTaskSnapshot> {
        self.projection(task_id)
    }

    pub(crate) fn source_scan_snapshot_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<SourceScanTaskSnapshot> {
        self.projection_for_tenant(tenant_id, task_id)
    }

    #[allow(dead_code)]

    pub(crate) fn source_scan_snapshots(&self) -> AppResult<Vec<SourceScanTaskSnapshot>> {
        let mut snapshots = self.list_projections::<SourceScanTaskSnapshot>(TaskKind::Scan)?;
        snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        Ok(snapshots)
    }

    pub(crate) fn source_scan_snapshots_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Vec<SourceScanTaskSnapshot>> {
        let mut snapshots =
            self.list_projections_for_tenant::<SourceScanTaskSnapshot>(tenant_id, TaskKind::Scan)?;
        snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        Ok(snapshots)
    }

    #[allow(dead_code)]

    pub(crate) fn cancel_source_scan(&self, task_id: &str) -> AppResult<SourceScanTaskSnapshot> {
        self.cancel_external_task(task_id)?;
        self.projection(task_id)
    }

    pub(crate) fn cancel_source_scan_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<SourceScanTaskSnapshot> {
        self.cancel_external_task_for_tenant(tenant_id, task_id)?;
        self.projection_for_tenant(tenant_id, task_id)
    }

    pub(crate) fn begin_remote_skill_acquire_for_tenant(
        &self,
        tenant_id: &str,
        params: &SkillAcquireParams,
    ) -> AppResult<(RemoteSkillAcquireTaskSnapshot, bool)> {
        let snapshot = RemoteSkillAcquireTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            status: BackgroundTaskStatus::Running,
            url: params.url.clone(),
            branch: params.branch.clone(),
            path: params.path.clone(),
            name: params.name.clone(),
            dry_run: params.dry_run,
            phase: if params.dry_run {
                "preparing".to_string()
            } else {
                "queued".to_string()
            },
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        let identity = [
            params.url.trim(),
            params.branch.as_deref().unwrap_or_default(),
            params.path.as_deref().unwrap_or_default(),
            params.name.as_deref().unwrap_or_default(),
            if params.dry_run { "dry-run" } else { "import" },
        ]
        .join("|");
        let registration = self.register_projection_for_tenant(
            Some(tenant_id),
            TaskKind::RemoteSkillAcquire,
            &snapshot.id,
            Some(format!("{tenant_id}:skill-acquire:{identity}")),
            [format!("skill-acquire:{tenant_id}")],
            &snapshot,
        )?;
        match registration {
            ExternalRegistrationOutcome::Started(ref runtime)
            | ExternalRegistrationOutcome::Existing(ref runtime)
            | ExternalRegistrationOutcome::Conflict(ref runtime) => Ok((
                self.projection_from_runtime(runtime)?,
                matches!(registration, ExternalRegistrationOutcome::Started(_)),
            )),
        }
    }

    pub(crate) fn update_remote_skill_acquire_phase(
        &self,
        task_id: &str,
        phase: impl Into<String>,
    ) -> AppResult<RemoteSkillAcquireTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: RemoteSkillAcquireTaskSnapshot = self.decode(&runtime)?;
        if runtime.state.is_active() {
            snapshot.phase = phase.into();
            self.write_projection(task_id, &snapshot)?;
        }
        self.projection(task_id)
    }

    pub(crate) fn finish_remote_skill_acquire(
        &self,
        task_id: &str,
        result: AppResult<Value>,
    ) -> AppResult<RemoteSkillAcquireTaskSnapshot> {
        let runtime = self.finish_external_result(task_id, result)?;
        let mut snapshot: RemoteSkillAcquireTaskSnapshot = self.decode(&runtime)?;
        match runtime.state {
            TaskState::Succeeded => {
                snapshot.phase = "completed".to_string();
                snapshot.result = runtime.result.clone();
                snapshot.error = None;
            }
            TaskState::Failed => {
                snapshot.phase = "failed".to_string();
                snapshot.result = None;
                snapshot.error = runtime.error.clone();
            }
            TaskState::Canceled => {
                snapshot.phase = "cancelled".to_string();
                snapshot.result = None;
                snapshot.error = runtime.error.clone();
            }
            TaskState::Pending | TaskState::Running | TaskState::Cancelling => {}
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn remote_skill_acquire_snapshot_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<RemoteSkillAcquireTaskSnapshot> {
        self.projection_for_tenant(tenant_id, task_id)
    }

    pub(crate) fn remote_skill_acquire_snapshots_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Vec<RemoteSkillAcquireTaskSnapshot>> {
        let mut snapshots = self.list_projections_for_tenant::<RemoteSkillAcquireTaskSnapshot>(
            tenant_id,
            TaskKind::RemoteSkillAcquire,
        )?;
        snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        Ok(snapshots)
    }

    pub(crate) fn cancel_remote_skill_acquire_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<RemoteSkillAcquireTaskSnapshot> {
        self.cancel_external_task_for_tenant(tenant_id, task_id)?;
        self.projection_for_tenant(tenant_id, task_id)
    }

    pub(crate) fn begin_skill_backup_for_tenant(
        &self,
        tenant_id: &str,
        asset_ids: Vec<String>,
    ) -> AppResult<(SkillBackupTaskSnapshot, bool)> {
        let asset_ids = dedupe_non_empty(asset_ids);
        if asset_ids.is_empty() {
            return Err(crate::backend::application::AppError::Validation(
                "skill backup requires at least one asset id".to_string(),
            ));
        }
        let snapshot = SkillBackupTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            status: BackgroundTaskStatus::Running,
            total_count: asset_ids.len(),
            completed_count: 0,
            failed_count: 0,
            current_asset_id: asset_ids.first().cloned(),
            asset_ids,
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            assets: Vec::new(),
            errors: Vec::new(),
            error: None,
        };
        let registration = self.register_projection_for_tenant(
            Some(tenant_id),
            TaskKind::Backup,
            &snapshot.id,
            Some(format!("{tenant_id}:skill-backup")),
            Vec::new(),
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

    pub(crate) fn update_skill_backup_progress(
        &self,
        task_id: &str,
        completed_count: usize,
        current_asset_id: Option<String>,
    ) -> AppResult<SkillBackupTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: SkillBackupTaskSnapshot = self.decode(&runtime)?;
        if runtime.state == TaskState::Running {
            snapshot.completed_count = completed_count.min(snapshot.total_count);
            snapshot.current_asset_id = current_asset_id;
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn finish_skill_backup(
        &self,
        task_id: &str,
        result: crate::backend::application::AppResult<Vec<CatalogAsset>>,
    ) -> AppResult<SkillBackupTaskSnapshot> {
        let runtime_result = match &result {
            Ok(assets) => serde_json::to_value(assets)
                .map(Some)
                .map_err(crate::backend::application::AppError::external),
            Err(error) => Err(crate::backend::application::AppError::from(error.view())),
        };
        let runtime_result = runtime_result.and_then(|result| {
            result.ok_or_else(|| {
                crate::backend::application::AppError::External(
                    "skill backup result was empty".to_string(),
                )
            })
        });
        self.finish_external_result(task_id, runtime_result)?;
        let mut snapshot: SkillBackupTaskSnapshot =
            self.decode(&self.external_task_snapshot(task_id)?)?;
        match result {
            Ok(assets) => snapshot.assets = assets,
            Err(error) => snapshot.errors.push(SkillBackupTaskError {
                asset_id: snapshot.current_asset_id.clone(),
                error: error.view(),
            }),
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    #[allow(dead_code)]

    pub(crate) fn skill_backup_snapshot(&self) -> AppResult<Option<SkillBackupTaskSnapshot>> {
        Ok(self
            .list_projections::<SkillBackupTaskSnapshot>(TaskKind::Backup)?
            .into_iter()
            .max_by(|left, right| left.started_at.cmp(&right.started_at)))
    }

    pub(crate) fn skill_backup_snapshot_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Option<SkillBackupTaskSnapshot>> {
        Ok(self
            .list_projections_for_tenant::<SkillBackupTaskSnapshot>(tenant_id, TaskKind::Backup)?
            .into_iter()
            .max_by(|left, right| left.started_at.cmp(&right.started_at)))
    }
}

impl BackgroundTaskProjection for RemoteSkillAcquireTaskSnapshot {
    fn project_with_runtime(mut self, runtime: &TaskSnapshot) -> Self {
        self.status = background_task_status(runtime.state);
        self.finished_at = runtime.finished_at.clone();
        match runtime.state {
            TaskState::Succeeded => {
                self.phase = "completed".to_string();
                self.result = runtime.result.clone();
                self.error = None;
            }
            TaskState::Failed => {
                self.phase = "failed".to_string();
                self.result = None;
                self.error = runtime.error.clone();
            }
            TaskState::Canceled => {
                self.phase = "cancelled".to_string();
                self.result = None;
                self.error = runtime.error.clone();
            }
            TaskState::Pending | TaskState::Running | TaskState::Cancelling => {}
        }
        self
    }
}

impl BackgroundTaskProjection for SourceScanTaskSnapshot {
    fn project_with_runtime(mut self, runtime: &TaskSnapshot) -> Self {
        self.status = background_task_status(runtime.state);
        self.finished_at = runtime.finished_at.clone();
        if let Some(progress) = &runtime.progress {
            self.progress.completed_source_count = progress.current;
            self.progress.total_source_count = progress.total;
            self.progress.current_source_name = progress.note.clone();
        }
        match runtime.state {
            TaskState::Running | TaskState::Pending => {
                self.progress.phase = SourceScanProgressPhase::Scanning;
            }
            TaskState::Cancelling => {}
            TaskState::Succeeded => {
                self.progress.phase = SourceScanProgressPhase::Completed;
                self.result = runtime
                    .result
                    .clone()
                    .and_then(|value| serde_json::from_value(value).ok());
                self.error = None;
            }
            TaskState::Failed => {
                self.progress.phase = SourceScanProgressPhase::Failed;
                self.error = runtime_error_message(runtime);
            }
            TaskState::Canceled => {
                self.progress.phase = SourceScanProgressPhase::Cancelled;
                self.result = None;
                self.error = runtime_error_message(runtime);
            }
        }
        self
    }
}

impl BackgroundTaskProjection for SkillBackupTaskSnapshot {
    fn project_with_runtime(mut self, runtime: &TaskSnapshot) -> Self {
        self.status = background_task_status(runtime.state);
        self.finished_at = runtime.finished_at.clone();
        match runtime.state {
            TaskState::Succeeded => {
                self.completed_count = self.total_count;
                self.failed_count = 0;
                self.assets = runtime
                    .result
                    .clone()
                    .and_then(|value| serde_json::from_value(value).ok())
                    .unwrap_or_default();
                self.errors.clear();
                self.error = None;
            }
            TaskState::Failed => {
                self.failed_count = 1;
                self.error = runtime_error_message(runtime);
            }
            TaskState::Canceled => {
                self.failed_count = 0;
                self.assets.clear();
                self.errors.clear();
                self.error = runtime_error_message(runtime);
            }
            TaskState::Pending | TaskState::Running | TaskState::Cancelling => {}
        }
        self
    }
}
