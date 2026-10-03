use super::*;
use crate::backend::domain::{
    ConversationContentCardDescriptor, ConversationPartKind, ConversationPartRole,
    ConversationSourceKind, NormalizedConversationPart, NormalizedConversationTurn,
};

#[tokio::test]
async fn test_reproject_conversation_session_materializes_parts_and_links() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-reproject-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .unwrap();
    let pool = database.pool();
    let tenant_id = "test-tenant-reproject";
    let now = Utc::now().to_rfc3339();

    // Ensure tenant exists
    sqlx::query("INSERT OR IGNORE INTO tenants (id, created_at, updated_at) VALUES (?1, ?2, ?2)")
        .bind(tenant_id)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();

    let source = ConversationSource {
        id: "source-reproject".to_string(),
        adapter_id: "codex".to_string(),
        name: "Test Source".to_string(),
        kind: ConversationSourceKind::File,
        location: "/tmp/test.jsonl".to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: now.clone(),
        updated_at: now.clone(),
    };

    upsert_conversation_source_sqlx(pool, tenant_id, &source)
        .await
        .unwrap();

    let cmd_part = NormalizedConversationPart {
        role: ConversationPartRole::Assistant,
        kind: ConversationPartKind::Command,
        text: None,
        language: None,
        command: Some("git status".to_string()),
        cwd: Some("/repo".to_string()),
        status: None,
        exit_code: None,
        command_label: None,
        source_execution_id: Some("exec-10".to_string()),
        content_card: Some(ConversationContentCardDescriptor {
            schema_version: 1,
            kind: "codex.command".to_string(),
            semantic_role: Some("command".to_string()),
            renderer: Some("command".to_string()),
        }),
        metadata_json: None,
    };

    let res_part = NormalizedConversationPart {
        role: ConversationPartRole::Tool,
        kind: ConversationPartKind::Tool,
        text: Some("clean working tree".to_string()),
        language: None,
        command: None,
        cwd: None,
        status: Some("success".to_string()),
        exit_code: Some(0),
        command_label: None,
        source_execution_id: Some("exec-10".to_string()),
        content_card: Some(ConversationContentCardDescriptor {
            schema_version: 1,
            kind: "codex.result".to_string(),
            semantic_role: Some("result".to_string()),
            renderer: Some("terminal_output".to_string()),
        }),
        metadata_json: None,
    };

    let subagent_part = NormalizedConversationPart {
        role: ConversationPartRole::Assistant,
        kind: ConversationPartKind::Subagent,
        text: Some(
            r#"{"agent_role": "Reviewer", "task": "review diff", "child_session_id": "child-session-99"}"#
                .to_string(),
        ),
        language: None,
        command: None,
        cwd: None,
        status: Some("completed".to_string()),
        exit_code: None,
        command_label: None,
        source_execution_id: None,
        content_card: Some(ConversationContentCardDescriptor {
            schema_version: 1,
            kind: "codex.tool".to_string(),
            semantic_role: Some("subagent".to_string()),
            renderer: Some("subagent_tree".to_string()),
        }),
        metadata_json: Some(
            r#"{"type": "subagent", "child_session_id": "child-session-99"}"#.to_string(),
        ),
    };

    let normalized = NormalizedConversationSession {
        external_id: "ext-reproject-session-1".to_string(),
        title: Some("Initial Projection".to_string()),
        project_path: Some("/repo".to_string()),
        started_at: Some(now.clone()),
        updated_at: Some(now.clone()),
        source_locator: Some("/tmp/test.jsonl".to_string()),
        source_fingerprint: Some("v1".to_string()),
        user_visible: Some(true),
        execution_origin: Some("user".to_string()),
        execution_purpose: None,
        turns: vec![NormalizedConversationTurn {
            external_id: "turn-1".to_string(),
            turn_index: 0,
            user_text: "check git status and review".to_string(),
            title: None,
            started_at: Some(now.clone()),
            ended_at: Some(now.clone()),
            model: None,
            parts: vec![cmd_part, res_part, subagent_part],
        }],
    };

    // 1. First materialization: projection_version = 1
    reproject_conversation_session_sqlx(
        pool,
        tenant_id,
        &source,
        &normalized,
        Some("hash-1"),
        Some(1),
        10,
        Some(1),
    )
    .await
    .unwrap();

    let session_id = stable_id(
        "conversation-session",
        &[&source.id, &normalized.external_id],
    );
    let detail = load_conversation_session_detail_sqlx(pool, tenant_id, &session_id)
        .await
        .unwrap();

    assert_eq!(detail.session.title, "Initial Projection");
    assert_eq!(detail.questions.len(), 1);
    assert_eq!(detail.questions[0].parts.len(), 3);

    // Verify part_links were created
    let turn_id = stable_id("conversation-turn", &[&session_id, "turn-1"]);
    let links = load_conversation_part_links_for_turn_sqlx(pool, tenant_id, &turn_id)
        .await
        .unwrap();

    assert!(!links.is_empty());
    assert!(links.iter().any(|l| l.relation == "execution_result"));
    assert!(links.iter().any(|l| l.relation == "execution_command"));
    assert!(links
        .iter()
        .any(|l| l.relation == "spawned_session" && l.target_id == "child-session-99"));

    // 2. Second materialization: simulated replay with updated title and projection_version = 2
    let mut updated_normalized = normalized.clone();
    updated_normalized.title = Some("Replayed Projection v2".to_string());

    reproject_conversation_session_sqlx(
        pool,
        tenant_id,
        &source,
        &updated_normalized,
        Some("hash-2"),
        Some(1),
        10,
        Some(2),
    )
    .await
    .unwrap();

    let updated_detail = load_conversation_session_detail_sqlx(pool, tenant_id, &session_id)
        .await
        .unwrap();

    assert_eq!(updated_detail.session.title, "Replayed Projection v2");

    // Verify observations track projection_version = 2
    let versions = load_conversation_session_versions_sqlx(
        pool,
        tenant_id,
        &source.id,
        ConversationRecordKind::Session,
        Some("hash-2"),
        Some(1),
        10,
        Some(2),
    )
    .await
    .unwrap();

    assert_eq!(
        versions.get(&normalized.external_id).map(|s| s.as_str()),
        Some("v1")
    );
    drop(database);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
}
