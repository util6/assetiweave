use asset_core::application::AppService;
use asset_core::domain::memory::{MemoryTreeNode, NodeStatus, TopicEvolutionCommitPayload};
use asset_core::infrastructure::memory::storage::MemoryStorage;
use chrono::Utc;
use std::fs;
use std::path::PathBuf;
use tokio::sync::Mutex;
use uuid::Uuid;

static TEST_MUTEX: Mutex<()> = Mutex::const_new(());

struct TestHarness {
    tmp_dir: PathBuf,
    mem_dir: PathBuf,
    db_path: PathBuf,
}

impl TestHarness {
    fn new(prefix: &str) -> Self {
        let tmp = std::env::temp_dir().join(format!("aiw-m2-adv-c2-{prefix}-{}", Uuid::new_v4()));
        let db_dir = tmp.join("db");
        let mem_dir = tmp.join("memories");
        fs::create_dir_all(&db_dir).unwrap();
        fs::create_dir_all(&mem_dir).unwrap();
        let db_path = db_dir.join("app.db");
        Self {
            tmp_dir: tmp,
            mem_dir,
            db_path,
        }
    }

    async fn create_service(&self) -> AppService {
        std::env::set_var("ASSETIWEAVE_HOME", &self.tmp_dir);
        std::env::set_var("ASSETIWEAVE_DB_PATH", &self.db_path);
        std::env::set_var("ASSETIWEAVE_MEMORY_ROOT", &self.mem_dir);
        AppService::open_for_engine().await.unwrap()
    }

    fn storage(&self, tenant: &str) -> MemoryStorage {
        MemoryStorage::new_with_root(self.mem_dir.clone(), tenant)
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.tmp_dir).ok();
    }
}

// =========================================================================
// 1. Multi-Tree Evolution & Sibling Preservation Under Commit
// =========================================================================

