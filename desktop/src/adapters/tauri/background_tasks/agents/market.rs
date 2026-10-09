//! Agents Background Tasks: Market & Lifecycle

use super::super::BackgroundTaskRegistry;
use super::super::{
    background_task_status, dedupe_non_empty, extension_lifecycle_key, impl_basic_projection,
    runtime_error_message, BackgroundTaskProjection, BackgroundTaskStatus,
};
use crate::backend::{
    application::{AgentMarketRefreshResult, AppResult},
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AgentMarketRefreshTaskState {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentMarketRefreshTaskSnapshot {
    pub(crate) id: String,
    pub(crate) state: AgentMarketRefreshTaskState,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<AgentMarketRefreshResult>,
    pub(crate) error: Option<AppErrorView>,
}

impl BackgroundTaskRegistry {
    pub(crate) fn begin_agent_market_refresh(
        &self,
    ) -> AppResult<(AgentMarketRefreshTaskSnapshot, bool)> {
        let now = Utc::now().to_rfc3339();
        let snapshot = AgentMarketRefreshTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            state: AgentMarketRefreshTaskState::Running,
            created_at: now.clone(),
            updated_at: now,
            finished_at: None,
            result: None,
            error: None,
        };
        let registration = self.register_projection(
            TaskKind::AgentMarketRefresh,
            &snapshot.id,
            Some("agent-market-refresh".to_string()),
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

    pub(crate) fn finish_agent_market_refresh(
        &self,
        task_id: &str,
        result: AppResult<AgentMarketRefreshResult>,
    ) -> AppResult<AgentMarketRefreshTaskSnapshot> {
        let runtime_result = match result.as_ref() {
            Ok(value) => {
                serde_json::to_value(value).map_err(crate::backend::application::AppError::external)
            }
            Err(error) => Err(crate::backend::application::AppError::from(error.view())),
        };
        let runtime = self.finish_external_result(task_id, runtime_result)?;
        let mut snapshot: AgentMarketRefreshTaskSnapshot = self.decode(&runtime)?;
        snapshot.result = runtime
            .result
            .clone()
            .map(serde_json::from_value)
            .transpose()
            .map_err(crate::backend::application::AppError::external)?;
        snapshot.updated_at = Utc::now().to_rfc3339();
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn agent_market_refresh_snapshot(
        &self,
        task_id: &str,
    ) -> AppResult<AgentMarketRefreshTaskSnapshot> {
        self.projection(task_id)
    }

    pub(crate) fn agent_market_refresh_snapshots(
        &self,
    ) -> AppResult<Vec<AgentMarketRefreshTaskSnapshot>> {
        let mut snapshots =
            self.list_projections::<AgentMarketRefreshTaskSnapshot>(TaskKind::AgentMarketRefresh)?;
        snapshots.sort_by(|left, right| left.created_at.cmp(&right.created_at));
        Ok(snapshots)
    }

    pub(crate) fn begin_agent_lifecycle(
        &self,
        agent_id: String,
        action: String,
        catalog_version: Option<String>,
        agent_version: Option<String>,
        distribution_id: Option<String>,
        distribution_type: Option<crate::backend::domain::agents::DistributionType>,
        ownership: Option<crate::backend::domain::agents::Ownership>,
    ) -> AppResult<(
        AgentLifecycleTaskSnapshot,
        tokio_util::sync::CancellationToken,
        bool,
    )> {
        let lifecycle_key = extension_lifecycle_key(
            PackageKind::Agent,
            &agent_id,
            agent_version.as_deref(),
            &action,
        )?;
        let now = Utc::now().to_rfc3339();
        let snapshot = AgentLifecycleTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            agent_id,
            action,
            state: LifecycleTaskState::Queued,
            phase: LifecycleTaskPhase::Queued,
            catalog_version,
            agent_version,
            distribution_id,
            distribution_type,
            ownership,
            progress: ProgressSnapshot {
                completed_units: 0,
                total_units: None,
                downloaded_bytes: None,
                total_bytes: None,
            },
            cancellable: true,
            created_at: now.clone(),
            updated_at: now,
            finished_at: None,
            result: None,
            error: None,
            warnings: Vec::new(),
        };
        match self.lifecycle.reserve(snapshot.id.clone(), lifecycle_key)? {
            LifecycleReservationOutcome::Existing(existing_id) => {
                let runtime = self.external_task_snapshot(&existing_id)?;
                let cancellation = self.task_runtime.cancellation_token(&existing_id)?;
                Ok((self.projection_from_runtime(&runtime)?, cancellation, false))
            }
            LifecycleReservationOutcome::Started => {
                self.write_projection(&snapshot.id, &snapshot)?;
                let cancellation = self.task_runtime.cancellation_token(&snapshot.id)?;
                Ok((self.projection(&snapshot.id)?, cancellation, true))
            }
        }
    }

    pub(crate) fn update_agent_lifecycle(
        &self,
        task_id: &str,
        phase: LifecycleTaskPhase,
        completed_units: u64,
        downloaded_bytes: Option<u64>,
        warnings: Vec<String>,
    ) -> AppResult<AgentLifecycleTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        if runtime.state == TaskState::Pending {
            self.task_runtime
                .activate_external(task_id, runtime.detail.clone())?;
        }
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: AgentLifecycleTaskSnapshot = self.decode(&runtime)?;
        if runtime.state == TaskState::Running && !snapshot.state.is_terminal() {
            snapshot.state = LifecycleTaskState::Running;
            snapshot.phase = phase;
            snapshot.progress.completed_units = completed_units;
            snapshot.progress.downloaded_bytes = downloaded_bytes;
            snapshot.warnings = warnings;
            snapshot.updated_at = Utc::now().to_rfc3339();
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn finish_agent_lifecycle(
        &self,
        task_id: &str,
        result: Result<(Option<Value>, Vec<String>), AgentMarketError>,
    ) -> AppResult<AgentLifecycleTaskSnapshot> {
        let (runtime_result, task_warnings, task_error) = match result {
            Ok((value, warnings)) => (Ok(value.unwrap_or(Value::Null)), warnings, None),
            Err(error) => {
                let view = (&error).into();
                (
                    Err(crate::backend::application::AppError::from(error)),
                    Vec::new(),
                    Some(view),
                )
            }
        };
        let runtime = self.finish_external_result(task_id, runtime_result)?;
        let mut snapshot: AgentLifecycleTaskSnapshot = self.decode(&runtime)?;
        snapshot.finished_at = runtime.finished_at.clone();
        snapshot.updated_at = Utc::now().to_rfc3339();
        snapshot.cancellable = false;
        if runtime.state == TaskState::Succeeded {
            snapshot.warnings = task_warnings;
        } else if let Some(error) = task_error {
            snapshot.error = Some(error);
        } else {
            snapshot.error = Some(
                (&AgentMarketError::new("task_state", "扩展生命周期任务未进入终态", false)).into(),
            );
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn agent_lifecycle_snapshot(
        &self,
        task_id: &str,
    ) -> AppResult<AgentLifecycleTaskSnapshot> {
        self.projection(task_id)
    }

    pub(crate) fn agent_lifecycle_snapshots(&self) -> AppResult<Vec<AgentLifecycleTaskSnapshot>> {
        let mut snapshots =
            self.list_projections::<AgentLifecycleTaskSnapshot>(TaskKind::ExtensionLifecycle)?;
        snapshots.sort_by(|left, right| left.created_at.cmp(&right.created_at));
        Ok(snapshots)
    }

    pub(crate) fn cancel_agent_lifecycle(
        &self,
        task_id: &str,
    ) -> AppResult<AgentLifecycleTaskSnapshot> {
        self.lifecycle.cancel(task_id);
        self.projection(task_id)
    }
}

impl BackgroundTaskProjection for AgentMarketRefreshTaskSnapshot {
    fn project_with_runtime(mut self, runtime: &TaskSnapshot) -> Self {
        self.state = match runtime.state {
            TaskState::Pending | TaskState::Running | TaskState::Cancelling => {
                AgentMarketRefreshTaskState::Running
            }
            TaskState::Succeeded => AgentMarketRefreshTaskState::Succeeded,
            TaskState::Failed => AgentMarketRefreshTaskState::Failed,
            TaskState::Canceled => AgentMarketRefreshTaskState::Cancelled,
        };
        self.finished_at = runtime.finished_at.clone();
        if runtime.state == TaskState::Succeeded {
            self.result = runtime
                .result
                .clone()
                .and_then(|value| serde_json::from_value(value).ok());
            self.error = None;
        } else if runtime.state == TaskState::Failed || runtime.state == TaskState::Canceled {
            self.result = None;
            self.error = runtime_error_message(runtime);
        }
        self
    }
}

impl BackgroundTaskProjection for AgentLifecycleTaskSnapshot {
    fn project_with_runtime(mut self, runtime: &TaskSnapshot) -> Self {
        match runtime.state {
            TaskState::Pending => {
                self.state = LifecycleTaskState::Queued;
                self.phase = LifecycleTaskPhase::Queued;
                self.cancellable = true;
            }
            TaskState::Running => {
                if !self.state.is_terminal() {
                    self.state = LifecycleTaskState::Running;
                    self.cancellable = true;
                }
            }
            TaskState::Cancelling => {
                self.state = LifecycleTaskState::Cancelling;
                self.phase = LifecycleTaskPhase::Cancelling;
                self.cancellable = false;
            }
            TaskState::Succeeded => {
                self.state = LifecycleTaskState::Succeeded;
                self.phase = LifecycleTaskPhase::Succeeded;
                self.cancellable = false;
                self.result = runtime.result.clone();
                self.error = None;
            }
            TaskState::Failed => {
                self.state = LifecycleTaskState::Failed;
                self.phase = LifecycleTaskPhase::Failed;
                self.cancellable = false;
                if self.error.is_none() {
                    self.error = runtime.error.as_ref().map(|error| {
                        let market_error =
                            AgentMarketError::new(&error.code, &error.message, error.retryable)
                                .with_details(error.details.clone());
                        (&market_error).into()
                    });
                }
            }
            TaskState::Canceled => {
                self.state = LifecycleTaskState::Cancelled;
                self.phase = LifecycleTaskPhase::Cancelled;
                self.cancellable = false;
                self.result = None;
                self.error = runtime.error.as_ref().map(|error| {
                    let market_error =
                        AgentMarketError::new(&error.code, &error.message, error.retryable)
                            .with_details(error.details.clone());
                    (&market_error).into()
                });
            }
        }
        self.finished_at = runtime.finished_at.clone();
        self
    }
}
