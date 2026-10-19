use crate::backend::store;
use chrono::Utc;
use sqlx::SqlitePool;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::warn;

pub const RECENT_MEMORY_LEASE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(25);

pub struct RecentMemoryLeaseGuard {
    task: tokio::task::JoinHandle<()>,
}

impl RecentMemoryLeaseGuard {
    pub fn start(
        pool: SqlitePool,
        tenant_id: String,
        job_id: String,
        ownership_token: String,
        cancellation: CancellationToken,
    ) -> Self {
        Self::start_with_interval(
            pool,
            tenant_id,
            job_id,
            ownership_token,
            cancellation,
            RECENT_MEMORY_LEASE_HEARTBEAT_INTERVAL,
        )
    }

    pub fn start_with_interval(
        pool: SqlitePool,
        tenant_id: String,
        job_id: String,
        ownership_token: String,
        cancellation: CancellationToken,
        interval: Duration,
    ) -> Self {
        let task = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            // Skip the immediate first tick so the initial claim lease remains valid
            ticker.tick().await;

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        if cancellation.is_cancelled() {
                            break;
                        }
                        let now = Utc::now().to_rfc3339();
                        match store::heartbeat_recent_memory_job_sqlx(
                            &pool,
                            &tenant_id,
                            &job_id,
                            &ownership_token,
                            &now,
                        )
                        .await
                        {
                            Ok(true) => {}
                            Ok(false) => {
                                warn!(
                                    tenant_id = %tenant_id,
                                    job_id = %job_id,
                                    "recent memory job lease heartbeat failed: ownership lost or job no longer running"
                                );
                                break;
                            }
                            Err(err) => {
                                warn!(
                                    tenant_id = %tenant_id,
                                    job_id = %job_id,
                                    error = ?err,
                                    "recent memory job lease heartbeat error"
                                );
                            }
                        }
                    }
                    _ = cancellation.cancelled() => {
                        break;
                    }
                }
            }
        });
        Self { task }
    }
}

impl Drop for RecentMemoryLeaseGuard {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
#[path = "lease_guard_tests.rs"]
mod tests;