#[tokio::test]
async fn test_adv_multi_tree_sibling_preservation_and_status_transitions() {
    let _guard = TEST_MUTEX.lock().await;
    let harness = TestHarness::new("sib-preserve");
    let service = harness.create_service().await;
    let tenant = service.tenant_id().to_string();
    let storage = harness.storage(&tenant);

    let project_slug = "preserve-project";
    let topic_id = "architecture-evolution";

    // Step 1: Commit root node with 3 unexplored sibling leaves (S1, S2, S3)
    let root_node = MemoryTreeNode {
        id: "sess0#T1".to_string(),
        parent_id: None,
        topic_id: topic_id.to_string(),
        summary: "Architecture Root Decision".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "session-root-000".to_string(),
        turn_index: 1,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    };

    let sib_1 = MemoryTreeNode {
        id: "sess0#T1.1".to_string(),
        parent_id: Some("sess0#T1".to_string()),
        topic_id: topic_id.to_string(),
        summary: "Unexplored Sibling Branch 1: Database Sharding".to_string(),
        status: NodeStatus::Unexplored,
        source_session_id: "session-root-000".to_string(),
        turn_index: 1,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    };
    let sib_2 = MemoryTreeNode {
        id: "sess0#T1.2".to_string(),
        parent_id: Some("sess0#T1".to_string()),
        topic_id: topic_id.to_string(),
        summary: "Unexplored Sibling Branch 2: Event Sourcing".to_string(),
        status: NodeStatus::Unexplored,
        source_session_id: "session-root-000".to_string(),
        turn_index: 1,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    };
    let sib_3 = MemoryTreeNode {
        id: "sess0#T1.3".to_string(),
        parent_id: Some("sess0#T1".to_string()),
        topic_id: topic_id.to_string(),
        summary: "Unexplored Sibling Branch 3: gRPC Transport".to_string(),
        status: NodeStatus::Unexplored,
        source_session_id: "session-root-000".to_string(),
        turn_index: 1,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    };

    let payload_init = TopicEvolutionCommitPayload {
        project_slug: project_slug.to_string(),
        topic_id: topic_id.to_string(),
        topic_name: Some("Architecture Evolution".to_string()),
        parent_node_id: None,
        parent_new_status: None,
        new_node: root_node,
        unexplored_siblings: vec![sib_1.clone(), sib_2.clone(), sib_3.clone()],
    };
    service
        .commit_memory_topic_evolution_to(payload_init, &storage)
        .await
        .unwrap();

    // Verify initial tree state: 4 nodes
    let state_init = storage
        .read_project_state(project_slug)
        .unwrap()
        .expect("state exists");
    let tree = state_init
        .topic_trees
        .get(topic_id)
        .expect("topic tree exists");
    assert_eq!(tree.nodes.len(), 4, "Initial tree must have 4 nodes");
    assert_eq!(
        tree.nodes.get("sess0#T1").unwrap().status,
        NodeStatus::Completed
    );
    assert_eq!(
        tree.nodes.get("sess0#T1.1").unwrap().status,
        NodeStatus::Unexplored
    );
    assert_eq!(
        tree.nodes.get("sess0#T1.2").unwrap().status,
        NodeStatus::Unexplored
    );
    assert_eq!(
        tree.nodes.get("sess0#T1.3").unwrap().status,
        NodeStatus::Unexplored
    );

    // Step 2: Explore Sibling 1 (sess0#T1.1).
    // Updating its status from Unexplored to Continued!
    // And branching new unexplored sibling leaves under it!
    let child_1 = MemoryTreeNode {
        id: "sess1#T2".to_string(),
        parent_id: Some("sess0#T1.1".to_string()),
        topic_id: topic_id.to_string(),
        summary: "Database Sharding Implemented".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "session-child-001".to_string(),
        turn_index: 2,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    };
    let sub_sib_1 = MemoryTreeNode {
        id: "sess1#T2.1".to_string(),
        parent_id: Some("sess0#T1.1".to_string()),
        topic_id: topic_id.to_string(),
        summary: "Sub-branch: Range vs Hash Sharding".to_string(),
        status: NodeStatus::Unexplored,
        source_session_id: "session-child-001".to_string(),
        turn_index: 2,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    };

    let payload_explore_s1 = TopicEvolutionCommitPayload {
        project_slug: project_slug.to_string(),
        topic_id: topic_id.to_string(),
        topic_name: Some("Architecture Evolution".to_string()),
        parent_node_id: Some("sess0#T1.1".to_string()),
        parent_new_status: Some(NodeStatus::Continued),
        new_node: child_1,
        unexplored_siblings: vec![sub_sib_1],
    };
    service
        .commit_memory_topic_evolution_to(payload_explore_s1, &storage)
        .await
        .unwrap();

    // Verify after Step 2:
    // - sess0#T1.1 must now be Continued!
    // - sess0#T1.2 and sess0#T1.3 (siblings of sess0#T1.1) must NOT be overwritten, dropped, or corrupted!
    let state_after_s1 = storage
        .read_project_state(project_slug)
        .unwrap()
        .expect("state exists");
    let tree_after_s1 = state_after_s1
        .topic_trees
        .get(topic_id)
        .expect("topic tree exists");

    let s1 = tree_after_s1.nodes.get("sess0#T1.1").expect("s1 exists");
    assert_eq!(
        s1.status,
        NodeStatus::Continued,
        "Parent node status must be updated from Unexplored to Continued"
    );

    let s2 = tree_after_s1.nodes.get("sess0#T1.2").expect("s2 preserved");
    assert_eq!(
        s2.status,
        NodeStatus::Unexplored,
        "Sibling 2 must remain Unexplored and NOT dropped or corrupted"
    );
    assert_eq!(s2.parent_id, Some("sess0#T1".to_string()));

    let s3 = tree_after_s1.nodes.get("sess0#T1.3").expect("s3 preserved");
    assert_eq!(
        s3.status,
        NodeStatus::Unexplored,
        "Sibling 3 must remain Unexplored and NOT dropped or corrupted"
    );
    assert_eq!(s3.parent_id, Some("sess0#T1".to_string()));

    // Step 3: Explore Sibling 2 (sess0#T1.2).
    // Updating its status from Unexplored to Completed!
    let child_2 = MemoryTreeNode {
        id: "sess2#T3".to_string(),
        parent_id: Some("sess0#T1.2".to_string()),
        topic_id: topic_id.to_string(),
        summary: "Event Sourcing Rejected after Evaluation".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "session-child-002".to_string(),
        turn_index: 3,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    };

    let payload_explore_s2 = TopicEvolutionCommitPayload {
        project_slug: project_slug.to_string(),
        topic_id: topic_id.to_string(),
        topic_name: Some("Architecture Evolution".to_string()),
        parent_node_id: Some("sess0#T1.2".to_string()),
        parent_new_status: Some(NodeStatus::Completed),
        new_node: child_2,
        unexplored_siblings: vec![],
    };
    service
        .commit_memory_topic_evolution_to(payload_explore_s2, &storage)
        .await
        .unwrap();

    let state_after_s2 = storage
        .read_project_state(project_slug)
        .unwrap()
        .expect("state exists");
    let tree_after_s2 = state_after_s2
        .topic_trees
        .get(topic_id)
        .expect("topic tree exists");

    assert_eq!(
        tree_after_s2.nodes.get("sess0#T1.2").unwrap().status,
        NodeStatus::Completed,
        "Sibling 2 must now be Completed"
    );
    assert_eq!(
        tree_after_s2.nodes.get("sess0#T1.1").unwrap().status,
        NodeStatus::Continued,
        "Sibling 1 must still be Continued"
    );
    assert_eq!(
        tree_after_s2.nodes.get("sess0#T1.3").unwrap().status,
        NodeStatus::Unexplored,
        "Sibling 3 must still be Unexplored"
    );
}

