use super::{
    append_outbox_event_sqlx_tx, ConsumerCx, ConsumerFuture, DomainEvent, DomainEventConsumer,
    EventDispatcher, InitialPosition, SequencedEvent, SessionMemoryConsumer,
};
use crate::backend::runtime::AppError;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use uuid::Uuid;

struct TestConsumer {
    id: &'static str,
    calls: Arc<AtomicUsize>,
    fail: bool,
    delay: Duration,
}

impl DomainEventConsumer for TestConsumer {
    fn id(&self) -> &'static str {
        self.id
    }

    fn initial_position(&self) -> InitialPosition {
        InitialPosition::GenesisZero
    }

    fn interested(&self, _event: &DomainEvent) -> bool {
        true
    }

    fn handle<'a>(
        &'a self,
        _batch: &'a [SequencedEvent],
        _cx: &'a ConsumerCx,
    ) -> ConsumerFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let delay = self.delay;
        let fail = self.fail;
        Box::pin(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            if fail {
                Err(AppError::External("test consumer failure".to_string()))
            } else {
                Ok(())
            }
        })
    }
}

async fn open_test_pool(path: &std::path::Path) -> sqlx::SqlitePool {
    let pool = crate::backend::store::open_migrated_pool(path)
        .await
        .expect("open test pool");
    crate::backend::store::seed_defaults_sqlx(&pool)
        .await
        .expect("seed defaults");
    pool
}

async fn append_test_event_sqlx(pool: &sqlx::SqlitePool, event: &DomainEvent) {
    let mut tx = pool.begin().await.expect("begin event tx");
    append_outbox_event_sqlx_tx(&mut tx, event)
        .await
        .expect("append test event");
    tx.commit().await.expect("commit event tx");
}

#[test]
fn built_in_consumers_declare_their_initial_position() {
    let consumers: Vec<Arc<dyn DomainEventConsumer>> = vec![
        Arc::new(super::SearchIndexAdvanceConsumer { database: None }),
        Arc::new(super::SessionMemoryConsumer),
    ];
    assert_eq!(
        consumers[0].initial_position(),
        InitialPosition::GenesisZero
    );
    assert_eq!(
        consumers[1].initial_position(),
        InitialPosition::BackfillThenCutoff
    );
}

#[tokio::test]
async fn session_commit_creates_one_durable_phase1_job() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-session-memory-job-red-{}.sqlite",
        Uuid::new_v4()
    ));
    let pool = open_test_pool(&path).await;
    let dispatcher = EventDispatcher::with_consumers(
        pool.clone(),
        path.clone(),
        vec![Arc::new(SessionMemoryConsumer)],
    );
    dispatcher
        .initialize_tenant("default")
        .await
        .expect("initialize session memory consumer");

    sqlx::query(
        r#"
        INSERT INTO conversation_sessions (
            tenant_id, id, source_id, adapter_id, external_id, title,
            project_path, started_at, updated_at, source_locator,
            source_fingerprint, missing, created_at, imported_at
        ) VALUES (
            'default', 'session-memory-red', 'source-red', 'adapter-red',
            'external-red', 'Red fixture', '/tmp/project-red',
            '2026-08-30T00:00:00Z', '2026-08-30T00:00:00Z',
            'fixture://session-memory-red', 'revision-red', 0,
            '2026-08-30T00:00:00Z', '2026-08-30T00:00:00Z'
        )
        "#,
    )
    .execute(&pool)
    .await
    .expect("insert conversation fixture");

    let event = DomainEvent::conversation_source_committed(
        "default",
        "sync-session-memory-red",
        "source-red",
        1,
        ["session-memory-red".to_string()],
    );
    let mut tx = pool.begin().await.expect("begin event transaction");
    append_outbox_event_sqlx_tx(&mut tx, &event)
        .await
        .expect("append conversation commit");
    tx.commit().await.expect("commit conversation event");

    dispatcher
        .dispatch_once("default")
        .await
        .expect("dispatch conversation commit");
    dispatcher
        .initialize_tenant("default")
        .await
        .expect("reinitialize session memory consumer");

    let jobs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM session_memory_jobs WHERE tenant_id = 'default'")
            .fetch_one(&pool)
            .await
            .expect("read durable phase1 jobs");
    assert_eq!(jobs, 1);
    drop(pool);
    let _ = std::fs::remove_file(&path);
}

