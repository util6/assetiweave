use std::marker::PhantomData;
use tokio_util::sync::CancellationToken;

/// 强类型任务输出。
///
/// 包含具体业务的强类型结果 `data`，以及面向用户界面展示的友好摘要 `summary`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskOutput<T> {
    pub data: T,
    pub summary: Option<String>,
}

impl<T> TaskOutput<T> {
    pub fn new(data: T) -> Self {
        Self {
            data,
            summary: None,
        }
    }

    pub fn with_summary(data: T, summary: impl Into<String>) -> Self {
        Self {
            data,
            summary: Some(summary.into()),
        }
    }
}

impl<T> From<T> for TaskOutput<T> {
    fn from(data: T) -> Self {
        Self::new(data)
    }
}

/// 强类型任务执行句柄。
///
/// 携带对应任务的 ID 与取消令牌，便于调用方优雅取消或查询状态。
#[derive(Debug, Clone)]
pub struct TaskHandle<T> {
    pub task_id: String,
    pub cancellation: CancellationToken,
    _marker: PhantomData<T>,
}

impl<T> TaskHandle<T> {
    pub(crate) fn new(task_id: String, cancellation: CancellationToken) -> Self {
        Self {
            task_id,
            cancellation,
            _marker: PhantomData,
        }
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }
}

#[cfg(test)]
#[path = "task_runner_tests.rs"]
mod tests;
