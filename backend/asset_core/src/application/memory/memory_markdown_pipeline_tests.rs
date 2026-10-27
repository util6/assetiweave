use super::*;
use crate::backend::application::memory::project_path_to_slug;
use crate::backend::application::memory::session_branch_workflow::{
    BranchItem, ExtractedSessionBranches, SessionBranchExtractorAgent,
};
use crate::backend::application::AppService;
use crate::backend::domain::{
    ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState,
    ConversationPartKind, ConversationPartRole, ConversationSource, ConversationSourceKind,
    NormalizedConversationPart, NormalizedConversationSession, NormalizedConversationTurn,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use uuid::Uuid;

fn make_text_turn(ext_id: &str, user_text: &str, bot_text: &str) -> NormalizedConversationTurn {
    let now = chrono::Utc::now();
    let ts = (now - chrono::Duration::hours(2)).to_rfc3339();
    NormalizedConversationTurn {
        external_id: ext_id.to_string(),
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

struct TestPipelineAgent {
    calls: AtomicUsize,
}

impl TestPipelineAgent {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl SessionBranchExtractorAgent for TestPipelineAgent {
    async fn extract_branches(
        &self,
        _session_id: &str,
        short_id: &str,
        _project_path: Option<&str>,
        qa_turns: &[(usize, String, String, Option<String>)],
    ) -> AppResult<ExtractedSessionBranches> {
        self.calls.fetch_add(1, Ordering::SeqCst);

        let is_session_1 = qa_turns
            .iter()
            .any(|(_, _, q, _)| q.contains("discuss architecture"));

        if is_session_1 {
            Ok(ExtractedSessionBranches {
                unexplored_branches: vec![
                    BranchItem {
                        turn_index: 1,
                        short_id: short_id.to_string(),
                        title: "Branch A: use redis cache".to_string(),
                        detail: "evaluate redis cache for distributed cluster".to_string(),
                    },
                    BranchItem {
                        turn_index: 2,
                        short_id: short_id.to_string(),
                        title: "Branch C: add compression".to_string(),
                        detail: "explore zstd compression for payloads".to_string(),
                    },
                ],
                completed_tasks: vec![BranchItem {
                    turn_index: 3,
                    short_id: short_id.to_string(),
                    title: "Done: base storage setup".to_string(),
                    detail: "completed sqlite storage initialization".to_string(),
                }],
                has_signal: true,
            })
        } else {
            // sess-2 completes Branch A!
            Ok(ExtractedSessionBranches {
                unexplored_branches: vec![BranchItem {
                    turn_index: 1,
                    short_id: short_id.to_string(),
                    title: "Branch D: metrics collection".to_string(),
                    detail: "new branch for metrics".to_string(),
                }],
                completed_tasks: vec![BranchItem {
                    turn_index: 2,
                    short_id: short_id.to_string(),
                    title: "Resolved redis cache".to_string(),
                    detail: "evaluate redis cache and completed integration".to_string(),
                }],
                has_signal: true,
            })
        }
    }
}

async fn setup_pipeline_fixture() -> (AppService, std::path::PathBuf, std::path::PathBuf) {
    let base = std::env::temp_dir().join(format!("assetiweave-pipe-{}", Uuid::new_v4()));
    let db_dir = base.join("db");
    let mem_dir = base.join("memories");
    std::fs::create_dir_all(&db_dir).unwrap();
    std::fs::create_dir_all(&mem_dir).unwrap();

    let service = AppService::open_with_db_path(db_dir.join("app.db"))
        .await
        .unwrap();
    (service, base, mem_dir)
}

#[tokio::test(flavor = "multi_thread")]
async fn test_end_to_end_full_memory_pipeline_and_leaf_ablation() {
    let (service, root_base, mem_dir) = setup_pipeline_fixture().await;
    let pool = service.pool();
    let tenant_id = "default";
    let storage = MemoryStorage::new_with_root(mem_dir.clone(), tenant_id);

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

    let project_path = "/code/assetiweave";
    let sess1 = make_session(
        "sess-1",
        "Session 1",
        Some(project_path),
        vec![make_text_turn(
            "t1",
            "discuss architecture",
            "Storage details",
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

    let agent = Arc::new(TestPipelineAgent::new());

    // --- Phase 1: Run Full Pipeline for Session 1 ---
    let report1 = service
        .run_memory_full_pipeline(Some(agent.as_ref()), Some(&storage))
        .await
        .expect("run full pipeline round 1");

    assert_eq!(report1.session_report.processed_count, 1);
    assert_eq!(report1.recent_project_report.project_count, 1);
    assert!(report1.global_report.updated);

    // Verify recent/latest.md exists and has both branches
    let recent = storage.read_markdown("recent/latest.md").unwrap().unwrap();
    assert!(recent.contains("redis cache"));
    assert!(recent.contains("zstd compression"));
    assert!(recent.contains("sqlite storage initialization"));

    // Verify projects/<slug>.md exists and has both Branch A and Branch C
    let proj_slug = project_path_to_slug(project_path);
    let proj_file = format!("projects/{}.md", proj_slug);
    let proj_content = storage.read_markdown(&proj_file).unwrap().unwrap();
    assert!(proj_content.contains("redis cache"));
    assert!(proj_content.contains("zstd compression"));

    // Verify MEMORY.md exists
    let global_mem = storage.read_markdown("MEMORY.md").unwrap().unwrap();
    assert!(global_mem.contains("全局长期记忆"));

    // --- Phase 2: Add Session 2 which resolves Branch A ---
    let mut sess2 = make_session(
        "sess-2",
        "Session 2",
        Some(project_path),
        vec![make_text_turn(
            "t2",
            "finish redis cache integration",
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

    // Verify Project Leaves ablation:
    // redis cache was completed in sess-2, so it MUST be ablated from project file!
    let updated_proj = storage.read_markdown(&proj_file).unwrap().unwrap();
    assert!(
        !updated_proj.contains("redis cache"),
        "Resolved branch 'redis cache' must be ablated from project leaves!"
    );
    // zstd compression was NOT completed, so it MUST remain!
    assert!(
        updated_proj.contains("zstd compression"),
        "Unresolved branch 'zstd compression' must remain in project leaves!"
    );
    // New metrics branch from sess-2 should be present
    assert!(updated_proj.contains("metrics"));

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