struct BackfillTestConsumer;

impl DomainEventConsumer for BackfillTestConsumer {
    fn id(&self) -> &'static str {
        "test.backfill"
    }

    fn initial_position(&self) -> InitialPosition {
        InitialPosition::BackfillThenCutoff
    }

    fn interested(&self, _event: &DomainEvent) -> bool {
        true
    }

    fn handle<'a>(
        &'a self,
        _batch: &'a [SequencedEvent],
        _cx: &'a ConsumerCx,
    ) -> ConsumerFuture<'a> {
        Box::pin(async move { Ok(()) })
    }
}

#[tokio::test]
async fn backfill_consumer_cannot_fall_back_to_a_zero_offset() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-dispatcher-backfill-registration-{}.sqlite",
        Uuid::new_v4()
    ));
    let pool = crate::backend::store::open_migrated_pool(&path)
        .await
        .expect("open test pool");
    let dispatcher = EventDispatcher::with_consumers(
        pool.clone(),
        path.clone(),
        vec![Arc::new(BackfillTestConsumer)],
    );

    let error = dispatcher
        .initialize_tenant("default")
        .await
        .expect_err("backfill registration must require an explicit migration");
    assert!(error.to_string().contains("backfill-and-cutoff"));

    drop(pool);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn outbox_append_is_atomic_with_the_business_transaction() {
    let path = std::env::temp_dir().join(format!("assetiweave-outbox-{}.sqlite", Uuid::new_v4()));
    let pool = open_test_pool(&path).await;
    let event = DomainEvent::conversation_source_committed(
        "default",
        "sync-atomic",
        "source-atomic",
        1,
        ["session-atomic".to_string()],
    );

    let mut tx = pool.begin().await.expect("begin rollback tx");
    append_outbox_event_sqlx_tx(&mut tx, &event)
        .await
        .expect("append event");
    tx.rollback().await.expect("rollback business tx");

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM domain_event_outbox WHERE event_id = ?1")
            .bind(event_id(&event))
            .fetch_one(&pool)
            .await
            .expect("count rolled back event");
    assert_eq!(count, 0);

    let mut tx = pool.begin().await.expect("begin commit tx");
    append_outbox_event_sqlx_tx(&mut tx, &event)
        .await
        .expect("append committed event");
    tx.commit().await.expect("commit business tx");

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM domain_event_outbox WHERE event_id = ?1")
            .bind(event_id(&event))
            .fetch_one(&pool)
            .await
            .expect("count committed event");
    assert_eq!(count, 1);

    drop(pool);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn resident_dispatcher_initializes_offsets_for_all_tenants() {
    let path =
        std::env::temp_dir().join(format!("assetiweave-dispatcher-{}.sqlite", Uuid::new_v4()));
    let pool = open_test_pool(&path).await;
    let dispatcher = EventDispatcher::with_consumers(
        pool.clone(),
        path.clone(),
        vec![
            Arc::new(super::SearchIndexAdvanceConsumer { database: None }),
            Arc::new(SessionMemoryConsumer),
        ],
    );
    dispatcher
        .initialize_all_tenants()
        .await
        .expect("initialize consumer offsets");
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM domain_event_consumer_offsets WHERE tenant_id = 'default'",
    )
    .fetch_one(&pool)
    .await
    .expect("count consumer offsets");
    assert_eq!(count, 2);
    drop(pool);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn consumer_failure_is_isolated_and_does_not_block_later_consumers() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-dispatcher-isolation-{}.sqlite",
        Uuid::new_v4()
    ));
    let pool = open_test_pool(&path).await;
    let failed_calls = Arc::new(AtomicUsize::new(0));
    let successful_calls = Arc::new(AtomicUsize::new(0));
    let dispatcher = EventDispatcher::with_consumers(
        pool.clone(),
        path.clone(),
        vec![
            Arc::new(TestConsumer {
                id: "test.failed",
                calls: failed_calls.clone(),
                fail: true,
                delay: Duration::ZERO,
            }),
            Arc::new(TestConsumer {
                id: "test.successful",
                calls: successful_calls.clone(),
                fail: false,
                delay: Duration::ZERO,
            }),
        ],
    );
    dispatcher
        .initialize_all_tenants()
        .await
        .expect("initialize test offsets");
    let event = DomainEvent::conversation_source_committed(
        "default",
        "sync-isolation",
        "source-isolation",
        1,
        ["session-isolation".to_string()],
    );
    append_test_event_sqlx(&pool, &event).await;

    assert_eq!(
        dispatcher
            .dispatch_once("default")
            .await
            .expect("dispatch cycle"),
        1
    );
    assert_eq!(failed_calls.load(Ordering::SeqCst), 1);
    assert_eq!(successful_calls.load(Ordering::SeqCst), 1);
    let successful_offset = sqlx::query_scalar::<_, i64>(
        "SELECT last_seq FROM domain_event_consumer_offsets WHERE consumer_id = 'test.successful' AND tenant_id = 'default'",
    )
    .fetch_one(&pool)
    .await
    .expect("read successful offset");
    assert_eq!(successful_offset, 1);

    // The failed consumer is backed off independently; an immediate cycle does
    // not hot-loop it and still leaves the successful consumer healthy.
    dispatcher
        .dispatch_once("default")
        .await
        .expect("second dispatch cycle");
    assert_eq!(failed_calls.load(Ordering::SeqCst), 1);
    assert_eq!(successful_calls.load(Ordering::SeqCst), 1);

    drop(pool);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn dispatcher_shutdown_is_bounded_when_a_consumer_does_not_cooperate() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-dispatcher-shutdown-{}.sqlite",
        Uuid::new_v4()
    ));
    let pool = open_test_pool(&path).await;
    let calls = Arc::new(AtomicUsize::new(0));
    let dispatcher = Arc::new(EventDispatcher::with_consumers(
        pool.clone(),
        path.clone(),
        vec![Arc::new(TestConsumer {
            id: "test.slow",
            calls: calls.clone(),
            fail: false,
            delay: Duration::from_millis(300),
        })],
    ));
    dispatcher
        .initialize_all_tenants()
        .await
        .expect("initialize shutdown offsets");
    let event = DomainEvent::conversation_source_committed(
        "default",
        "sync-shutdown",
        "source-shutdown",
        1,
        ["session-shutdown".to_string()],
    );
    append_test_event_sqlx(&pool, &event).await;

    let mut handle = dispatcher.start(&tokio::runtime::Handle::current());
    let started = Instant::now();
    let report = handle.stop_with_timeout(Duration::from_millis(40)).await;
    assert!(started.elapsed() < Duration::from_millis(250));
    assert!(report.timed_out);
    assert!(!report.drained);

    // Allow background work time to finish if still executing
    tokio::time::sleep(Duration::from_millis(350)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    drop(pool);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn notify_interrupts_idle_wait() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-dispatcher-notify-{}.sqlite",
        Uuid::new_v4()
    ));
    let pool = open_test_pool(&path).await;
    let calls = Arc::new(AtomicUsize::new(0));
    let dispatcher = Arc::new(EventDispatcher::with_consumers(
        pool.clone(),
        path.clone(),
        vec![Arc::new(TestConsumer {
            id: "test.notify",
            calls: calls.clone(),
            fail: false,
            delay: Duration::ZERO,
        })],
    ));
    dispatcher
        .initialize_all_tenants()
        .await
        .expect("initialize offsets");

    let mut handle = dispatcher.start(&tokio::runtime::Handle::current());

    // Append event and immediately notify
    let event = DomainEvent::conversation_source_committed(
        "default",
        "sync-notify",
        "source-notify",
        1,
        ["session-notify".to_string()],
    );
    append_test_event_sqlx(&pool, &event).await;

    let started = Instant::now();
    handle.notify();

    let mut woken = false;
    for _ in 0..25 {
        if calls.load(Ordering::SeqCst) > 0 {
            woken = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(woken, "worker should be woken up by notify");
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "notify should interrupt idle wait within 500ms, elapsed: {:?}",
        started.elapsed()
    );

    let report = handle.stop_with_timeout(Duration::from_millis(200)).await;
    assert!(report.drained);
    drop(pool);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn cancellation_interrupts_retry_sleep() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-dispatcher-cancel-retry-{}.sqlite",
        Uuid::new_v4()
    ));
    let pool = open_test_pool(&path).await;
    let calls = Arc::new(AtomicUsize::new(0));
    let dispatcher = Arc::new(EventDispatcher::with_consumers(
        pool.clone(),
        path.clone(),
        vec![Arc::new(TestConsumer {
            id: "test.retry_cancel",
            calls: calls.clone(),
            fail: true,
            delay: Duration::ZERO,
        })],
    ));
    dispatcher
        .initialize_all_tenants()
        .await
        .expect("initialize offsets");

    let event = DomainEvent::conversation_source_committed(
        "default",
        "sync-cancel-retry",
        "source-cancel-retry",
        1,
        ["session-cancel-retry".to_string()],
    );
    append_test_event_sqlx(&pool, &event).await;

    let mut handle = dispatcher.start(&tokio::runtime::Handle::current());

    // Wait until it has attempted once and failed, entering retry sleep
    for _ in 0..25 {
        if calls.load(Ordering::SeqCst) > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // Cancellation must interrupt the retry sleep immediately
    let started = Instant::now();
    let report = handle.stop_with_timeout(Duration::from_millis(150)).await;
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "cancellation should interrupt retry sleep quickly, elapsed: {:?}",
        started.elapsed()
    );
    assert!(!report.drained);

    drop(pool);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn stop_waits_for_tracked_worker_without_detach() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-dispatcher-stop-wait-{}.sqlite",
        Uuid::new_v4()
    ));
    let pool = open_test_pool(&path).await;
    let calls = Arc::new(AtomicUsize::new(0));
    let dispatcher = Arc::new(EventDispatcher::with_consumers(
        pool.clone(),
        path.clone(),
        vec![Arc::new(TestConsumer {
            id: "test.slow_finish",
            calls: calls.clone(),
            fail: false,
            delay: Duration::from_millis(80),
        })],
    ));
    dispatcher
        .initialize_all_tenants()
        .await
        .expect("initialize offsets");

    let event = DomainEvent::conversation_source_committed(
        "default",
        "sync-stop-wait",
        "source-stop-wait",
        1,
        ["session-stop-wait".to_string()],
    );
    append_test_event_sqlx(&pool, &event).await;

    let mut handle = dispatcher.start(&tokio::runtime::Handle::current());

    // Stop with enough grace for the slow consumer to complete cleanly
    let report = handle.stop_with_timeout(Duration::from_millis(350)).await;
    assert!(report.drained);
    assert!(!report.timed_out);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    drop(pool);
    let _ = std::fs::remove_file(&path);
}

fn event_id(event: &DomainEvent) -> &str {
    match event {
        DomainEvent::ConversationSourceCommitted { event_id, .. } => event_id,
        DomainEvent::TeamRunConfirmed { event_id, .. } => event_id,
    }
}
