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

    let now = chrono::Utc::now();
    let ts1 = (now - chrono::Duration::hours(5)).to_rfc3339();
    let ts2 = (now - chrono::Duration::hours(4)).to_rfc3339();

    let session1 = create_test_session(
        "s-branch-001",
        &ts1,
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
        &ts2,
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
        .run_session_branch_workflow_at(Some(fake_agent.as_ref()), Some(&storage), now)
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
        .run_session_branch_workflow_at(Some(fake_agent.as_ref()), Some(&storage), now)
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
    let ts2_mutated = (now - chrono::Duration::hours(2)).to_rfc3339();
    let mut session2_mutated = session2.clone();
    session2_mutated.updated_at = Some(ts2_mutated.clone());
    session2_mutated.turns.push(NormalizedConversationTurn {
        external_id: "t2-2".to_string(),
        turn_index: 1,
        user_text: "explore benchmark options".to_string(),
        title: None,
        started_at: Some(ts2_mutated.clone()),
        ended_at: Some(ts2_mutated.clone()),
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
        .run_session_branch_workflow_at(Some(fake_agent.as_ref()), Some(&storage), now)
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

#[tokio::test(flavor = "multi_thread")]
async fn test_session_branch_workflow_without_extractor_does_not_write_junk_to_disk() {
    let (service, root_base, mem_dir) = setup_fixture_service("no-extractor").await;
    let pool = service.pool();
    let tenant_id = "default";
    let storage = MemoryStorage::new_with_root(mem_dir.clone(), tenant_id);

    let adapter = ConversationAdapter {
        id: "adapter-no-ext".to_string(),
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
        id: "source-no-ext".to_string(),
        adapter_id: "adapter-no-ext".to_string(),
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

    let now = chrono::Utc::now();
    let ts = (now - chrono::Duration::hours(2)).to_rfc3339();
    let session = create_test_session(
        "s-no-ext-001",
        &ts,
        Some("/code/proj-no-ext"),
        vec![NormalizedConversationTurn {
            external_id: "turn-1".to_string(),
            turn_index: 0,
            user_text: "what is memory?".to_string(),
            title: None,
            started_at: Some(ts.clone()),
            ended_at: Some(ts.clone()),
            model: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some("Memory is state preservation.".to_string()),
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
        &[session],
        false,
    )
    .await
    .unwrap();

    // Call workflow with extractor = None
    let report = service
        .run_session_branch_workflow_at(None, Some(&storage), now)
        .await
        .expect("run workflow without extractor");

    assert_eq!(report.scanned_count, 1);
    assert_eq!(report.processed_count, 0);
    assert_eq!(report.skipped_count, 1);

    // Verify: No sessions/*.md written to disk!
    let sessions_dir = storage.sessions_dir();
    if sessions_dir.exists() {
        let entries: Vec<_> = std::fs::read_dir(&sessions_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(
            entries.len(),
            0,
            "No markdown files written without extractor"
        );
    }

    // Verify: No watermarks inserted!
    let watermarks = crate::backend::store::load_watermarks_sqlx(pool, tenant_id)
        .await
        .unwrap();
    assert_eq!(watermarks.len(), 0);

    drop(service);
    std::fs::remove_dir_all(root_base).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_session_branch_workflow_filters_expired_and_internal_sessions() {
    let (service, root_base, mem_dir) = setup_fixture_service("filter-window").await;
    let pool = service.pool();
    let tenant_id = "default";
    let storage = MemoryStorage::new_with_root(mem_dir.clone(), tenant_id);

    let adapter = ConversationAdapter {
        id: "adapter-filter".to_string(),
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
        id: "source-filter".to_string(),
        adapter_id: "adapter-filter".to_string(),
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

    let now = chrono::Utc::now();
    // 1. Valid recent session (2h ago)
    let ts_valid = (now - chrono::Duration::hours(2)).to_rfc3339();
    let valid_session = create_test_session(
        "s-valid",
        &ts_valid,
        Some("/code/proj-valid"),
        vec![NormalizedConversationTurn {
            external_id: "t-val-1".to_string(),
            turn_index: 0,
            user_text: "valid recent work".to_string(),
            title: None,
            started_at: Some(ts_valid.clone()),
            ended_at: Some(ts_valid.clone()),
            model: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some("Valid work response".to_string()),
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

    // 2. Expired session (100h ago, outside 48h/72h window)
    let ts_expired = (now - chrono::Duration::hours(100)).to_rfc3339();
    let expired_session = create_test_session(
        "s-expired",
        &ts_expired,
        Some("/code/proj-expired"),
        vec![NormalizedConversationTurn {
            external_id: "t-exp-1".to_string(),
            turn_index: 0,
            user_text: "old forgotten work".to_string(),
            title: None,
            started_at: Some(ts_expired.clone()),
            ended_at: Some(ts_expired.clone()),
            model: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some("Old work response".to_string()),
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

    // 3. Internal agent-runs session
    let internal_root =
        crate::backend::infrastructure::agent_execution::agent_execution_workspace_root(
            &service.db_path,
        );
    let mut internal_session = create_test_session(
        "s-internal",
        &ts_valid,
        Some(&format!("{internal_root}/session-run-123")),
        vec![NormalizedConversationTurn {
            external_id: "t-int-1".to_string(),
            turn_index: 0,
            user_text: "internal agent operation".to_string(),
            title: None,
            started_at: Some(ts_valid.clone()),
            ended_at: Some(ts_valid.clone()),
            model: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some("Internal agent execution output".to_string()),
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
    internal_session.execution_origin = Some("internal_agent".to_string());
    internal_session.user_visible = Some(false);

    crate::backend::store::import_conversation_sessions_sqlx(
        pool,
        tenant_id,
        &source,
        &[valid_session, expired_session, internal_session],
        false,
    )
    .await
    .unwrap();

    let fake_agent = Arc::new(FakeBranchExtractorAgent::new());
    let report = service
        .run_session_branch_workflow_at(Some(fake_agent.as_ref()), Some(&storage), now)
        .await
        .expect("run workflow with filters");

    // Only valid_session should be scanned and processed!
    assert_eq!(report.scanned_count, 1);
    assert_eq!(report.processed_count, 1);
    assert_eq!(fake_agent.calls(), 1);

    drop(service);
    std::fs::remove_dir_all(root_base).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_session_branch_workflow_normalizes_multiple_subdirectories_to_same_git_root() {
    let (service, root_base, mem_dir) = setup_fixture_service("git-norm").await;
    let pool = service.pool();
    let tenant_id = "default";
    let storage = MemoryStorage::new_with_root(mem_dir.clone(), tenant_id);

    // Create a mock git repository with 2 subpackages
    let repo_dir = root_base.join("my-git-repo");
    let git_dir = repo_dir.join(".git");
    let subpkg_a = repo_dir.join("crates").join("pkg_a");
    let subpkg_b = repo_dir.join("crates").join("pkg_b");
    std::fs::create_dir_all(&git_dir).unwrap();
    std::fs::create_dir_all(&subpkg_a).unwrap();
    std::fs::create_dir_all(&subpkg_b).unwrap();

    let adapter = ConversationAdapter {
        id: "adapter-norm".to_string(),
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
        id: "source-norm".to_string(),
        adapter_id: "adapter-norm".to_string(),
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

    let now = chrono::Utc::now();
    let ts1 = (now - chrono::Duration::hours(2)).to_rfc3339();
    let ts2 = (now - chrono::Duration::hours(1)).to_rfc3339();

    let session_a = create_test_session(
        "s-pkg-a",
        &ts1,
        Some(subpkg_a.to_str().unwrap()),
        vec![NormalizedConversationTurn {
            external_id: "turn-a".to_string(),
            turn_index: 0,
            user_text: "work in subpkg a".to_string(),
            title: None,
            started_at: Some(ts1.clone()),
            ended_at: Some(ts1.clone()),
            model: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some("Done in a".to_string()),
                language: None,
                command: None,
                cwd: Some(subpkg_a.to_str().unwrap().to_string()),
                status: None,
                exit_code: None,
                command_label: None,
                source_execution_id: None,
                content_card: None,
                metadata_json: None,
            }],
        }],
    );

    let session_b = create_test_session(
        "s-pkg-b",
        &ts2,
        Some(subpkg_b.to_str().unwrap()),
        vec![NormalizedConversationTurn {
            external_id: "turn-b".to_string(),
            turn_index: 0,
            user_text: "work in subpkg b".to_string(),
            title: None,
            started_at: Some(ts2.clone()),
            ended_at: Some(ts2.clone()),
            model: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some("Done in b".to_string()),
                language: None,
                command: None,
                cwd: Some(subpkg_b.to_str().unwrap().to_string()),
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
        &[session_a, session_b],
        false,
    )
    .await
    .unwrap();

    let fake_agent = Arc::new(FakeBranchExtractorAgent::new());
    let report = service
        .run_session_branch_workflow_at(Some(fake_agent.as_ref()), Some(&storage), now)
        .await
        .expect("run workflow with git normalization");

    assert_eq!(report.scanned_count, 2);
    assert_eq!(report.processed_count, 2);

    // Read both session markdown files and verify project_path is normalized to Git repo root
    let sessions_dir = storage.sessions_dir();
    let entries: Vec<_> = std::fs::read_dir(&sessions_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(entries.len(), 2);

    let canonical_repo = repo_dir
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string();
    for entry in entries {
        let content = std::fs::read_to_string(entry.path()).unwrap();
        assert!(
            content.contains(&format!("project_path: {}", canonical_repo))
                || content.contains(&format!("project_path: {}", repo_dir.display())),
            "Expected project_path in session markdown to normalize to git repo root: {content}"
        );
    }

    drop(service);
    std::fs::remove_dir_all(root_base).ok();
}
