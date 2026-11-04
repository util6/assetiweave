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
async fn test_conversation_outline_and_card_get_full_suite() {
    let (service, root) = setup_fixture_service("outline-suite").await;
    let pool = service.db.pool();
    let tenant_id = "default";

    let adapter_id = "test-adapter-outline";
    let source_id = "test-source-outline";
    let timestamp = "2026-10-21T10:00:00Z";

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
        external_id: "session-ext-outline-001".to_string(),
        title: Some("Session Outline Test Title".to_string()),
        project_path: None,
        started_at: Some(timestamp.to_string()),
        updated_at: Some(timestamp.to_string()),
        source_locator: Some("fixture://session-ext-outline-001".to_string()),
        source_fingerprint: Some("fingerprint-outline-1".to_string()),
        turns: vec![
            NormalizedConversationTurn {
                external_id: "turn-ext-outline-01".to_string(),
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
                ..Default::default()
            },
            NormalizedConversationTurn {
                external_id: "turn-ext-outline-02".to_string(),
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
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    crate::backend::store::upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter)
        .await
        .expect("upsert adapter");
    crate::backend::store::upsert_conversation_source_sqlx(pool, tenant_id, &source)
        .await
        .expect("upsert source");
    crate::backend::store::import_conversation_sessions_sqlx(
        pool,
        tenant_id,
        &source,
        &[session],
        false,
    )
    .await
    .expect("import session");

    let full_session_id: String = sqlx::query_scalar(
        "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND external_id = 'session-ext-outline-001'",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .expect("load full session id");

    let short_session_id = crate::backend::domain::conversation_id_fragment(&full_session_id);

    // 1. Query outline by full session ID
    let outline_session = service
        .get_conversation_outline(ConversationOutlineParams::new(full_session_id.clone()))
        .await
        .expect("get outline with full session ID");

    match &outline_session {
        ConversationOutline::Session(s) => {
            assert_eq!(s.session_id, short_session_id);
            assert_eq!(s.total_turns, 2);
            assert_eq!(
                s.turns.columns,
                vec![
                    "index",
                    "turn_id",
                    "question",
                    "activities",
                    "cards",
                    "turn_kind",
                    "agent_role",
                    "parent_turn_id"
                ]
            );
            assert_eq!(s.turns.rows.len(), 2);
            assert_eq!(s.turns.rows[0].index(), 1);
            assert_eq!(
                s.turns.rows[0].question(),
                "Please run diagnostics and show status"
            );
            assert_eq!(
                s.turns.rows[0].activities().unwrap(),
                &["check", "test", "status"]
            );
            assert_eq!(s.turns.rows[0].cards().unwrap(), 4);
            assert_eq!(s.turns.rows[1].index(), 2);
            assert_eq!(s.turns.rows[1].question(), "Now clean up");
            assert_eq!(s.turns.rows[1].cards().unwrap(), 1);
        }
        _ => panic!("expected ConversationOutline::Session"),
    }

    // 2. Query outline by 8-hex short session ID
    let outline_short = service
        .get_conversation_outline(ConversationOutlineParams::new(short_session_id.clone()))
        .await
        .expect("get outline with short session ID");

    match outline_short {
        ConversationOutline::Session(s) => {
            assert_eq!(s.session_id, short_session_id);
            assert_eq!(s.total_turns, 2);
        }
        _ => panic!("expected ConversationOutline::Session"),
    }

    // Load full turn 1 ID from database
    let full_turn_id: String = sqlx::query_scalar(
        "SELECT id FROM conversation_turns WHERE tenant_id = ?1 AND external_id = 'turn-ext-outline-01'",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .expect("load full turn id");
    let short_turn_id = crate::backend::domain::conversation_id_fragment(&full_turn_id);

    // 3. Query outline by Turn ID
    let outline_turn = service
        .get_conversation_outline(ConversationOutlineParams::new(short_turn_id.clone()))
        .await
        .expect("get outline with turn ID");

    match &outline_turn {
        ConversationOutline::Turn(t) => {
            assert_eq!(t.session_id, short_session_id);
            assert_eq!(t.turn_id, short_turn_id);
            assert_eq!(t.turn_index, 1);
            assert_eq!(t.total_turns, 2);
            assert_eq!(t.user_question, "Please run diagnostics and show status");
            assert_eq!(t.started_at, Some(timestamp.to_string()));
            assert_eq!(
                t.cards.columns,
                vec!["order", "card_id", "kind", "label", "status", "chars"]
            );
            assert_eq!(t.cards.rows.len(), 4);
            assert_eq!(t.cards.rows[0].order, 1);
            assert_eq!(t.cards.rows[0].label.as_deref(), Some("check"));
            assert_eq!(t.cards.rows[1].order, 2);
            assert_eq!(t.cards.rows[1].label.as_deref(), Some("test"));
            assert_eq!(t.cards.rows[2].order, 3);
            assert_eq!(t.cards.rows[2].label.as_deref(), Some("status"));
            assert_eq!(t.cards.rows[3].order, 4);
            assert_eq!(t.cards.rows[3].label, None);
        }
        _ => panic!("expected ConversationOutline::Turn"),
    }

    // Derive full part 2 (cargo test) ID
    let full_part_id = crate::backend::domain::derive_conversation_part_id(&full_turn_id, 1);
    let short_part_id = crate::backend::domain::conversation_id_fragment(&full_part_id);

    // 4. Query outline by Card ID
    let outline_card = service
        .get_conversation_outline(ConversationOutlineParams::new(short_part_id.clone()))
        .await
        .expect("get outline with card ID");

    match &outline_card {
        ConversationOutline::Card(c) => {
            assert_eq!(c.session_id, short_session_id);
            assert_eq!(c.turn_id, short_turn_id);
            assert_eq!(c.turn_index, 1);
            assert_eq!(c.target.card_id, short_part_id);
            assert_eq!(c.target.order, 2);
            assert_eq!(c.target.label.as_deref(), Some("test"));
            assert_eq!(
                c.neighborhood.columns,
                vec!["order", "card_id", "kind", "label", "is_target"]
            );
            assert_eq!(c.neighborhood.rows.len(), 3);
            assert_eq!(c.neighborhood.rows[0].order, 1);
            assert!(!c.neighborhood.rows[0].is_target);
            assert_eq!(c.neighborhood.rows[1].order, 2);
            assert!(c.neighborhood.rows[1].is_target);
            assert_eq!(c.neighborhood.rows[2].order, 3);
            assert!(!c.neighborhood.rows[2].is_target);
        }
        _ => panic!("expected ConversationOutline::Card"),
    }

    // 5. Query card get by card ID
    let card_detail = service
        .get_conversation_card(ConversationCardGetParams {
            card_id: short_part_id.clone(),
        })
        .await
        .expect("get conversation card");

    assert_eq!(card_detail.card_id, short_part_id);
    assert_eq!(card_detail.session_id, short_session_id);
    assert_eq!(card_detail.turn_id, short_turn_id);
    assert_eq!(card_detail.label.as_deref(), Some("test"));
    assert_eq!(card_detail.content, "cargo test");

    // 6. Test JSON serialization and round-trip
    let json_session = serde_json::to_string(&outline_session).expect("serialize session outline");
    assert!(json_session.contains("\"level\":\"session\""));
    assert!(json_session.contains(
        "\"columns\":[\"index\",\"turn_id\",\"question\",\"activities\",\"cards\",\"turn_kind\",\"agent_role\",\"parent_turn_id\"]"
    ));
    assert!(!json_session.contains("\"stats\""));
    assert!(!json_session.contains("\"title\""));
    assert!(!json_session.contains("\"answer_id\""));

    // 6.1 Row 序列化必须能无损回读（子代理语义列随行回读）
    if let ConversationOutline::Session(s) = &outline_session {
        let row = &s.turns.rows[0];
        let round_trip: SessionTurnRow =
            serde_json::from_str(&serde_json::to_string(row).unwrap()).unwrap();
        assert_eq!(&round_trip, row);
    } else {
        panic!("expected session outline");
    }

    // 7. Test Detail Level: Questions
    let outline_questions = service
        .get_conversation_outline(ConversationOutlineParams {
            id: full_session_id.clone(),
            detail_level: ConversationOutlineDetailLevel::Questions,
        })
        .await
        .expect("get outline with questions mode");
    if let ConversationOutline::Session(s) = &outline_questions {
        assert_eq!(
            s.turns.columns,
            vec![
                "index",
                "turn_id",
                "question",
                "turn_kind",
                "agent_role",
                "parent_turn_id"
            ]
        );
        assert_eq!(s.turns.rows.len(), 2);
        assert_eq!(s.turns.rows[0].index(), 1);
        assert_eq!(
            s.turns.rows[0].question(),
            "Please run diagnostics and show status"
        );
        let q_val = serde_json::to_value(&s.turns.rows[0]).unwrap();
        assert_eq!(
            q_val,
            serde_json::json!([
                1,
                s.turns.rows[0].turn_id(),
                "Please run diagnostics and show status",
                null,
                null,
                null
            ])
        );
    } else {
        panic!("expected session outline");
    }

    // 8. Test Detail Level: QA
    let outline_qa = service
        .get_conversation_outline(ConversationOutlineParams {
            id: full_session_id.clone(),
            detail_level: ConversationOutlineDetailLevel::Qa,
        })
        .await
        .expect("get outline with qa mode");
    if let ConversationOutline::Session(s) = &outline_qa {
        assert_eq!(
            s.turns.columns,
            vec![
                "index",
                "turn_id",
                "question",
                "answer",
                "turn_kind",
                "agent_role",
                "parent_turn_id"
            ]
        );
        assert_eq!(s.turns.rows.len(), 2);
        let qa_val = serde_json::to_value(&s.turns.rows[0]).unwrap();
        assert!(qa_val.is_array());
        assert_eq!(qa_val.as_array().unwrap().len(), 7);
    } else {
        panic!("expected session outline");
    }

    // 9. Test Detail Level: Full
    let outline_full = service
        .get_conversation_outline(ConversationOutlineParams {
            id: full_session_id.clone(),
            detail_level: ConversationOutlineDetailLevel::Full,
        })
        .await
        .expect("get outline with full mode");
    if let ConversationOutline::Session(s) = &outline_full {
        assert_eq!(
            s.turns.columns,
            vec![
                "index",
                "turn_id",
                "question",
                "activities",
                "cards",
                "answer",
                "turn_kind",
                "agent_role",
                "parent_turn_id"
            ]
        );
        assert_eq!(s.turns.rows.len(), 2);
        let full_val = serde_json::to_value(&s.turns.rows[0]).unwrap();
        assert!(full_val.is_array());
        assert_eq!(full_val.as_array().unwrap().len(), 9);
    } else {
        panic!("expected session outline");
    }

    let json_turn = serde_json::to_string(&outline_turn).expect("serialize turn outline");
    assert!(json_turn.contains("\"level\":\"turn\""));
    assert!(json_turn
        .contains("\"columns\":[\"order\",\"card_id\",\"kind\",\"label\",\"status\",\"chars\"]"));

    let json_card = serde_json::to_string(&outline_card).expect("serialize card outline");
    assert!(json_card.contains("\"level\":\"card\""));
    assert!(json_card.contains("\"is_target\""));
    assert!(json_card.contains("true"));

    // 10. Non-existent ID returns NotFound
    let not_found = service
        .get_conversation_outline(ConversationOutlineParams {
            id: "non-existent-99999999".to_string(),
            detail_level: ConversationOutlineDetailLevel::Default,
        })
        .await;
    assert!(matches!(not_found, Err(AppError::NotFound(_))));

    drop(service);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn test_truncate_qa_digest() {
    // 1. Text <= 500 characters
    let short_text = "Hello world";
    assert_eq!(truncate_qa_digest(short_text), "Hello world");

    let exact_500: String = (0..500).map(|_| 'a').collect();
    assert_eq!(truncate_qa_digest(&exact_500), exact_500);

    // 2. Text > 500 characters
    let chars_520: String = (0..200)
        .map(|_| 'a')
        .chain((0..20).map(|_| 'b'))
        .chain((0..300).map(|_| 'c'))
        .collect();
    assert_eq!(chars_520.chars().count(), 520);
    let digested = truncate_qa_digest(&chars_520);
    assert_eq!(
        digested,
        format!("{}... [省略 20 字] ...{}", "a".repeat(200), "c".repeat(300))
    );

    // 3. Multi-byte Unicode (Chinese chars)
    let cn_600: String = (0..200)
        .map(|_| '前')
        .chain((0..100).map(|_| '中'))
        .chain((0..300).map(|_| '后'))
        .collect();
    assert_eq!(cn_600.chars().count(), 600);
    let cn_digested = truncate_qa_digest(&cn_600);
    assert_eq!(
        cn_digested,
        format!(
            "{}... [省略 100 字] ...{}",
            "前".repeat(200),
            "后".repeat(300)
        )
    );
}

fn make_text_part(text: &str) -> NormalizedConversationPart {
    NormalizedConversationPart {
        role: ConversationPartRole::Assistant,
        kind: ConversationPartKind::Text,
        text: Some(text.to_string()),
        language: None,
        command: None,
        cwd: None,
        status: None,
        exit_code: None,
        command_label: None,
        source_execution_id: None,
        content_card: None,
        metadata_json: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_qa_digest_and_batch_outline_suite() {
    let (service, root) = setup_fixture_service("batch-suite").await;
    let pool = service.db.pool();
    let tenant_id = "default";
    let timestamp = "2026-11-04T12:00:00Z";

    let adapter = ConversationAdapter {
        id: "adapter-batch".to_string(),
        name: "Batch Adapter".to_string(),
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
        id: "source-batch".to_string(),
        adapter_id: "adapter-batch".to_string(),
        name: "Batch Source".to_string(),
        kind: ConversationSourceKind::Directory,
        location: "/fixture/batch".to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: timestamp.to_string(),
        updated_at: timestamp.to_string(),
    };

    crate::backend::store::upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter)
        .await
        .expect("upsert adapter");
    crate::backend::store::upsert_conversation_source_sqlx(pool, tenant_id, &source)
        .await
        .expect("upsert source");

    // Session 1 has a long answer > 500 chars
    let long_answer: String = (0..200)
        .map(|_| 'A')
        .chain((0..100).map(|_| 'X'))
        .chain((0..300).map(|_| 'Z'))
        .collect();

    let session1 = NormalizedConversationSession {
        external_id: "sess-batch-001".to_string(),
        title: Some("Session Batch 1".to_string()),
        project_path: None,
        started_at: Some(timestamp.to_string()),
        updated_at: Some(timestamp.to_string()),
        source_locator: Some("fixture://sess-batch-001".to_string()),
        source_fingerprint: Some("fp-1".to_string()),
        turns: vec![NormalizedConversationTurn {
            external_id: "turn-batch-1".to_string(),
            turn_index: 0,
            user_text: "Explain quantum computing".to_string(),
            title: None,
            started_at: Some(timestamp.to_string()),
            ended_at: Some(timestamp.to_string()),
            model: None,
            parts: vec![make_text_part(&long_answer)],
            ..Default::default()
        }],
        ..Default::default()
    };

    let session2 = NormalizedConversationSession {
        external_id: "sess-batch-002".to_string(),
        title: Some("Session Batch 2".to_string()),
        project_path: None,
        started_at: Some(timestamp.to_string()),
        updated_at: Some(timestamp.to_string()),
        source_locator: Some("fixture://sess-batch-002".to_string()),
        source_fingerprint: Some("fp-2".to_string()),
        turns: vec![NormalizedConversationTurn {
            external_id: "turn-batch-2".to_string(),
            turn_index: 0,
            user_text: "What is 2+2?".to_string(),
            title: None,
            started_at: Some(timestamp.to_string()),
            ended_at: Some(timestamp.to_string()),
            model: None,
            parts: vec![make_text_part("4")],
            ..Default::default()
        }],
        ..Default::default()
    };

    crate::backend::store::import_conversation_sessions_sqlx(
        pool,
        tenant_id,
        &source,
        &[session1, session2],
        false,
    )
    .await
    .expect("import sessions");

    let s1_id: String = sqlx::query_scalar(
        "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND external_id = 'sess-batch-001'",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .expect("load session 1 id");

    let s2_id: String = sqlx::query_scalar(
        "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND external_id = 'sess-batch-002'",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .expect("load session 2 id");

    // 1. Test QaDigest on single session: answer should be truncated
    let digest_outline = service
        .get_conversation_outline(ConversationOutlineParams {
            id: s1_id.clone(),
            detail_level: ConversationOutlineDetailLevel::QaDigest,
        })
        .await
        .expect("get qa digest outline");

    match digest_outline {
        ConversationOutline::Session(s) => {
            assert_eq!(s.turns.rows.len(), 1);
            let ans = s.turns.rows[0].answer().expect("has answer");
            assert!(ans.contains("... [省略 100 字] ..."));
            assert!(ans.starts_with(&"A".repeat(200)));
            assert!(ans.ends_with(&"Z".repeat(300)));
            assert_eq!(s.turns.rows[0].question(), "Explain quantum computing");
        }
        _ => panic!("expected session outline"),
    }

    // 2. Test get_conversation_outlines with single ID: returns Session outline directly
    let single_batch = service
        .get_conversation_outlines(&[s1_id.clone()], ConversationOutlineDetailLevel::Questions)
        .await
        .expect("single id outline");
    assert!(matches!(single_batch, ConversationOutline::Session(_)));

    // 3. Test get_conversation_outlines with multiple IDs: returns Batch outline
    let multi_batch = service
        .get_conversation_outlines(
            &[s1_id.clone(), s2_id.clone()],
            ConversationOutlineDetailLevel::Questions,
        )
        .await
        .expect("multi id outline");
    match multi_batch {
        ConversationOutline::Batch { items, truncated } => {
            assert_eq!(items.len(), 2);
            assert!(!truncated);
        }
        _ => panic!("expected batch outline"),
    }

    // 4. Test budget guardrail degradation:
    // Create sessions with very large answers totaling > 64KB
    let huge_answer = "H".repeat(40 * 1024); // 40KB
    let session_huge_1 = NormalizedConversationSession {
        external_id: "sess-huge-001".to_string(),
        title: Some("Session Huge 1".to_string()),
        source_locator: Some("fixture://sess-huge-001".to_string()),
        turns: vec![NormalizedConversationTurn {
            external_id: "turn-huge-1".to_string(),
            turn_index: 0,
            user_text: "Q1".to_string(),
            parts: vec![make_text_part(&huge_answer)],
            ..Default::default()
        }],
        ..Default::default()
    };
    let session_huge_2 = NormalizedConversationSession {
        external_id: "sess-huge-002".to_string(),
        title: Some("Session Huge 2".to_string()),
        source_locator: Some("fixture://sess-huge-002".to_string()),
        turns: vec![NormalizedConversationTurn {
            external_id: "turn-huge-2".to_string(),
            turn_index: 0,
            user_text: "Q2".to_string(),
            parts: vec![make_text_part(&huge_answer)],
            ..Default::default()
        }],
        ..Default::default()
    };
    crate::backend::store::import_conversation_sessions_sqlx(
        pool,
        tenant_id,
        &source,
        &[session_huge_1, session_huge_2],
        false,
    )
    .await
    .expect("import huge sessions");

    let h1_id: String = sqlx::query_scalar(
        "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND external_id = 'sess-huge-001'",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .expect("load huge session 1 id");

    let h2_id: String = sqlx::query_scalar(
        "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND external_id = 'sess-huge-002'",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .expect("load huge session 2 id");

    // With --qa, total JSON size of two 40KB sessions will exceed 64KB
    let huge_batch = service
        .get_conversation_outlines(&[h1_id, h2_id], ConversationOutlineDetailLevel::Qa)
        .await
        .expect("huge qa batch outline");

    match huge_batch {
        ConversationOutline::Batch { items, truncated } => {
            assert_eq!(items.len(), 2);
            assert!(
                truncated,
                "expected output budget degradation to set truncated: true"
            );
            if let ConversationOutline::Session(s) = &items[0] {
                let ans = s.turns.rows[0].answer().unwrap();
                assert!(
                    ans.contains("... [省略 "),
                    "answer should be degraded to qa_digest"
                );
            } else {
                panic!("expected session outline");
            }
        }
        _ => panic!("expected batch outline"),
    }

    drop(service);
    std::fs::remove_dir_all(root).ok();
}
