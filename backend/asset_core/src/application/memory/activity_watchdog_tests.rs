use super::*;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

#[tokio::test]
async fn test_activity_watchdog_immediate_success() {
    let cancel = CancellationToken::new();
    let config = ActivityWatchdogConfig {
        idle_timeout: Duration::from_millis(100),
        safety_ceiling: Some(Duration::from_millis(500)),
        poll_interval: Duration::from_millis(10),
    };

    let result = run_with_activity_watchdog(
        async { "completed" },
        cancel.clone(),
        || Duration::ZERO,
        config,
    )
    .await;

    assert_eq!(result, Ok("completed"));
    assert!(!cancel.is_cancelled());
}

#[tokio::test]
async fn test_activity_watchdog_active_past_idle_threshold() {
    let cancel = CancellationToken::new();
    let config = ActivityWatchdogConfig {
        idle_timeout: Duration::from_millis(60),
        safety_ceiling: Some(Duration::from_millis(500)),
        poll_interval: Duration::from_millis(10),
    };

    let last_active = Arc::new(AtomicU64::new(0));
    let last_active_clone = last_active.clone();

    // Task runs for 150ms (longer than 60ms idle_timeout), but keeps touching activity
    let result = run_with_activity_watchdog(
        async move {
            for _ in 0..6 {
                tokio::time::sleep(Duration::from_millis(25)).await;
                last_active_clone.fetch_add(1, Ordering::SeqCst);
            }
            "steady_stream"
        },
        cancel.clone(),
        move || {
            // Simulated idle duration: always recent (under 30ms)
            Duration::from_millis(10)
        },
        config,
    )
    .await;

    assert_eq!(result, Ok("steady_stream"));
    assert!(!cancel.is_cancelled());
}

#[tokio::test]
async fn test_activity_watchdog_unbounded_when_streaming() {
    let cancel = CancellationToken::new();
    // Default has safety_ceiling: None
    let config = ActivityWatchdogConfig {
        idle_timeout: Duration::from_millis(60),
        safety_ceiling: None,
        poll_interval: Duration::from_millis(10),
    };

    // Task runs for 150ms with no safety ceiling, streaming activity continuously
    let result = run_with_activity_watchdog(
        async move {
            for _ in 0..6 {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            "unbounded_stream_success"
        },
        cancel.clone(),
        || Duration::from_millis(10),
        config,
    )
    .await;

    assert_eq!(result, Ok("unbounded_stream_success"));
    assert!(!cancel.is_cancelled());
}

#[tokio::test]
async fn test_activity_watchdog_triggers_inactivity_timeout_when_silent() {
    let cancel = CancellationToken::new();
    let config = ActivityWatchdogConfig {
        idle_timeout: Duration::from_millis(50),
        safety_ceiling: Some(Duration::from_millis(500)),
        poll_interval: Duration::from_millis(10),
    };

    let start = std::time::Instant::now();
    let result = run_with_activity_watchdog(
        async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            "never_reached"
        },
        cancel.clone(),
        move || start.elapsed(),
        config,
    )
    .await;

    assert!(matches!(
        result,
        Err(WatchdogTimeoutKind::Inactivity { .. })
    ));
    assert!(cancel.is_cancelled());
}

#[tokio::test]
async fn test_activity_watchdog_triggers_safety_ceiling() {
    let cancel = CancellationToken::new();
    let config = ActivityWatchdogConfig {
        idle_timeout: Duration::from_millis(200),
        safety_ceiling: Some(Duration::from_millis(60)),
        poll_interval: Duration::from_millis(10),
    };

    let result = run_with_activity_watchdog(
        async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            "never_reached"
        },
        cancel.clone(),
        || Duration::from_millis(5), // always active
        config,
    )
    .await;

    assert!(matches!(
        result,
        Err(WatchdogTimeoutKind::SafetyCeiling { .. })
    ));
    assert!(cancel.is_cancelled());
}

#[test]
fn test_timeout_kind_messages() {
    let idle = WatchdogTimeoutKind::Inactivity { idle_seconds: 90 };
    assert_eq!(idle.error_code(), "inactivity_timeout");
    assert!(idle
        .display_message("近期快照生成")
        .contains("静默无响应超时(90s)"));

    let ceiling = WatchdogTimeoutKind::SafetyCeiling { total_seconds: 600 };
    assert_eq!(ceiling.error_code(), "safety_ceiling_timeout");
    assert!(ceiling
        .display_message("近期快照生成")
        .contains("总执行时限超时(600s)"));
}
