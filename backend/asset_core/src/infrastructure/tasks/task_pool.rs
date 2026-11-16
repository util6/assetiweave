use super::task_models::TaskKind;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

/// 任务资源域划分，用于控制同域并发度并执行 FIFO 排队 (Issue #187)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResourcePool {
    SqliteWrite,
    DiskIo,
    Network,
    AgentLlm,
    #[default]
    Unbounded,
}

impl ResourcePool {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SqliteWrite => "sqlite_write",
            Self::DiskIo => "disk_io",
            Self::Network => "network",
            Self::AgentLlm => "agent_llm",
            Self::Unbounded => "unbounded",
        }
    }

    pub fn capacity(self) -> Option<usize> {
        match self {
            Self::SqliteWrite => Some(1),
            Self::DiskIo => Some(2),
            Self::Network => Some(3),
            Self::AgentLlm => Some(2),
            Self::Unbounded => None,
        }
    }
}

impl TaskKind {
    /// 各任务类型在未显式覆盖时的默认资源域归属。
    pub fn default_resource_pool(&self) -> ResourcePool {
        match self {
            Self::ConversationSync
            | Self::ConversationUsageScan
            | Self::ConversationDataMaintenance
            | Self::SearchIndexRebuild => ResourcePool::SqliteWrite,

            Self::ConversationContentSearch | Self::Scan | Self::Backup | Self::BatchMount => {
                ResourcePool::DiskIo
            }

            Self::RemoteSkillAcquire
            | Self::GitSourcePull
            | Self::AgentMarketRefresh
            | Self::ModelPricingSync
            | Self::ExtensionLifecycle => ResourcePool::Network,

            Self::AiExecution | Self::Memory => ResourcePool::AgentLlm,

            Self::ScriptInstall | Self::Other => ResourcePool::Unbounded,
        }
    }
}

#[derive(Debug)]
struct PoolQueueState {
    capacity: usize,
    active: usize,
    waiters: VecDeque<oneshot::Sender<()>>,
}

impl PoolQueueState {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            active: 0,
            waiters: VecDeque::new(),
        }
    }
}

/// 确定性 FIFO 非抢占式资源域并发限制器。
#[derive(Debug)]
pub struct ResourcePoolLimiter {
    queues: [Arc<Mutex<PoolQueueState>>; 4],
}

impl Default for ResourcePoolLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourcePoolLimiter {
    fn pool_index(pool: ResourcePool) -> Option<usize> {
        match pool {
            ResourcePool::SqliteWrite => Some(0),
            ResourcePool::DiskIo => Some(1),
            ResourcePool::Network => Some(2),
            ResourcePool::AgentLlm => Some(3),
            ResourcePool::Unbounded => None,
        }
    }

    pub fn new() -> Self {
        const BOUNDED_POOLS: [ResourcePool; 4] = [
            ResourcePool::SqliteWrite,
            ResourcePool::DiskIo,
            ResourcePool::Network,
            ResourcePool::AgentLlm,
        ];
        Self {
            queues: BOUNDED_POOLS.map(|pool| {
                Arc::new(Mutex::new(PoolQueueState::new(
                    pool.capacity().unwrap_or(1),
                )))
            }),
        }
    }

    fn queue_for(&self, pool: ResourcePool) -> Option<Arc<Mutex<PoolQueueState>>> {
        Self::pool_index(pool).map(|idx| self.queues[idx].clone())
    }

    /// 同步申请资源槽位或按提交顺序入队，确保后续 `tokio::spawn` 调度不破坏 FIFO 顺序。
    pub fn acquire_or_enqueue(&self, pool: ResourcePool) -> PoolSlot {
        let Some(queue) = self.queue_for(pool) else {
            return PoolSlot::Unbounded;
        };
        {
            let mut state = match queue.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            if state.active < state.capacity && state.waiters.is_empty() {
                state.active += 1;
                drop(state);
                return PoolSlot::Granted(PoolPermit { queue });
            }
            let (tx, rx) = oneshot::channel();
            state.waiters.push_back(tx);
            drop(state);
            PoolSlot::Queued(PoolWaiter {
                queue: Some(queue),
                rx,
            })
        }
    }
}

pub enum PoolSlot {
    Unbounded,
    Granted(PoolPermit),
    Queued(PoolWaiter),
}

pub enum PoolAcquireOutcome {
    Granted(Option<PoolPermit>),
    Cancelled,
}

impl PoolSlot {
    pub fn is_queued(&self) -> bool {
        matches!(self, Self::Queued(_))
    }

    /// 异步等待资源许可；若任务在排队期间被取消，返回 `Cancelled` 并释放排队占位。
    pub async fn await_permit(self, cancellation: &CancellationToken) -> PoolAcquireOutcome {
        match self {
            Self::Unbounded => PoolAcquireOutcome::Granted(None),
            Self::Granted(permit) => PoolAcquireOutcome::Granted(Some(permit)),
            Self::Queued(waiter) => match waiter.wait(cancellation).await {
                Some(permit) => PoolAcquireOutcome::Granted(Some(permit)),
                None => PoolAcquireOutcome::Cancelled,
            },
        }
    }
}

pub struct PoolWaiter {
    queue: Option<Arc<Mutex<PoolQueueState>>>,
    rx: oneshot::Receiver<()>,
}

impl PoolWaiter {
    async fn wait(mut self, cancellation: &CancellationToken) -> Option<PoolPermit> {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => None,
            res = &mut self.rx => match res {
                Ok(()) => {
                    let queue = self.queue.take()?;
                    let permit = PoolPermit { queue };
                    if cancellation.is_cancelled() {
                        drop(permit);
                        None
                    } else {
                        Some(permit)
                    }
                }
                Err(_) => None,
            }
        }
    }
}

impl Drop for PoolWaiter {
    fn drop(&mut self) {
        let Some(queue) = self.queue.take() else {
            return;
        };
        // 关闭接收端阻止后续 send 成功；若并发的 send 已成功，则立即归还许可防止槽位泄漏。
        self.rx.close();
        if self.rx.try_recv().is_ok() {
            drop(PoolPermit { queue });
        }
    }
}

pub struct PoolPermit {
    queue: Arc<Mutex<PoolQueueState>>,
}

impl Drop for PoolPermit {
    fn drop(&mut self) {
        let mut state = match self.queue.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        while let Some(tx) = state.waiters.pop_front() {
            if tx.send(()).is_ok() {
                return;
            }
        }
        state.active = state.active.saturating_sub(1);
    }
}
