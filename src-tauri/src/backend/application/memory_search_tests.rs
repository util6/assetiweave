use super::*;
use crate::backend::application::AppService;
use crate::backend::models::{
    ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState,
    ConversationPartKind, ConversationPartRole, ConversationSource, ConversationSourceKind,
    MemoryRecordKind, MemoryScope, NormalizedConversationPart, NormalizedConversationSession,
    NormalizedConversationTurn,
};
use uuid::Uuid;

async fn setup_fixture_service(test_name: &str) -> (AppService, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-search-{}-{}",
        test_name,
        Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create test root");
    let service = AppService::open_with_db_path(root.join("app.db"))
        .await
        .expect("open test service");
    (service, root)
}

async fn insert_test_source_and_session(
    service: &AppService,
    tenant_id: &str,
    source_id: &str,
    session_external_id: &str,
    session_title: &str,
    user_query: &str,
    assistant_reply: &str,
) -> (ConversationSource, String) {
    let timestamp = "2026-08-30T22:00:00Z";
    let adapter_id = format!("{source_id}-adapter");
    let adapter = ConversationAdapter {
        id: adapter_id.clone(),
        name: "Test Adapter".to_string(),
        kind: ConversationAdapterKind::External,
        version: "1.0.0".to_string(),
        enabled: true,
        manifest_path: None,
        executable_path: None,
        content_hash: None,
        trusted_hash: None,
        trust_state: ConversationAdapterTrustState::Trusted,
        protocol_version: Some(1),
        capabilities: vec!["read_session".to_string()],
        input_kinds: vec![ConversationSourceKind::Directory],
        card_contract_version: None,
        card_kinds: Vec::new(),
        created_at: timestamp.to_string(),
        updated_at: timestamp.to_string(),
    };
    let source = ConversationSource {
        id: source_id.to_string(),
        adapter_id,
        name: "Test Source".to_string(),
        kind: ConversationSourceKind::Directory,
        location: "/fixture/path".to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: timestamp.to_string(),
        updated_at: timestamp.to_string(),
    };
    let session = NormalizedConversationSession {
        external_id: session_external_id.to_string(),
        title: Some(session_title.to_string()),
        project_path: None,
        started_at: Some(timestamp.to_string()),
        updated_at: Some(timestamp.to_string()),
        source_locator: Some(format!("fixture://{session_external_id}")),
        source_fingerprint: Some("fingerprint-1".to_string()),
        turns: vec![NormalizedConversationTurn {
            external_id: "turn-1".to_string(),
            turn_index: 0,
            user_text: user_query.to_string(),
            title: None,
            started_at: Some(timestamp.to_string()),
            ended_at: Some(timestamp.to_string()),
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some(assistant_reply.to_string()),
                language: None,
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
                command_label: None,
                source_execution_id: None,
                content_card: None,
                metadata_json: None,
            }],
        }],
        ..Default::default()
    };
    crate::backend::store::upsert_conversation_adapter_sqlx(service.db.pool(), tenant_id, &adapter)
        .await
        .expect("upsert adapter");
    crate::backend::store::upsert_conversation_source_sqlx(service.db.pool(), tenant_id, &source)
        .await
        .expect("upsert source");
    crate::backend::store::import_conversation_sessions_sqlx(
        service.db.pool(),
        tenant_id,
        &source,
        &[session],
        false,
    )
    .await
    .expect("import session");
    let session_id: String = sqlx::query_scalar(
        "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND external_id = ?2",
    )
    .bind(tenant_id)
    .bind(session_external_id)
    .fetch_one(service.db.pool())
    .await
    .expect("load session id");
    (source, session_id)
}

