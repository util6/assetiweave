use super::*;
use crate::backend::application::AppService;
use crate::backend::domain::{
    ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState,
    ConversationPartKind, ConversationPartRole, ConversationSource, ConversationSourceKind,
    NormalizedConversationPart, NormalizedConversationSession, NormalizedConversationTurn,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use uuid::Uuid;

struct FakeBranchExtractorAgent {
    call_count: AtomicUsize,
}

impl FakeBranchExtractorAgent {
    fn new() -> Self {
        Self {
            call_count: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl SessionBranchExtractorAgent for FakeBranchExtractorAgent {
    async fn extract_branches(
        &self,
        _session_id: &str,
        short_id: &str,
        _project_path: Option<&str>,
        qa_turns: &[(usize, String, String, Option<String>)],
    ) -> AppResult<ExtractedSessionBranches> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let mut unexplored = Vec::new();
        let mut completed = Vec::new();

        for (idx, _turn_id, question, answer) in qa_turns {
            if question.contains("explore") || question.contains("options") {
                unexplored.push(BranchItem {
                    turn_index: *idx,
                    short_id: short_id.to_string(),
                    title: "Branch A: alternative cache strategy".to_string(),
                    detail: format!("Considered in question '{question}', answer: {:?}", answer),
                });
            } else {
                completed.push(BranchItem {
                    turn_index: *idx,
                    short_id: short_id.to_string(),
                    title: "Implemented SQLite watermark store".to_string(),
                    detail: format!("Completed turn {idx}"),
                });
            }
        }

        Ok(ExtractedSessionBranches {
            unexplored_branches: unexplored,
            completed_tasks: completed,
            has_signal: true,
        })
    }
}

async fn setup_fixture_service(
    test_name: &str,
) -> (AppService, std::path::PathBuf, std::path::PathBuf) {
    let base = std::env::temp_dir().join(format!(
        "assetiweave-branch-{}-{}",
        test_name,
        Uuid::new_v4()
    ));
    let db_dir = base.join("db");
    let mem_dir = base.join("memories");
    std::fs::create_dir_all(&db_dir).expect("create db dir");
    std::fs::create_dir_all(&mem_dir).expect("create mem dir");

    let service = AppService::open_with_db_path(db_dir.join("app.db"))
        .await
        .expect("open test service");
    (service, base, mem_dir)
}

fn create_test_session(
    ext_id: &str,
    timestamp: &str,
    project_path: Option<&str>,
    turns: Vec<NormalizedConversationTurn>,
) -> NormalizedConversationSession {
    NormalizedConversationSession {
        external_id: ext_id.to_string(),
        title: Some(format!("Test Session {}", ext_id)),
        project_path: project_path.map(|s| s.to_string()),
        started_at: Some(timestamp.to_string()),
        updated_at: Some(timestamp.to_string()),
        source_locator: Some(format!("fixture://{}", ext_id)),
        source_fingerprint: Some(format!("fp-{}", ext_id)),
        turns,
        ..Default::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_session_branch_workflow_incremental_scan_and_watermark_skip() {
    let (service, root_base, mem_dir) = setup_fixture_service("branch-workflow").await;
    let pool = service.pool();
    let tenant_id = "default";
    let storage = MemoryStorage::new_with_root(mem_dir.clone(), tenant_id);

    let adapter = ConversationAdapter {
        id: "adapter-branch-1".to_string(),
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
        created_at: "2026-10-21T10:00:00Z".to_string(),
        updated_at: "2026-10-21T10:00:00Z".to_string(),
    };
    let source = ConversationSource {
        id: "source-branch-1".to_string(),
        adapter_id: "adapter-branch-1".to_string(),
        name: "Test Source".to_string(),
        kind: ConversationSourceKind::Directory,
        location: "/fixture/path".to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: "2026-10-21T10:00:00Z".to_string(),
        updated_at: "2026-10-21T10:00:00Z".to_string(),
    };

    crate::backend::store::upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter)
        .await
        .unwrap();
    crate::backend::store::upsert_conversation_source_sqlx(pool, tenant_id, &source)
        .await
        .unwrap();

    let session1 = create_test_session(
        "s-branch-001",
        "2026-10-21T10:00:00Z",
        Some("/code/proj-a"),
        vec![
            NormalizedConversationTurn {
                external_id: "t1-1".to_string(),
                turn_index: 0,
                user_text: "explore options for database cache".to_string(),
                title: None,
                started_at: Some("2026-10-21T10:00:00Z".to_string()),
                ended_at: Some("2026-10-21T10:01:00Z".to_string()),
                model: None,
                parts: vec![NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Text,
                    text: Some("We could use Redis or SQLite or In-Memory. Secret: Bearer abcdef12345678901234567890".to_string()),
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
            NormalizedConversationTurn {
                external_id: "t1-2".to_string(),
                turn_index: 1,
                user_text: "proceed with sqlite".to_string(),
                title: None,
                started_at: Some("2026-10-21T10:02:00Z".to_string()),
                ended_at: Some("2026-10-21T10:03:00Z".to_string()),
                model: None,
                parts: vec![NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Text,
                    text: Some("SQLite implemented successfully.".to_string()),
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
    );

    let session2 = create_test_session(
        "s-branch-002",
        "2026-10-21T11:00:00Z",
        Some("/code/proj-b"),
        vec![NormalizedConversationTurn {
            external_id: "t2-1".to_string(),
            turn_index: 0,
            user_text: "implement unit tests".to_string(),
            title: None,
            started_at: Some("2026-10-21T11:00:00Z".to_string()),
            ended_at: Some("2026-10-21T11:01:00Z".to_string()),
            model: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some("Unit tests written.".to_string()),
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
    );

    crate::backend::store::import_conversation_sessions_sqlx(
        pool,
        tenant_id,
        &source,
        &[session1.clone(), session2.clone()],
        false,
    )
    .await
    .unwrap();

    let fake_agent = Arc::new(FakeBranchExtractorAgent::new());

    // 1. Run workflow for the first time
    let report1 = service
        .run_session_branch_workflow(fake_agent.as_ref(), Some(&storage))
        .await
        .expect("run workflow round 1");

    assert_eq!(report1.processed_count, 2);
    assert_eq!(report1.skipped_count, 0);
    assert_eq!(fake_agent.calls(), 2);

    // Verify session files created
    let sessions_dir = storage.sessions_dir();
    let entries: Vec<_> = std::fs::read_dir(&sessions_dir)
        .expect("read sessions dir")
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(entries.len(), 2);

    // Verify Markdown content and redaction
    let mut found_session1_md = false;
    for entry in &entries {
        let path = entry.path();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("---"));
        assert!(content.contains("session_id:"));
        assert!(content.contains("## 🍂 待探索的悬空叶子节点"));
        assert!(content.contains("## ✅ 已完成的闭环任务"));
        assert!(content.contains("aiwc://session/"));

        if content.contains("/code/proj-a") {
            found_session1_md = true;
            // Secret must be redacted!
            assert!(!content.contains("abcdef12345678901234567890"));
            assert!(content.contains("[REDACTED"));
        }
    }
    assert!(found_session1_md);

    // Verify SQLite watermark table has 2 rows
    let watermarks = crate::backend::store::load_watermarks_sqlx(pool, tenant_id)
        .await
        .unwrap();
    assert_eq!(watermarks.len(), 2);

    // 2. Run workflow a second time without changes -> IDEMPOTENT SKIP!
    let report2 = service
        .run_session_branch_workflow(fake_agent.as_ref(), Some(&storage))
        .await
        .expect("run workflow round 2");

    assert_eq!(report2.processed_count, 0);
    assert_eq!(report2.skipped_count, 2);
    assert_eq!(
        fake_agent.calls(),
        2,
        "Agent must not be called when watermarks match"
    );

    // 3. Mutate session 2 by adding a new turn -> updated_at advances
    let mut session2_mutated = session2.clone();
    session2_mutated.updated_at = Some("2026-10-21T12:00:00Z".to_string());
    session2_mutated.turns.push(NormalizedConversationTurn {
        external_id: "t2-2".to_string(),
        turn_index: 1,
        user_text: "explore benchmark options".to_string(),
        title: None,
        started_at: Some("2026-10-21T12:00:00Z".to_string()),
        ended_at: Some("2026-10-21T12:01:00Z".to_string()),
        model: None,
        parts: vec![NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some("Benchmarking details".to_string()),
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
    });

    crate::backend::store::import_conversation_sessions_sqlx(
        pool,
        tenant_id,
        &source,
        &[session1.clone(), session2_mutated],
        false,
    )
    .await
    .unwrap();

    // 4. Run workflow a third time -> only mutated session is processed
    let report3 = service
        .run_session_branch_workflow(fake_agent.as_ref(), Some(&storage))
        .await
        .expect("run workflow round 3");

    assert_eq!(report3.processed_count, 1);
    assert_eq!(report3.skipped_count, 1);
    assert_eq!(
        fake_agent.calls(),
        3,
        "Agent called exactly once for the updated session"
    );

    drop(service);
    std::fs::remove_dir_all(root_base).ok();
}
