use super::{
    ConsumerCx, DomainEventConsumer, InitialPosition, SearchIndexAdvanceConsumer, SequencedEvent,
    SessionMemoryConsumer,
};
use crate::backend::{runtime::AppError, store::Database};
use sqlx::{Row, SqlitePool};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

const IDLE_POLL_MIN: Duration = Duration::from_secs(2);
const IDLE_POLL_MAX: Duration = Duration::from_secs(30);
const RETRY_MAX: Duration = Duration::from_secs(5 * 60);
const DEFAULT_SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

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
    delivered_events: usize,
    failures: usize,
}

impl DispatchCycleReport {
    fn had_work(&self) -> bool {
        self.advanced_rows > 0 || self.failures > 0
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct RetryKey {
    consumer_id: String,
    tenant_id: String,
}

#[derive(Debug, Clone)]
struct RetryState {
    attempts: u32,
    next_attempt_at: Instant,
}

pub(crate) struct EventDispatcher {
    pool: SqlitePool,
    db_path: PathBuf,
    consumers: Vec<Arc<dyn DomainEventConsumer>>,
    notify: Arc<tokio::sync::Notify>,
    cancellation: CancellationToken,
    retry_states: Mutex<HashMap<RetryKey, RetryState>>,
}

pub(crate) struct EventDispatcherHandle {
    cancellation: CancellationToken,
    notify: Arc<tokio::sync::Notify>,
    shutdown_deadline: Arc<Mutex<Option<Instant>>>,
    pool: SqlitePool,
    consumer_ids: Vec<String>,
    task: Option<tokio::task::JoinHandle<EventDispatcherShutdownReport>>,
}

impl EventDispatcher {
    pub(crate) fn new(database: Database, db_path: PathBuf) -> Self {
        Self::with_consumers(
            database.pool().clone(),
            db_path,
            vec![
                Arc::new(SearchIndexAdvanceConsumer::new(database)),
                Arc::new(SessionMemoryConsumer),
            ],
        )
    }

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

    pub(crate) async fn initialize_tenant(&self, tenant_id: &str) -> Result<(), AppError> {
        let tenant = tenant_id.to_string();
        let consumers = self
            .consumers
            .iter()
            .map(|consumer| (consumer.id().to_string(), consumer.initial_position()))
            .collect::<Vec<_>>();
        let cutoff_seq = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(seq) FROM domain_event_outbox WHERE tenant_id = ?1",
        )
        .bind(&tenant)
        .fetch_one(&self.pool)
        .await
        .map(|value| value.unwrap_or(0))
        .map_err(AppError::Db)?;

        for consumer in &self.consumers {
            if consumer.initial_position() != InitialPosition::BackfillThenCutoff {
                continue;
            }
            let consumer_id = consumer.id().to_string();
            let already_initialized = sqlx::query_scalar::<_, i64>(
                "SELECT EXISTS (SELECT 1 FROM domain_event_consumer_offsets WHERE consumer_id = ?1 AND tenant_id = ?2)",
            )
            .bind(&consumer_id)
            .bind(&tenant)
            .fetch_one(&self.pool)
            .await
            .map(|value| value != 0)
            .map_err(AppError::Db)?;
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

        let now = chrono::Utc::now().to_rfc3339();
        for (consumer_id, initial_position) in consumers {
            let initial_seq = match initial_position {
                InitialPosition::GenesisZero => 0,
                InitialPosition::BackfillThenCutoff => cutoff_seq,
            };
            sqlx::query(
                "INSERT OR IGNORE INTO domain_event_consumer_offsets (consumer_id, tenant_id, last_seq, updated_at) VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(consumer_id)
            .bind(&tenant)
            .bind(initial_seq)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(AppError::Db)?;
        }
        Ok(())
    }

    pub(crate) async fn initialize_all_tenants(&self) -> Result<(), AppError> {
        for tenant_id in self.tenant_ids().await? {
            self.initialize_tenant(&tenant_id).await?;
        }
        Ok(())
    }

    async fn tenant_ids(&self) -> Result<Vec<String>, AppError> {
        sqlx::query_scalar::<_, String>("SELECT id FROM tenants ORDER BY id")
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::Db)
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
            .and_then(|states| states.get(key).cloned())
            .is_none_or(|state| state.next_attempt_at <= Instant::now())
    }

    fn record_success(&self, key: &RetryKey) {
        if let Ok(mut states) = self.retry_states.lock() {
            states.remove(key);
        }
    }

    fn record_failure(&self, key: RetryKey) {
        let Ok(mut states) = self.retry_states.lock() else {
            return;
        };
        let attempts = states
            .get(&key)
            .map(|state| state.attempts.saturating_add(1))
            .unwrap_or(1);
        states.insert(
            key,
            RetryState {
                attempts,
                next_attempt_at: Instant::now() + retry_delay(attempts),
            },
        );
    }

    fn next_retry_delay(&self) -> Option<Duration> {
        let now = Instant::now();
        self.retry_states.lock().ok().and_then(|states| {
            states
                .values()
                .map(|state| state.next_attempt_at.saturating_duration_since(now))
                .min()
        })
    }

    async fn dispatch_consumer(
        &self,
        consumer: &Arc<dyn DomainEventConsumer>,
        tenant_id: &str,
    ) -> Result<(usize, usize), AppError> {
        let consumer_id = consumer.id().to_string();
        let tenant = tenant_id.to_string();
        let rows = sqlx::query(
            "SELECT seq, payload FROM domain_event_outbox WHERE tenant_id = ?1 AND seq > COALESCE((SELECT last_seq FROM domain_event_consumer_offsets WHERE consumer_id = ?2 AND tenant_id = ?1), 0) ORDER BY seq ASC LIMIT 100",
        )
        .bind(&tenant)
        .bind(&consumer_id)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Db)?;

        if rows.is_empty() {
            return Ok((0, 0));
        }
        let batch = rows
            .iter()
            .map(|row| {
                Ok(SequencedEvent {
                    seq: row
                        .try_get(0)
                        .map_err(|error: sqlx::Error| AppError::External(error.to_string()))?,
                    event: serde_json::from_str(
                        &row.try_get::<String, _>(1)
                            .map_err(|error: sqlx::Error| AppError::External(error.to_string()))?,
                    )
                    .map_err(|error| AppError::External(error.to_string()))?,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
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
                tenant_id: tenant.clone(),
                batch_last_seq: batch.last().map(|item| item.seq).unwrap_or_default(),
            };
            consumer.handle(&interested, &cx).await?;
        }
        let last_seq = batch.last().map(|item| item.seq).unwrap_or_default();
        sqlx::query(
            "INSERT INTO domain_event_consumer_offsets (consumer_id, tenant_id, last_seq, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (consumer_id, tenant_id) DO UPDATE SET last_seq = excluded.last_seq, updated_at = excluded.updated_at",
        )
        .bind(&consumer_id)
        .bind(&tenant)
        .bind(last_seq)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(AppError::Db)?;
        Ok((batch.len(), interested.len()))
    }

    async fn dispatch_tenant(&self, tenant_id: &str) -> DispatchCycleReport {
        let mut report = DispatchCycleReport::default();
        for consumer in &self.consumers {
            let key = Self::retry_key(consumer.id(), tenant_id);
            if !self.can_attempt(&key) {
                continue;
            }
            match self.dispatch_consumer(consumer, tenant_id).await {
                Ok((advanced_rows, delivered_events)) => {
                    self.record_success(&key);
                    report.advanced_rows += advanced_rows;
                    report.delivered_events += delivered_events;
                }
                Err(error) => {
                    // One consumer owns only its own offset and retry state. A
                    // failure must never prevent later consumers from running.
                    eprintln!(
                        "domain event consumer {} failed for tenant {tenant_id}: {error}",
                        consumer.id()
                    );
                    self.record_failure(key);
                    report.failures += 1;
                }
            }
        }
        report
    }

    #[cfg(test)]
    pub(crate) async fn dispatch_once(&self, tenant_id: &str) -> Result<usize, AppError> {
        Ok(self.dispatch_tenant(tenant_id).await.delivered_events)
    }

    async fn dispatch_all_tenants(&self) -> DispatchCycleReport {
        let Ok(tenant_ids) = self.tenant_ids().await else {
            return DispatchCycleReport {
                failures: 1,
                ..DispatchCycleReport::default()
            };
        };
        let mut report = DispatchCycleReport::default();
        for tenant_id in tenant_ids {
            if let Err(error) = self.initialize_tenant(&tenant_id).await {
                eprintln!(
                    "domain event offset initialization failed for tenant {tenant_id}: {error}"
                );
                report.failures += 1;
                continue;
            }
            let tenant_report = self.dispatch_tenant(&tenant_id).await;
            report.advanced_rows += tenant_report.advanced_rows;
            report.delivered_events += tenant_report.delivered_events;
            report.failures += tenant_report.failures;
        }
        report
    }

    fn next_wait(&self, idle_delay: Duration) -> Duration {
        self.next_retry_delay()
            .map(|retry| retry.min(idle_delay))
            .unwrap_or(idle_delay)
    }

    async fn pending_event_count(&self) -> Result<usize, AppError> {
        let consumer_ids = self
            .consumers
            .iter()
            .map(|consumer| consumer.id().to_string())
            .collect::<Vec<_>>();
        let mut count = 0usize;
        for consumer_id in consumer_ids {
            let pending = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM domain_event_outbox AS events WHERE EXISTS (SELECT 1 FROM domain_event_consumer_offsets AS offsets WHERE offsets.consumer_id = ?1 AND offsets.tenant_id = events.tenant_id AND offsets.last_seq < events.seq)",
            )
            .bind(consumer_id)
            .fetch_one(&self.pool)
            .await
            .map_err(AppError::Db)?;
            count = count.saturating_add(pending.max(0) as usize);
        }
        Ok(count)
    }

    async fn drain_until(&self, deadline: Instant) -> EventDispatcherShutdownReport {
        // Shutdown is the one path that deliberately ignores normal retry
        // backoff: make one final attempt for every consumer, then report any
        // remaining failure instead of claiming a clean drain.
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

    pub(crate) async fn cleanup_retained_events(&self) -> Result<usize, AppError> {
        let mut deleted = 0usize;
        let consumer_ids = self
            .consumers
            .iter()
            .map(|consumer| consumer.id().to_string())
            .collect::<Vec<_>>();
        for tenant_id in self.tenant_ids().await? {
            let mut safe_seq = i64::MAX;
            for consumer_id in &consumer_ids {
                let last_seq = sqlx::query_scalar::<_, i64>(
                    "SELECT last_seq FROM domain_event_consumer_offsets WHERE consumer_id = ?1 AND tenant_id = ?2",
                )
                .bind(consumer_id)
                .bind(&tenant_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(AppError::Db)?;
                safe_seq = safe_seq.min(last_seq.unwrap_or(0));
            }
            if safe_seq == 0 || safe_seq == i64::MAX {
                continue;
            }
            let removed = sqlx::query(
                "DELETE FROM domain_event_outbox WHERE tenant_id = ?1 AND seq < ?2 AND created_at < datetime('now', '-30 days')",
            )
            .bind(&tenant_id)
            .bind(safe_seq)
            .execute(&self.pool)
            .await
            .map_err(AppError::Db)?;
            deleted += removed.rows_affected() as usize;
        }
        Ok(deleted)
    }

    pub(crate) fn start(
        self: Arc<Self>,
        runtime_handle: &tokio::runtime::Handle,
    ) -> EventDispatcherHandle {
        let cancellation = self.cancellation.clone();
        let notify = self.notify.clone();
        let shutdown_deadline = Arc::new(Mutex::new(None));
        let worker_shutdown_deadline = shutdown_deadline.clone();
        let pool = self.pool.clone();
        let consumer_ids = self
            .consumers
            .iter()
            .map(|consumer| consumer.id().to_string())
            .collect::<Vec<_>>();

        let worker_self = self.clone();
        let task = runtime_handle.spawn(async move {
            let mut idle_delay = IDLE_POLL_MIN;
            loop {
                if worker_self.cancellation.is_cancelled() {
                    let deadline = worker_shutdown_deadline
                        .lock()
                        .ok()
                        .and_then(|deadline| *deadline)
                        .unwrap_or_else(|| Instant::now() + DEFAULT_SHUTDOWN_GRACE);
                    return worker_self.drain_until(deadline).await;
                }

                let cycle = worker_self.dispatch_all_tenants().await;
                if cycle.had_work() {
                    idle_delay = IDLE_POLL_MIN;
                } else {
                    idle_delay = (idle_delay * 2).min(IDLE_POLL_MAX);
                }
                let _ = worker_self.cleanup_retained_events().await;

                let wait_duration = worker_self.next_wait(idle_delay);
                let sleep_target = tokio::time::Instant::now() + wait_duration;

                tokio::select! {
                    biased;
                    _ = worker_self.cancellation.cancelled() => {
                        let deadline = worker_shutdown_deadline
                            .lock()
                            .ok()
                            .and_then(|deadline| *deadline)
                            .unwrap_or_else(|| Instant::now() + DEFAULT_SHUTDOWN_GRACE);
                        return worker_self.drain_until(deadline).await;
                    }
                    _ = worker_self.notify.notified() => {
                        // Woken by notify, proceed immediately to next cycle
                    }
                    _ = tokio::time::sleep_until(sleep_target) => {
                        // Timeout expired, proceed to next cycle
                    }
                }
            }
        });

        EventDispatcherHandle {
            cancellation,
            notify,
            shutdown_deadline,
            pool,
            consumer_ids,
            task: Some(task),
        }
    }
}

impl EventDispatcherHandle {
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
            match tokio::time::timeout(remaining + Duration::from_millis(50), &mut task).await {
                Ok(Ok(report)) => report,
                Ok(Err(_join_err)) => EventDispatcherShutdownReport {
                    drained: false,
                    remaining_events: self.pending_event_count().await.unwrap_or_default(),
                    timed_out: false,
                },
                Err(_elapsed) => {
                    task.abort();
                    EventDispatcherShutdownReport {
                        drained: false,
                        remaining_events: self.pending_event_count().await.unwrap_or_default(),
                        timed_out: true,
                    }
                }
            }
        } else {
            EventDispatcherShutdownReport::default()
        }
    }

    pub(crate) async fn stop_with_timeout(
        &mut self,
        grace: Duration,
    ) -> EventDispatcherShutdownReport {
        self.stop_until(Instant::now() + grace).await
    }

    async fn pending_event_count(&self) -> Result<usize, AppError> {
        let mut count = 0usize;
        for consumer_id in &self.consumer_ids {
            let pending = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM domain_event_outbox AS events WHERE EXISTS (SELECT 1 FROM domain_event_consumer_offsets AS offsets WHERE offsets.consumer_id = ?1 AND offsets.tenant_id = events.tenant_id AND offsets.last_seq < events.seq)",
            )
            .bind(consumer_id)
            .fetch_one(&self.pool)
            .await
            .map_err(AppError::Db)?;
            count = count.saturating_add(pending.max(0) as usize);
        }
        Ok(count)
    }
}

fn retry_delay(attempts: u32) -> Duration {
    let multiplier = attempts.saturating_sub(1).min(8);
    let seconds = 5u64.saturating_pow(multiplier).min(RETRY_MAX.as_secs());
    Duration::from_secs(seconds)
}

#[cfg(test)]
mod tests {
    use super::retry_delay;
    use std::time::Duration;

    #[test]
    fn retry_backoff_is_bounded_and_exponential() {
        assert_eq!(retry_delay(1), Duration::from_secs(1));
        assert_eq!(retry_delay(2), Duration::from_secs(5));
        assert_eq!(retry_delay(3), Duration::from_secs(25));
        assert_eq!(retry_delay(4), Duration::from_secs(125));
        assert_eq!(retry_delay(5), Duration::from_secs(300));
        assert_eq!(retry_delay(20), Duration::from_secs(300));
    }
}
