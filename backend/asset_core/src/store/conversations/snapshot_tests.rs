//! ADR-0021 会话快照整包替换与轮次位置身份的接缝测试。
//!
//! 只验证公共可观测行为：会话详情、全文检索、用量归因与数据库终态，
//! 不绑定内部私有辅助函数的签名。

use sqlx::SqlitePool;
use uuid::Uuid;

use crate::backend::domain::{
    derive_conversation_turn_id, ConversationAdapter, ConversationAdapterKind,
    ConversationAdapterTrustState, ConversationContentCardDescriptor, ConversationPartKind,
    ConversationPartRole, ConversationSource, ConversationSourceKind, NormalizedConversationPart,
    NormalizedConversationSession, NormalizedConversationTurn,
};
use crate::backend::store::conversations::{
    import_conversation_sessions_sqlx, import_web_record_sessions_sqlx,
    load_conversation_session_detail_sqlx, update_conversation_part_translation_sqlx,
    upsert_conversation_adapter_sqlx, upsert_conversation_source_sqlx, upsert_usage_events_sqlx,
    RawUsageEventInput,
};
use crate::backend::store::Database;

const TENANT: &str = "default";

fn adapter(id: &str) -> ConversationAdapter {
    ConversationAdapter {
        id: id.to_string(),
        name: id.to_string(),
        kind: ConversationAdapterKind::External,
        version: "1.0.0".to_string(),
        enabled: true,
        manifest_path: Some(format!("/tmp/{id}/manifest.json")),
        executable_path: Some(format!("/tmp/{id}/adapter")),
        content_hash: Some(format!("{id}-hash")),
        trusted_hash: Some(format!("{id}-hash")),
        trust_state: ConversationAdapterTrustState::Trusted,
        protocol_version: Some(1),
        capabilities: vec!["read".to_string()],
        input_kinds: vec![ConversationSourceKind::Directory],
        card_contract_version: None,
        card_kinds: Vec::new(),
        projection_version: Some(1),
        created_at: "2026-11-06T00:00:00Z".to_string(),
        updated_at: "2026-11-06T00:00:00Z".to_string(),
    }
}

fn source(adapter_id: &str) -> ConversationSource {
    ConversationSource {
        id: format!("{adapter_id}-source"),
        adapter_id: adapter_id.to_string(),
        name: "Fixture source".to_string(),
        kind: ConversationSourceKind::Directory,
        location: "/tmp/fixture".to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: "2026-11-06T00:00:00Z".to_string(),
        updated_at: "2026-11-06T00:00:00Z".to_string(),
    }
}

fn answer_part(text: &str) -> NormalizedConversationPart {
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
        content_card: Some(ConversationContentCardDescriptor {
            schema_version: 1,
            kind: "answer".to_string(),
            semantic_role: Some("answer".to_string()),
            renderer: Some("markdown".to_string()),
        }),
        metadata_json: None,
    }
}

fn turn(index: i64, user_text: &str, answer: &str) -> NormalizedConversationTurn {
    NormalizedConversationTurn {
        turn_index: index,
        user_text: user_text.to_string(),
        parts: vec![answer_part(answer)],
        ..Default::default()
    }
}

async fn open_pool(label: &str) -> (Database, std::path::PathBuf) {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-snapshot-{label}-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    (database, db_path)
}

