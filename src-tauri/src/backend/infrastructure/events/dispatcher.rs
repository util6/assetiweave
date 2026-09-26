use super::{
    ConsumerCx, ConsumerFuture, DomainEventConsumer, EventDispatcherHandle, InitialPosition,
    SequencedEvent,
};
use crate::backend::infrastructure::InfraError;
use crate::backend::store::{
    count_pending_outbox_events_sqlx, init_consumer_offset_if_missing_sqlx,
    is_consumer_offset_initialized_sqlx, load_all_tenant_ids_sqlx, load_consumer_last_seq_sqlx,
    load_max_outbox_seq_sqlx, load_pending_outbox_rows_sqlx, prune_retained_outbox_events_sqlx,
    upsert_consumer_offset_sqlx,
};
use sqlx::SqlitePool;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

const IDLE_POLL_MIN: Duration = Duration::from_secs(2);
const IDLE_POLL_MAX: Duration = Duration::from_secs(30);
const RETRY_MAX: Duration = Duration::from_secs(5 * 60);
pub(crate) const DEFAULT_SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EventDispatcherShutdownReport {
    pub(crate) drained: bool,
    pub(crate) remaining_events: usize,
    pub(crate) timed_out: bool,
}

impl Default for EventDispatcherShutdownReport {
    fn default() -> Self {
        Self {
            drained: true,
            remaining_events: 0,
            timed_out: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct DispatchCycleReport {
    advanced_rows: usize,
    failures: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RetryKey {
    consumer_id: String,
    tenant_id: String,
}

#[derive(Debug, Clone)]
struct ConsumerRetryState {
    attempts: u32,
    next_retry: Instant,
}

pub(crate) struct EventDispatcher {
    pool: SqlitePool,
    db_path: PathBuf,
    consumers: Vec<Arc<dyn DomainEventConsumer>>,
    notify: Arc<tokio::sync::Notify>,
    cancellation: CancellationToken,
    retry_states: Mutex<HashMap<RetryKey, ConsumerRetryState>>,
}

impl EventDispatcher {
    pub(crate) fn with_consumers(
        pool: SqlitePool,
        db_path: PathBuf,
        consumers: Vec<Arc<dyn DomainEventConsumer>>,
    ) -> Self {
        Self {
            pool,
            db_path,
            consumers,
            notify: Arc::new(tokio::sync::Notify::new()),
            cancellation: CancellationToken::new(),
            retry_states: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) async fn initialize_tenant(&self, tenant_id: &str) -> Result<(), InfraError> {
        let tenant = tenant_id.to_string();
        let consumers = self
            .consumers
            .iter()
            .map(|consumer| (consumer.id().to_string(), consumer.initial_position()))
            .collect::<Vec<_>>();
        let cutoff_seq = load_max_outbox_seq_sqlx(&self.pool, &tenant).await?;

        for consumer in &self.consumers {
            if consumer.initial_position() != InitialPosition::BackfillThenCutoff {
                continue;
            }
            let consumer_id = consumer.id().to_string();
            let already_initialized =
                is_consumer_offset_initialized_sqlx(&self.pool, &consumer_id, &tenant).await?;
            if already_initialized {
                continue;
            }
            let cx = ConsumerCx {
                pool: self.pool.clone(),
                db_path: self.db_path.clone(),
                consumer_id,
                tenant_id: tenant.clone(),
                batch_last_seq: cutoff_seq,
            };
            consumer.backfill(&cx).await?;
        }

        for (consumer_id, initial_position) in consumers {
            let initial_seq = match initial_position {
                InitialPosition::GenesisZero => 0,
                InitialPosition::BackfillThenCutoff => cutoff_seq,
            };
            init_consumer_offset_if_missing_sqlx(&self.pool, &consumer_id, &tenant, initial_seq)
                .await?;
        }
        Ok(())
    }

    pub(crate) async fn initialize_all_tenants(&self) -> Result<(), InfraError> {
        for tenant_id in self.tenant_ids().await? {
            self.initialize_tenant(&tenant_id).await?;
        }
        Ok(())
    }

    async fn tenant_ids(&self) -> Result<Vec<String>, InfraError> {
        Ok(load_all_tenant_ids_sqlx(&self.pool).await?)
    }

    fn retry_key(consumer_id: &str, tenant_id: &str) -> RetryKey {
        RetryKey {
            consumer_id: consumer_id.to_string(),
            tenant_id: tenant_id.to_string(),
        }
    }

    fn can_attempt(&self, key: &RetryKey) -> bool {
        self.retry_states
            .lock()
            .ok()
            .and_then(|states| {
                states
                    .get(key)
                    .map(|state| Instant::now() >= state.next_retry)
            })
            .unwrap_or(true)
    }

    fn record_success(&self, key: &RetryKey) {
        if let Ok(mut states) = self.retry_states.lock() {
            states.remove(key);
        }
    }

    fn record_failure(&self, key: &RetryKey) -> Duration {
        let mut states = match self.retry_states.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let attempts = states
            .get(key)
            .map(|s| s.attempts)
            .unwrap_or(0)
            .saturating_add(1);
        let delay = retry_delay(attempts);
        states.insert(
            key.clone(),
            ConsumerRetryState {
                attempts,
                next_retry: Instant::now() + delay,
            },
        );
        delay
    }

    const BATCH_SIZE: i64 = 64;

    async fn dispatch_one(
        &self,
        consumer: &Arc<dyn DomainEventConsumer>,
        tenant: &str,
    ) -> Result<(usize, usize), InfraError> {
        let consumer_id = consumer.id().to_string();
        let rows =
            load_pending_outbox_rows_sqlx(&self.pool, tenant, &consumer_id, Self::BATCH_SIZE)
                .await?;

        if rows.is_empty() {
            return Ok((0, 0));
        }
        let batch = rows
            .into_iter()
            .map(|(seq, payload)| {
                Ok(SequencedEvent {
                    seq,
                    event: serde_json::from_str(&payload)
                        .map_err(|error| InfraError::External(error.to_string()))?,
                })
            })
            .collect::<Result<Vec<_>, InfraError>>()?;
        let interested = batch
            .iter()
            .filter(|item| consumer.interested(&item.event))
            .cloned()
            .collect::<Vec<_>>();
        if !interested.is_empty() {
            let cx = ConsumerCx {
                pool: self.pool.clone(),
                db_path: self.db_path.clone(),
                consumer_id: consumer_id.clone(),
                tenant_id: tenant.to_string(),
                batch_last_seq: batch.last().map(|item| item.seq).unwrap_or_default(),
            };
            consumer.handle(&interested, &cx).await?;
        }
        let last_seq = batch.last().map(|item| item.seq).unwrap_or_default();
        upsert_consumer_offset_sqlx(&self.pool, &consumer_id, tenant, last_seq).await?;
        Ok((batch.len(), interested.len()))
    }

    async fn dispatch_tenant(&self, tenant_id: &str) -> DispatchCycleReport {
        let span = tracing::info_span!("domain_events.dispatch_tenant", tenant = %tenant_id);
        let _enter = span.enter();
        let mut report = DispatchCycleReport::default();
        for consumer in &self.consumers {
            let key = Self::retry_key(consumer.id(), tenant_id);
            if !self.can_attempt(&key) {
                continue;
            }
            match self.dispatch_one(consumer, tenant_id).await {
                Ok((advanced, _)) => {
                    self.record_success(&key);
                    report.advanced_rows += advanced;
                }
                Err(error) => {
                    let delay = self.record_failure(&key);
                    report.failures += 1;
                    tracing::warn!(
                        consumer_id = %consumer.id(),
                        tenant = %tenant_id,
                        error = %error,
                        retry_in_secs = delay.as_secs(),
                        "consumer dispatch failed; backing off"
                    );
                }
            }
        }
        report
    }

    #[cfg(test)]
    pub(crate) async fn dispatch_once(&self, tenant_id: &str) -> Result<usize, InfraError> {
        Ok(self.dispatch_tenant(tenant_id).await.advanced_rows)
    }

    async fn dispatch_all_tenants(&self) -> DispatchCycleReport {
        let mut aggregated = DispatchCycleReport::default();
        let tenant_ids = match self.tenant_ids().await {
            Ok(ids) => ids,
            Err(error) => {
                tracing::warn!(error = %error, "failed to enumerate tenants for event dispatch");
                aggregated.failures += 1;
                return aggregated;
            }
        };
        for tenant_id in tenant_ids {
            let report = self.dispatch_tenant(&tenant_id).await;
            aggregated.advanced_rows += report.advanced_rows;
            aggregated.failures += report.failures;
        }
        aggregated
    }

    async fn pending_event_count(&self) -> Result<usize, InfraError> {
        let mut count = 0usize;
        for consumer in &self.consumers {
            let pending = count_pending_outbox_events_sqlx(&self.pool, consumer.id()).await?;
            count = count.saturating_add(pending.max(0) as usize);
        }
        Ok(count)
    }

    async fn drain_until(&self, deadline: Instant) -> EventDispatcherShutdownReport {
        if let Ok(mut states) = self.retry_states.lock() {
            states.clear();
        }
        loop {
            if Instant::now() >= deadline {
                return EventDispatcherShutdownReport {
                    drained: false,
                    remaining_events: self.pending_event_count().await.unwrap_or_default(),
                    timed_out: true,
                };
            }
            let report = self.dispatch_all_tenants().await;
            if report.failures > 0 {
                return EventDispatcherShutdownReport {
                    drained: false,
                    remaining_events: self.pending_event_count().await.unwrap_or_default(),
                    timed_out: false,
                };
            }
            if report.advanced_rows == 0 {
                return EventDispatcherShutdownReport {
                    drained: true,
                    remaining_events: 0,
                    timed_out: false,
                };
            }
        }
    }

    pub(crate) async fn cleanup_retained_events(&self) -> Result<usize, InfraError> {
        let mut deleted = 0usize;
        let consumer_ids = self
            .consumers
            .iter()
            .map(|consumer| consumer.id().to_string())
            .collect::<Vec<_>>();
        for tenant_id in self.tenant_ids().await? {
            let mut safe_seq = i64::MAX;
            for consumer_id in &consumer_ids {
                let last_seq =
                    load_consumer_last_seq_sqlx(&self.pool, consumer_id, &tenant_id).await?;
                safe_seq = safe_seq.min(last_seq.unwrap_or(0));
            }
            if safe_seq == 0 || safe_seq == i64::MAX {
                continue;
            }
            let removed =
                prune_retained_outbox_events_sqlx(&self.pool, &tenant_id, safe_seq).await?;
            deleted += removed as usize;
        }
        Ok(deleted)
    }

    fn next_retry_delay(&self) -> Option<Duration> {
        let now = Instant::now();
        self.retry_states.lock().ok().and_then(|states| {
            states
                .values()
                .map(|state| state.next_retry.saturating_duration_since(now))
                .min()
        })
    }

    fn next_wait(&self, idle_delay: Duration) -> Duration {
        self.next_retry_delay()
            .map(|retry| retry.min(idle_delay))
            .unwrap_or(idle_delay)
    }

    pub(crate) fn start(
        self: Arc<Self>,
        runtime_handle: &tokio::runtime::Handle,
    ) -> EventDispatcherHandle {
        let cancellation = CancellationToken::new();
        let handle_cancellation = cancellation.clone();
        let shutdown_deadline = Arc::new(Mutex::new(None));
        let worker_shutdown_deadline = shutdown_deadline.clone();
        let worker_cancellation = cancellation.clone();
        let notify = self.notify.clone();
        let pool = self.pool.clone();
        let consumer_ids = self
            .consumers
            .iter()
            .map(|consumer| consumer.id().to_string())
            .collect();
        let worker_self = self.clone();

        let task = runtime_handle.spawn(async move {
            let mut idle_delay = IDLE_POLL_MIN;
            loop {
                if worker_cancellation.is_cancelled() {
                    let deadline = worker_shutdown_deadline
                        .lock()
                        .ok()
                        .and_then(|deadline| *deadline)
                        .unwrap_or_else(|| Instant::now() + DEFAULT_SHUTDOWN_GRACE);
                    return worker_self.drain_until(deadline).await;
                }

                let cycle = worker_self.dispatch_all_tenants().await;
                if cycle.advanced_rows > 0 {
                    idle_delay = IDLE_POLL_MIN;
                } else {
                    idle_delay = (idle_delay * 2).min(IDLE_POLL_MAX);
                }
                let _ = worker_self.cleanup_retained_events().await;

                let wait_duration = worker_self.next_wait(idle_delay);
                let sleep_target = tokio::time::Instant::now() + wait_duration;

                tokio::select! {
                    biased;
                    _ = worker_cancellation.cancelled() => {
                        let deadline = worker_shutdown_deadline
                            .lock()
                            .ok()
                            .and_then(|deadline| *deadline)
                            .unwrap_or_else(|| Instant::now() + DEFAULT_SHUTDOWN_GRACE);
                        return worker_self.drain_until(deadline).await;
                    }
                    _ = worker_self.notify.notified() => {}
                    _ = tokio::time::sleep_until(sleep_target) => {}
                }
            }
        });

        EventDispatcherHandle::new(
            notify,
            handle_cancellation,
            shutdown_deadline,
            pool,
            consumer_ids,
            Some(task),
        )
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn retry_delay(attempts: u32) -> Duration {
    let multiplier = attempts.saturating_sub(1).min(8);
    let seconds = 5u64.saturating_pow(multiplier).min(RETRY_MAX.as_secs());
    Duration::from_secs(seconds)
}

#[cfg(test)]
#[path = "dispatcher_tests.rs"]
mod tests;
