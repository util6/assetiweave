use super::*;
use crate::backend::application::memory::project_path_to_slug;
use crate::backend::application::AppService;
use crate::backend::domain::agents::AgentProtocol;
use crate::backend::domain::{
    ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState,
    ConversationPartKind, ConversationPartRole, ConversationSource, ConversationSourceKind,
    NormalizedConversationPart, NormalizedConversationSession, NormalizedConversationTurn,
};
use crate::backend::infrastructure::agent_execution::executor::BackendFuture;
use crate::backend::infrastructure::agent_execution::{
    AgentExecutionRuntime, AiExecutionRequest, AiExecutionResult, SessionCleanupStatus,
};
use crate::backend::infrastructure::memory::storage::MemoryStorage;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use uuid::Uuid;

struct MockBranchAgentRuntime {
    call_count: AtomicUsize,
    prompts_received: Mutex<Vec<String>>,
    responses: Mutex<Vec<AppResult<String>>>,
}

impl MockBranchAgentRuntime {
    fn new(responses: Vec<AppResult<String>>) -> Self {
        Self {
            call_count: AtomicUsize::new(0),
            prompts_received: Mutex::new(Vec::new()),
            responses: Mutex::new(responses),
        }
    }

    fn calls(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    fn prompts(&self) -> Vec<String> {
        self.prompts_received.lock().unwrap().clone()
    }
}

impl AgentExecutionRuntime for MockBranchAgentRuntime {
    fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        self.prompts_received
            .lock()
            .unwrap()
            .push(request.prompt.clone());
        let res = {
            let mut list = self.responses.lock().unwrap();
            if list.is_empty() {
                Ok(r#"{
                    "action": "commit",
                    "topic_id": "default-topic",
                    "status": "completed",
                    "turn_index": 0,
                    "summary": "Default mock commit"
                }"#
                .to_string())
            } else {
                list.remove(0)
            }
        };