// =========================================================================
// 2. Protection of Continued / Completed Siblings against Stale Commits
// =========================================================================

#[tokio::test]
async fn test_adv_protect_continued_completed_siblings_from_stale_overwrites() {
    let _guard = TEST_MUTEX.lock().await;
    let harness = TestHarness::new("stale-sib");
    let service = harness.create_service().await;
    let tenant = service.tenant_id().to_string();
    let storage = harness.storage(&tenant);

    let project_slug = "stale-protect-proj";
    let topic_id = "core-engine";

    // Initialize root with S1
    let root = MemoryTreeNode {
        id: "init#T1".to_string(),
        parent_id: None,
        topic_id: topic_id.to_string(),
        summary: "Initial Core Engine Setup".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "sess-init".to_string(),
        turn_index: 1,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    };
    let s1 = MemoryTreeNode {
        id: "init#T1.1".to_string(),
        parent_id: Some("init#T1".to_string()),
        topic_id: topic_id.to_string(),
        summary: "Task 1: Add cache".to_string(),
        status: NodeStatus::Unexplored,
        source_session_id: "sess-init".to_string(),
        turn_index: 1,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    };
    service
        .commit_memory_topic_evolution_to(
            TopicEvolutionCommitPayload {
                project_slug: project_slug.to_string(),
                topic_id: topic_id.to_string(),
                topic_name: Some("Core Engine".to_string()),
                parent_node_id: None,
                parent_new_status: None,
                new_node: root,
                unexplored_siblings: vec![s1.clone()],
            },
            &storage,
        )
        .await
        .unwrap();

    // Advance S1 to Continued
    let n1 = MemoryTreeNode {
        id: "adv#T2".to_string(),
        parent_id: Some("init#T1.1".to_string()),
        topic_id: topic_id.to_string(),
        summary: "Cache added".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "sess-adv".to_string(),
        turn_index: 2,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    };
    service
        .commit_memory_topic_evolution_to(
            TopicEvolutionCommitPayload {
                project_slug: project_slug.to_string(),
                topic_id: topic_id.to_string(),
                topic_name: Some("Core Engine".to_string()),
                parent_node_id: Some("init#T1.1".to_string()),
                parent_new_status: Some(NodeStatus::Continued),
                new_node: n1,
                unexplored_siblings: vec![],
            },
            &storage,
        )
        .await
        .unwrap();

    // Verify S1 is Continued
    let state = storage.read_project_state(project_slug).unwrap().unwrap();
    assert_eq!(
        state.topic_trees[topic_id].nodes["init#T1.1"].status,
        NodeStatus::Continued
    );

    // Adversarial step: A stale commit arrives listing "init#T1.1" as an unexplored sibling again!
    let mut stale_s1 = s1.clone();
    stale_s1.summary = "Attacked summary attempting to overwrite".to_string();
    stale_s1.status = NodeStatus::Unexplored;

    let stale_commit = TopicEvolutionCommitPayload {
        project_slug: project_slug.to_string(),
        topic_id: topic_id.to_string(),
        topic_name: Some("Core Engine".to_string()),
        parent_node_id: Some("init#T1".to_string()),
        parent_new_status: None,
        new_node: MemoryTreeNode {
            id: "stale#T3".to_string(),
            parent_id: Some("init#T1".to_string()),
            topic_id: topic_id.to_string(),
            summary: "Concurrent task".to_string(),
            status: NodeStatus::Completed,
            source_session_id: "sess-stale".to_string(),
            turn_index: 3,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        },
        unexplored_siblings: vec![stale_s1],
    };

    service
        .commit_memory_topic_evolution_to(stale_commit, &storage)
        .await
        .unwrap();

    // Empirical assertion: S1 MUST REMAIN Continued, and must NOT be overwritten!
    let state_after = storage.read_project_state(project_slug).unwrap().unwrap();
    let s1_node = &state_after.topic_trees[topic_id].nodes["init#T1.1"];
    assert_eq!(
        s1_node.status,
        NodeStatus::Continued,
        "Continued node must NOT be overwritten back to Unexplored by stale sibling commit"
    );
    assert_eq!(
        s1_node.summary, "Task 1: Add cache",
        "Summary of Continued node must be preserved from overwrite"
    );
}

