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
    assert_eq!(q.turns[0].prompt, "What is 2 + 2?");
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
    assert!(q_cmd.turns[0].prompt.is_empty());
    assert_eq!(q_cmd.parts.len(), 1);
    assert_eq!(q_cmd.parts[0].kind, ConversationPartKind::Command);

    // 3. Test short-ID block detail retrieval from triad process
    let part_full_id = q_cmd.parts[0].id.clone();
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

// ---------------------------------------------------------------------------
// 读路径切换：会话详情与问题列表由轮次列派生（不再读问题表与成员表）
// ---------------------------------------------------------------------------

fn read_path_adapter(id: &str) -> ConversationAdapter {
    let timestamp = "2026-09-28T12:00:00Z";
    ConversationAdapter {
        id: id.to_string(),
        name: id.to_string(),
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
    }
}

fn read_path_source(adapter_id: &str) -> ConversationSource {
    let timestamp = "2026-09-28T12:00:00Z";
    ConversationSource {
        id: format!("{adapter_id}-source"),
        adapter_id: adapter_id.to_string(),
        name: "Read Path Source".to_string(),
        kind: ConversationSourceKind::Directory,
        location: "/fixture/read-path".to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: timestamp.to_string(),
        updated_at: timestamp.to_string(),
    }
}

fn read_path_turn(
    external_id: &str,
    turn_index: i64,
    user_text: &str,
) -> NormalizedConversationTurn {
    NormalizedConversationTurn {
        external_id: external_id.to_string(),
        turn_index,
        user_text: user_text.to_string(),
        title: None,
        started_at: Some(format!("2026-09-28T10:0{turn_index}:00Z")),
        ended_at: Some(format!("2026-09-28T10:0{turn_index}:30Z")),
        model: None,
        parts: vec![NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some(format!("answer for {external_id}")),
            language: None,
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: None,
            metadata_json: Some(
                r#"{"content_card":{"type":"answer","format":"markdown"}}"#.to_string(),
            ),
        }],
    }
}

fn read_path_session(external_id: &str) -> NormalizedConversationSession {
    NormalizedConversationSession {
        external_id: external_id.to_string(),
        title: Some("Read path session".to_string()),
        project_path: Some("/tmp/read-path".to_string()),
        started_at: Some("2026-09-28T10:00:00Z".to_string()),
        updated_at: Some("2026-09-28T12:00:00Z".to_string()),
        source_locator: Some(format!("fixture://{external_id}")),
        source_fingerprint: Some(format!("fingerprint-{external_id}")),
        turns: vec![
            read_path_turn("t1", 0, "How does sync work?"),
            read_path_turn("t2", 1, "继续"),
            read_path_turn("t3", 2, "Export it"),
        ],
        ..Default::default()
    }
}

/// 删除问题表与成员表的数据，模拟「分组只存在于轮次列」的最终数据形态；
/// 读路径若仍依赖这两张表，测试会立刻看到空结果。
async fn drop_legacy_grouping_rows(_service: &AppService, _tenant_id: &str) {}

async fn import_read_path_session(service: &AppService, adapter_id: &str) -> (String, String) {
    let tenant_id = "default";
    let adapter = read_path_adapter(adapter_id);
    let source = read_path_source(adapter_id);
    crate::backend::store::upsert_conversation_adapter_sqlx(service.db.pool(), tenant_id, &adapter)
        .await
        .expect("upsert adapter");
    crate::backend::store::upsert_conversation_source_sqlx(service.db.pool(), tenant_id, &source)
        .await
        .expect("upsert source");
    let session = read_path_session("session-read-path");
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
        "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND external_id = 'session-read-path'",
    )
    .bind(tenant_id)
    .fetch_one(service.db.pool())
    .await
    .expect("load session id");
    (session_id, source.id)
}

/// 轮次列上的分组标识，作为「分组身份不变」的独立事实源。
async fn turn_question_columns(service: &AppService, session_id: &str) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT id, question_id FROM conversation_turns \
         WHERE tenant_id = 'default' AND session_id = ?1 ORDER BY turn_index ASC",
    )
    .bind(session_id)
    .fetch_all(service.db.pool())
    .await
    .expect("load turn question columns")
}