fn cleanup(db_path: &std::path::Path) {
    let _ = std::fs::remove_file(db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}

#[tokio::test]
async fn snapshot_assigns_positional_identity_and_fills_started_at() {
    let (database, db_path) = open_pool("identity").await;
    let pool = database.pool();
    let adapter = adapter("snapshot-identity");
    let source = source(&adapter.id);
    upsert_conversation_adapter_sqlx(pool, TENANT, &adapter)
        .await
        .unwrap();
    upsert_conversation_source_sqlx(pool, TENANT, &source)
        .await
        .unwrap();

    let mut blank = turn(9, "   ", "should be dropped");
    blank.external_id = Some("blank-turn".to_string());
    let mut second = turn(7, "Second question", "second answer");
    second.external_id = Some("external-second".to_string());
    second.started_at = None;
    let mut subagent = turn(11, "Subagent question", "subagent answer");
    subagent.external_id = Some("subagent-source-key".to_string());
    subagent.turn_kind = Some("subagent".to_string());
    subagent.agent_role = Some("auditor".to_string());
    subagent.parent_external_id = Some("external-second".to_string());
    let mut first = turn(3, "First question", "first answer");
    first.external_id = None;
    first.started_at = Some("2026-11-06T01:00:00Z".to_string());

    let session = NormalizedConversationSession {
        external_id: "identity-session".to_string(),
        title: Some("Identity".to_string()),
        project_path: Some("/tmp/identity".to_string()),
        started_at: Some("2026-11-06T00:00:00Z".to_string()),
        updated_at: Some("2026-11-06T02:00:00Z".to_string()),
        source_locator: Some("fixture://identity".to_string()),
        source_fingerprint: Some("identity-v1".to_string()),
        turns: vec![first, blank, second, subagent],
        ..Default::default()
    };

    let result = import_conversation_sessions_sqlx(pool, TENANT, &source, &[session], false)
        .await
        .expect("import snapshot");
    assert_eq!(result.changed_session_count, 1);

    let session_id = crate::backend::domain::stable_id(
        "conversation-session",
        &[&source.id, "identity-session"],
    );
    let detail = load_conversation_session_detail_sqlx(pool, TENANT, &session_id)
        .await
        .expect("load detail");
    let turns = detail
        .questions
        .iter()
        .flat_map(|question| question.turns.iter())
        .collect::<Vec<_>>();

    assert_eq!(turns.len(), 3, "空白 prompt 的轮次必须在导入时被过滤");

    assert_eq!(
        turns.iter().map(|turn| turn.turn_index).collect::<Vec<_>>(),
        vec![0, 1, 2],
        "turn_index 必须由核心按快照顺序重排为 0..N"
    );
    for turn in &turns {
        assert_eq!(
            turn.id,
            derive_conversation_turn_id(&session_id, turn.turn_index),
            "轮次主键必须是 sha256(session_id, turn_index)"
        );
    }
    assert_eq!(
        turns[0].started_at, "2026-11-06T01:00:00Z",
        "轮次原生时间优先"
    );
    assert_eq!(
        turns[1].started_at, "2026-11-06T02:00:00Z",
        "缺失时间回退到会话更新时间"
    );
    assert_eq!(
        turns[2].parent_turn_id.as_deref(),
        Some(turns[1].id.as_str()),
        "子代理父引用必须解析成父轮次内部主键"
    );

    drop(database);
    cleanup(&db_path);
}

#[tokio::test]
async fn snapshot_rewind_replaces_turns_parts_and_index_wholesale() {
    let (database, db_path) = open_pool("rewind").await;
    let pool = database.pool();
    let adapter = adapter("snapshot-rewind");
    let source = source(&adapter.id);
    upsert_conversation_adapter_sqlx(pool, TENANT, &adapter)
        .await
        .unwrap();
    upsert_conversation_source_sqlx(pool, TENANT, &source)
        .await
        .unwrap();

    let full = NormalizedConversationSession {
        external_id: "rewind-session".to_string(),
        title: Some("Rewind".to_string()),
        project_path: Some("/tmp/rewind".to_string()),
        started_at: Some("2026-11-06T00:00:00Z".to_string()),
        updated_at: Some("2026-11-06T01:00:00Z".to_string()),
        source_locator: Some("fixture://rewind".to_string()),
        source_fingerprint: Some("rewind-v1".to_string()),
        turns: vec![
            turn(0, "q1", "ghost answer alpha"),
            turn(1, "q2", "ghost answer beta"),
            turn(2, "q3", "ghost answer gamma"),
            turn(3, "q4", "ghost answer delta"),
            turn(4, "q5", "ghost answer epsilon"),
        ],
        ..Default::default()
    };
    import_conversation_sessions_sqlx(pool, TENANT, &source, &[full], false)
        .await
        .expect("import full snapshot");

    let session_id =
        crate::backend::domain::stable_id("conversation-session", &[&source.id, "rewind-session"]);
    let before = load_conversation_session_detail_sqlx(pool, TENANT, &session_id)
        .await
        .expect("load detail");
    let before_turns = before
        .questions
        .iter()
        .flat_map(|question| question.turns.iter())
        .collect::<Vec<_>>();
    assert_eq!(before_turns.len(), 5);
    let first_part_id = before.questions[0].parts[0].id.clone();
    let first_turn_id = before_turns[0].id.clone();

    // 给第一轮打上局部翻译，验证整包替换会把它物理清空。
    update_conversation_part_translation_sqlx(pool, TENANT, &first_part_id, "译文")
        .await
        .expect("stamp translation");

    // 模拟宿主 /rewind：总轮数从 5 缩到 2，第二轮内容与开始时间都变了。
    let mut rewound = NormalizedConversationSession {
        external_id: "rewind-session".to_string(),
        title: Some("Rewind".to_string()),
        project_path: Some("/tmp/rewind".to_string()),
        started_at: Some("2026-11-06T00:00:00Z".to_string()),
        updated_at: Some("2026-11-06T03:00:00Z".to_string()),
        source_locator: Some("fixture://rewind".to_string()),
        source_fingerprint: Some("rewind-v2".to_string()),
        turns: vec![
            turn(0, "q1", "ghost answer alpha"),
            turn(1, "q2 rewound", "fresh answer"),
        ],
        ..Default::default()
    };
    rewound.turns[1].started_at = Some("2026-11-06T03:00:00Z".to_string());
    import_conversation_sessions_sqlx(pool, TENANT, &source, &[rewound], false)
        .await
        .expect("import rewound snapshot");

    let after = load_conversation_session_detail_sqlx(pool, TENANT, &session_id)
        .await
        .expect("load rewound detail");
    let after_turns = after
        .questions
        .iter()
        .flat_map(|question| question.turns.iter())
        .collect::<Vec<_>>();
    let after_parts = after
        .questions
        .iter()
        .flat_map(|question| question.parts.iter())
        .collect::<Vec<_>>();
    assert_eq!(after_turns.len(), 2, "回滚后的轮次必须整包替换");
    assert_eq!(after_parts.len(), 2, "旧轮次内容事实必须一并清除");
    assert_eq!(
        after_turns[1].started_at, "2026-11-06T03:00:00Z",
        "回滚后的新轮次必须带上新的开始时间"
    );
    assert_eq!(
        after_turns[0].id, first_turn_id,
        "位置身份在回滚前后保持不变"
    );
    assert_eq!(
        after_parts[0].translated_text, None,
        "整包替换必须物理清空局部翻译"
    );
    // 回滚后的内容事实必须只剩当前快照：被抹掉的轮次正文不得残留。
    let rolled_back_content: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_turns \
         WHERE tenant_id = ?1 AND session_id = ?2 \
           AND (COALESCE(response, '') LIKE '%ghost answer epsilon%' \
                OR COALESCE(response, '') LIKE '%ghost answer delta%')",
    )
    .bind(TENANT)
    .bind(&session_id)
    .fetch_one(pool)
    .await
    .expect("count rolled back content");
    assert_eq!(rolled_back_content, 0, "被回滚掉的轮次正文必须消失");
    let fresh_content: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_turns \
         WHERE tenant_id = ?1 AND session_id = ?2 \
           AND COALESCE(response, '') LIKE '%fresh answer%'",
    )
    .bind(TENANT)
    .bind(&session_id)
    .fetch_one(pool)
    .await
    .expect("count fresh content");
    assert_eq!(fresh_content, 1, "新轮次内容必须立即成为事实");

    // 整包替换后必须没有孤儿轮次 / 孤儿零件等结构异常。
    let audit = crate::backend::application::conversations::conversation_maintenance_ops::audit_conversation_data_sqlx(
        pool,
        TENANT,
        None,
        None,
        false,
        None,
        &mut |_, _, _| {},
    )
    .await
    .expect("audit conversation data");
    let structural_categories = ["orphan_turns", "orphan_parts", "legacy_split_shell_parts"];
    let structural_issues = audit["issues"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|issue| {
            issue["category"]
                .as_str()
                .is_some_and(|category| structural_categories.contains(&category))
        })
        .collect::<Vec<_>>();
    assert!(
        structural_issues.is_empty(),
        "快照替换后不得残留孤儿轮次或孤儿零件: {structural_issues:?}"
    );

    drop(database);
    cleanup(&db_path);
}

