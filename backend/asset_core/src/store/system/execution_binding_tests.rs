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

/// 便携路径锚点契约：`workspace_path` 落库必须折叠，读回必须展开。
///
/// `load` 的返回值会被 agent_engine 直接当作进程工作目录，因此这里断言的是
/// 一对不可拆开的行为——只折叠不展开会让 Agent 起不来，只展开不折叠会让数据库不可迁移。
#[tokio::test]
async fn binding_stores_portable_anchor_and_expands_on_load() {
    let pool = crate::backend::store::test_support::create_test_pool().await;
    let store = PersistentBindingStore::new(pool.clone());
    let tenant_id = "tenant_anchor";
    let key = "context_anchor";

    let home = dirs::home_dir().expect("home directory");
    let workspace_abs = home
        .join(".assetiweave/agent-executions/execution-anchor-test")
        .to_string_lossy()
        .into_owned();

    let binding = PersistentExecutionBinding {
        tenant_id: tenant_id.to_string(),
        execution_context_key: key.to_string(),
        provider_session_id: "prov_session_anchor".to_string(),
        agent_id: "agent_alpha".to_string(),
        installation_id: None,
        model: None,
        workspace_path: workspace_abs.clone(),
        binding_version: 1,
        provider_metadata_json: "{}".to_string(),
    };

    store.save(&binding).await.expect("save binding");

    let stored: String = sqlx::query_scalar(
        "SELECT workspace_path FROM agent_execution_bindings \
         WHERE tenant_id = ?1 AND execution_context_key = ?2",
    )
    .bind(tenant_id)
    .bind(key)
    .fetch_one(&pool)
    .await
    .expect("read stored workspace_path");
    assert_eq!(
        stored, "~/.assetiweave/agent-executions/execution-anchor-test",
        "workspace_path 必须以便携锚点形态落库"
    );

    let loaded = store
        .load(tenant_id, key)
        .await
        .expect("load binding")
        .expect("binding exists");
    assert_eq!(
        loaded.workspace_path, workspace_abs,
        "load 必须把锚点展开回当前主机的真实路径"
    );
}
