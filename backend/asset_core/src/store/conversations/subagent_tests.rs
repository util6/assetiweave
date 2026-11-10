use super::*;
use crate::backend::domain::{
    ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState,
    ConversationPartKind, ConversationPartRole, ConversationSource, ConversationSourceKind,
    NormalizedConversationPart, NormalizedConversationSession, NormalizedConversationTurn,
};
use crate::backend::store::conversations::conversation_repo::{
    import_conversation_sessions_with_control_sqlx, upsert_conversation_adapter_sqlx,
    upsert_conversation_source_sqlx,
};
use crate::backend::store::Database;
use uuid::Uuid;

const TEST_TENANT_ID: &str = "default";

#[test]
fn anchor_meta_requires_child_external_id() {
    assert!(SubagentAnchorMeta::parse(None).is_none());
    assert!(SubagentAnchorMeta::parse(Some("not-json")).is_none());
    assert!(SubagentAnchorMeta::parse(Some(r#"{"title":"x"}"#)).is_none());
    assert!(SubagentAnchorMeta::parse(Some(r#"{"child_external_id":"   "}"#)).is_none());

    let meta = SubagentAnchorMeta::parse(Some(
        r#"{"child_external_id":"parent:subagent:agent-1","title":"审计","agent_role":"Explore","step_count":7}"#,
    ))
    .expect("anchor meta");
    assert_eq!(meta.child_external_id, "parent:subagent:agent-1");
    assert_eq!(meta.title.as_deref(), Some("审计"));
    assert_eq!(meta.agent_role.as_deref(), Some("Explore"));
    assert_eq!(meta.step_count, Some(7));
}

#[test]
fn annotate_anchors_precomputes_child_session_id_and_process_item() {
    let source_id = "claude-code-live";
    let child_external_id = "parent-1:subagent:agent-1";
    let mut triad = crate::backend::domain::conversations::triad::split_normalized_turn(
        "claude-code",
        "审计 Windows 兼容性",
        &[anchor_part(
            child_external_id,
            "扫描兼容性",
            "发现 3 处路径分隔符问题",
        )],
    );

    let referenced = annotate_subagent_anchors(
        source_id,
        &[anchor_part(
            child_external_id,
            "扫描兼容性",
            "发现 3 处路径分隔符问题",
        )],
        &mut triad,
    );

    let expected_child = derive_child_session_id(source_id, child_external_id);
    assert_eq!(referenced, vec![expected_child.clone()]);

    let anchor = triad.parts.first().expect("anchor part retained");
    assert_eq!(anchor.kind, "claude-code.subagent");
    let attrs: serde_json::Value =
        serde_json::from_str(anchor.attrs_json.as_deref().expect("attrs")).expect("attrs json");
    assert_eq!(
        attrs.get("child_session_id").and_then(|v| v.as_str()),
        Some(expected_child.as_str())
    );
    assert_eq!(
        attrs.get("child_external_id").and_then(|v| v.as_str()),
        Some(child_external_id)
    );
    assert_eq!(
        attrs.get("renderer").and_then(|v| v.as_str()),
        Some("subagent_tree")
    );

    let process = triad.process.expect("process");
    let item = process
        .items
        .iter()
        .find(|item| item.kind == SUBAGENT_ANCHOR_KIND)
        .expect("subagent process item");
    assert_eq!(item.content.as_deref(), Some(expected_child.as_str()));
    assert_eq!(item.label.as_deref(), Some("扫描兼容性"));
    assert_eq!(item.summary.as_deref(), Some("发现 3 处路径分隔符问题"));
    assert_eq!(item.status.as_deref(), Some("completed"));
}

#[tokio::test]
async fn parent_anchor_resolves_child_session_id_regardless_of_import_order() {
    for child_first in [true, false] {
        let (database, source) = fixture_database("order").await;
        let pool = database.pool();
        let child_external_id = "parent-1:subagent:agent-1";
        let expected_child_id = derive_child_session_id(&source.id, child_external_id);
        let parent = parent_session("parent-1", Some(child_external_id), "fp-1");
        let child = child_session(child_external_id, "/tmp/project/subagents/agent-1.jsonl");
        let discovered = discovered_ids(&["parent-1", child_external_id]);

        let ordered: Vec<NormalizedConversationSession> = if child_first {
            vec![child, parent]
        } else {
            vec![parent, child]
        };
        import_conversation_sessions_with_control_sqlx(
            pool,
            TEST_TENANT_ID,
            &source,
            &ordered,
            Some(&discovered),
            false,
            None,
            &mut |_, _| {},
        )
        .await
        .unwrap();

        let stored_child_id: String = sqlx::query_scalar(
            "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND external_id = ?2",
        )
        .bind(TEST_TENANT_ID)
        .bind(child_external_id)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(stored_child_id, expected_child_id);

        let child_session_row: (i64, String) = sqlx::query_as(
            "SELECT user_visible, execution_origin FROM conversation_sessions WHERE tenant_id = ?1 AND id = ?2",
        )
        .bind(TEST_TENANT_ID)
        .bind(&expected_child_id)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(child_session_row.0, 0);
        assert_eq!(child_session_row.1, SUBAGENT_EXECUTION_ORIGIN);

        let (process_json, response): (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT t.process, t.response FROM conversation_turns t \
             JOIN conversation_sessions s ON s.tenant_id = t.tenant_id AND s.id = t.session_id \
             WHERE t.tenant_id = ?1 AND s.external_id = ?2 AND t.turn_index = 0",
        )
        .bind(TEST_TENANT_ID)
        .bind("parent-1")
        .fetch_one(pool)
        .await
        .unwrap();
        let process: crate::backend::domain::conversations::triad::TurnProcess =
            serde_json::from_str(process_json.as_deref().expect("process json")).unwrap();
        let anchor = process
            .items
            .iter()
            .find(|item| item.kind == SUBAGENT_ANCHOR_KIND)
            .expect("anchor item persisted");
        assert_eq!(anchor.content.as_deref(), Some(expected_child_id.as_str()));
        assert_eq!(
            response.as_deref(),
            Some("父会话最终回答"),
            "子代理派发不得截断父轮次，最终回答仍属于同一个父轮次"
        );

        let anchor_attrs: Option<String> = sqlx::query_scalar(
            "SELECT p.attrs_json FROM conversation_parts p \
             JOIN conversation_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id \
             JOIN conversation_sessions s ON s.tenant_id = t.tenant_id AND s.id = t.session_id \
             WHERE p.tenant_id = ?1 AND s.external_id = ?2 AND p.kind LIKE '%.subagent'",
        )
        .bind(TEST_TENANT_ID)
        .bind("parent-1")
        .fetch_one(pool)
        .await
        .unwrap();
        let anchor_attrs: serde_json::Value =
            serde_json::from_str(anchor_attrs.as_deref().unwrap()).unwrap();
        assert_eq!(
            anchor_attrs
                .get("child_session_id")
                .and_then(|v| v.as_str()),
            Some(expected_child_id.as_str())
        );
    }
}

#[tokio::test]
async fn rewind_keeps_subagent_assets_and_marks_them_missing() {
    let (database, source) = fixture_database("rewind").await;
    let pool = database.pool();
    let inline_child = "parent-1:subagent:agent-inline";
    let file_child = "parent-1:subagent:agent-file";
    let inline_child_id = derive_child_session_id(&source.id, inline_child);
    let file_child_id = derive_child_session_id(&source.id, file_child);

    let mut parent = parent_session_with_anchors(
        "parent-1",
        "fp-1",
        &[
            (inline_child, "内联子代理", "内联汇报"),
            (file_child, "落盘子代理", "落盘汇报"),
        ],
    );
    let inline_session = child_session(
        inline_child,
        &format!("/tmp/project/parent-1.jsonl#subagent:{inline_child}"),
    );
    let file_session = child_session(file_child, "/tmp/project/subagents/agent-file.jsonl");
    parent.turns[0].parts.push(NormalizedConversationPart {
        role: ConversationPartRole::Assistant,
        kind: ConversationPartKind::Text,
        text: Some("父会话最终回答".to_string()),
        language: None,
        command: None,
        cwd: None,
        status: None,
        exit_code: None,
        command_label: None,
        source_execution_id: None,
        content_card: None,
        metadata_json: None,
    });

    // 1. 首轮同步：父会话引用两个子会话。
    let first = discovered_ids(&["parent-1", inline_child, file_child]);
    import_conversation_sessions_with_control_sqlx(
        pool,
        TEST_TENANT_ID,
        &source,
        &[parent.clone(), inline_session.clone(), file_session.clone()],
        Some(&first),
        false,
        None,
        &mut |_, _| {},
    )
    .await
    .unwrap();
    assert!(session_row_exists(pool, &inline_child_id).await);
    assert!(session_row_exists(pool, &file_child_id).await);

    // 2. 宿主 /rewind：父会话抹掉全部派发，子代理文件也不再出现在扫描集合里。
    //    本地资产遵循保险箱语义（ADR-0023）：父快照替换不承担子会话的物理清理，
    //    源端消失只表达为 missing 状态，绝不会把已入库的事实顺手删掉。
    let rewound = parent_session("parent-1", None, "fp-2");
    let parent_only = discovered_ids(&["parent-1"]);
    import_conversation_sessions_with_control_sqlx(
        pool,
        TEST_TENANT_ID,
        &source,
        &[rewound.clone()],
        Some(&parent_only),
        false,
        None,
        &mut |_, _| {},
    )
    .await
    .unwrap();

    for child_id in [&inline_child_id, &file_child_id] {
        let row: (i64, Option<String>) = sqlx::query_as(
            "SELECT missing, execution_purpose FROM conversation_sessions \
             WHERE tenant_id = ?1 AND id = ?2",
        )
        .bind(TEST_TENANT_ID)
        .bind(child_id)
        .fetch_one(pool)
        .await
        .expect("subagent session must survive the parent rewind");
        assert_eq!(row.0, 1, "源端消失只标记 missing，不做物理删除");
        let purpose: serde_json::Value = serde_json::from_str(row.1.as_deref().unwrap()).unwrap();
        assert!(
            purpose.get("parent_session_id").is_some(),
            "父子溯源必须保留，子会话重新出现时才能自愈"
        );
    }

    // 3. 再次整包替换父快照也不会级联删除子会话事实。
    let mut rewound_twice = rewound;
    rewound_twice.source_fingerprint = Some("fp-3".to_string());
    import_conversation_sessions_with_control_sqlx(
        pool,
        TEST_TENANT_ID,
        &source,
        &[rewound_twice],
        Some(&parent_only),
        false,
        None,
        &mut |_, _| {},
    )
    .await
    .unwrap();
    for child_id in [&inline_child_id, &file_child_id] {
        assert!(
            session_row_exists(pool, child_id).await,
            "父快照替换不得把子代理会话当成隐式垃圾回收"
        );
    }
    assert!(parent_session_exists(pool, "parent-1").await);
}

async fn fixture_database(label: &str) -> (Database, ConversationSource) {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-subagent-{label}-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.unwrap();
    let adapter = ConversationAdapter {
        id: format!("{label}-adapter"),
        name: format!("{label}-adapter"),
        kind: ConversationAdapterKind::External,
        version: "1.0.0".to_string(),
        enabled: true,
        manifest_path: None,
        executable_path: Some(format!("/tmp/{label}-adapter/adapter")),
        content_hash: Some(format!("{label}-hash")),
        trusted_hash: Some(format!("{label}-hash")),
        trust_state: ConversationAdapterTrustState::Trusted,
        protocol_version: Some(1),
        capabilities: vec!["read_session".to_string()],
        input_kinds: vec![ConversationSourceKind::Directory],
        card_contract_version: None,
        card_kinds: Vec::new(),
        projection_version: Some(1),
        created_at: "2026-11-09T00:00:00Z".to_string(),
        updated_at: "2026-11-09T00:00:00Z".to_string(),
    };
    let source = ConversationSource {
        id: format!("{label}-source"),
        adapter_id: adapter.id.clone(),
        name: format!("{label} source"),
        kind: ConversationSourceKind::Directory,
        location: format!("/tmp/{label}/sessions"),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: "2026-11-09T00:00:00Z".to_string(),
        updated_at: "2026-11-09T00:00:00Z".to_string(),
    };
    let pool = database.pool();
    upsert_conversation_adapter_sqlx(pool, TEST_TENANT_ID, &adapter)
        .await
        .unwrap();
    upsert_conversation_source_sqlx(pool, TEST_TENANT_ID, &source)
        .await
        .unwrap();
    (database, source)
}

fn discovered_ids(external_ids: &[&str]) -> std::collections::BTreeSet<String> {
    external_ids.iter().map(|id| id.to_string()).collect()
}

fn parent_session(
    external_id: &str,
    child_external_id: Option<&str>,
    fingerprint: &str,
) -> NormalizedConversationSession {
    match child_external_id {
        Some(child) => parent_session_with_anchors(
            external_id,
            fingerprint,
            &[(child, "扫描兼容性", "发现 3 处问题")],
        ),
        None => parent_session_with_anchors(external_id, fingerprint, &[]),
    }
}

fn parent_session_with_anchors(
    external_id: &str,
    fingerprint: &str,
    anchors: &[(&str, &str, &str)],
) -> NormalizedConversationSession {
    let mut parts = anchors
        .iter()
        .map(|(child, title, summary)| anchor_part(child, title, summary))
        .collect::<Vec<_>>();
    parts.push(NormalizedConversationPart {
        role: ConversationPartRole::Assistant,
        kind: ConversationPartKind::Text,
        text: Some("父会话最终回答".to_string()),
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
    });
    NormalizedConversationSession {
        external_id: external_id.to_string(),
        title: Some("父会话".to_string()),
        project_path: Some("/tmp/project".to_string()),
        started_at: Some("2026-11-09T00:00:00Z".to_string()),
        updated_at: Some("2026-11-09T00:05:00Z".to_string()),
        source_locator: Some(format!("/tmp/project/{external_id}.jsonl")),
        source_fingerprint: Some(fingerprint.to_string()),
        turns: vec![NormalizedConversationTurn {
            external_id: Some(format!("{external_id}-turn-1")),
            turn_index: 0,
            user_text: "审计 Windows 兼容性".to_string(),
            title: None,
            started_at: Some("2026-11-09T00:00:00Z".to_string()),
            ended_at: Some("2026-11-09T00:05:00Z".to_string()),
            model: Some("claude".to_string()),
            parts,
            ..Default::default()
        }],
        execution_origin: Some("user".to_string()),
        execution_purpose: None,
        user_visible: Some(true),
    }
}

fn child_session(external_id: &str, source_locator: &str) -> NormalizedConversationSession {
    NormalizedConversationSession {
        external_id: external_id.to_string(),
        title: Some("子代理会话".to_string()),
        project_path: Some("/tmp/project".to_string()),
        started_at: Some("2026-11-09T00:01:00Z".to_string()),
        updated_at: Some("2026-11-09T00:04:00Z".to_string()),
        source_locator: Some(source_locator.to_string()),
        source_fingerprint: Some(format!("{external_id}-fp")),
        turns: vec![NormalizedConversationTurn {
            external_id: Some(format!("{external_id}-turn-1")),
            turn_index: 0,
            user_text: "全量委派 Prompt".to_string(),
            title: None,
            started_at: Some("2026-11-09T00:01:00Z".to_string()),
            ended_at: Some("2026-11-09T00:04:00Z".to_string()),
            model: Some("claude".to_string()),
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some("子代理最终汇报".to_string()),
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
        }],
        execution_origin: Some(SUBAGENT_EXECUTION_ORIGIN.to_string()),
        execution_purpose: Some(
            serde_json::json!({
                "parent_external_id": "parent-1",
                "parent_turn_index": 0,
                "agent_role": "Explore",
                "spawn_anchor": "toolu_1",
            })
            .to_string(),
        ),
        user_visible: Some(false),
    }
}

fn anchor_part(child_external_id: &str, title: &str, summary: &str) -> NormalizedConversationPart {
    NormalizedConversationPart {
        role: ConversationPartRole::Assistant,
        kind: ConversationPartKind::Subagent,
        text: Some(format!("[委派任务] {title}\n[汇报结论] {summary}")),
        language: None,
        command: None,
        cwd: None,
        status: Some("completed".to_string()),
        exit_code: None,
        command_label: None,
        source_execution_id: Some("toolu_1".to_string()),
        content_card: None,
        metadata_json: Some(
            serde_json::json!({
                "child_external_id": child_external_id,
                "agent_role": "Explore",
                "title": title,
                "status": "completed",
                "model": "claude",
                "step_count": 12,
                "report_summary": summary,
                "spawn_anchor": "toolu_1",
            })
            .to_string(),
        ),
    }
}

async fn session_row_exists(pool: &sqlx::SqlitePool, session_id: &str) -> bool {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_sessions WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(TEST_TENANT_ID)
    .bind(session_id)
    .fetch_one(pool)
    .await
    .unwrap();
    count > 0
}

async fn parent_session_exists(pool: &sqlx::SqlitePool, external_id: &str) -> bool {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_sessions WHERE tenant_id = ?1 AND external_id = ?2",
    )
    .bind(TEST_TENANT_ID)
    .bind(external_id)
    .fetch_one(pool)
    .await
    .unwrap();
    count > 0
}
