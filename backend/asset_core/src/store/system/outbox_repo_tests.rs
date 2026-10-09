use super::*;
use crate::backend::domain::conversations::DomainEvent;

#[tokio::test]
async fn outbox_append_is_atomic_with_the_business_transaction() {
    let pool = crate::backend::store::test_support::create_test_pool().await;
    let tenant_id = "tenant_test";

    let mut tx = pool.begin().await.expect("begin tx");
    let event = DomainEvent::conversation_source_committed(
        tenant_id,
        "sync_run_1",
        "source_1",
        1,
        vec!["s1".to_string()],
    );
    append_outbox_event_sqlx_tx(&mut tx, &event)
        .await
        .expect("append outbox");
    tx.commit().await.expect("commit tx");

    let max_seq = load_max_outbox_seq_sqlx(&pool, tenant_id)
        .await
        .expect("load max seq");
    assert!(max_seq >= 1);

    let rows = load_pending_outbox_rows_sqlx(&pool, tenant_id, "test_consumer", 10)
        .await
        .expect("load pending rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, max_seq);
}