        Box::pin(async move {
            match res {
                Ok(text) => Ok(AiExecutionResult {
                    text,
                    agent_id: request.agent_id,
                    protocol: AgentProtocol::Acp,
                    requested_model: request.model,
                    elapsed_ms: 1,
                    persistent_binding: None,
                    replay_text: None,
                    session_cleanup: SessionCleanupStatus::Deleted,
                }),
                Err(err) => Err(
                    crate::backend::infrastructure::agent_execution::AiExecutionError::Output {
                        message: err.to_string(),
                    },
                ),
            }
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
        user_visible: Some(true),
        ..Default::default()
    }
}

fn make_qa_turn(idx: u32, q: &str, a: &str) -> NormalizedConversationTurn {
    let now = chrono::Utc::now();
    let ts = (now - chrono::Duration::hours(2)).to_rfc3339();
    NormalizedConversationTurn {
        external_id: Some(format!("turn-{idx}")),
        turn_index: idx as i64,
        user_text: q.to_string(),
        title: None,
        started_at: Some(ts.clone()),
        ended_at: Some(ts),
        model: None,
        parts: vec![NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some(a.to_string()),
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
    }
}

async fn insert_adapter_and_source(
    service: &AppService,
    adapter_id: &str,
    source_id: &str,
) -> ConversationSource {
    let pool = service.pool();
    let tenant_id = service.tenant_id();

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
        created_at: "2026-10-21T10:00:00Z".to_string(),
        updated_at: "2026-10-21T10:00:00Z".to_string(),
    };
    let source = ConversationSource {
        id: source_id.to_string(),
        adapter_id: adapter_id.to_string(),
        name: "Test Source".to_string(),
        kind: ConversationSourceKind::Directory,
        location: "/code/proj-test".to_string(),
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
    source
}

#[tokio::test(flavor = "multi_thread")]
async fn test_oldest_first_sorting_order_workflow() {
    let (service, root_base, mem_dir) = setup_fixture_service("oldest-first").await;
    let pool = service.pool();
    let tenant_id = service.tenant_id();
    let storage = MemoryStorage::new_with_root(mem_dir.clone(), tenant_id);
    let source = insert_adapter_and_source(&service, "ad-order", "src-order").await;

    // Create 3 sessions with distinct timestamps: t1 (oldest) < t2 (middle) < t3 (newest)
    let now = chrono::Utc::now();
    let t1 = (now - chrono::Duration::hours(10)).to_rfc3339();
    let t2 = (now - chrono::Duration::hours(5)).to_rfc3339();
    let t3 = (now - chrono::Duration::hours(1)).to_rfc3339();

    let sess1 = create_test_session(
        "sess-old-001",
        &t1,
        Some("/code/proj-order"),
        vec![make_qa_turn(0, "question 1", "answer 1")],
    );
    let sess2 = create_test_session(
        "sess-mid-002",
        &t2,
        Some("/code/proj-order"),
        vec![make_qa_turn(0, "question 2", "answer 2")],
    );
    let sess3 = create_test_session(
        "sess-new-003",
        &t3,
        Some("/code/proj-order"),
        vec![make_qa_turn(0, "question 3", "answer 3")],
    );

    // Import into DB in REVERSE chronological order (newest first, then oldest, then mid)
    crate::backend::store::import_conversation_sessions_sqlx(
        pool,
        tenant_id,
        &source,
        &[sess3.clone(), sess1.clone(), sess2.clone()],
        false,
    )
    .await
    .unwrap();

    let runtime = MockBranchAgentRuntime::new(vec![
        Ok(r#"{
            "action": "commit",
            "topic_id": "order-topic",
            "status": "completed",
            "turn_index": 0,
            "summary": "Processed session 1"
        }"#
        .to_string()),
        Ok(r#"{
            "action": "commit",
            "topic_id": "order-topic",
            "status": "completed",
            "turn_index": 0,
            "summary": "Processed session 2"
        }"#
        .to_string()),
        Ok(r#"{
            "action": "commit",
            "topic_id": "order-topic",
            "status": "completed",
            "turn_index": 0,
            "summary": "Processed session 3"
        }"#
        .to_string()),
    ]);

    let report = service
        .run_session_branch_workflow_at(Some(&runtime), Some(&storage), now)
        .await
        .unwrap();

    assert_eq!(report.processed_count, 3);
    assert_eq!(report.skipped_count, 0);
    assert_eq!(runtime.calls(), 3);

    // Verify chronological execution order from prompts received
    let prompts = runtime.prompts();
    assert_eq!(prompts.len(), 3);
    assert!(
        prompts[0].contains("question 1"),
        "First prompt must be oldest session (sess-old-001)"
    );
    assert!(
        prompts[1].contains("question 2"),
        "Second prompt must be middle session (sess-mid-002)"
    );
    assert!(
        prompts[2].contains("question 3"),
        "Third prompt must be newest session (sess-new-003)"
    );

    drop(service);
    std::fs::remove_dir_all(root_base).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_watermark_match_zero_llm_calls_short_circuit() {
    let (service, root_base, mem_dir) = setup_fixture_service("watermark-zero-llm").await;
    let pool = service.pool();
    let tenant_id = service.tenant_id();
    let storage = MemoryStorage::new_with_root(mem_dir.clone(), tenant_id);
    let source = insert_adapter_and_source(&service, "ad-wm", "src-wm").await;

    let now = chrono::Utc::now();
    let ts = (now - chrono::Duration::hours(2)).to_rfc3339();

    let sess = create_test_session(
        "sess-wm-001",
        &ts,
        Some("/code/proj-wm"),
        vec![
            make_qa_turn(0, "question 0", "answer 0"),
            make_qa_turn(1, "question 1", "answer 1"),
        ],
    );

    crate::backend::store::import_conversation_sessions_sqlx(
        pool,
        tenant_id,
        &source,
        &[sess.clone()],
        false,
    )
    .await
    .unwrap();

    let proj_slug = project_path_to_slug("/code/proj-wm");
    let db_session_id: String = sqlx::query_scalar(
        "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND external_id = ?2",
    )
    .bind(tenant_id)
    .bind("sess-wm-001")
    .fetch_one(pool)
    .await
    .unwrap();

    // Manually record watermark turn = 2 in .state/projects/<slug>.json
    service
        .advance_memory_session_watermark_in(&proj_slug, &db_session_id, 2, &storage)
        .await
        .unwrap();

    // Mock agent with 0 responses: if it is called, it would error or increment call count
    let runtime = MockBranchAgentRuntime::new(Vec::new());

    let report = service
        .run_session_branch_workflow_at(Some(&runtime), Some(&storage), now)
        .await
        .unwrap();

    // Must be skipped with 0 LLM calls!
    assert_eq!(report.scanned_count, 1);
    assert_eq!(report.processed_count, 0);
    assert_eq!(report.skipped_count, 1);
    assert_eq!(
        runtime.calls(),
        0,
        "Watermark match MUST result in 0 LLM calls!"
    );

    drop(service);
    std::fs::remove_dir_all(root_base).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zero_sessions_markdown_and_zero_sqlite_watermarks() {
    let (service, root_base, mem_dir) = setup_fixture_service("zero-sessions-md").await;
    let pool = service.pool();
    let tenant_id = service.tenant_id();
    let storage = MemoryStorage::new_with_root(mem_dir.clone(), tenant_id);
    let source = insert_adapter_and_source(&service, "ad-zero", "src-zero").await;

    let now = chrono::Utc::now();
    let ts = (now - chrono::Duration::hours(2)).to_rfc3339();

    let sess = create_test_session(
        "sess-zero-001",
        &ts,
        Some("/code/proj-zero"),
        vec![make_qa_turn(0, "implement zero sessions md", "done")],
    );

    crate::backend::store::import_conversation_sessions_sqlx(
        pool,
        tenant_id,
        &source,
        &[sess.clone()],
        false,
    )
    .await
    .unwrap();

    let runtime = MockBranchAgentRuntime::new(vec![Ok(r#"{
        "action": "commit",
        "topic_id": "zero-sessions",
        "topic_name": "Zero Sessions Architecture",
        "status": "completed",
        "turn_index": 0,
        "summary": "Eliminated sessions/*.md entirely",
        "unexplored_siblings": [
            {"turn_index": 0, "summary": "Verify multi-tree branching"}
        ]
    }"#
    .to_string())]);

    let report = service
        .run_session_branch_workflow_at(Some(&runtime), Some(&storage), now)
        .await
        .unwrap();

    assert_eq!(report.processed_count, 1);
    assert_eq!(runtime.calls(), 1);

    // 1. Verify ZERO sessions/*.md generated
    let sessions_dir = storage.sessions_dir();
    if sessions_dir.exists() {
        let entries: Vec<_> = std::fs::read_dir(&sessions_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(
            entries.is_empty(),
            "sessions/ directory must NOT contain any files"
        );
    }

    // 2. Verify ZERO SQLite watermarks written (watermarks are decoupled from SQLite)
    let sqlite_watermarks =
        crate::backend::store::memory::watermark_repo::load_watermarks_sqlx(pool, tenant_id)
            .await
            .unwrap();
    assert!(
        sqlite_watermarks.is_empty(),
        "SQLite memory_session_watermarks table must NOT be written"
    );

    // 3. Verify .state/projects/<slug>.json contains watermark and topic tree
    let proj_slug = project_path_to_slug("/code/proj-zero");
    let db_session_id: String = sqlx::query_scalar(
        "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND external_id = ?2",
    )
    .bind(tenant_id)
    .bind("sess-zero-001")
    .fetch_one(pool)
    .await
    .unwrap();
    let state = service
        .get_project_memory_state_in(&proj_slug, &storage)
        .await
        .unwrap()
        .expect("Project state must exist");
    assert_eq!(state.get_watermark(&db_session_id), Some(1));
    assert!(state.topic_trees.contains_key("zero-sessions"));

    // 4. Verify projects/<slug>.md is projected
    let proj_md = storage
        .read_markdown(format!("projects/{proj_slug}.md"))
        .unwrap()
        .expect("projects/<slug>.md must exist");
    assert!(proj_md.contains("Zero Sessions Architecture"));
    assert!(proj_md.contains("Eliminated sessions/*.md entirely"));

    drop(service);
    std::fs::remove_dir_all(root_base).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_empty_qa_turns_session_advances_watermark_without_llm() {
    let (service, root_base, mem_dir) = setup_fixture_service("empty-turns").await;
    let pool = service.pool();
    let tenant_id = service.tenant_id();
    let storage = MemoryStorage::new_with_root(mem_dir.clone(), tenant_id);
    let source = insert_adapter_and_source(&service, "ad-empty", "src-empty").await;

    let now = chrono::Utc::now();
    let ts = (now - chrono::Duration::hours(2)).to_rfc3339();

    // Session with NO turns
    let sess = create_test_session("sess-empty-001", &ts, Some("/code/proj-empty"), Vec::new());

    crate::backend::store::import_conversation_sessions_sqlx(
        pool,
        tenant_id,
        &source,
        &[sess.clone()],
        false,
    )
    .await
    .unwrap();

    let runtime = MockBranchAgentRuntime::new(Vec::new());

    let report = service
        .run_session_branch_workflow_at(Some(&runtime), Some(&storage), now)
        .await
        .unwrap();

    assert_eq!(report.scanned_count, 1);
    assert_eq!(report.processed_count, 0);
    assert_eq!(report.skipped_count, 1);
    assert_eq!(runtime.calls(), 0, "No QA turns means 0 LLM calls");

    // Watermark advanced to 0
    let proj_slug = project_path_to_slug("/code/proj-empty");
    let db_session_id: String = sqlx::query_scalar(
        "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND external_id = ?2",
    )
    .bind(tenant_id)
    .bind("sess-empty-001")
    .fetch_one(pool)
    .await
    .unwrap();
    let state = service
        .get_project_memory_state_in(&proj_slug, &storage)
        .await
        .unwrap()
        .expect("Project state must exist");
    assert_eq!(state.get_watermark(&db_session_id), Some(0));

    drop(service);
    std::fs::remove_dir_all(root_base).ok();
}
