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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use uuid::Uuid;

fn make_text_turn(ext_id: &str, user_text: &str, bot_text: &str) -> NormalizedConversationTurn {
    let now = chrono::Utc::now();
    let ts = (now - chrono::Duration::hours(2)).to_rfc3339();
    NormalizedConversationTurn {
        external_id: Some(ext_id.to_string()),
        turn_index: 0,
        user_text: user_text.to_string(),
        title: None,
        started_at: Some(ts.clone()),
        ended_at: Some(ts),
        model: None,
        parts: vec![NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some(bot_text.to_string()),
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

fn make_session(
    ext_id: &str,
    title: &str,
    project_path: Option<&str>,
    turns: Vec<NormalizedConversationTurn>,
) -> NormalizedConversationSession {
    let now = chrono::Utc::now();
    let ts = (now - chrono::Duration::hours(2)).to_rfc3339();
    NormalizedConversationSession {
        external_id: ext_id.to_string(),
        title: Some(title.to_string()),
        project_path: project_path.map(|p| p.to_string()),
        started_at: Some(ts.clone()),
        updated_at: Some(ts),
        source_locator: None,
        source_fingerprint: None,
        turns,
        execution_origin: None,
        execution_purpose: None,
        user_visible: Some(true),
    }
}

struct TestPipelineRuntime {
    calls: AtomicUsize,
}

impl TestPipelineRuntime {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }
}

impl AgentExecutionRuntime for TestPipelineRuntime {
    fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let prompt = request.prompt.clone();
        let is_session_1 = prompt.contains("discuss architecture");

        let response = if is_session_1 {
            r#"{
                "action": "commit",
                "topic_id": "storage-engine",
                "topic_name": "Storage Engine Architecture",
                "status": "completed",
                "turn_index": 0,
                "summary": "Completed sqlite storage initialization",
                "unexplored_siblings": [
                    {"turn_index": 0, "summary": "Branch A: evaluate redis cache for distributed cluster"},
                    {"turn_index": 0, "summary": "Branch C: explore zstd compression for payloads"}
                ]
            }"#
            .to_string()
        } else {
            r#"{
                "action": "commit",
                "topic_id": "storage-engine",
                "topic_name": "Storage Engine Architecture",
                "status": "completed",
                "turn_index": 0,
                "summary": "Resolved redis cache and completed integration",
                "unexplored_siblings": [
                    {"turn_index": 0, "summary": "Branch D: metrics collection"}
                ]
            }"#
            .to_string()
        };

        Box::pin(async move {
            Ok(AiExecutionResult {
                text: response,
                agent_id: request.agent_id,
                protocol: AgentProtocol::Acp,
                requested_model: request.model,
                elapsed_ms: 1,
                persistent_binding: None,
                replay_text: None,
                session_cleanup: SessionCleanupStatus::Deleted,
            })
        })
    }
}

async fn setup_pipeline_fixture() -> (AppService, std::path::PathBuf, std::path::PathBuf) {
    let base = std::env::temp_dir().join(format!("assetiweave-pipe-{}", Uuid::new_v4()));
    let db_dir = base.join("db");
    // 记忆根目录由运行时锚定在数据库同级目录，不再另建兄弟目录。
    let mem_dir = db_dir.join("memories");
    std::fs::create_dir_all(&db_dir).unwrap();
    std::fs::create_dir_all(&mem_dir).unwrap();

    let service = AppService::open_with_db_path(db_dir.join("app.db"))
        .await
        .unwrap();
    (service, base, mem_dir)
}

#[tokio::test(flavor = "multi_thread")]
async fn test_end_to_end_full_memory_pipeline_and_leaf_ablation() {
    let (service, root_base, _mem_dir) = setup_pipeline_fixture().await;
    let pool = service.pool();
    let tenant_id = "default";
    let storage = service.memory_storage();

    let adapter = ConversationAdapter {
        id: "adapter-pipe-1".to_string(),
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
        id: "source-pipe-1".to_string(),
        adapter_id: "adapter-pipe-1".to_string(),
        name: "Test Pipe Source".to_string(),
        kind: ConversationSourceKind::Directory,
        location: "/code/pipe-proj".to_string(),
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

    let sess1 = make_session(
        "sess-pipe-001",
        "Pipeline Initial Architecture",
        Some("/code/pipe-proj"),
        vec![make_text_turn(
            "turn-1",
            "discuss architecture and redis cache",
            "Initial architecture discussed",
        )],
    );

    crate::backend::store::import_conversation_sessions_sqlx(
        pool,
        tenant_id,
        &source,
        &[sess1.clone()],
        false,
    )
    .await
    .unwrap();

    let agent = Arc::new(TestPipelineRuntime::new());

    // --- Phase 1: Round 1 Execution ---
    let report1 = service
        .run_memory_full_pipeline(Some(agent.as_ref()), Some(&storage))
        .await
        .expect("run full pipeline round 1");

    assert_eq!(report1.session_report.processed_count, 1);
    assert_eq!(report1.session_report.skipped_count, 0);
    assert_eq!(agent.calls.load(Ordering::SeqCst), 1);

    // Verify Project file generated
    let proj_slug = project_path_to_slug("/code/pipe-proj");
    let proj_file = format!("projects/{proj_slug}.md");
    let proj_content = storage
        .read_markdown(&proj_file)
        .unwrap()
        .expect("project markdown must exist");
    assert!(proj_content.contains("Branch A: evaluate redis cache"));
    assert!(proj_content.contains("Branch C: explore zstd compression"));

    // Verify recent latest generated
    let recent = storage
        .read_markdown("recent/latest.md")
        .unwrap()
        .expect("recent latest must exist");
    assert!(recent.contains("Branch A: evaluate redis cache"));

    // Verify global memory generated
    assert!(report1.global_report.updated);
    let global_mem = storage
        .read_markdown("MEMORY.md")
        .unwrap()
        .expect("MEMORY.md must exist");
    assert!(global_mem.contains("全局长期记忆"));

    // --- Phase 2: Incremental Session with Topic Evolution ---
    let mut sess2 = make_session(
        "sess-pipe-002",
        "Pipeline Branch Resolution",
        Some("/code/pipe-proj"),
        vec![make_text_turn(
            "turn-2",
            "resolve redis cache and metrics",
            "Redis integration complete",
        )],
    );
    let ts_sess2 = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    sess2.updated_at = Some(ts_sess2);

    crate::backend::store::import_conversation_sessions_sqlx(
        pool,
        tenant_id,
        &source,
        &[sess1.clone(), sess2.clone()],
        false,
    )
    .await
    .unwrap();

    let report2 = service
        .run_memory_full_pipeline(Some(agent.as_ref()), Some(&storage))
        .await
        .expect("run full pipeline round 2");

    assert_eq!(report2.session_report.processed_count, 1); // Only sess-2 processed
    assert_eq!(report2.session_report.skipped_count, 1); // sess-1 skipped via watermark!

    let updated_proj = storage.read_markdown(&proj_file).unwrap().unwrap();
    assert!(updated_proj.contains("Branch D: metrics collection"));

    // --- Phase 3: Idempotent Run without changes ---
    let report3 = service
        .run_memory_full_pipeline(Some(agent.as_ref()), Some(&storage))
        .await
        .expect("run full pipeline round 3");

    assert_eq!(report3.session_report.processed_count, 0);
    assert_eq!(report3.session_report.skipped_count, 2);
    assert_eq!(
        agent.calls.load(Ordering::SeqCst),
        2,
        "Agent not called when all watermarks match"
    );

    drop(service);
    std::fs::remove_dir_all(root_base).ok();
}
