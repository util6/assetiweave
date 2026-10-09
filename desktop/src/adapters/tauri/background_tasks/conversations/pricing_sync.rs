//! Background Tasks: Conversations Model Pricing Sync

use super::super::BackgroundTaskRegistry;
use super::super::{
    background_task_status, impl_basic_projection, runtime_error_message, BackgroundTaskProjection,
    BackgroundTaskStatus,
};
use crate::backend::{
    application::AppResult,
    domain::AppErrorView,
    infrastructure::tasks::{ExternalRegistrationOutcome, TaskKind, TaskSnapshot, TaskState},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ModelPricingSyncProgressPhase {
    Preparing,
    Fetching,
    Persisting,
    Recalculating,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelPricingSyncTaskProgress {
    pub(crate) phase: ModelPricingSyncProgressPhase,
    pub(crate) message: Option<String>,
    pub(crate) channel: Option<String>,
    pub(crate) models_count: usize,
    pub(crate) recalculated_events_count: usize,
    pub(crate) exchange_rate: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelPricingSyncTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) force: bool,
    pub(crate) progress: ModelPricingSyncTaskProgress,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<Value>,
    pub(crate) error: Option<AppErrorView>,
}

impl_basic_projection!(ModelPricingSyncTaskSnapshot);

impl BackgroundTaskRegistry {
    pub(crate) fn begin_model_pricing_sync_for_tenant(
        &self,
        tenant_id: &str,
        force: bool,
    ) -> AppResult<(ModelPricingSyncTaskSnapshot, bool)> {
        let snapshot = ModelPricingSyncTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            status: BackgroundTaskStatus::Running,
            force,
            progress: ModelPricingSyncTaskProgress {
                phase: ModelPricingSyncProgressPhase::Preparing,
                message: Some("正在初始化价格同步任务".to_string()),
                channel: None,
                models_count: 0,
                recalculated_events_count: 0,
                exchange_rate: 7.25,
            },
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        let dedup_key = format!("{tenant_id}:model_pricing_sync");
        let conflict_key = format!("{tenant_id}:model_pricing_sync");
        let registration = self.register_projection_for_tenant(
            Some(tenant_id),
            TaskKind::ModelPricingSync,
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

    pub(crate) fn update_model_pricing_sync_progress(
        &self,
        task_id: &str,
        phase: ModelPricingSyncProgressPhase,
        message: Option<String>,
        channel: Option<String>,
        models_count: usize,
        recalculated_events_count: usize,
        exchange_rate: f64,
    ) -> AppResult<ModelPricingSyncTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: ModelPricingSyncTaskSnapshot = self.decode(&runtime)?;
        if runtime.state == TaskState::Running {
            snapshot.progress = ModelPricingSyncTaskProgress {
                phase,
                message,
                channel,
                models_count,
                recalculated_events_count,
                exchange_rate,
            };
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn finish_model_pricing_sync(
        &self,
        task_id: &str,
        result: AppResult<Value>,
    ) -> AppResult<ModelPricingSyncTaskSnapshot> {
        let runtime = self.finish_external_task(task_id, result)?;
        let mut snapshot: ModelPricingSyncTaskSnapshot = self.decode(&runtime)?;
        if runtime.state == TaskState::Succeeded {
            snapshot.progress.phase = ModelPricingSyncProgressPhase::Completed;
        } else if runtime.state == TaskState::Failed {
            snapshot.progress.phase = ModelPricingSyncProgressPhase::Failed;
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn model_pricing_sync_snapshot_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Option<ModelPricingSyncTaskSnapshot>> {
        Ok(self
            .list_projections_for_tenant::<ModelPricingSyncTaskSnapshot>(
                tenant_id,
                TaskKind::ModelPricingSync,
            )?
            .into_iter()
            .max_by(|left, right| left.started_at.cmp(&right.started_at)))
    }
}

#[cfg(test)]
#[path = "pricing_sync_tests.rs"]
mod tests;
