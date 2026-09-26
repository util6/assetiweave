use super::tasks::{TaskActivity, TaskRuntime};
use chrono::Utc;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub const DEFAULT_ACTIVITY_THROTTLE_INTERVAL: Duration = Duration::from_millis(100);

/// Worker 细粒度活动追踪器 (带节流上报与 RAII 自动清理)
///
/// 专为并发 Worker 或高频循环设计：
/// 1. 内存中活动状态（`stage.current_activities`）即时最新更新；
/// 2. 外部广播（IPC 事件）默认进行 100ms 窗口节流，消除事件风暴；
/// 3. Worker 退出或 drop 时自动清理活动并触发即时广播。
pub struct WorkerTracker {
    task_id: String,
    stage_id: String,
    worker_id: String,
    runtime: TaskRuntime,
    throttle_interval: Duration,
    last_published: Arc<Mutex<Instant>>,
    completed: bool,
}

impl WorkerTracker {
    pub(crate) fn new(
        task_id: impl Into<String>,
        stage_id: impl Into<String>,
        worker_id: impl Into<String>,
        runtime: TaskRuntime,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            stage_id: stage_id.into(),
            worker_id: worker_id.into(),
            runtime,
            throttle_interval: DEFAULT_ACTIVITY_THROTTLE_INTERVAL,
            last_published: Arc::new(Mutex::new(
                Instant::now()
                    .checked_sub(DEFAULT_ACTIVITY_THROTTLE_INTERVAL)
                    .unwrap_or_else(Instant::now),
            )),
            completed: false,
        }
    }

    pub fn with_throttle_interval(mut self, interval: Duration) -> Self {
        self.throttle_interval = interval;
        self
    }

    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    pub fn stage_id(&self) -> &str {
        &self.stage_id
    }

    /// 上报该 Worker 的当前细粒度活动
    pub fn report(
        &self,
        operation: impl Into<String>,
        current: Option<u64>,
        total: Option<u64>,
        display_path: Option<String>,
    ) {
        let activity = TaskActivity {
            stage_id: self.stage_id.clone(),
            worker_id: self.worker_id.clone(),
            operation: operation.into(),
            path: None,
            display_path,
            started_at: Utc::now().to_rfc3339(),
            current,
            total,
        };

        let should_publish = {
            let mut last = self
                .last_published
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            let now = Instant::now();
            if now.duration_since(*last) >= self.throttle_interval {
                *last = now;
                true
            } else {
                false
            }
        };

        if should_publish {
            let _ = self.runtime.record_activity(&self.task_id, activity);
        } else {
            let _ = self.runtime.record_activity_silent(&self.task_id, activity);
        }
    }

    /// 显式标记完成 Worker 活动
    pub fn complete(mut self) {
        self.do_complete();
    }

    fn do_complete(&mut self) {
        if self.completed {
            return;
        }
        self.completed = true;
        let _ = self
            .runtime
            .remove_activity(&self.task_id, &self.stage_id, &self.worker_id);
    }
}

impl Drop for WorkerTracker {
    fn drop(&mut self) {
        self.do_complete();
    }
}

#[cfg(test)]
#[path = "task_activity_tests.rs"]
mod tests;
