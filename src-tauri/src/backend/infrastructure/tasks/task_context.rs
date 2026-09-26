use super::task_models::*;
use super::tasks::TaskRuntime;
use super::{InfraError, InfraResult};
use crate::backend::domain::AppErrorView;
use chrono::Utc;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

pub(crate) struct TaskContext {
    pub(crate) cancellation: CancellationToken,
    pub(crate) progress: ProgressHandle,
}

impl TaskContext {
    pub(crate) fn untracked() -> Self {
        Self {
            cancellation: CancellationToken::new(),
            progress: ProgressHandle {
                task_id: String::new(),
                runtime: TaskRuntime::new(),
            },
        }
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }
    pub(crate) fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }
    pub(crate) fn progress(&self) -> ProgressHandle {
        self.progress.clone()
    }
    pub(crate) fn runtime(&self) -> &TaskRuntime {
        &self.progress.runtime
    }
    pub(crate) fn task_id(&self) -> &str {
        &self.progress.task_id
    }
    pub(crate) fn enter_stage(
        &self,
        stage_id: impl Into<String>,
    ) -> super::task_pipeline::StageGuard {
        super::task_pipeline::StageGuard::enter(
            &self.progress.task_id,
            stage_id,
            self.progress.runtime.clone(),
            self.cancellation.clone(),
        )
    }
    pub(crate) fn check_cancellation(&self) -> InfraResult<()> {
        if self.is_cancelled() {
            Err(InfraError::Cancelled("后台任务已取消".to_string()))
        } else {
            Ok(())
        }
    }
    pub(crate) fn set_outcome(
        &self,
        outcome: TaskOutcome,
        summary: Option<String>,
        detail: Option<String>,
    ) {
        self.progress.set_outcome(outcome, summary, detail);
    }
}

#[derive(Clone)]
pub(crate) struct ProgressHandle {
    pub(crate) task_id: String,
    pub(crate) runtime: TaskRuntime,
}

impl ProgressHandle {
    pub(crate) fn task_id(&self) -> &str {
        &self.task_id
    }
    pub(crate) fn enter_stage(
        &self,
        stage_id: impl Into<String>,
        cancellation: CancellationToken,
    ) -> super::task_pipeline::StageGuard {
        super::task_pipeline::StageGuard::enter(
            &self.task_id,
            stage_id,
            self.runtime.clone(),
            cancellation,
        )
    }

    pub(crate) fn progress(&self, current: u64, total: Option<u64>, note: Option<&str>) {
        let snapshot = if let Ok(mut tasks) = self.runtime.tasks.lock() {
            if let Some(entry) = tasks.get_mut(&self.task_id) {
                entry.snapshot.progress = Some(TaskProgress {
                    current,
                    total,
                    note: note.map(str::to_string),
                });
                entry.snapshot.revision += 1;
                entry.snapshot.updated_at = Utc::now().to_rfc3339();
                Some(entry.snapshot.clone())
            } else {
                None
            }
        } else {
            None
        };
        if let Some(snapshot) = snapshot {
            self.runtime.publish(&snapshot);
        }
    }

    pub(crate) fn set_stages(&self, stages: Vec<TaskStage>) {
        let _ = self.runtime.set_stages(&self.task_id, stages);
    }

    pub(crate) fn update_stage_status(&self, stage_id: &str, status: StageStatus) {
        let _ = self
            .runtime
            .update_stage_status(&self.task_id, stage_id, status);
    }

    pub(crate) fn record_activity(&self, activity: TaskActivity) {
        let _ = self.runtime.record_activity(&self.task_id, activity);
    }

    pub(crate) fn remove_activity(&self, stage_id: &str, worker_id: &str) {
        let _ = self
            .runtime
            .remove_activity(&self.task_id, stage_id, worker_id);
    }

    pub(crate) fn finish_stage(
        &self,
        stage_id: &str,
        status: StageStatus,
        metrics: Vec<TaskMetric>,
        failures: Vec<TaskFailure>,
        skipped: Vec<TaskSkippedGroup>,
    ) {
        let _ =
            self.runtime
                .finish_stage(&self.task_id, stage_id, status, metrics, failures, skipped);
    }

    pub(crate) fn set_outcome(
        &self,
        outcome: TaskOutcome,
        result_summary: Option<String>,
        error_summary: Option<String>,
    ) {
        let _ = self
            .runtime
            .set_outcome(&self.task_id, outcome, result_summary, error_summary);
    }

    pub(crate) fn set_stage_agent_session_ref(
        &self,
        stage_id: &str,
        session_ref: Option<crate::backend::domain::agents::AgentSessionRef>,
    ) {
        let _ = self
            .runtime
            .set_stage_agent_session_ref(&self.task_id, stage_id, session_ref);
    }
}

pub(crate) type TaskFn =
    Box<dyn FnOnce(TaskContext) -> Result<Value, AppErrorView> + Send + 'static>;
