use crate::backend::infrastructure::InfraError;
use crate::backend::store::count_pending_outbox_events_sqlx;
use sqlx::SqlitePool;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::dispatcher::{EventDispatcherShutdownReport, DEFAULT_SHUTDOWN_GRACE};

pub(crate) struct EventDispatcherHandle {
    pub(crate) notify: Arc<tokio::sync::Notify>,
    pub(crate) cancellation: CancellationToken,
    pub(crate) shutdown_deadline: Arc<Mutex<Option<Instant>>>,
    pub(crate) pool: SqlitePool,
    pub(crate) consumer_ids: Vec<String>,
    pub(crate) task: Option<tokio::task::JoinHandle<EventDispatcherShutdownReport>>,
}

impl EventDispatcherHandle {
    pub(crate) fn new(
        notify: Arc<tokio::sync::Notify>,
        cancellation: CancellationToken,
        shutdown_deadline: Arc<Mutex<Option<Instant>>>,
        pool: SqlitePool,
        consumer_ids: Vec<String>,
        task: Option<tokio::task::JoinHandle<EventDispatcherShutdownReport>>,
    ) -> Self {
        Self {
            notify,
            cancellation,
            shutdown_deadline,
            pool,
            consumer_ids,
            task,
        }
    }

    pub(crate) fn notify(&self) {
        self.notify.notify_one();
    }

    pub(crate) async fn stop_until(&mut self, deadline: Instant) -> EventDispatcherShutdownReport {
        if let Ok(mut slot) = self.shutdown_deadline.lock() {
            *slot = Some(deadline);
        }
        self.cancellation.cancel();
        self.notify.notify_one();
        if let Some(mut task) = self.task.take() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(remaining, &mut task).await {
                Ok(Ok(report)) => report,
                Ok(Err(_join_err)) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    let remaining_events = if !remaining.is_zero() {
                        tokio::time::timeout(remaining, self.pending_event_count())
                            .await
                            .ok()
                            .and_then(|res| res.ok())
                            .unwrap_or_default()
                    } else {
                        0
                    };
                    EventDispatcherShutdownReport {
                        drained: false,
                        remaining_events,
                        timed_out: false,
                    }
                }
                Err(_elapsed) => {
                    task.abort();
                    let _ = task.await;
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    let remaining_events = if !remaining.is_zero() {
                        tokio::time::timeout(remaining, self.pending_event_count())
                            .await
                            .ok()
                            .and_then(|res| res.ok())
                            .unwrap_or_default()
                    } else {
                        0
                    };
                    EventDispatcherShutdownReport {
                        drained: false,
                        remaining_events,
                        timed_out: true,
                    }
                }
            }
        } else {
            EventDispatcherShutdownReport::default()
        }
    }

    pub(crate) async fn stop(&mut self) -> EventDispatcherShutdownReport {
        self.stop_until(Instant::now() + DEFAULT_SHUTDOWN_GRACE)
            .await
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn stop_with_timeout(
        &mut self,
        grace: Duration,
    ) -> EventDispatcherShutdownReport {
        self.stop_until(Instant::now() + grace).await
    }

    async fn pending_event_count(&self) -> Result<usize, InfraError> {
        let mut count = 0usize;
        for consumer_id in &self.consumer_ids {
            let pending = count_pending_outbox_events_sqlx(&self.pool, consumer_id).await?;
            count = count.saturating_add(pending.max(0) as usize);
        }
        Ok(count)
    }
}