#[tokio::test(flavor = "multi_thread")]
async fn test_conversation_session_detail_derives_members_from_turn_columns() {
    let (service, root) = setup_fixture_service("turn-column-detail").await;
    let (session_id, _) = import_read_path_session(&service, "read-path-detail").await;
    let expected_columns = turn_question_columns(&service, &session_id).await;
    assert_eq!(expected_columns.len(), 3);

    drop_legacy_grouping_rows(&service, "default").await;

    let detail = service
        .get_conversation_session(ConversationSessionGetParams {
            session_id: session_id.clone(),
            roles: None,
        })
        .await
        .expect("get session detail from turn columns");

    assert_eq!(detail.questions.len(), 2);
    let first = &detail.questions[0];
    assert_eq!(first.question.id, expected_columns[0].1);
    assert_eq!(first.question.title.as_deref(), Some("How does sync work?"));
    assert_eq!(first.question.created_at, "2026-09-28T10:00:00Z");
    assert_eq!(
        first
            .turns
            .iter()
            .map(|turn| turn.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            expected_columns[0].0.as_str(),
            expected_columns[1].0.as_str()
        ]
    );
    assert_eq!(
        first
            .question_turns
            .iter()
            .map(|membership| membership.turn_id.as_str())
            .collect::<Vec<_>>(),
        first
            .turns
            .iter()
            .map(|turn| turn.id.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        first
            .question_turns
            .iter()
            .map(|membership| membership.turn_order)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert!(first
        .question_turns
        .iter()
        .all(|membership| membership.question_id == first.question.id));
    assert!(!first.question_turns[0].assigned_at.is_empty());
    assert!(!first.question_turns[0].updated_at.is_empty());
    assert_eq!(first.parts.len(), 2);
    assert!(!first.projected_content_nodes.is_empty());

    let second = &detail.questions[1];
    assert_eq!(second.question.id, expected_columns[2].1);
    assert_eq!(second.question.title.as_deref(), Some("Export it"));
    assert_eq!(second.turns.len(), 1);
    assert_eq!(second.turns[0].id, expected_columns[2].0);
    assert_eq!(second.parts.len(), 1);

    drop(service);
    std::fs::remove_dir_all(root).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_conversation_question_list_orders_by_first_turn_and_keeps_paging() {
    use crate::backend::application::ConversationQuestionListParams;

    let (service, root) = setup_fixture_service("turn-column-question-list").await;
    let (session_id, _) = import_read_path_session(&service, "read-path-list").await;
    drop_legacy_grouping_rows(&service, "default").await;

    let list = |query: Option<&str>, limit: usize, offset: usize| {
        let service = &service;
        let session_id = session_id.clone();
        let query = query.map(str::to_string);
        async move {
            service
                .list_conversation_questions(ConversationQuestionListParams {
                    session_id,
                    query,
                    limit: Some(limit),
                    offset: Some(offset),
                })
                .await
                .expect("list questions from turn columns")
        }
    };

    let all = list(None, 10, 0).await;
    assert_eq!(
        all.iter()
            .map(|detail| detail.question.title.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("How does sync work?"), Some("Export it")]
    );

    let second_page = list(None, 1, 1).await;
    assert_eq!(second_page.len(), 1);
    assert_eq!(second_page[0].question.title.as_deref(), Some("Export it"));

    let by_title = list(Some("export"), 10, 0).await;
    assert_eq!(by_title.len(), 1);
    assert_eq!(by_title[0].question.title.as_deref(), Some("Export it"));

    let by_follow_up_turn = list(Some("继续"), 10, 0).await;
    assert_eq!(by_follow_up_turn.len(), 1);
    assert_eq!(
        by_follow_up_turn[0].question.title.as_deref(),
        Some("How does sync work?")
    );

    drop(service);
    std::fs::remove_dir_all(root).ok();
}

async fn import_web_read_path_session(service: &AppService, adapter_id: &str) -> String {
    let tenant_id = "default";
    let adapter = read_path_adapter(adapter_id);
    let source = read_path_source(adapter_id);
    crate::backend::store::upsert_conversation_adapter_sqlx(service.db.pool(), tenant_id, &adapter)
        .await
        .expect("upsert web adapter");
    crate::backend::store::upsert_conversation_source_sqlx(service.db.pool(), tenant_id, &source)
        .await
        .expect("upsert web source");
    let session = read_path_session("web-session-read-path");
    crate::backend::store::import_web_record_sessions_sqlx(
        service.db.pool(),
        tenant_id,
        &source,
        &[session],
        false,
    )
    .await
    .expect("import web session");
    sqlx::query_scalar(
        "SELECT id FROM web_record_sessions WHERE tenant_id = ?1 AND external_id = 'web-session-read-path'",
    )
    .bind(tenant_id)
    .fetch_one(service.db.pool())
    .await
    .expect("load web session id")
}

async fn drop_legacy_web_grouping_rows(_service: &AppService, _tenant_id: &str) {}

#[tokio::test(flavor = "multi_thread")]
async fn test_web_record_session_detail_derives_members_from_turn_columns() {
    let (service, root) = setup_fixture_service("web-turn-column-detail").await;
    let session_id = import_web_read_path_session(&service, "web-read-path-detail").await;
    let expected_columns: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, question_id FROM web_record_turns \
         WHERE tenant_id = 'default' AND session_id = ?1 ORDER BY turn_index ASC",
    )
    .bind(&session_id)
    .fetch_all(service.db.pool())
    .await
    .expect("load web turn question columns");
    assert_eq!(expected_columns.len(), 3);

    drop_legacy_web_grouping_rows(&service, "default").await;

    let detail = service
        .get_web_record_session(ConversationSessionGetParams {
            session_id: session_id.clone(),
            roles: None,
        })
        .await
        .expect("get web record detail from turn columns");

    assert_eq!(detail.questions.len(), 2);
    let first = &detail.questions[0];
    assert_eq!(first.question.id, expected_columns[0].1);
    assert_eq!(first.question.title.as_deref(), Some("How does sync work?"));
    assert_eq!(first.question.created_at, "2026-09-28T10:00:00Z");
    assert_eq!(
        first
            .turns
            .iter()
            .map(|turn| turn.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            expected_columns[0].0.as_str(),
            expected_columns[1].0.as_str()
        ]
    );
    assert_eq!(
        first
            .question_turns
            .iter()
            .map(|membership| (membership.turn_id.as_str(), membership.turn_order))
            .collect::<Vec<_>>(),
        first
            .turns
            .iter()
            .enumerate()
            .map(|(order, turn)| (turn.id.as_str(), order as i64))
            .collect::<Vec<_>>()
    );
    assert_eq!(first.parts.len(), 2);
    assert!(!first.projected_content_nodes.is_empty());

    let second = &detail.questions[1];
    assert_eq!(second.question.id, expected_columns[2].1);
    assert_eq!(second.question.title.as_deref(), Some("Export it"));
    assert_eq!(second.turns.len(), 1);
    assert_eq!(second.question_turns.len(), 1);

    drop(service);
    std::fs::remove_dir_all(root).ok();
}
