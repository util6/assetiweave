use super::*;

#[tokio::test]
async fn persistent_binding_lifecycle() {
    let pool = crate::backend::store::test_support::create_test_pool().await;
    let store = PersistentBindingStore::new(pool);
    let tenant_id = "tenant_test";
    let key = "context_key_1";

    let loaded = store.load(tenant_id, key).await.expect("load binding");
    assert!(loaded.is_none());

    let binding = PersistentExecutionBinding {
        tenant_id: tenant_id.to_string(),
        execution_context_key: key.to_string(),
        provider_session_id: "prov_session_123".to_string(),
        agent_id: "agent_alpha".to_string(),
        installation_id: Some("inst_1".to_string()),
        model: Some("model_x".to_string()),
        workspace_path: "/tmp/workspace".to_string(),
        binding_version: 1,
        provider_metadata_json: "{}".to_string(),
    };

    store.save(&binding).await.expect("save binding");

    let loaded = store.load(tenant_id, key).await.expect("load binding");
    assert_eq!(loaded, Some(binding));

    store.delete(tenant_id, key).await.expect("delete binding");
    let loaded = store.load(tenant_id, key).await.expect("load binding");
    assert!(loaded.is_none());
}
