use super::*;

#[tokio::test]
async fn memory_generation_tool_returns_err_for_missing_or_empty_lease() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-gen-tool-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();

    let err1 = service
        .call_memory_generation_tool("", "", "get_session_outline", &serde_json::json!({}))
        .await;
    assert!(err1.is_err());

    let err2 = service
        .call_memory_generation_tool("job-1", "lease-1", "unknown_tool", &serde_json::json!({}))
        .await;
    assert!(err2.is_err());
}
