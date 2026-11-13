use super::task_activity::WorkerTracker;
use super::tasks::{
    StageStatus, TaskActivity, TaskFailure, TaskKind, TaskMetric, TaskRuntime, TaskSkippedGroup,
    TaskStage,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
pub const STAGE_TIMEOUT_ERROR_CODE: &str = "stage_timeout";

pub mod stages {
    pub const DISCOVER: &str = "discover";
    pub const SCAN_FILES: &str = "scan_files";
    pub const PERSIST_INDEX: &str = "persist_index";
    pub const VERIFY: &str = "verify";
    pub const FINALIZE: &str = "finalize";
    pub const PREPARE: &str = "prepare";
    pub const PRUNE: &str = "prune";
    pub const VACUUM: &str = "vacuum";
    pub const SESSION_FACTS: &str = "session_facts";
    pub const RECENT_SNAPSHOT: &str = "recent_snapshot";
    pub const GLOBAL_CONSOLIDATION: &str = "global_consolidation";
    pub const MARKDOWN_PROJECTION: &str = "markdown_projection";
    pub const AGENT_DOWNLOAD: &str = "download";
    pub const AGENT_INSTALL: &str = "install";
    pub const AGENT_VERIFY: &str = "verify";
    pub const AGENT_CLEANUP: &str = "cleanup";
}

/// 开放任务分类。
///
/// 既可以承接系统内置的 `TaskKind` 命名空间，也可以承接第三方或动态扩展的自由命名空间。
/// 例如：`conversation/sync`、`system/backup`、`custom/my_flow`。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskCategory(pub String);

impl TaskCategory {
    pub fn new(category: impl Into<String>) -> Self {
        Self(category.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for TaskCategory {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for TaskCategory {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<TaskKind> for TaskCategory {
    fn from(kind: TaskKind) -> Self {
        Self(kind.default_category_string().to_string())
    }
}

impl std::fmt::Display for TaskCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 流水线阶段描述符。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageDescriptor {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub user_visible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

fn default_true() -> bool {
    true
}

impl StageDescriptor {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            user_visible: true,
            timeout_ms: None,
        }
    }

    pub fn with_user_visible(mut self, user_visible: bool) -> Self {
        self.user_visible = user_visible;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout_ms = Some(timeout.as_millis().max(1) as u64);
        self
    }
}

/// 声明式流水线骨架描述符。
///
/// 任务提交时提供该骨架，TaskRuntime 将在任务初始化阶段预置全量 stages（状态为 Pending），
/// 确保前端/CLI 一次性感知完整执行计划，杜绝运行时突兀跳出新阶段。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineDescriptor {
    pub stages: Vec<StageDescriptor>,
}

impl PipelineDescriptor {
    pub fn new(stages: impl IntoIterator<Item = StageDescriptor>) -> Self {
        Self {
            stages: stages.into_iter().collect(),
        }
    }

    pub fn builder() -> PipelineDescriptorBuilder {
        PipelineDescriptorBuilder::default()
    }

    pub fn to_stage_timeouts(&self) -> HashMap<String, Duration> {
        self.stages
            .iter()
            .filter_map(|desc| {
                desc.timeout_ms
                    .filter(|ms| *ms > 0)
                    .map(|ms| (desc.id.clone(), Duration::from_millis(ms)))
            })
            .collect()
    }

    /// 将描述符转换为初始 TaskStage 列表（全部预置为 Pending）。
    pub fn to_initial_stages(&self) -> Vec<TaskStage> {
        self.stages
            .iter()
            .map(|desc| TaskStage {
                id: desc.id.clone(),
                name: desc.name.clone(),
                status: StageStatus::Pending,
                started_at: None,
                finished_at: None,
                duration_ms: None,
                progress: None,
                current_activities: Vec::new(),
                metrics: Vec::new(),
                failures: Vec::new(),
                skipped: Vec::new(),
                agent_session_ref: None,
                steps: Vec::new(),
            })
            .collect()
    }
}

/// 流水线描述符构建器。
#[derive(Debug, Default)]
pub struct PipelineDescriptorBuilder {
    stages: Vec<StageDescriptor>,
}

impl PipelineDescriptorBuilder {
    pub fn stage(mut self, id: impl Into<String>, name: impl Into<String>) -> Self {
        self.stages.push(StageDescriptor::new(id, name));
        self
    }

    pub fn stage_with_visibility(
        mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        user_visible: bool,
    ) -> Self {
        self.stages
            .push(StageDescriptor::new(id, name).with_user_visible(user_visible));
        self
    }

    /// 为最近添加的阶段附加软超时配置。
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        if let Some(last) = self.stages.pop() {
            self.stages.push(last.with_timeout(timeout));
        }
        self
    }

    pub fn build(self) -> PipelineDescriptor {
        PipelineDescriptor {
            stages: self.stages,
        }
    }
}

/// 作用域阶段哨兵 (RAII StageGuard)
///
/// 生命周期与流水线阶段严格绑定：
/// 1. 构造/enter 时自动将阶段标记为 `Running` 并记录开始时间；
/// 2. 离开作用域 (Drop) 时：
///    - 若发生 Panic 展开，自动闭环为 `Failed` 并记录 `stage_panic` 失败凭证；
///    - 若收到取消信号，自动闭环为 `Canceled`；
///    - 若内部已收集失败项或显式标记失败，自动闭环为 `Failed`；
///    - 若正常结束且未指定终态，自动以 `Succeeded` 终结，并自动计算耗时；
///    - 杜绝传统异步任务中因 `?` 早期返回或漏调 finish 导致的阶段永久挂起。
pub struct StageGuard {
    task_id: String,
    stage_id: String,
    runtime: TaskRuntime,
    task_cancellation: CancellationToken,
    stage_cancellation: CancellationToken,
    timed_out: Arc<AtomicBool>,
    watchdog_abort: Option<tokio::task::AbortHandle>,
    status: Option<StageStatus>,
    metrics: Vec<TaskMetric>,
    failures: Vec<TaskFailure>,
    skipped: Vec<TaskSkippedGroup>,
    completed: bool,
}

impl StageGuard {
    pub fn enter(
        task_id: impl Into<String>,
        stage_id: impl Into<String>,
        runtime: TaskRuntime,
        cancellation: CancellationToken,
    ) -> Self {
        let task_id = task_id.into();
        let stage_id = stage_id.into();
        let configured_timeout = runtime.stage_timeout(&task_id, &stage_id);
        let _ = runtime.update_stage_status(&task_id, &stage_id, StageStatus::Running);
        let stage_cancellation = cancellation.child_token();
        let mut guard = Self {
            task_id,
            stage_id,
            runtime,
            task_cancellation: cancellation,
            stage_cancellation,
            timed_out: Arc::new(AtomicBool::new(false)),
            watchdog_abort: None,
            status: None,
            metrics: Vec::new(),
            failures: Vec::new(),
            skipped: Vec::new(),
            completed: false,
        };
        if let Some(timeout) = configured_timeout {
            guard.arm_watchdog(timeout);
        }
        guard
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.arm_watchdog(timeout);
        self
    }

    fn arm_watchdog(&mut self, timeout: Duration) {
        if let Some(abort) = self.watchdog_abort.take() {
            abort.abort();
        }
        let handle_opt = self
            .runtime
            .runtime_handle()
            .or_else(|| tokio::runtime::Handle::try_current().ok());
        let Some(handle) = handle_opt else {
            return;
        };
        let task_cancel = self.task_cancellation.clone();
        let stage_cancel = self.stage_cancellation.clone();
        let timed_out = self.timed_out.clone();
        let join = handle.spawn(async move {
            tokio::select! {
                _ = task_cancel.cancelled() => {}
                _ = tokio::time::sleep(timeout) => {
                    timed_out.store(true, Ordering::Release);
                    stage_cancel.cancel();
                }
            }
        });
        self.watchdog_abort = Some(join.abort_handle());
    }

    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    pub fn stage_id(&self) -> &str {
        &self.stage_id
    }

    pub fn is_cancelled(&self) -> bool {
        self.task_cancellation.is_cancelled()
    }

    pub fn is_timed_out(&self) -> bool {
        self.timed_out.load(Ordering::Acquire) && !self.task_cancellation.is_cancelled()
    }

    pub fn cancellation(&self) -> &CancellationToken {
        &self.stage_cancellation
    }

    pub fn record_metric(&mut self, code: impl Into<String>, value: u64) {
        let code = code.into();
        if let Some(existing) = self.metrics.iter_mut().find(|m| m.code == code) {
            existing.value += value;
        } else {
            self.metrics.push(TaskMetric { code, value });
        }
    }

    pub fn record_failure(
        &mut self,
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
    ) {
        self.failures.push(TaskFailure {
            code: code.into(),
            message: message.into(),
            stage: self.stage_id.clone(),
            identity: None,
            retryable,
            path: None,
            timestamp: Utc::now().to_rfc3339(),
        });
    }

    pub fn record_skipped(&mut self, reason_code: impl Into<String>, sample: impl Into<String>) {
        let reason_code = reason_code.into();
        let sample = sample.into();
        if let Some(group) = self
            .skipped
            .iter_mut()
            .find(|g| g.reason_code == reason_code)
        {
            group.count += 1;
            if group.samples.len() < 5 {
                group.samples.push(sample);
            }
        } else {
            self.skipped.push(TaskSkippedGroup {
                reason_code,
                count: 1,
                samples: vec![sample],
            });
        }
    }

    pub fn worker(&self, worker_id: impl Into<String>) -> WorkerTracker {
        WorkerTracker::new(
            self.task_id.clone(),
            self.stage_id.clone(),
            worker_id,
            self.runtime.clone(),
        )
    }

    pub fn progress(&self, current: u64, total: Option<u64>, note: Option<String>) {
        self.set_progress(current, total, note);
    }

    pub fn record_step(
        &self,
        operation: impl Into<String>,
        detail: Option<String>,
        current: Option<u64>,
        total: Option<u64>,
    ) {
        let _ = self.runtime.record_stage_step(
            &self.task_id,
            &self.stage_id,
            operation,
            detail,
            current,
            total,
        );
    }

    pub fn activity(
        &self,
        worker_id: impl Into<String>,
        operation: impl Into<String>,
        display_path: Option<String>,
    ) {
        let activity = TaskActivity {
            stage_id: self.stage_id.clone(),
            worker_id: worker_id.into(),
            operation: operation.into(),
            path: None,
            display_path,
            started_at: Utc::now().to_rfc3339(),
            current: None,
            total: None,
        };
        let _ = self.runtime.record_activity(&self.task_id, activity);
    }

    pub fn remove_activity(&self, worker_id: &str) {
        let _ = self
            .runtime
            .remove_activity(&self.task_id, &self.stage_id, worker_id);
    }

    pub fn record_skipped_group(
        &mut self,
        reason_code: impl Into<String>,
        samples: impl IntoIterator<Item = impl Into<String>>,
    ) {
        let reason_code = reason_code.into();
        let samples: Vec<String> = samples.into_iter().map(Into::into).collect();
        let count = samples.len() as u64;
        if let Some(group) = self
            .skipped
            .iter_mut()
            .find(|g| g.reason_code == reason_code)
        {
            group.count += count;
            for sample in samples {
                if group.samples.len() < 5 {
                    group.samples.push(sample);
                }
            }
        } else {
            let mut stored_samples = samples;
            stored_samples.truncate(5);
            self.skipped.push(TaskSkippedGroup {
                reason_code,
                count,
                samples: stored_samples,
            });
        }
    }

    pub fn set_progress(&self, current: u64, total: Option<u64>, note: Option<String>) {
        let _ =
            self.runtime
                .set_stage_progress(&self.task_id, &self.stage_id, current, total, note);
    }

    pub fn set_agent_session_ref(
        &self,
        session_ref: Option<crate::backend::domain::agents::AgentSessionRef>,
    ) {
        let _ =
            self.runtime
                .set_stage_agent_session_ref(&self.task_id, &self.stage_id, session_ref);
    }

    /// 显式标记阶段为跳过 (Skipped)
    pub fn skip(&mut self, reason_code: impl Into<String>, reason_message: impl Into<String>) {
        self.status = Some(StageStatus::Skipped);
        self.record_skipped(reason_code, reason_message);
    }

    /// 显式标记阶段为失败 (Failed)
    pub fn fail(&mut self, code: impl Into<String>, message: impl Into<String>, retryable: bool) {
        self.status = Some(StageStatus::Failed);
        self.record_failure(code, message, retryable);
    }

    /// 显式指定闭环状态
    pub fn finish_with_status(&mut self, status: StageStatus) {
        self.status = Some(status);
    }

    /// 提前闭环 StageGuard，不再等待 drop
    pub fn finish(mut self) {
        self.do_finish();
    }

    fn do_finish(&mut self) {
        if self.completed {
            return;
        }
        self.completed = true;
        if let Some(abort) = self.watchdog_abort.take() {
            abort.abort();
        }

        let timed_out = self.is_timed_out();
        if timed_out
            && !self
                .failures
                .iter()
                .any(|f| f.code == STAGE_TIMEOUT_ERROR_CODE)
        {
            self.record_failure(
                STAGE_TIMEOUT_ERROR_CODE,
                format!("阶段 '{}' 执行超时", self.stage_id),
                true,
            );
        }

        let final_status = if std::thread::panicking() {
            self.failures.push(TaskFailure {
                code: "stage_panic".to_string(),
                message: "阶段执行过程中触发意外 panic".to_string(),
                stage: self.stage_id.clone(),
                identity: None,
                retryable: false,
                path: None,
                timestamp: Utc::now().to_rfc3339(),
            });
            StageStatus::Failed
        } else if let Some(explicit) = self.status {
            explicit
        } else if self.task_cancellation.is_cancelled() {
            StageStatus::Canceled
        } else if timed_out || !self.failures.is_empty() {
            StageStatus::Failed
        } else {
            StageStatus::Succeeded
        };

        let _ = self.runtime.finish_stage(
            &self.task_id,
            &self.stage_id,
            final_status,
            std::mem::take(&mut self.metrics),
            std::mem::take(&mut self.failures),
            std::mem::take(&mut self.skipped),
        );
    }
}

impl Drop for StageGuard {
    fn drop(&mut self) {
        self.do_finish();
    }
}

#[cfg(test)]
#[path = "task_pipeline_tests.rs"]
mod tests;