#[tokio::test(flavor = "multi_thread")]
async fn search_memory_recall_empty_and_no_result() {
    let (service, root) = setup_fixture_service("empty-no-result").await;

    let empty_params = MemoryRecallSearchParams {
        query: "   ".to_string(),
        scope: MemoryScope::default(),
        since: None,
        until: None,
        file: None,
        command: None,
        error: None,
        limit: None,
        offset: None,
    };
    let err = service
        .search_memory_recall(empty_params)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));

    let long_query_params = MemoryRecallSearchParams {
        query: "a".repeat(513),
        scope: MemoryScope::default(),
        since: None,
        until: None,
        file: None,
        command: None,
        error: None,
        limit: None,
        offset: None,
    };
    let err = service
        .search_memory_recall(long_query_params)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));

    let long_hint_params = MemoryRecallSearchParams {
        query: "valid query".to_string(),
        scope: MemoryScope::default(),
        since: None,
        until: None,
        file: Some("f".repeat(513)),
        command: None,
        error: None,
        limit: None,
        offset: None,
    };
    let err = service
        .search_memory_recall(long_hint_params)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));

    let no_match_params = MemoryRecallSearchParams {
        query: "nonexistent query xyz 123".to_string(),
        scope: MemoryScope::default(),
        since: None,
        until: None,
        file: None,
        command: None,
        error: None,
        limit: None,
        offset: None,
    };
    let result = service.search_memory_recall(no_match_params).await.unwrap();
    assert!(result.hits.is_empty());
    assert_eq!(result.total_count, 0);

    drop(service);
    std::fs::remove_dir_all(root).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn search_memory_recall_locators_and_ranking() {
    let (service, root) = setup_fixture_service("locators-ranking").await;

    let (_source, session_id) = insert_test_source_and_session(
        &service,
        "default",
        "source-migration",
        "session-migration",
        "Database Migration Session",
        "How do we handle database migration?",
        "Execute database migration carefully with schema updates.",
    )
    .await;

    let params = MemoryRecallSearchParams {
        query: "database migration".to_string(),
        scope: MemoryScope::default(),
        since: None,
        until: None,
        file: None,
        command: None,
        error: None,
        limit: Some(10),
        offset: None,
    };
    let result = service.search_memory_recall(params).await.unwrap();
    assert!(
        !result.hits.is_empty(),
        "expected hits for 'database migration'"
    );

    for window in result.hits.windows(2) {
        assert!(window[0].score >= window[1].score);
    }

    let hit = &result.hits[0];
    assert_eq!(hit.record_kind, MemoryRecordKind::Session);
    assert_eq!(hit.session_id, session_id);
    assert!(!hit.question_id.is_empty());
    assert!(!hit.block_id.is_empty());
    assert!(
        hit.snippet.to_lowercase().contains("migration")
            || hit.snippet.to_lowercase().contains("database")
    );

    drop(service);
    std::fs::remove_dir_all(root).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn search_memory_recall_tenant_isolation() {
    let (service, root) = setup_fixture_service("tenant-isolation").await;

    sqlx::query(
            "INSERT OR IGNORE INTO tenants (id, name, created_at, updated_at) VALUES ('tenant-b', 'Tenant B', '2026-08-30T22:00:00Z', '2026-08-30T22:00:00Z')",
        )
        .execute(service.db.pool())
        .await
        .expect("insert tenant-b");

    let (_source, _session_id) = insert_test_source_and_session(
        &service,
        "tenant-b",
        "source-tenant-b",
        "session-tenant-b",
        "Tenant B Exclusive Session",
        "Tenant B special migration secrets",
        "Confidential database migration info for tenant B only.",
    )
    .await;

    let params = MemoryRecallSearchParams {
        query: "Tenant B special migration secrets".to_string(),
        scope: MemoryScope::default(),
        since: None,
        until: None,
        file: None,
        command: None,
        error: None,
        limit: None,
        offset: None,
    };
    let result = service.search_memory_recall(params).await.unwrap();
    assert!(
        result.hits.is_empty(),
        "tenant-b data must not leak into default tenant search"
    );

    drop(service);
    std::fs::remove_dir_all(root).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn search_memory_recall_excluded_records() {
    let (service, root) = setup_fixture_service("excluded-records").await;

    let (source, _session_id) = insert_test_source_and_session(
        &service,
        "default",
        "source-excluded",
        "session-excluded",
        "Database Indexing Optimization",
        "How do we optimize database indexing?",
        "Use B-tree indexing for database optimization.",
    )
    .await;

    let query_params = || MemoryRecallSearchParams {
        query: "database indexing".to_string(),
        scope: MemoryScope::default(),
        since: None,
        until: None,
        file: None,
        command: None,
        error: None,
        limit: None,
        offset: None,
    };

    let initial_result = service.search_memory_recall(query_params()).await.unwrap();
    assert!(
        !initial_result.hits.is_empty(),
        "initial search should find record"
    );

    // (a) Disabled source
    sqlx::query(
        "UPDATE conversation_sources SET enabled = 0 WHERE tenant_id = 'default' AND id = ?1",
    )
    .bind(&source.id)
    .execute(service.db.pool())
    .await
    .expect("disable source");
    let disabled_result = service.search_memory_recall(query_params()).await.unwrap();
    assert!(
        disabled_result.hits.is_empty(),
        "disabled source records must be excluded"
    );

    sqlx::query(
        "UPDATE conversation_sources SET enabled = 1 WHERE tenant_id = 'default' AND id = ?1",
    )
    .bind(&source.id)
    .execute(service.db.pool())
    .await
    .expect("re-enable source");

    // (b) Missing session
    sqlx::query("UPDATE conversation_sessions SET missing = 1 WHERE tenant_id = 'default'")
        .execute(service.db.pool())
        .await
        .expect("mark session missing");
    let missing_result = service.search_memory_recall(query_params()).await.unwrap();
    assert!(
        missing_result.hits.is_empty(),
        "missing session records must be excluded"
    );

    sqlx::query("UPDATE conversation_sessions SET missing = 0 WHERE tenant_id = 'default'")
        .execute(service.db.pool())
        .await
        .expect("unmark session missing");

    // (c) assetiweave-memory-recall source
    sqlx::query("UPDATE conversation_sources SET adapter_id = 'assetiweave-memory-recall' WHERE tenant_id = 'default' AND id = ?1")
            .bind(&source.id)
            .execute(service.db.pool())
            .await
            .expect("set adapter to assetiweave-memory-recall");
    let recall_adapter_result = service.search_memory_recall(query_params()).await.unwrap();
    assert!(
        recall_adapter_result.hits.is_empty(),
        "assetiweave-memory-recall sources must be excluded"
    );

    drop(service);
    std::fs::remove_dir_all(root).ok();
}
