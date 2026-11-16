use super::*;
use crate::backend::domain::{
    ConversationContentCardDescriptor, ConversationPartKind, ConversationPartRole,
    ConversationSourceKind, NormalizedConversationPart, NormalizedConversationTurn,
};

#[tokio::test]
async fn test_reproject_conversation_session_materializes_parts_and_derives_execution_pairs() {
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
            external_id: Some("turn-1".to_string()),
            turn_index: 0,
            user_text: "check git status and review".to_string(),
            title: None,
            started_at: Some(now.clone()),
            ended_at: Some(now.clone()),
            model: None,
            parts: vec![cmd_part, res_part, subagent_part],
            ..Default::default()
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

    // 执行分组由零件表在读取时派生：同一 source_execution_id 的命令与结果按
    // part_index 排序、按卡片语义角色区分，不依赖任何关系边表。
    let question = &detail.questions[0];
    assert_eq!(
        question
            .parts
            .iter()
            .map(|part| part.part_index)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    let expected_exec_id = format!("exec-{}-0", question.turns[0].id);
    for part in &question.parts[..2] {
        assert_eq!(
            part.source_execution_id.as_deref(),
            Some(expected_exec_id.as_str())
        );
    }
    let execution_nodes = question
        .projected_content_nodes
        .iter()
        .filter(|node| node.source_execution_id.as_deref() == Some(expected_exec_id.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(execution_nodes.len(), 2);
    assert_eq!(execution_nodes[0].semantic_role.as_deref(), Some("command"));
    assert_eq!(execution_nodes[1].semantic_role.as_deref(), Some("result"));
    assert!(execution_nodes[0].part_order < execution_nodes[1].part_order);
    assert_eq!(execution_nodes[1].status.as_deref(), None);
    assert_eq!(execution_nodes[1].exit_code, None);

    // 子代理卡片显示的子会话标识来自卡片正文，不来自关系边表。
    let subagent_node = question
        .projected_content_nodes
        .iter()
        .find(|node| node.semantic_role.as_deref() == Some("subagent"))
        .expect("subagent card must project a content node");
    assert!(subagent_node.content.contains("child-session-99"));

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

    // 重复同步不再放大写入：在瘦身后命令收纳进 process.items，零件表仅留存 2 个中间零件
    let part_row_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversation_parts WHERE tenant_id = ?1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(part_row_count, 2);

    drop(database);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
}

#[tokio::test]
async fn test_apply_reprojected_sessions_preserves_qa_missing_flag_and_translations() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-reproject-legacy-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .unwrap();
    let pool = database.pool();
    let tenant_id = "test-tenant-legacy-reproject";
    let now = Utc::now().to_rfc3339();

    sqlx::query("INSERT OR IGNORE INTO tenants (id, created_at, updated_at) VALUES (?1, ?2, ?2)")
        .bind(tenant_id)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();

    let source = ConversationSource {
        id: "source-legacy-reproject".to_string(),
        adapter_id: "antigravity".to_string(),
        name: "Antigravity".to_string(),
        kind: ConversationSourceKind::Directory,
        location: "/tmp/nonexistent".to_string(),
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

    let legacy_session = NormalizedConversationSession {
        external_id: "missing-session-1".to_string(),
        title: Some("Legacy Missing Session".to_string()),
        project_path: Some("/tmp/project".to_string()),
        user_visible: Some(true),
        execution_origin: None,
        execution_purpose: None,
        started_at: Some("2026-01-01T00:00:00Z".to_string()),
        updated_at: Some("2026-01-01T00:01:00Z".to_string()),
        source_locator: None,
        source_fingerprint: Some("fp-legacy-1".to_string()),
        turns: vec![NormalizedConversationTurn {
            external_id: Some("t-1".to_string()),
            turn_index: 0,
            user_text: "Original user prompt line".to_string(),
            started_at: Some("2026-01-01T00:00:00Z".to_string()),
            ended_at: Some("2026-01-01T00:00:30Z".to_string()),
            model: Some("gemini-test".to_string()),
            parts: vec![
                NormalizedConversationPart {
                    role: ConversationPartRole::Tool,
                    kind: ConversationPartKind::Tool,
                    text: Some("Tool: read_file\nPath: /tmp/project/src/a.ts".to_string()),
                    language: None,
                    command: None,
                    cwd: None,
                    status: None,
                    exit_code: None,
                    command_label: None,
                    source_execution_id: None,
                    content_card: Some(ConversationContentCardDescriptor {
                        schema_version: 1,
                        kind: "antigravity.tool".to_string(),
                        semantic_role: None,
                        renderer: Some("card".to_string()),
                    }),
                    metadata_json: None,
                },
                NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Text,
                    text: Some("Original final assistant response".to_string()),
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
        }],
    };

    import_conversation_sessions_sqlx(pool, tenant_id, &source, &[legacy_session], false)
        .await
        .unwrap();

    let session_id = stable_id("conversation-session", &[&source.id, "missing-session-1"]);
    // Mark session as missing = 1 AND attach response_translated_text in TurnProcess
    sqlx::query("UPDATE conversation_sessions SET missing = 1 WHERE tenant_id = ?1 AND id = ?2")
        .bind(tenant_id)
        .bind(&session_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"UPDATE conversation_turns
           SET process = '{"response_translated_text":"原始回答翻译"}'
           WHERE tenant_id = ?1 AND session_id = ?2"#,
    )
    .bind(tenant_id)
    .bind(&session_id)
    .execute(pool)
    .await
    .unwrap();

    let origins = load_sessions_for_reprojection_sqlx(
        pool,
        tenant_id,
        None,
        None,
        &std::collections::BTreeSet::new(),
    )
    .await
    .unwrap();
    assert_eq!(origins.len(), 1);
    assert_eq!(origins[0].session.id, session_id);

    // 1. Invariant guard rejects tampering with prompt/response or turn count.
    let mut bad_payload = origins[0].payload.clone();
    bad_payload.turns[0].prompt = "Tampered prompt".to_string();
    let bad_report = apply_reprojected_sessions_sqlx(pool, tenant_id, &origins, &[bad_payload])
        .await
        .unwrap();
    assert_eq!(bad_report.failed_sessions, 1);
    assert_eq!(bad_report.updated_sessions, 0);

    // 2. Valid transformation updates `antigravity.tool` -> `antigravity.ambient` while keeping QA + missing + translation.
    let mut valid_payload = origins[0].payload.clone();
    let tool_part = valid_payload.turns[0]
        .parts
        .iter_mut()
        .find(|p| p.kind == ConversationPartKind::Tool)
        .unwrap();
    tool_part.text = Some("Read /tmp/project/src/a.ts".to_string());
    tool_part.content_card = Some(ConversationContentCardDescriptor {
        schema_version: 1,
        kind: "antigravity.ambient".to_string(),
        semantic_role: Some("ambient".to_string()),
        renderer: Some("compact_action".to_string()),
    });

    let ok_report = apply_reprojected_sessions_sqlx(
        pool,
        tenant_id,
        &origins,
        std::slice::from_ref(&valid_payload),
    )
    .await
    .unwrap();
    assert_eq!(ok_report.updated_sessions, 1);
    assert_eq!(ok_report.failed_sessions, 0);

    // Verify missing = 1 intact & prompt/response intact & translation intact & ambient card persisted
    let missing_after: i64 = sqlx::query_scalar(
        "SELECT missing FROM conversation_sessions WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(&session_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(missing_after, 1);

    let detail = load_conversation_session_detail_sqlx(pool, tenant_id, &session_id)
        .await
        .unwrap();
    assert_eq!(
        detail.questions[0].turns[0].prompt,
        "Original user prompt line"
    );
    assert_eq!(
        detail.questions[0].turns[0].response.as_deref(),
        Some("Original final assistant response")
    );
    assert!(detail.questions[0].turns[0]
        .process
        .as_deref()
        .unwrap_or("")
        .contains("原始回答翻译"));
    assert_eq!(
        detail.questions[0].parts[0]
            .content_card
            .as_ref()
            .map(|c| c.kind.as_str()),
        Some("antigravity.ambient")
    );

    // 3. Re-running with identical transformed state skips SQLite writes cleanly.
    let origins_second = load_sessions_for_reprojection_sqlx(
        pool,
        tenant_id,
        None,
        None,
        &std::collections::BTreeSet::new(),
    )
    .await
    .unwrap();
    let second_report = apply_reprojected_sessions_sqlx(
        pool,
        tenant_id,
        &origins_second,
        &[origins_second[0].payload.clone()],
    )
    .await
    .unwrap();
    assert_eq!(second_report.updated_sessions, 0);
    assert_eq!(second_report.skipped_unchanged_sessions, 1);

    drop(database);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
}
