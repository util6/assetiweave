use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug)]
pub struct ActivityWatchdogConfig {
    pub idle_timeout: Duration,
    pub safety_ceiling: Option<Duration>,
    pub poll_interval: Duration,
}

impl Default for ActivityWatchdogConfig {
    fn default() -> Self {
        Self {
            idle_timeout: Duration::from_secs(180),
            safety_ceiling: None,
            poll_interval: Duration::from_millis(500),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WatchdogTimeoutKind {
    Inactivity { idle_seconds: u64 },
    SafetyCeiling { total_seconds: u64 },
}

impl WatchdogTimeoutKind {
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Inactivity { .. } => "inactivity_timeout",
            Self::SafetyCeiling { .. } => "safety_ceiling_timeout",
        }
    }

    pub fn display_message(&self, stage_name: &str) -> String {
        match self {
            Self::Inactivity { idle_seconds } => {
                format!("{stage_name}静默无响应超时({idle_seconds}s)")
            }
            Self::SafetyCeiling { total_seconds } => {
                format!("{stage_name}总执行时限超时({total_seconds}s)")
            }
        }
    }
}

pub async fn run_with_activity_watchdog<F, T, CheckIdle>(
    future: F,
    cancellation: CancellationToken,
    check_idle: CheckIdle,
    config: ActivityWatchdogConfig,
) -> Result<T, WatchdogTimeoutKind>
where
    F: std::future::Future<Output = T>,
    CheckIdle: Fn() -> Duration,
{
    tokio::pin!(future);
    let start = tokio::time::Instant::now();
    let mut ticker = tokio::time::interval(config.poll_interval);

    loop {
        tokio::select! {
            output = &mut future => return Ok(output),
            _ = ticker.tick() => {
                if let Some(ceiling) = config.safety_ceiling {
                    let total_elapsed = start.elapsed();
                    if total_elapsed >= ceiling {
                        cancellation.cancel();
                        return Err(WatchdogTimeoutKind::SafetyCeiling {
                            total_seconds: total_elapsed.as_secs(),
                        });
                    }
                }

                let idle_elapsed = check_idle();
                if idle_elapsed >= config.idle_timeout {
                    cancellation.cancel();
                    return Err(WatchdogTimeoutKind::Inactivity {
                        idle_seconds: idle_elapsed.as_secs(),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "activity_watchdog_tests.rs"]
mod tests;
