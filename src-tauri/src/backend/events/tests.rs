use super::{
    append_outbox_event_sqlx_tx, ConsumerCx, DomainEvent, DomainEventConsumer, EventDispatcher,
    InitialPosition, SequencedEvent,
};
use crate::backend::{runtime::AppError, store::Database};
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

    fn handle(&self, _batch: &[SequencedEvent], _cx: &ConsumerCx) -> Result<(), AppError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
        if self.fail {
            Err(AppError::External("test consumer failure".to_string()))
        } else {
            Ok(())
        }
    }
}

#[test]
fn built_in_consumers_declare_their_initial_position() {
    let consumers: Vec<Arc<dyn DomainEventConsumer>> = vec![
        Arc::new(super::SearchIndexAdvanceConsumer),
        Arc::new(super::MemoryEvidenceStaleConsumer),
    ];
    assert!(consumers
        .iter()
        .all(|consumer| consumer.initial_position() == InitialPosition::GenesisZero));
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

    fn handle(&self, _batch: &[SequencedEvent], _cx: &ConsumerCx) -> Result<(), AppError> {
        Ok(())
    }
}

#[test]
fn backfill_consumer_cannot_fall_back_to_a_zero_offset() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-dispatcher-backfill-registration-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open(&path).expect("open test database");
    let dispatcher = EventDispatcher::with_consumers(
        database.clone(),
        path.clone(),
        vec![Arc::new(BackfillTestConsumer)],
    );

    let error = dispatcher
        .initialize_tenant("default")
        .expect_err("backfill registration must require an explicit migration");
    assert!(error.to_string().contains("backfill-and-cutoff"));

    drop(database);
    let _ = std::fs::remove_file(&path);
}

fn append_test_event(database: &Database, event: &DomainEvent) {
    database.block_on(async {
        let mut tx = database.pool().begin().await.expect("begin event tx");
        append_outbox_event_sqlx_tx(&mut tx, event)
            .await
            .expect("append test event");
        tx.commit().await.expect("commit event tx");
    });
}

#[test]
fn outbox_append_is_atomic_with_the_business_transaction() {
    let path = std::env::temp_dir().join(format!("assetiweave-outbox-{}.sqlite", Uuid::new_v4()));
    let database = Database::open_initialized(&path).expect("open test database");
    let event = DomainEvent::conversation_source_committed(
        "default",
        "sync-atomic",
        "source-atomic",
        1,
        ["session-atomic".to_string()],
    );

    database.block_on(async {
        let mut tx = database.pool().begin().await.expect("begin rollback tx");
        append_outbox_event_sqlx_tx(&mut tx, &event)
            .await
            .expect("append event");
        tx.rollback().await.expect("rollback business tx");

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM domain_event_outbox WHERE event_id = ?1")
                .bind(event_id(&event))
                .fetch_one(database.pool())
                .await
                .expect("count rolled back event");
        assert_eq!(count, 0);

        let mut tx = database.pool().begin().await.expect("begin commit tx");
        append_outbox_event_sqlx_tx(&mut tx, &event)
            .await
            .expect("append committed event");
        tx.commit().await.expect("commit business tx");

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM domain_event_outbox WHERE event_id = ?1")
                .bind(event_id(&event))
                .fetch_one(database.pool())
                .await
                .expect("count committed event");
        assert_eq!(count, 1);
    });
    drop(database);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn resident_dispatcher_initializes_offsets_for_all_tenants() {
    let path =
        std::env::temp_dir().join(format!("assetiweave-dispatcher-{}.sqlite", Uuid::new_v4()));
    let database = Database::open_initialized(&path).expect("open test database");
    let dispatcher = EventDispatcher::new(database.clone(), path.clone());
    dispatcher
        .initialize_all_tenants()
        .expect("initialize consumer offsets");
    let count = database.block_on(async {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM domain_event_consumer_offsets WHERE tenant_id = 'default'",
        )
        .fetch_one(database.pool())
        .await
        .expect("count consumer offsets")
    });
    assert_eq!(count, 2);
    drop(database);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn consumer_failure_is_isolated_and_does_not_block_later_consumers() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-dispatcher-isolation-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_initialized(&path).expect("open test database");
    let failed_calls = Arc::new(AtomicUsize::new(0));
    let successful_calls = Arc::new(AtomicUsize::new(0));
    let dispatcher = EventDispatcher::with_consumers(
        database.clone(),
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
        .expect("initialize test offsets");
    let event = DomainEvent::conversation_source_committed(
        "default",
        "sync-isolation",
        "source-isolation",
        1,
        ["session-isolation".to_string()],
    );
    append_test_event(&database, &event);

    assert_eq!(
        dispatcher.dispatch_once("default").expect("dispatch cycle"),
        1
    );
    assert_eq!(failed_calls.load(Ordering::SeqCst), 1);
    assert_eq!(successful_calls.load(Ordering::SeqCst), 1);
    let successful_offset = database.block_on(async {
        sqlx::query_scalar::<_, i64>(
            "SELECT last_seq FROM domain_event_consumer_offsets WHERE consumer_id = 'test.successful' AND tenant_id = 'default'",
        )
        .fetch_one(database.pool())
        .await
        .expect("read successful offset")
    });
    assert_eq!(successful_offset, 1);

    // The failed consumer is backed off independently; an immediate cycle does
    // not hot-loop it and still leaves the successful consumer healthy.
    dispatcher
        .dispatch_once("default")
        .expect("second dispatch cycle");
    assert_eq!(failed_calls.load(Ordering::SeqCst), 1);
    assert_eq!(successful_calls.load(Ordering::SeqCst), 1);

    drop(database);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn dispatcher_shutdown_is_bounded_when_a_consumer_does_not_cooperate() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-dispatcher-shutdown-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_initialized(&path).expect("open test database");
    let calls = Arc::new(AtomicUsize::new(0));
    let dispatcher = Arc::new(EventDispatcher::with_consumers(
        database.clone(),
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
        .expect("initialize shutdown offsets");
    let event = DomainEvent::conversation_source_committed(
        "default",
        "sync-shutdown",
        "source-shutdown",
        1,
        ["session-shutdown".to_string()],
    );
    append_test_event(&database, &event);

    let handle = dispatcher.start();
    let started = Instant::now();
    let report = handle.stop_with_timeout(Duration::from_millis(40));
    assert!(started.elapsed() < Duration::from_millis(250));
    assert!(report.timed_out);
    assert!(!report.drained);

    // The detached worker is allowed to finish its current blocking call before
    // the fixture is removed; shutdown itself remains bounded above.
    std::thread::sleep(Duration::from_millis(350));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    drop(database);
    let _ = std::fs::remove_file(&path);
}

fn event_id(event: &DomainEvent) -> &str {
    match event {
        DomainEvent::ConversationSourceCommitted { event_id, .. } => event_id,
        DomainEvent::TeamRunConfirmed { event_id, .. } => event_id,
    }
}