// =========================================================================
// 3. Multi-Topic Evolution Coexistence in Single Project
// =========================================================================

#[tokio::test]
async fn test_adv_multi_topic_evolution_coexistence() {
    let _guard = TEST_MUTEX.lock().await;
    let harness = TestHarness::new("multi-topic");
    let service = harness.create_service().await;
    let tenant = service.tenant_id().to_string();
    let storage = harness.storage(&tenant);

    let project_slug = "multi-topic-hub";
    let topics = vec![
        ("auth", "Authentication & Tokens"),
        ("db", "Database & Migrations"),
        ("ui", "Frontend Design System"),
        ("net", "Networking & Transports"),
    ];

    // Interleave commits across 4 topics
    for (t_id, t_name) in &topics {
        let root = MemoryTreeNode {
            id: format!("sess-{}#T1", t_id),
            parent_id: None,
            topic_id: t_id.to_string(),
            summary: format!("Root for {}", t_name),
            status: NodeStatus::Completed,
            source_session_id: format!("sess-{}", t_id),
            turn_index: 1,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        let sib = MemoryTreeNode {
            id: format!("sess-{}#T1.1", t_id),
            parent_id: Some(format!("sess-{}#T1", t_id)),
            topic_id: t_id.to_string(),
            summary: format!("Pending investigation for {}", t_name),
            status: NodeStatus::Unexplored,
            source_session_id: format!("sess-{}", t_id),
            turn_index: 1,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        service
            .commit_memory_topic_evolution_to(
                TopicEvolutionCommitPayload {
                    project_slug: project_slug.to_string(),
                    topic_id: t_id.to_string(),
                    topic_name: Some(t_name.to_string()),
                    parent_node_id: None,
                    parent_new_status: None,
                    new_node: root,
                    unexplored_siblings: vec![sib],
                },
                &storage,
            )
            .await
            .unwrap();
    }

    // Now advance db and ui topics
    let db_advance = TopicEvolutionCommitPayload {
        project_slug: project_slug.to_string(),
        topic_id: "db".to_string(),
        topic_name: Some("Database & Migrations".to_string()),
        parent_node_id: Some("sess-db#T1.1".to_string()),
        parent_new_status: Some(NodeStatus::Continued),
        new_node: MemoryTreeNode {
            id: "sess-db-2#T2".to_string(),
            parent_id: Some("sess-db#T1.1".to_string()),
            topic_id: "db".to_string(),
            summary: "Executed 14-digit migration refactor".to_string(),
            status: NodeStatus::Completed,
            source_session_id: "sess-db-2".to_string(),
            turn_index: 2,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        },
        unexplored_siblings: vec![],
    };
    service
        .commit_memory_topic_evolution_to(db_advance, &storage)
        .await
        .unwrap();

    // Verify all 4 topics exist independently
    let state = storage.read_project_state(project_slug).unwrap().unwrap();
    assert_eq!(
        state.topic_trees.len(),
        4,
        "Must have exactly 4 topic trees"
    );

    for (t_id, _) in &topics {
        assert!(
            state.topic_trees.contains_key(*t_id),
            "Topic tree {} must exist",
            t_id
        );
    }

    assert_eq!(
        state.topic_trees["db"].nodes["sess-db#T1.1"].status,
        NodeStatus::Continued
    );
    assert_eq!(
        state.topic_trees["auth"].nodes["sess-auth#T1.1"].status,
        NodeStatus::Unexplored
    );
    assert_eq!(
        state.topic_trees["ui"].nodes["sess-ui#T1.1"].status,
        NodeStatus::Unexplored
    );
    assert_eq!(
        state.topic_trees["net"].nodes["sess-net#T1.1"].status,
        NodeStatus::Unexplored
    );

    // Verify Markdown projection contains all 4 topics
    let md_path = storage.projects_dir().join(format!("{}.md", project_slug));
    assert!(md_path.exists(), "Project markdown file must exist");
    let md_content = fs::read_to_string(&md_path).unwrap();
    assert!(md_content.contains("Authentication & Tokens"));
    assert!(md_content.contains("Database & Migrations"));
    assert!(md_content.contains("Frontend Design System"));
    assert!(md_content.contains("Networking & Transports"));
}

// =========================================================================
// 4. Sessionless Integrity: ZERO `sessions/*.md` Created
// =========================================================================

#[tokio::test]
async fn test_adv_sessionless_integrity_zero_sessions_markdown_files() {
    let _guard = TEST_MUTEX.lock().await;
    let harness = TestHarness::new("sessionless");
    let service = harness.create_service().await;
    let tenant = service.tenant_id().to_string();
    let storage = harness.storage(&tenant);

    storage.init_dirs().unwrap();

    // Explicitly create a `sessions/` directory to check if any workflow writes into it
    let sessions_dir = storage.root_dir().join("sessions");
    fs::create_dir_all(&sessions_dir).unwrap();

    // Run recent and project workflow
    let report = service
        .run_recent_and_project_workflow(&storage)
        .await
        .unwrap();
    println!("Workflow report: {:?}", report);

    // Perform commits across multiple projects
    for p in ["proj-alpha", "proj-beta", "proj-gamma"] {
        let root = MemoryTreeNode {
            id: format!("{}#T1", p),
            parent_id: None,
            topic_id: "main".to_string(),
            summary: format!("Root node for {}", p),
            status: NodeStatus::Completed,
            source_session_id: format!("session-{}", p),
            turn_index: 1,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        service
            .commit_memory_topic_evolution_to(
                TopicEvolutionCommitPayload {
                    project_slug: p.to_string(),
                    topic_id: "main".to_string(),
                    topic_name: Some("Main Topic".to_string()),
                    parent_node_id: None,
                    parent_new_status: None,
                    new_node: root,
                    unexplored_siblings: vec![],
                },
                &storage,
            )
            .await
            .unwrap();
    }

    // Run workflow again to sync projections
    service
        .run_recent_and_project_workflow(&storage)
        .await
        .unwrap();

    // Verify .state/ files exist
    assert!(storage.state_project_file("proj-alpha").exists());
    assert!(storage.state_project_file("proj-beta").exists());
    assert!(storage.state_project_file("proj-gamma").exists());
    assert!(storage.state_recent_file().exists());

    // Verify project markdown files exist
    assert!(storage.projects_dir().join("proj-alpha.md").exists());
    assert!(storage.projects_dir().join("proj-beta.md").exists());
    assert!(storage.projects_dir().join("proj-gamma.md").exists());
    assert!(storage.recent_dir().join("latest.md").exists());

    // EMPIRICAL VERIFICATION: Check sessions/ directory
    let session_files: Vec<_> = fs::read_dir(&sessions_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(
        session_files.len(),
        0,
        "VIOLATION: sessions/ directory must contain ZERO files, found: {:?}",
        session_files
    );

    // Recursively scan memory root for any .md files outside allowed locations
    for entry in walkdir::WalkDir::new(storage.root_dir())
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "md") {
            let rel = path.strip_prefix(storage.root_dir()).unwrap();
            let rel_str = rel.to_string_lossy();
            assert!(
                rel_str.starts_with("projects/")
                    || rel_str.starts_with("recent/")
                    || rel_str == "MEMORY.md",
                "VIOLATION: Unexpected markdown file found outside allowed paths: {}",
                rel_str
            );
            assert!(
                !rel_str.starts_with("sessions/"),
                "VIOLATION: sessions/*.md file found: {}",
                rel_str
            );
        }
    }
}

// =========================================================================
// 5. UTF-8 Multibyte Safety Stress (Chinese, Emojis, Short, Empty)
// =========================================================================

#[tokio::test]
async fn test_adv_utf8_multibyte_safety_stress_all_patterns() {
    let _guard = TEST_MUTEX.lock().await;
    let harness = TestHarness::new("utf8-stress");
    let service = harness.create_service().await;
    let tenant = service.tenant_id().to_string();
    let storage = harness.storage(&tenant);

    let test_cases = vec![
        // (session_id, description)
        ("会话测试-12345678", "CJK multi-byte 3-byte boundary split"),
        ("🚀🌟🔥🤖💻🎉✨🔧", "4-byte unicode emojis"),
        ("短", "Short 1-char multibyte string"),
        ("a", "Short 1-char ASCII string"),
        ("ab", "Short 2-char ASCII string"),
        ("", "Empty session string"),
        ("éàüñ-café-resumé", "2-byte Latin-1 characters"),
        ("混合-🚀-mixed-123", "Mixed ASCII, CJK and 4-byte emoji"),
    ];

    for (session_id, desc) in test_cases {
        let node = MemoryTreeNode {
            id: "nonstandard_raw_id".to_string(),
            parent_id: None,
            topic_id: "utf8-test".to_string(),
            summary: format!("Testing {}", desc),
            status: NodeStatus::Completed,
            source_session_id: session_id.to_string(),
            turn_index: 1,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        // 1. short_id() must NOT panic on non-char-boundary
        let short_id_result = std::panic::catch_unwind(|| node.short_id());
        assert!(
            short_id_result.is_ok(),
            "short_id() panics on session_id: '{}' ({})",
            session_id,
            desc
        );
        let short_id = short_id_result.unwrap();
        assert!(
            short_id.chars().count() <= 8,
            "short_id must be at most 8 chars, got: {}",
            short_id.chars().count()
        );

        // 2. format_id and parse_id
        let formatted = MemoryTreeNode::format_id(&short_id, 42, Some(1));
        let parsed = MemoryTreeNode::parse_id(&formatted);
        if !short_id.is_empty() {
            assert!(
                parsed.is_some(),
                "parse_id must succeed on formatted ID: {}",
                formatted
            );
            let (parsed_short, parsed_turn, parsed_sub) = parsed.unwrap();
            assert_eq!(parsed_short, short_id);
            assert_eq!(parsed_turn, 42);
            assert_eq!(parsed_sub, Some(1));
        }

        // 3. Commit evolution with this node into project state
        let commit_node = MemoryTreeNode {
            id: if short_id.is_empty() {
                "sess-empty#T1".to_string()
            } else {
                MemoryTreeNode::format_id(&short_id, 1, None)
            },
            parent_id: None,
            topic_id: "utf8-topic".to_string(),
            summary: format!("Commit node for {}", desc),
            status: NodeStatus::Completed,
            source_session_id: session_id.to_string(),
            turn_index: 1,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        let commit_res = service
            .commit_memory_topic_evolution_to(
                TopicEvolutionCommitPayload {
                    project_slug: "utf8-project".to_string(),
                    topic_id: "utf8-topic".to_string(),
                    topic_name: Some("UTF-8 Topic".to_string()),
                    parent_node_id: None,
                    parent_new_status: None,
                    new_node: commit_node,
                    unexplored_siblings: vec![],
                },
                &storage,
            )
            .await;

        assert!(
            commit_res.is_ok(),
            "commit_memory_topic_evolution_to must succeed for session_id '{}' ({}), got error: {:?}",
            session_id,
            desc,
            commit_res.err()
        );
    }
}
