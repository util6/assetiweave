use super::task_models::*;
use super::tasks::TaskRuntime;
use super::{InfraError, InfraResult};
use crate::backend::domain::AppErrorView;
use chrono::Utc;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

pub struct TaskContext {
    pub cancellation: CancellationToken,
    pub progress: ProgressHandle,
}

impl TaskContext {
    #[cfg(test)]
    pub fn untracked() -> Self {
        Self::detached("test_untracked")
    }

    /// 构造受控直跑穿透上下文 (Direct No-Op Context - ADR-0017 / Issue #79)
    ///
    /// 专供 Direct 模式下的业务快速内联直跑：
    /// - 内部 runtime 标记为 is_noop，所有的 enter_stage、progress、record_activity 零开销穿透；
    /// - 零事件广播、零数据库访问、零内存锁竞争；
    /// - 业务代码内的阶段划分埋点原样保留，无需任何代码清理即可优雅退出任务中心管理。
    pub fn direct() -> Self {
        Self::direct_with_cancellation(CancellationToken::new())
    }

    /// 携带外部取消令牌的直跑穿透上下文（支持外部超时或调用链协同取消）
    pub fn direct_with_cancellation(cancellation: CancellationToken) -> Self {
        Self {
            cancellation,
            progress: ProgressHandle {
                task_id: "direct:noop".to_string(),
                runtime: TaskRuntime::noop(),
            },
        }
    }

    pub fn detached(reason: &'static str) -> Self {
        tracing::debug!(target: "task_runtime", reason, "Creating detached task context");
        Self {
            cancellation: CancellationToken::new(),
            progress: ProgressHandle {
                task_id: format!("detached:{}", reason),
                runtime: TaskRuntime::new(),
            },
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }
    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }
    pub fn progress(&self) -> ProgressHandle {
        self.progress.clone()
    }
    pub fn runtime(&self) -> &TaskRuntime {
        &self.progress.runtime
    }
    pub fn task_id(&self) -> &str {
        &self.progress.task_id
    }
    pub fn enter_stage(&self, stage_id: impl Into<String>) -> super::task_pipeline::StageGuard {
        super::task_pipeline::StageGuard::enter(
            &self.progress.task_id,
            stage_id,
            self.progress.runtime.clone(),
            self.cancellation.clone(),
        )
    }
    pub fn check_cancellation(&self) -> InfraResult<()> {
        if self.is_cancelled() {
            Err(InfraError::Cancelled("后台任务已取消".to_string()))
        } else {
            Ok(())
        }
    }
    pub fn set_outcome(
        &self,
        outcome: TaskOutcome,
        summary: Option<String>,
        detail: Option<String>,
    ) {
        self.progress.set_outcome(outcome, summary, detail);
    }
}

#[derive(Clone)]
pub struct ProgressHandle {
    pub task_id: String,
    pub runtime: TaskRuntime,
}

impl ProgressHandle {
    pub fn task_id(&self) -> &str {
        &self.task_id
    }
    pub fn enter_stage(
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

    pub fn progress(&self, current: u64, total: Option<u64>, note: Option<&str>) {
        if self.runtime.is_noop {
            return;
        }
        let snapshot = if let Ok(mut tasks) = self.runtime.tasks.lock() {
            if let Some(entry) = tasks.get_mut(&self.task_id) {
                entry.snapshot.progress = Some(TaskProgress {
                    current,
                    total,
                    note: note.map(str::to_string),
                });
                entry.snapshot.revision += 1;
                entry.snapshot.updated_at = Utc::now().to_rfc3339();

                let now = std::time::Instant::now();
                let should_throttle = entry
                    .last_cosmetic_publish
                    .map(|last| now.duration_since(last) < std::time::Duration::from_millis(150))
                    .unwrap_or(false);

                if should_throttle {
                    entry.pending_cosmetic = true;
                    None
                } else {
                    entry.last_cosmetic_publish = Some(now);
                    entry.pending_cosmetic = false;
                    let mut snap = entry.snapshot.clone();
                    snap.change_class = TaskChangeClass::Cosmetic;
                    Some(snap)
                }
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

    pub fn set_stages(&self, stages: Vec<TaskStage>) {
        let _ = self.runtime.set_stages(&self.task_id, stages);
    }

    pub fn update_stage_status(&self, stage_id: &str, status: StageStatus) {
        let _ = self
            .runtime
            .update_stage_status(&self.task_id, stage_id, status);
    }

    pub fn record_activity(&self, activity: TaskActivity) {
        let _ = self.runtime.record_activity(&self.task_id, activity);
    }

    pub fn remove_activity(&self, stage_id: &str, worker_id: &str) {
        let _ = self
            .runtime
            .remove_activity(&self.task_id, stage_id, worker_id);
    }

    pub fn finish_stage(
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

    pub fn set_outcome(
        &self,
        outcome: TaskOutcome,
        result_summary: Option<String>,
        error_summary: Option<String>,
    ) {
        let _ = self
            .runtime
            .set_outcome(&self.task_id, outcome, result_summary, error_summary);
    }

    pub fn set_stage_agent_session_ref(
        &self,
        stage_id: &str,
        session_ref: Option<crate::backend::domain::agents::AgentSessionRef>,
    ) {
        let _ = self
            .runtime
            .set_stage_agent_session_ref(&self.task_id, stage_id, session_ref);
    }
}

pub type TaskFn = Box<dyn FnOnce(TaskContext) -> Result<Value, AppErrorView> + Send + 'static>;
