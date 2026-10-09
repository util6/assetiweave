use super::*;
use crate::backend::domain::conversations::DomainEvent;

#[test]
fn built_in_consumers_declare_their_initial_position() {
    let search = SearchIndexAdvanceConsumer { database: None };
    assert_eq!(search.id(), "search.index_advance");
    assert_eq!(search.initial_position(), InitialPosition::GenesisZero);

    let session = SessionMemoryConsumer;
    assert_eq!(session.id(), "memory.session_enqueue");
    assert_eq!(
        session.initial_position(),
        InitialPosition::BackfillThenCutoff
    );
}

#[tokio::test]
async fn session_commit_creates_one_durable_phase1_job() {
    let pool = crate::backend::store::test_support::create_test_pool().await;
    let tenant_id = "default";
    let sync_run_id = "sync_1";
    let source_id = "src_1";
    let session_id = "sess_1";

    // Setup an initial session record
    sqlx::query(
        "INSERT INTO conversation_sessions (id, tenant_id, adapter_id, source_id, external_id, title, missing, created_at, imported_at) VALUES (?1, ?2, 'test_adapter', ?3, ?4, ?5, 0, datetime('now'), datetime('now'))"
    )
    .bind(session_id)
    .bind(tenant_id)
    .bind(source_id)
    .bind("ext_1")
    .bind("Session 1")
    .execute(&pool)
    .await
    .expect("insert session");

    let event = DomainEvent::conversation_source_committed(
        tenant_id,
        sync_run_id,
        source_id,
        1,
        vec![session_id.to_string()],
    );

    let consumer = SessionMemoryConsumer;
    let cx = ConsumerCx {
        pool: pool.clone(),
        db_path: std::path::PathBuf::from("/tmp/test_db"),
        consumer_id: consumer.id().to_string(),
        tenant_id: tenant_id.to_string(),
        batch_last_seq: 1,
    };

    let batch = vec![SequencedEvent { seq: 1, event }];
    consumer
        .handle(&batch, &cx)
        .await
        .expect("handle event batch");

    let count = crate::backend::store::count_session_memory_rows_sqlx(&pool, tenant_id, "jobs")
        .await
        .expect("count rows");
    assert_eq!(count, 1); // 1 job created
}
