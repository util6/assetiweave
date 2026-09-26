use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::app_runtime::AppRuntime;
use crate::backend::infrastructure::events::EventDispatcherShutdownReport;

#[derive(Debug, Default)]
pub(crate) struct ShutdownState {
    accepting: AtomicBool,
    shutdown_started: AtomicBool,
    finished_report: std::sync::Mutex<Option<ShutdownReport>>,
}

impl ShutdownState {
    pub(crate) fn new() -> Self {
        Self {
            accepting: AtomicBool::new(true),
            shutdown_started: AtomicBool::new(false),
            finished_report: std::sync::Mutex::new(None),
        }
    }

    pub(crate) fn begin(&self) -> bool {
        self.accepting.store(false, Ordering::Release);
        !self.shutdown_started.swap(true, Ordering::AcqRel)
    }

    pub(crate) fn get_finished_report(&self) -> Option<ShutdownReport> {
        self.finished_report
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
    }

    pub(crate) fn set_finished_report(&self, report: ShutdownReport) {
        if let Ok(mut guard) = self.finished_report.lock() {
            *guard = Some(report);
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ShutdownReport {
    pub(crate) unfinished_task_ids: Vec<String>,
    pub(crate) dispatcher_drained: bool,
    pub(crate) dispatcher_remaining_events: usize,
    pub(crate) dispatcher_timed_out: bool,
    pub(crate) unfinished_stages: Vec<String>,
}

impl Default for ShutdownReport {
    fn default() -> Self {
        Self {
            unfinished_task_ids: Vec::new(),
            dispatcher_drained: true,
            dispatcher_remaining_events: 0,
            dispatcher_timed_out: false,
            unfinished_stages: Vec::new(),
        }
    }
}

impl ShutdownReport {
    pub(crate) fn is_clean(&self) -> bool {
        self.unfinished_task_ids.is_empty()
            && self.dispatcher_drained
            && self.dispatcher_remaining_events == 0
            && !self.dispatcher_timed_out
            && self.unfinished_stages.is_empty()
    }
}

pub(super) struct SessionMemoryCoordinatorHandle {
    pub(super) cancellation: CancellationToken,
    pub(super) join: Option<tokio::task::JoinHandle<()>>,
}

impl SessionMemoryCoordinatorHandle {
    pub(super) async fn stop_until(&mut self, deadline: Instant) -> bool {
        self.cancellation.cancel();
        if let Some(join) = self.join.as_mut() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(remaining, &mut *join).await {
                Ok(res) => {
                    let _ = res;
                    true
                }
                Err(_) => {
                    join.abort();
                    false
                }
            }
        } else {
            true
        }
    }
}

impl AppRuntime {
    /// Stop accepting work and wait for resident tasks before close-time
    /// persistence runs. The final dispatcher/database shutdown remains in
    /// `shutdown_with_grace` so callers can persist through this same runtime.
    #[allow(dead_code)]
    pub(crate) async fn stop_tasks_with_grace(&self, grace: Duration) -> Vec<String> {
        self.stop_tasks_until(Instant::now() + grace).await
    }

    pub(crate) async fn stop_tasks_until(&self, deadline: Instant) -> Vec<String> {
        let clean = self.stop_session_memory_coordinator_until(deadline).await;
        if !clean {
            self.coordinator_timed_out.store(true, Ordering::Release);
        }
        self.task_runtime.stop_accepting();
        self.task_runtime
            .shutdown_until(deadline)
            .await
            .unfinished_task_ids
    }

    pub(super) async fn stop_session_memory_coordinator_until(&self, deadline: Instant) -> bool {
        let handle = self
            .session_memory_coordinator
            .lock()
            .ok()
            .and_then(|mut slot| slot.take());
        if let Some(mut handle) = handle {
            handle.stop_until(deadline).await
        } else {
            true
        }
    }

    pub(crate) fn register_session_memory_coordinator(
        &self,
        cancellation: CancellationToken,
        join: tokio::task::JoinHandle<()>,
    ) {
        if let Ok(mut slot) = self.session_memory_coordinator.lock() {
            *slot = Some(SessionMemoryCoordinatorHandle {
                cancellation,
                join: Some(join),
            });
        }
    }

    pub(crate) async fn shutdown_until(&self, deadline: Instant) -> ShutdownReport {
        let _gate = self.shutdown_gate.lock().await;
        if let Some(report) = self.shutdown.get_finished_report() {
            return report;
        }

        self.shutdown.begin();
        self.task_runtime.stop_accepting();

        let mut unfinished_stages = Vec::new();

        // 1. Session memory coordinator
        let coordinator_clean = self.stop_session_memory_coordinator_until(deadline).await;
        if !coordinator_clean || self.coordinator_timed_out.load(Ordering::Acquire) {
            unfinished_stages.push("session_memory_coordinator".to_string());
        }

        // 2. Task runtime: cancels active tasks and awaits tracked tasks until deadline
        let task_report = self.task_runtime.shutdown_until(deadline).await;
        if !task_report.unfinished_task_ids.is_empty() {
            unfinished_stages.push("tasks".to_string());
        }

        // 3. Dispatcher: drain domain events until deadline
        let mut dispatcher_handle = self.dispatcher.lock().ok().and_then(|mut slot| slot.take());
        let dispatcher_report = match dispatcher_handle.as_mut() {
            Some(handle) => handle.stop_until(deadline).await,
            None => EventDispatcherShutdownReport::default(),
        };
        if dispatcher_report.timed_out || !dispatcher_report.drained {
            unfinished_stages.push("dispatcher".to_string());
        }

        // 4. Session streams
        self.session_streams.clear();

        // 5. Database pool: close bounded by remaining deadline
        let pool = self.db.pool().clone();
        let remaining = deadline.saturating_duration_since(Instant::now());
        if tokio::time::timeout(remaining, pool.close()).await.is_err() {
            unfinished_stages.push("database_pool".to_string());
        }

        let report = ShutdownReport {
            unfinished_task_ids: task_report.unfinished_task_ids,
            dispatcher_drained: dispatcher_report.drained,
            dispatcher_remaining_events: dispatcher_report.remaining_events,
            dispatcher_timed_out: dispatcher_report.timed_out,
            unfinished_stages,
        };

        self.shutdown.set_finished_report(report.clone());
        report
    }

    pub(crate) async fn shutdown_with_grace(&self, grace: Duration) -> ShutdownReport {
        self.shutdown_until(Instant::now() + grace).await
    }
}
