//! Conversations Background Tasks: Search Index, Usage Scan & Maintenance

use super::super::BackgroundTaskRegistry;
use super::super::{
    background_task_status, dedupe_non_empty, extension_lifecycle_key, impl_basic_projection,
    runtime_error_message, BackgroundTaskProjection, BackgroundTaskStatus,
};
use crate::backend::{
    application::{
        AgentMarketRefreshResult, AppResult, ConversationAdapterPackageInstallParams,
        ConversationAdapterPackageUninstallParams, ConversationScriptInstallParams,
        ConversationSyncMode, ConversationSyncParams, SkillAcquireParams,
    },
    domain::{AppErrorView, CatalogAsset},
    infrastructure::agent_market::{AgentLifecycleTaskSnapshot, ProgressSnapshot},
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

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct ConversationUsageScanTaskProgress {
    pub(crate) phase: ConversationUsageScanProgressPhase,
    pub(crate) completed_source_count: usize,
    pub(crate) total_source_count: usize,
    pub(crate) current_source_name: Option<String>,
    pub(crate) total_events_ingested: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConversationUsageScanProgressPhase {
    Preparing,
    Scanning,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct ConversationUsageScanTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) source_id: Option<String>,
    pub(crate) mode: String,
    pub(crate) progress: ConversationUsageScanTaskProgress,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<Value>,
    pub(crate) error: Option<AppErrorView>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct ConversationDataMaintenanceTaskProgress {
    pub(crate) phase: String,
    pub(crate) completed_stage: usize,
    pub(crate) total_stage: usize,
    pub(crate) note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct ConversationDataMaintenanceTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) operation: String,
    pub(crate) source_id: Option<String>,
    pub(crate) record_kind: Option<String>,
    pub(crate) dry_run: bool,
    pub(crate) progress: ConversationDataMaintenanceTaskProgress,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<Value>,
    pub(crate) error: Option<AppErrorView>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct ConversationSearchIndexTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<Value>,
    pub(crate) error: Option<AppErrorView>,
}

impl BackgroundTaskRegistry {
    pub(crate) fn begin_conversation_search_index_rebuild_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<(ConversationSearchIndexTaskSnapshot, bool)> {
        let snapshot = ConversationSearchIndexTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            status: BackgroundTaskStatus::Running,
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        let detail = serde_json::to_value(&snapshot)
            .map_err(|error| crate::backend::application::AppError::External(error.to_string()))?;
        let mut spec = TaskSpec::new(
            TaskKind::SearchIndexRebuild,
            Some(format!("{tenant_id}:conversation-search-index")),
        )
        .with_task_id(snapshot.id.clone())
        .with_tenant_id(tenant_id);
        spec.detail = detail;
        let registration = self.task_runtime.register_external(spec)?;
        match registration {
            ExternalRegistrationOutcome::Started(ref runtime)
            | ExternalRegistrationOutcome::Existing(ref runtime)
            | ExternalRegistrationOutcome::Conflict(ref runtime) => Ok((
                self.projection_from_runtime(&runtime)?,
                matches!(&registration, ExternalRegistrationOutcome::Started(_)),
            )),
        }
    }

    pub(crate) fn finish_conversation_search_index_rebuild(
        &self,
        task_id: &str,
        result: AppResult<Value>,
    ) -> AppResult<ConversationSearchIndexTaskSnapshot> {
        let runtime = self.finish_external_task(task_id, result)?;
        let mut snapshot: ConversationSearchIndexTaskSnapshot = self.decode(&runtime)?;
        snapshot.result = runtime.result.clone();
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    #[allow(dead_code)]

    pub(crate) fn conversation_search_index_snapshot(
        &self,
    ) -> AppResult<Option<ConversationSearchIndexTaskSnapshot>> {
        Ok(self
            .list_projections::<ConversationSearchIndexTaskSnapshot>(TaskKind::SearchIndexRebuild)?
            .into_iter()
            .max_by(|left, right| left.started_at.cmp(&right.started_at)))
    }

    pub(crate) fn conversation_search_index_snapshot_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Option<ConversationSearchIndexTaskSnapshot>> {
        Ok(self
            .list_projections_for_tenant::<ConversationSearchIndexTaskSnapshot>(
                tenant_id,
                TaskKind::SearchIndexRebuild,
            )?
            .into_iter()
            .max_by(|left, right| left.started_at.cmp(&right.started_at)))
    }

    pub(crate) fn begin_conversation_usage_scan_for_tenant(
        &self,
        tenant_id: &str,
        source_id: Option<String>,
        mode: &str,
    ) -> AppResult<(ConversationUsageScanTaskSnapshot, bool)> {
        let snapshot = ConversationUsageScanTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            status: BackgroundTaskStatus::Running,
            source_id: source_id.clone(),
            mode: mode.to_string(),
            progress: ConversationUsageScanTaskProgress {
                phase: ConversationUsageScanProgressPhase::Preparing,
                completed_source_count: 0,
                total_source_count: 0,
                current_source_name: None,
                total_events_ingested: 0,
            },
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        let dedup_key = format!(
            "{tenant_id}:conversation_usage_scan:{}",
            source_id.as_deref().unwrap_or("all")
        );
        let conflict_key = format!("{tenant_id}:conversation_usage_scan");
        let registration = self.register_projection_for_tenant(
            Some(tenant_id),
            TaskKind::ConversationUsageScan,
            &snapshot.id,
            Some(dedup_key),
            std::iter::once(conflict_key),
            &snapshot,
        )?;
        match registration {
            ExternalRegistrationOutcome::Started(ref runtime)
            | ExternalRegistrationOutcome::Existing(ref runtime)
            | ExternalRegistrationOutcome::Conflict(ref runtime) => Ok((
                self.projection_from_runtime(runtime)?,
                matches!(&registration, ExternalRegistrationOutcome::Started(_)),
            )),
        }
    }

    pub(crate) fn update_conversation_usage_scan_progress(
        &self,
        task_id: &str,
        completed_source_count: usize,
        total_source_count: usize,
        current_source_name: Option<String>,
        total_events_ingested: usize,
    ) -> AppResult<ConversationUsageScanTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: ConversationUsageScanTaskSnapshot = self.decode(&runtime)?;
        if runtime.state == TaskState::Running {
            snapshot.progress = ConversationUsageScanTaskProgress {
                phase: ConversationUsageScanProgressPhase::Scanning,
                completed_source_count: completed_source_count.min(total_source_count),
                total_source_count,
                current_source_name,
                total_events_ingested,
            };
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn finish_conversation_usage_scan(
        &self,
        task_id: &str,
        result: AppResult<Value>,
    ) -> AppResult<ConversationUsageScanTaskSnapshot> {
        let runtime = self.finish_external_task(task_id, result)?;
        let mut snapshot: ConversationUsageScanTaskSnapshot = self.decode(&runtime)?;
        snapshot.result = runtime.result.clone();
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn begin_conversation_data_maintenance_for_tenant(
        &self,
        tenant_id: &str,
        operation: &str,
        source_id: Option<String>,
        record_kind: Option<String>,
        dry_run: bool,
    ) -> AppResult<(ConversationDataMaintenanceTaskSnapshot, bool)> {
        let snapshot = ConversationDataMaintenanceTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            status: BackgroundTaskStatus::Running,
            operation: operation.to_string(),
            source_id,
            record_kind,
            dry_run,
            progress: ConversationDataMaintenanceTaskProgress {
                phase: "preparing".to_string(),
                completed_stage: 0,
                total_stage: 10,
                note: None,
            },
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        let registration = self.register_projection_for_tenant(
            Some(tenant_id),
            TaskKind::ConversationDataMaintenance,
            &snapshot.id,
            Some(format!("{tenant_id}:conversation-data-maintenance")),
            [format!("{tenant_id}:conversation-data-maintenance")],
            &snapshot,
        )?;
        match registration {
            ExternalRegistrationOutcome::Started(ref runtime)
            | ExternalRegistrationOutcome::Existing(ref runtime)
            | ExternalRegistrationOutcome::Conflict(ref runtime) => Ok((
                self.projection_from_runtime(runtime)?,
                matches!(&registration, ExternalRegistrationOutcome::Started(_)),
            )),
        }
    }

    pub(crate) fn update_conversation_data_maintenance_progress(
        &self,
        task_id: &str,
        completed_stage: usize,
        total_stage: usize,
        note: Option<String>,
    ) -> AppResult<ConversationDataMaintenanceTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: ConversationDataMaintenanceTaskSnapshot = self.decode(&runtime)?;
        if runtime.state == TaskState::Running {
            snapshot.progress = ConversationDataMaintenanceTaskProgress {
                phase: note.clone().unwrap_or_else(|| "running".to_string()),
                completed_stage: completed_stage.min(total_stage),
                total_stage,
                note,
            };
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn finish_conversation_data_maintenance(
        &self,
        task_id: &str,
        result: AppResult<Value>,
    ) -> AppResult<ConversationDataMaintenanceTaskSnapshot> {
        let runtime = self.finish_external_task(task_id, result)?;
        let mut snapshot: ConversationDataMaintenanceTaskSnapshot = self.decode(&runtime)?;
        snapshot.result = runtime.result.clone();
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn conversation_data_maintenance_snapshot_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Option<ConversationDataMaintenanceTaskSnapshot>> {
        Ok(self
            .list_projections_for_tenant::<ConversationDataMaintenanceTaskSnapshot>(
                tenant_id,
                TaskKind::ConversationDataMaintenance,
            )?
            .into_iter()
            .max_by(|left, right| left.started_at.cmp(&right.started_at)))
    }

    pub(crate) fn conversation_data_maintenance_snapshots_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Vec<ConversationDataMaintenanceTaskSnapshot>> {
        let mut snapshots = self
            .list_projections_for_tenant::<ConversationDataMaintenanceTaskSnapshot>(
                tenant_id,
                TaskKind::ConversationDataMaintenance,
            )?;
        snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        Ok(snapshots)
    }

    pub(crate) fn cancel_conversation_data_maintenance_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<ConversationDataMaintenanceTaskSnapshot> {
        self.cancel_external_task_for_tenant(tenant_id, task_id)?;
        self.projection_for_tenant(tenant_id, task_id)
    }

    pub(crate) fn active_conversation_usage_scan_id(&self, tenant_id: &str) -> Option<String> {
        self.task_runtime
            .list(crate::backend::infrastructure::tasks::TaskFilter {
                kind: Some(TaskKind::ConversationUsageScan),
                active_only: true,
                ..Default::default()
            })
            .into_iter()
            .find(|task| task.tenant_id.as_deref() == Some(tenant_id))
            .map(|task| task.task_id)
    }
}

impl_basic_projection!(ConversationDataMaintenanceTaskSnapshot);
impl_basic_projection!(ConversationSearchIndexTaskSnapshot);
impl_basic_projection!(ConversationUsageScanTaskSnapshot);
