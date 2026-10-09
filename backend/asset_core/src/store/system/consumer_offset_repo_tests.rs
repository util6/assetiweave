use super::*;

#[tokio::test]
async fn consumer_offset_lifecycle() {
    let pool = crate::backend::store::test_support::create_test_pool().await;
    let tenant_id = "tenant_test";
    let consumer_id = "test_consumer";

    let initialized = is_consumer_offset_initialized_sqlx(&pool, consumer_id, tenant_id)
        .await
        .expect("check init");
    assert!(!initialized);

    init_consumer_offset_if_missing_sqlx(&pool, consumer_id, tenant_id, 10)
        .await
        .expect("init offset");

    let initialized_after = is_consumer_offset_initialized_sqlx(&pool, consumer_id, tenant_id)
        .await
        .expect("check init after");
    assert!(initialized_after);

    let last_seq = load_consumer_last_seq_sqlx(&pool, consumer_id, tenant_id)
        .await
        .expect("load seq");
    assert_eq!(last_seq, Some(10));

    upsert_consumer_offset_sqlx(&pool, consumer_id, tenant_id, 25)
        .await
        .expect("upsert offset");

    let updated_seq = load_consumer_last_seq_sqlx(&pool, consumer_id, tenant_id)
        .await
        .expect("load updated seq");
    assert_eq!(updated_seq, Some(25));
}
