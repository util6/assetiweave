//! Agents Background Tasks: Execution

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
pub(crate) enum AiExecutionTaskState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl AiExecutionTaskState {
    pub(crate) fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct AiExecutionPublicResult {
    pub(crate) text: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct AiExecutionTaskSnapshot {
    pub(crate) id: String,
    pub(crate) purpose: AiExecutionPurpose,
    pub(crate) agent_id: String,
    pub(crate) state: AiExecutionTaskState,
    pub(crate) phase: AiExecutionPhase,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<AiExecutionPublicResult>,
    pub(crate) error: Option<AiExecutionErrorView>,
    pub(crate) cleanup: Option<AiExecutionCleanupReport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AiExecutionShutdownReport {
    pub(crate) cancelled_count: usize,
    pub(crate) remaining_count: usize,
    pub(crate) converged: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AiExecutionTaskGetParams {
    pub(crate) task_id: String,
}

impl BackgroundTaskRegistry {
    pub(crate) fn begin_ai_execution_for_tenant(
        &self,
        tenant_id: &str,
        purpose: AiExecutionPurpose,
        agent_id: impl std::fmt::Display,
    ) -> AppResult<(AiExecutionTaskSnapshot, AiExecutionCancellation)> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let snapshot = AiExecutionTaskSnapshot {
            id: id.clone(),
            purpose,
            agent_id: agent_id.to_string(),
            state: AiExecutionTaskState::Queued,
            phase: AiExecutionPhase::Queued,
            created_at: now.clone(),
            updated_at: now,
            finished_at: None,
            result: None,
            error: None,
            cleanup: None,
        };
        let runtime = match self.register_external_task_for_tenant(
            Some(tenant_id),
            TaskKind::AiExecution,
            &id,
            None,
            Vec::new(),
            serde_json::to_value(&snapshot).map_err(|error| {
                crate::backend::application::AppError::External(error.to_string())
            })?,
        )? {
            ExternalRegistrationOutcome::Started(runtime) => runtime,
            ExternalRegistrationOutcome::Existing(runtime)
            | ExternalRegistrationOutcome::Conflict(runtime) => {
                return Err(crate::backend::application::AppError::Conflict(format!(
                    "AI execution task id was already registered: {}",
                    runtime.task_id
                )))
            }
        };
        let cancellation = AiExecutionCancellation::from_token(
            self.task_runtime.cancellation_token(&runtime.task_id)?,
        );
        Ok((self.projection_from_runtime(&runtime)?, cancellation))
    }

    pub(crate) fn update_ai_execution_phase(
        &self,
        task_id: &str,
        phase: AiExecutionPhase,
    ) -> AppResult<AiExecutionTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: AiExecutionTaskSnapshot = self.decode(&runtime)?;
        if matches!(runtime.state, TaskState::Running | TaskState::Cancelling)
            && !snapshot.state.is_terminal()
        {
            snapshot.state = if phase == AiExecutionPhase::Queued {
                AiExecutionTaskState::Queued
            } else {
                AiExecutionTaskState::Running
            };
            snapshot.phase = phase;
            snapshot.updated_at = Utc::now().to_rfc3339();
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn update_ai_execution_cleanup(
        &self,
        task_id: &str,
        cleanup: AiExecutionCleanupReport,
    ) -> AppResult<AiExecutionTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: AiExecutionTaskSnapshot = self.decode(&runtime)?;
        if !snapshot.state.is_terminal() {
            snapshot.cleanup = Some(cleanup);
            snapshot.updated_at = Utc::now().to_rfc3339();
            self.write_projection(task_id, &snapshot)?;
        }
        self.projection(task_id)
    }

    #[allow(dead_code)]

    pub(crate) fn finish_ai_execution(
        &self,
        task_id: &str,
        result: Result<AiExecutionResult, AiExecutionError>,
    ) -> AppResult<AiExecutionTaskSnapshot> {
        self.finish_ai_execution_with_phase(task_id, result, None)
    }

    pub(crate) fn finish_ai_execution_with_phase(
        &self,
        task_id: &str,
        result: Result<AiExecutionResult, AiExecutionError>,
        failure_phase: Option<AiExecutionPhase>,
    ) -> AppResult<AiExecutionTaskSnapshot> {
        let runtime_result = result
            .as_ref()
            .map(|value| serde_json::json!({"text": value.text}))
            .map_err(|error| {
                let view = error.to_view();
                crate::backend::application::AppError::Domain {
                    code: view.code,
                    message: view.message,
                    retryable: view.retryable,
                    details: view.phase.map(|phase| serde_json::json!({"phase": phase})),
                }
            });
        self.finish_external_result(task_id, runtime_result)?;
        let mut snapshot: AiExecutionTaskSnapshot =
            self.decode(&self.external_task_snapshot(task_id)?)?;
        match result {
            Ok(result) => {
                snapshot.result = Some(AiExecutionPublicResult { text: result.text });
            }
            Err(error) => {
                let mut error_view = error.to_view();
                // Cleanup is a separate lifecycle observation. Preserve the
                // phase in which the execution failed so a cleanup result
                // cannot hide the actionable root cause.
                error_view.phase = failure_phase.or(Some(snapshot.phase));
                snapshot.error = Some(error_view);
            }
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    #[allow(dead_code)]

    pub(crate) fn cancel_ai_execution(&self, task_id: &str) -> AppResult<AiExecutionTaskSnapshot> {
        self.cancel_external_task(task_id)?;
        self.projection(task_id)
    }

    pub(crate) fn cancel_ai_execution_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<AiExecutionTaskSnapshot> {
        self.cancel_external_task_for_tenant(tenant_id, task_id)?;
        self.projection_for_tenant(tenant_id, task_id)
    }

    #[allow(dead_code)]

    pub(crate) fn ai_execution_snapshot(
        &self,
        task_id: &str,
    ) -> AppResult<Option<AiExecutionTaskSnapshot>> {
        match self.task_runtime.get(task_id) {
            Some(runtime) => self.projection_from_runtime(&runtime).map(Some),
            None => Ok(None),
        }
    }

    pub(crate) fn ai_execution_snapshot_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<Option<AiExecutionTaskSnapshot>> {
        match self.task_runtime.get_for_tenant(tenant_id, task_id) {
            Some(runtime) => self.projection_from_runtime(&runtime).map(Some),
            None => Ok(None),
        }
    }

    #[allow(dead_code)]

    pub(crate) fn ai_execution_snapshots(&self) -> AppResult<Vec<AiExecutionTaskSnapshot>> {
        let mut snapshots =
            self.list_projections::<AiExecutionTaskSnapshot>(TaskKind::AiExecution)?;
        snapshots.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(snapshots)
    }

    pub(crate) fn ai_execution_snapshots_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Vec<AiExecutionTaskSnapshot>> {
        let mut snapshots = self.list_projections_for_tenant::<AiExecutionTaskSnapshot>(
            tenant_id,
            TaskKind::AiExecution,
        )?;
        snapshots.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(snapshots)
    }

    pub(crate) fn cancel_all_ai_executions(&self) -> AppResult<Vec<AiExecutionTaskSnapshot>> {
        let task_ids = self
            .task_runtime
            .list(crate::backend::infrastructure::tasks::TaskFilter {
                kind: Some(TaskKind::AiExecution),
                active_only: true,
                ..Default::default()
            })
            .into_iter()
            .map(|snapshot| snapshot.task_id)
            .collect::<Vec<_>>();
        let mut cancelled = Vec::new();
        for task_id in task_ids {
            self.cancel_external_task(&task_id)?;
            if let Ok(snapshot) = self.projection::<AiExecutionTaskSnapshot>(&task_id) {
                cancelled.push(snapshot);
            }
        }
        cancelled.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(cancelled)
    }

    pub(crate) async fn cancel_ai_executions_and_wait(
        &self,
        timeout: Duration,
        poll_interval: Duration,
    ) -> AppResult<AiExecutionShutdownReport> {
        let cancelled_count = self.cancel_all_ai_executions()?.len();
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining_count = self.active_ai_execution_count()?;
            if remaining_count == 0 {
                return Ok(AiExecutionShutdownReport {
                    cancelled_count,
                    remaining_count,
                    converged: true,
                });
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Ok(AiExecutionShutdownReport {
                    cancelled_count,
                    remaining_count,
                    converged: false,
                });
            }
            let next_poll = now + poll_interval.max(Duration::from_millis(1));
            tokio::time::sleep_until(next_poll.min(deadline)).await;
        }
    }

    fn active_ai_execution_count(&self) -> AppResult<usize> {
        Ok(self
            .task_runtime
            .list(crate::backend::infrastructure::tasks::TaskFilter {
                kind: Some(TaskKind::AiExecution),
                active_only: true,
                ..Default::default()
            })
            .len())
    }
}

impl BackgroundTaskProjection for AiExecutionTaskSnapshot {
    fn project_with_runtime(mut self, runtime: &TaskSnapshot) -> Self {
        match runtime.state {
            TaskState::Pending => self.state = AiExecutionTaskState::Queued,
            TaskState::Running => {
                if !self.state.is_terminal() {
                    self.state = if self.phase == AiExecutionPhase::Queued {
                        AiExecutionTaskState::Queued
                    } else {
                        AiExecutionTaskState::Running
                    };
                }
            }
            TaskState::Cancelling => {
                self.state = AiExecutionTaskState::Running;
                if !matches!(
                    self.phase,
                    AiExecutionPhase::Closing | AiExecutionPhase::CleaningUp
                ) {
                    self.phase = AiExecutionPhase::Cancelling;
                }
            }
            TaskState::Succeeded => {
                self.state = AiExecutionTaskState::Succeeded;
                self.result = runtime.result.clone().and_then(|value| {
                    value
                        .get("text")
                        .and_then(Value::as_str)
                        .map(|text| AiExecutionPublicResult {
                            text: text.to_string(),
                        })
                });
                self.error = None;
            }
            TaskState::Failed => {
                self.state = AiExecutionTaskState::Failed;
                if self.error.is_none() {
                    self.error = runtime.error.as_ref().map(|error| AiExecutionErrorView {
                        code: error.code.clone(),
                        message: error.message.clone(),
                        retryable: error.retryable,
                        phase: Some(self.phase),
                    });
                }
            }
            TaskState::Canceled => {
                self.state = AiExecutionTaskState::Cancelled;
                self.result = None;
                self.error = Some(AiExecutionErrorView {
                    code: "cancelled".to_string(),
                    message: runtime_error_message(runtime)
                        .map(|error| error.message)
                        .unwrap_or_else(|| "AI execution task was cancelled".to_string()),
                    phase: Some(self.phase),
                    retryable: false,
                });
            }
        }
        self.finished_at = runtime.finished_at.clone();
        self
    }
}
