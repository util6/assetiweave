use super::*;
use crate::backend::application::AppService;
use crate::backend::domain::{
    ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState,
    ConversationPartKind, ConversationPartRole, ConversationSource, ConversationSourceKind,
    NormalizedConversationPart, NormalizedConversationSession, NormalizedConversationTurn,
};
use uuid::Uuid;

async fn setup_fixture_service(test_name: &str) -> (AppService, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-outline-{}-{}",
        test_name,
        Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create test root");
    let service = AppService::open_with_db_path(root.join("app.db"))
        .await
        .expect("open test service");
    (service, root)
}

#[tokio::test(flavor = "multi_thread")]
async fn test_conversation_session_outline_folding_and_universal_resolver() {
    let (service, root) = setup_fixture_service("folding-resolver").await;
    let tenant_id = "default";
    let timestamp = "2026-09-28T12:00:00Z";
    let adapter_id = "test-adapter";
    let source_id = "test-source";

    let adapter = ConversationAdapter {
        id: adapter_id.to_string(),
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
        projection_version: Some(1),
        created_at: timestamp.to_string(),
        updated_at: timestamp.to_string(),
    };
    let source = ConversationSource {
        id: source_id.to_string(),
        adapter_id: adapter_id.to_string(),
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
        external_id: "session-ext-001".to_string(),
        title: Some("Session Outline Test Title".to_string()),
        project_path: None,
        started_at: Some(timestamp.to_string()),
        updated_at: Some(timestamp.to_string()),
        source_locator: Some("fixture://session-ext-001".to_string()),
        source_fingerprint: Some("fingerprint-1".to_string()),
        turns: vec![
            NormalizedConversationTurn {
                external_id: "turn-ext-01".to_string(),
                turn_index: 0,
                user_text: "Please run diagnostics and show status".to_string(),
                title: None,
                started_at: Some(timestamp.to_string()),
                ended_at: Some(timestamp.to_string()),
                model: None,
                parts: vec![
                    NormalizedConversationPart {
                        role: ConversationPartRole::Assistant,
                        kind: ConversationPartKind::Command,
                        text: None,
                        language: None,
                        command: Some("cargo check".to_string()),
                        cwd: None,
                        status: Some("success".to_string()),
                        exit_code: Some(0),
                        command_label: Some("check".to_string()),
                        source_execution_id: None,
                        content_card: None,
                        metadata_json: None,
                    },
                    NormalizedConversationPart {
                        role: ConversationPartRole::Assistant,
                        kind: ConversationPartKind::Command,
                        text: None,
                        language: None,
                        command: Some("cargo test".to_string()),
                        cwd: None,
                        status: Some("success".to_string()),
                        exit_code: Some(0),
                        command_label: Some("test".to_string()),
                        source_execution_id: None,
                        content_card: None,
                        metadata_json: None,
                    },
                    NormalizedConversationPart {
                        role: ConversationPartRole::Assistant,
                        kind: ConversationPartKind::Command,
                        text: None,
                        language: None,
                        command: Some("git status".to_string()),
                        cwd: None,
                        status: Some("success".to_string()),
                        exit_code: Some(0),
                        command_label: Some("status".to_string()),
                        source_execution_id: None,
                        content_card: None,
                        metadata_json: None,
                    },
                    NormalizedConversationPart {
                        role: ConversationPartRole::Assistant,
                        kind: ConversationPartKind::Text,
                        text: Some("All diagnostics passed successfully.".to_string()),
                        language: None,
                        command: None,
                        cwd: None,
                        status: None,
                        exit_code: None,
                        command_label: None,
                        source_execution_id: None,
                        content_card: None,
                        metadata_json: None,
                    },
                ],
            },
            NormalizedConversationTurn {
                external_id: "turn-ext-02".to_string(),
                turn_index: 1,
                user_text: "Now clean up".to_string(),
                title: None,
                started_at: Some(timestamp.to_string()),
                ended_at: Some(timestamp.to_string()),
                model: None,
                parts: vec![NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Text,
                    text: Some("Cleaned up.".to_string()),
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
            },
        ],
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

    // Retrieve full session ID
    let full_session_id: String = sqlx::query_scalar(
        "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND external_id = 'session-ext-001'",
    )
    .bind(tenant_id)
    .fetch_one(service.db.pool())
    .await
    .expect("load full session id");

    let short_session_id = crate::backend::domain::conversation_id_fragment(&full_session_id);

    // 1. Query by full session ID
    let outline = service
        .get_conversation_session_outline(ConversationSessionOutlineParams {
            id: full_session_id.clone(),
        })
        .await
        .expect("get outline with full session ID");

    match outline {
        ConversationOutline::Session(s) => {
            assert_eq!(s.session_id, short_session_id);
            assert_eq!(s.total_turns, 2);
            assert_eq!(s.turns.rows[0].index, 1);
            assert_eq!(
                s.turns.rows[0].question,
                "Please run diagnostics and show status"
            );
            assert_eq!(s.turns.rows[0].activities, vec!["check", "test", "status"]);
            assert_eq!(s.turns.rows[0].cards, 4);
            assert!(s.turns.rows[0].answer_id.is_some());
            assert_eq!(s.turns.rows[1].index, 2);
            assert_eq!(s.turns.rows[1].question, "Now clean up");
        }
        _ => panic!("expected session outline"),
    }

    // 2. Query by 8-hex short session ID
    let outline_by_short = service
        .get_conversation_session_outline(ConversationSessionOutlineParams {
            id: short_session_id.clone(),
        })
        .await
        .expect("get outline with short session ID");
    assert!(matches!(outline_by_short, ConversationOutline::Session(_)));

    drop(service);
    std::fs::remove_dir_all(root).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_conversation_session_get_roles_filter_and_short_id_block_get() {
    let (service, root) = setup_fixture_service("roles-block-get").await;
    let tenant_id = "default";
    let timestamp = "2026-09-28T12:00:00Z";
    let adapter_id = "test-adapter";
    let source_id = "test-source";

    let adapter = ConversationAdapter {
        id: adapter_id.to_string(),
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
        projection_version: Some(1),
        created_at: timestamp.to_string(),
        updated_at: timestamp.to_string(),
    };
    let source = ConversationSource {
        id: source_id.to_string(),
        adapter_id: adapter_id.to_string(),
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
        external_id: "session-ext-002".to_string(),
        title: Some("Session Filter Test".to_string()),
        project_path: None,
        started_at: Some(timestamp.to_string()),
        updated_at: Some(timestamp.to_string()),
        source_locator: Some("fixture://session-ext-002".to_string()),
        source_fingerprint: Some("fingerprint-2".to_string()),
        turns: vec![NormalizedConversationTurn {
            external_id: "turn-ext-01".to_string(),
            turn_index: 0,
            user_text: "What is 2 + 2?".to_string(),
            title: None,
            started_at: Some(timestamp.to_string()),
            ended_at: Some(timestamp.to_string()),
            model: None,
            parts: vec![
                NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Command,
                    text: None,
                    language: None,
                    command: Some("calc 2+2".to_string()),
                    cwd: None,
                    status: Some("success".to_string()),
                    exit_code: Some(0),
                    command_label: Some("calc".to_string()),
                    source_execution_id: None,
                    content_card: None,
                    metadata_json: None,
                },
                NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Text,
                    text: Some("The result is 4.".to_string()),
                    language: None,
                    command: None,
                    cwd: None,
                    status: None,
                    exit_code: None,
                    command_label: None,
                    source_execution_id: None,
                    content_card: None,
                    metadata_json: None,
                },
            ],
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

    let full_session_id: String = sqlx::query_scalar(
        "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND external_id = 'session-ext-002'",
    )
    .bind(tenant_id)
    .fetch_one(service.db.pool())
    .await
    .expect("load full session id");

    // 1. Test get_conversation_session with roles = ["question", "answer"]
    let filtered_session = service
        .get_conversation_session(ConversationSessionGetParams {
            session_id: full_session_id.clone(),
            roles: Some(vec!["question".to_string(), "answer".to_string()]),
        })
        .await
        .expect("get session with role filter");

    let q = &filtered_session.questions[0];
    assert_eq!(q.turns[0].user_text, "What is 2 + 2?");
    assert_eq!(q.parts.len(), 1);
    assert_eq!(q.parts[0].role, ConversationPartRole::Assistant);
    assert_eq!(q.parts[0].kind, ConversationPartKind::Text);

    // 2. Test get_conversation_session with roles = ["command"]
    let cmd_session = service
        .get_conversation_session(ConversationSessionGetParams {
            session_id: full_session_id.clone(),
            roles: Some(vec!["command".to_string()]),
        })
        .await
        .expect("get session with command role filter");

    let q_cmd = &cmd_session.questions[0];
    assert!(q_cmd.turns[0].user_text.is_empty());
    assert_eq!(q_cmd.parts.len(), 1);
    assert_eq!(q_cmd.parts[0].kind, ConversationPartKind::Command);

    // 3. Test short-ID block detail retrieval
    let part_full_id: String = sqlx::query_scalar(
        "SELECT id FROM conversation_parts WHERE tenant_id = ?1 AND command = 'calc 2+2'",
    )
    .bind(tenant_id)
    .fetch_one(service.db.pool())
    .await
    .expect("load part id");

    let short_part_id = crate::backend::domain::conversation_id_fragment(&part_full_id);

    let block_detail = service
        .get_conversation_block(crate::backend::application::ConversationBlockGetParams {
            block_id: short_part_id.clone(),
        })
        .await
        .expect("get block by short ID");

    assert!(block_detail.content.contains("calc 2+2"));

    drop(service);
    std::fs::remove_dir_all(root).ok();
}
