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

#[tokio::test]
async fn stop_until_never_extends_an_expired_deadline() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("create test pool");

    let _held = pool.acquire().await.expect("acquire connection");

    let cancellation = tokio_util::sync::CancellationToken::new();
    let notify = std::sync::Arc::new(tokio::sync::Notify::new());
    let shutdown_deadline = std::sync::Arc::new(std::sync::Mutex::new(None));
    let task = tokio::spawn(async {
        tokio::time::sleep(Duration::from_secs(10)).await;
        super::EventDispatcherShutdownReport::default()
    });

    let mut handle = super::EventDispatcherHandle {
        cancellation,
        notify,
        shutdown_deadline,
        pool,
        consumer_ids: vec!["test_consumer".to_string()],
        task: Some(task),
    };

    let start = tokio::time::Instant::now();
    let deadline = start - Duration::from_millis(10);
    let report = handle.stop_until(deadline).await;
    let elapsed = start.elapsed();

    assert!(report.timed_out, "report must indicate timed_out");
    assert!(
        elapsed <= Duration::from_millis(25),
        "stop_until took {:?}, expected <= 25ms",
        elapsed
    );
}

#[tokio::test]
async fn stop_until_awaits_aborted_dispatcher_completion() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    struct DropGuard(Arc<AtomicBool>);
    impl Drop for DropGuard {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let dropped = Arc::new(AtomicBool::new(false));
    let dropped_clone = dropped.clone();

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .expect("create test pool");

    let cancellation = tokio_util::sync::CancellationToken::new();
    let notify = std::sync::Arc::new(tokio::sync::Notify::new());
    let shutdown_deadline = std::sync::Arc::new(std::sync::Mutex::new(None));

    let task = tokio::spawn(async move {
        let _guard = DropGuard(dropped_clone);
        tokio::time::sleep(Duration::from_secs(10)).await;
        super::EventDispatcherShutdownReport::default()
    });

    let mut handle = super::EventDispatcherHandle {
        cancellation,
        notify,
        shutdown_deadline,
        pool,
        consumer_ids: vec![],
        task: Some(task),
    };

    let deadline = tokio::time::Instant::now() + Duration::from_millis(5);
    let report = handle.stop_until(deadline).await;

    assert!(report.timed_out, "must report timed_out");
    assert!(
        dropped.load(Ordering::SeqCst),
        "aborted worker task must be awaited so its resources/guards are dropped before returning"
    );
}