#[tokio::test]
async fn web_snapshot_import_matches_session_family_contract() {
    let (database, db_path) = open_pool("web").await;
    let pool = database.pool();
    let adapter = adapter("snapshot-web");
    let source = source(&adapter.id);
    upsert_conversation_adapter_sqlx(pool, TENANT, &adapter)
        .await
        .unwrap();
    upsert_conversation_source_sqlx(pool, TENANT, &source)
        .await
        .unwrap();

    let session = NormalizedConversationSession {
        external_id: "web-session".to_string(),
        title: Some("Web".to_string()),
        project_path: None,
        started_at: Some("2026-11-06T00:00:00Z".to_string()),
        updated_at: Some("2026-11-06T01:00:00Z".to_string()),
        source_locator: Some("fixture://web".to_string()),
        source_fingerprint: Some("web-v1".to_string()),
        turns: vec![
            turn(4, "web question one", "web answer one"),
            turn(8, "web question two", "web answer two"),
        ],
        ..Default::default()
    };
    let result = import_web_record_sessions_sqlx(pool, TENANT, &source, &[session], false)
        .await
        .expect("import web snapshot");
    assert_eq!(result.changed_session_count, 1);

    let session_id =
        crate::backend::domain::stable_id("web-record-session", &[&source.id, "web-session"]);
    let rows: Vec<(String, i64, String)> = sqlx::query_as(
        "SELECT id, turn_index, started_at FROM web_record_turns WHERE tenant_id = ?1 AND session_id = ?2 ORDER BY turn_index",
    )
    .bind(TENANT)
    .bind(&session_id)
    .fetch_all(pool)
    .await
    .expect("load web turns");
    assert_eq!(rows.len(), 2);
    assert_eq!((rows[0].1, rows[1].1), (0, 1));
    assert_eq!(
        rows[0].0,
        crate::backend::domain::derive_web_record_turn_id(&session_id, 0)
    );
    assert_eq!(
        rows[1].0,
        crate::backend::domain::derive_web_record_turn_id(&session_id, 1)
    );
    assert_eq!(rows[0].2, "2026-11-06T01:00:00Z");

    drop(database);
    cleanup(&db_path);
}

#[tokio::test]
async fn usage_events_attribute_through_turn_index() {
    let (database, db_path) = open_pool("usage").await;
    let pool = database.pool();
    let adapter = adapter("snapshot-usage");
    let source = source(&adapter.id);
    upsert_conversation_adapter_sqlx(pool, TENANT, &adapter)
        .await
        .unwrap();
    upsert_conversation_source_sqlx(pool, TENANT, &source)
        .await
        .unwrap();
    let session = NormalizedConversationSession {
        external_id: "session-1".to_string(),
        title: Some("Usage".to_string()),
        project_path: Some("/tmp/usage".to_string()),
        started_at: Some("2026-11-06T00:00:00Z".to_string()),
        updated_at: Some("2026-11-06T00:30:00Z".to_string()),
        source_locator: Some("fixture://usage".to_string()),
        source_fingerprint: Some("usage-v1".to_string()),
        turns: vec![
            turn(0, "q0", "a0"),
            turn(1, "q1", "a1"),
            turn(2, "q2", "a2"),
        ],
        ..Default::default()
    };
    import_conversation_sessions_sqlx(pool, TENANT, &source, &[session], false)
        .await
        .expect("seed usage session");

    let events = vec![
        RawUsageEventInput {
            external_event_id: "event-1".to_string(),
            session_id: Some("session-1".to_string()),
            turn_id: Some("turn-2".to_string()),
            logical_request_id: None,
            attempt_index: None,
            timestamp: "2026-11-06T00:01:00Z".to_string(),
            provider: "anthropic".to_string(),
            model: "claude".to_string(),
            status: Some("success".to_string()),
            input_tokens: 10,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
            output_tokens: 5,
            host_reported_cost: None,
            currency: None,
        },
        RawUsageEventInput {
            external_event_id: "event-2".to_string(),
            session_id: Some("session-1".to_string()),
            turn_id: Some("msg-9f2ab1".to_string()),
            logical_request_id: None,
            attempt_index: None,
            timestamp: "2026-11-06T00:02:00Z".to_string(),
            provider: "anthropic".to_string(),
            model: "claude".to_string(),
            status: Some("success".to_string()),
            input_tokens: 10,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
            output_tokens: 5,
            host_reported_cost: None,
            currency: None,
        },
    ];
    upsert_usage_events_sqlx(pool, TENANT, &source.id, &adapter.id, &events, None)
        .await
        .expect("upsert usage events");

    let attributed: Option<i64> = sqlx::query_scalar(
        "SELECT turn_index FROM conversation_usage_events WHERE tenant_id = ?1 AND external_event_id = 'event-1'",
    )
    .bind(TENANT)
    .fetch_one(pool)
    .await
    .expect("query attributed event");
    assert_eq!(attributed, Some(2), "turn-N 约定必须归因到会话内序号");

    let unattributed: Option<i64> = sqlx::query_scalar(
        "SELECT turn_index FROM conversation_usage_events WHERE tenant_id = ?1 AND external_event_id = 'event-2'",
    )
    .bind(TENANT)
    .fetch_one(pool)
    .await
    .expect("query unattributed event");
    assert_eq!(unattributed, None, "无法定位到轮次序号的源键必须置空");

    // 序号解析成功但会话里没有这一轮（turn-9）同样必须置空。
    let out_of_range = vec![RawUsageEventInput {
        external_event_id: "event-3".to_string(),
        session_id: Some("session-1".to_string()),
        turn_id: Some("turn-9".to_string()),
        logical_request_id: None,
        attempt_index: None,
        timestamp: "2026-11-06T00:03:00Z".to_string(),
        provider: "anthropic".to_string(),
        model: "claude".to_string(),
        status: Some("success".to_string()),
        input_tokens: 10,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
        output_tokens: 5,
        host_reported_cost: None,
        currency: None,
    }];
    upsert_usage_events_sqlx(pool, TENANT, &source.id, &adapter.id, &out_of_range, None)
        .await
        .expect("upsert out-of-range usage event");
    let out_of_range_index: Option<i64> = sqlx::query_scalar(
        "SELECT turn_index FROM conversation_usage_events WHERE tenant_id = ?1 AND external_event_id = 'event-3'",
    )
    .bind(TENANT)
    .fetch_one(pool)
    .await
    .expect("query out-of-range event");
    assert_eq!(out_of_range_index, None, "越界轮次序号必须置空");

    drop(database);
    cleanup(&db_path);
}
