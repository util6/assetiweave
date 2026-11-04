use asset_core::application::AppService;
use asset_core::domain::memory::{MemoryTreeNode, NodeStatus, TopicEvolutionCommitPayload};
use asset_core::infrastructure::memory::storage::MemoryStorage;
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
        let tmp = std::env::temp_dir().join(format!("aiw-m1-adv-{prefix}-{}", Uuid::new_v4()));
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
// 1. Adversarial Scenario 1: Deeply Nested Trees (30 generations)
// =========================================================================

#[tokio::test]
async fn test_adv_deeply_nested_tree_generations() {
    let _guard = TEST_MUTEX.lock().await;
    let harness = TestHarness::new("deep-tree");
    let service = harness.create_service().await;
    let tenant = service.tenant_id().to_string();
    let storage = harness.storage(&tenant);

    let project_slug = "deep-nested-project";
    let topic_id = "deep-evolution";

    // Commit root turn 1
    let root_node = MemoryTreeNode {
        id: "sess0#T1".to_string(),
        parent_id: None,
        topic_id: topic_id.to_string(),
        summary: "Generation 0 root node".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "session-root".to_string(),
        turn_index: 1,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };

    let payload = TopicEvolutionCommitPayload {
        project_slug: project_slug.to_string(),
        topic_id: topic_id.to_string(),
        topic_name: Some("Deep Evolution Hierarchy".to_string()),
        parent_node_id: None,
        parent_new_status: None,
        new_node: root_node,
        unexplored_siblings: vec![],
    };
    service
        .commit_memory_topic_evolution(payload)
        .await
        .unwrap();

    // Commit 30 consecutive generations of continued nodes
    let mut prev_id = "sess0#T1".to_string();
    for gen in 2..=31 {
        let child_node = MemoryTreeNode {
            id: format!("sess{}#T{}", gen, gen),
            parent_id: Some(prev_id.clone()),
            topic_id: topic_id.to_string(),
            summary: format!("Generation {} child continuation", gen),
            status: NodeStatus::Completed,
            source_session_id: format!("session-{}", gen),
            turn_index: gen as u32,
            turn_started_at: None,
            created_at: "2026-11-02T10:00:00Z".to_string(),
            updated_at: "2026-11-02T10:00:00Z".to_string(),
        };

        let payload = TopicEvolutionCommitPayload {
            project_slug: project_slug.to_string(),
            topic_id: topic_id.to_string(),
            topic_name: None,
            parent_node_id: Some(prev_id.clone()),
            parent_new_status: Some(NodeStatus::Continued),
            new_node: child_node,
            unexplored_siblings: vec![],
        };

        service
            .commit_memory_topic_evolution(payload)
            .await
            .unwrap();
        prev_id = format!("sess{}#T{}", gen, gen);
    }

    // Inspect final state
    let state = service
        .get_project_memory_state(project_slug)
        .await
        .unwrap()
        .expect("state exists");
    let tree = state.topic_trees.get(topic_id).expect("tree exists");

    // Total nodes: 31 (root + 30 children)
    assert_eq!(tree.nodes.len(), 31, "Total nodes must be 31");
    assert_eq!(
        tree.root_node_ids.len(),
        1,
        "Only root must be in root_node_ids"
    );
    assert_eq!(tree.root_node_ids[0], "sess0#T1");

    // Generations 0..29 must be Continued
    for gen in 0..=29 {
        let node_id = if gen == 0 {
            "sess0#T1".to_string()
        } else {
            format!("sess{}#T{}", gen + 1, gen + 1)
        };
        let n = tree
            .nodes
            .get(&node_id)
            .unwrap_or_else(|| panic!("missing node {}", node_id));
        assert_eq!(
            n.status,
            NodeStatus::Continued,
            "Node {} should have status Continued",
            node_id
        );
    }

    // Leaf (generation 30, turn 31) must be Completed
    let leaf = tree.nodes.get("sess31#T31").expect("leaf exists");
    assert_eq!(leaf.status, NodeStatus::Completed);
    assert_eq!(leaf.parent_id, Some("sess30#T30".to_string()));

    // Markdown projection must be generated without panic and contain the leaf node
    let proj_md = storage
        .read_markdown(format!("projects/{}.md", project_slug))
        .unwrap()
        .expect("markdown exists");
    assert!(proj_md.contains("[sess31#T31](aiwc://session/sess31/turn/31)"));
    // Continued nodes should NOT be listed in Completed tasks
    assert!(!proj_md.contains("[sess0#T1](aiwc://session/sess0/turn/1)"));
}

#[tokio::test]
async fn test_adv_commit_with_nonexistent_parent_id() {
    let _guard = TEST_MUTEX.lock().await;
    let harness = TestHarness::new("nonexistent-parent");
    let service = harness.create_service().await;

    let project_slug = "phantom-parent-test";
    let topic_id = "phantom-topic";

    let orphan_node = MemoryTreeNode {
        id: "s1#T1".to_string(),
        parent_id: Some("phantom_parent_999".to_string()),
        topic_id: topic_id.to_string(),
        summary: "Orphan node claiming nonexistent parent".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "sess-orphan".to_string(),
        turn_index: 1,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };

    let payload = TopicEvolutionCommitPayload {
        project_slug: project_slug.to_string(),
        topic_id: topic_id.to_string(),
        topic_name: Some("Phantom Topic".to_string()),
        parent_node_id: Some("phantom_parent_999".to_string()),
        parent_new_status: Some(NodeStatus::Continued),
        new_node: orphan_node,
        unexplored_siblings: vec![],
    };

    // Commit with nonexistent parent must return an error
    let res = service.commit_memory_topic_evolution(payload).await;
    println!("Commit with nonexistent parent returned: {:?}", res);
    assert!(
        res.is_err(),
        "Commit with nonexistent parent must return an error"
    );
}

// =========================================================================
// 2. Adversarial Scenario 2: Sibling Branching & Continuation
// =========================================================================

#[tokio::test]
async fn test_adv_orthogonal_sibling_branching_and_subsequent_continuation() {
    let _guard = TEST_MUTEX.lock().await;
    let harness = TestHarness::new("sibling-branch");
    let service = harness.create_service().await;
    let tenant = service.tenant_id().to_string();
    let storage = harness.storage(&tenant);

    let project_slug = "branching-project";
    let topic_id = "branching-topic";

    // 1. Commit root with 3 orthogonal unexplored siblings
    let root_node = MemoryTreeNode {
        id: "s1#T1".to_string(),
        parent_id: None,
        topic_id: topic_id.to_string(),
        summary: "Root strategy".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "session-root".to_string(),
        turn_index: 1,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };

    let sib1 = MemoryTreeNode {
        id: "s1#T1.1".to_string(),
        parent_id: None,
        topic_id: topic_id.to_string(),
        summary: "Sibling 1: OAuth2".to_string(),
        status: NodeStatus::Unexplored,
        source_session_id: "session-root".to_string(),
        turn_index: 1,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };

    let sib2 = MemoryTreeNode {
        id: "s1#T1.2".to_string(),
        parent_id: None,
        topic_id: topic_id.to_string(),
        summary: "Sibling 2: SAML SSO".to_string(),
        status: NodeStatus::Unexplored,
        source_session_id: "session-root".to_string(),
        turn_index: 1,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };

    let sib3 = MemoryTreeNode {
        id: "s1#T1.3".to_string(),
        parent_id: None,
        topic_id: topic_id.to_string(),
        summary: "Sibling 3: Passkeys / WebAuthn".to_string(),
        status: NodeStatus::Unexplored,
        source_session_id: "session-root".to_string(),
        turn_index: 1,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };

    let payload = TopicEvolutionCommitPayload {
        project_slug: project_slug.to_string(),
        topic_id: topic_id.to_string(),
        topic_name: Some("Authentication System Architecture".to_string()),
        parent_node_id: None,
        parent_new_status: None,
        new_node: root_node,
        unexplored_siblings: vec![sib1, sib2, sib3],
    };

    let res = service
        .commit_memory_topic_evolution(payload)
        .await
        .unwrap();
    assert_eq!(res.unexplored_node_ids.len(), 3);

    // Verify initial markdown has 3 unexplored siblings
    let md1 = storage
        .read_markdown(format!("projects/{}.md", project_slug))
        .unwrap()
        .unwrap();
    assert!(md1.contains("[s1#T1.1]"));
    assert!(md1.contains("[s1#T1.2]"));
    assert!(md1.contains("[s1#T1.3]"));

    // 2. Subsequent commit: continue sibling 1 (OAuth2), marking it Continued, and adding sub-siblings
    let oauth_child = MemoryTreeNode {
        id: "s2#T2".to_string(),
        parent_id: Some("s1#T1.1".to_string()),
        topic_id: topic_id.to_string(),
        summary: "OAuth2 GitHub provider implemented".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "session-oauth".to_string(),
        turn_index: 2,
        turn_started_at: None,
        created_at: "2026-11-02T11:00:00Z".to_string(),
        updated_at: "2026-11-02T11:00:00Z".to_string(),
    };

    let oauth_sub_sib = MemoryTreeNode {
        id: "s2#T2.1".to_string(),
        parent_id: Some("s1#T1.1".to_string()),
        topic_id: topic_id.to_string(),
        summary: "OAuth2 Google provider pending".to_string(),
        status: NodeStatus::Unexplored,
        source_session_id: "session-oauth".to_string(),
        turn_index: 2,
        turn_started_at: None,
        created_at: "2026-11-02T11:00:00Z".to_string(),
        updated_at: "2026-11-02T11:00:00Z".to_string(),
    };

    let payload2 = TopicEvolutionCommitPayload {
        project_slug: project_slug.to_string(),
        topic_id: topic_id.to_string(),
        topic_name: None,
        parent_node_id: Some("s1#T1.1".to_string()),
        parent_new_status: Some(NodeStatus::Continued),
        new_node: oauth_child,
        unexplored_siblings: vec![oauth_sub_sib],
    };

    service
        .commit_memory_topic_evolution(payload2)
        .await
        .unwrap();

    let state = service
        .get_project_memory_state(project_slug)
        .await
        .unwrap()
        .unwrap();
    let tree = state.topic_trees.get(topic_id).unwrap();

    // Sibling 1 must now be Continued
    assert_eq!(
        tree.nodes.get("s1#T1.1").unwrap().status,
        NodeStatus::Continued
    );
    // Siblings 2 and 3 must still be Unexplored
    assert_eq!(
        tree.nodes.get("s1#T1.2").unwrap().status,
        NodeStatus::Unexplored
    );
    assert_eq!(
        tree.nodes.get("s1#T1.3").unwrap().status,
        NodeStatus::Unexplored
    );
    // Sub-sibling must be Unexplored
    assert_eq!(
        tree.nodes.get("s2#T2.1").unwrap().status,
        NodeStatus::Unexplored
    );
    // New node must be Completed
    assert_eq!(
        tree.nodes.get("s2#T2").unwrap().status,
        NodeStatus::Completed
    );

    // Markdown check
    let md2 = storage
        .read_markdown(format!("projects/{}.md", project_slug))
        .unwrap()
        .unwrap();
    // s1#T1.1 should NOT be in unexplored branches anymore
    let unexplored_section = md2.split("## ✅ 已完成的闭环任务").next().unwrap();
    assert!(!unexplored_section.contains("[s1#T1.1]"));
    assert!(unexplored_section.contains("[s1#T1.2]"));
    assert!(unexplored_section.contains("[s1#T1.3]"));
    assert!(unexplored_section.contains("[s2#T2.1]"));
}

#[tokio::test]
async fn test_adv_recommitting_parent_overwrites_continued_sibling() {
    let _guard = TEST_MUTEX.lock().await;
    let harness = TestHarness::new("recommit-parent");
    let service = harness.create_service().await;

    let project_slug = "recommit-test";
    let topic_id = "recommit-topic";

    // Turn 1: Commit root with unexplored sibling S1
    let root = MemoryTreeNode {
        id: "r1#T1".to_string(),
        parent_id: None,
        topic_id: topic_id.to_string(),
        summary: "Root".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "s1".to_string(),
        turn_index: 1,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };
    let s1 = MemoryTreeNode {
        id: "r1#T1.1".to_string(),
        parent_id: None,
        topic_id: topic_id.to_string(),
        summary: "Sibling 1".to_string(),
        status: NodeStatus::Unexplored,
        source_session_id: "s1".to_string(),
        turn_index: 1,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };
    let payload1 = TopicEvolutionCommitPayload {
        project_slug: project_slug.to_string(),
        topic_id: topic_id.to_string(),
        topic_name: Some("Recommit Topic".to_string()),
        parent_node_id: None,
        parent_new_status: None,
        new_node: root.clone(),
        unexplored_siblings: vec![s1.clone()],
    };
    service
        .commit_memory_topic_evolution(payload1.clone())
        .await
        .unwrap();

    // Turn 2: Continue sibling S1
    let child = MemoryTreeNode {
        id: "r2#T2".to_string(),
        parent_id: Some("r1#T1.1".to_string()),
        topic_id: topic_id.to_string(),
        summary: "Continuing S1".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "s2".to_string(),
        turn_index: 2,
        turn_started_at: None,
        created_at: "2026-11-02T11:00:00Z".to_string(),
        updated_at: "2026-11-02T11:00:00Z".to_string(),
    };
    let payload2 = TopicEvolutionCommitPayload {
        project_slug: project_slug.to_string(),
        topic_id: topic_id.to_string(),
        topic_name: None,
        parent_node_id: Some("r1#T1.1".to_string()),
        parent_new_status: Some(NodeStatus::Continued),
        new_node: child,
        unexplored_siblings: vec![],
    };
    service
        .commit_memory_topic_evolution(payload2)
        .await
        .unwrap();

    // Check that S1 is Continued
    let state = service
        .get_project_memory_state(project_slug)
        .await
        .unwrap()
        .unwrap();
    let tree = state.topic_trees.get(topic_id).unwrap();
    assert_eq!(
        tree.nodes.get("r1#T1.1").unwrap().status,
        NodeStatus::Continued
    );

    // Turn 1 retry / re-commit: Re-commit original payload1
    service
        .commit_memory_topic_evolution(payload1)
        .await
        .unwrap();

    // Check S1 status again
    let state2 = service
        .get_project_memory_state(project_slug)
        .await
        .unwrap()
        .unwrap();
    let tree2 = state2.topic_trees.get(topic_id).unwrap();
    let s1_status_after_recommit = tree2.nodes.get("r1#T1.1").unwrap().status;
    println!(
        "S1 status after re-committing parent: {:?}",
        s1_status_after_recommit
    );

    // S1 should remain Continued, not be reverted to Unexplored!
    assert_eq!(
        s1_status_after_recommit,
        NodeStatus::Continued,
        "BUG FOUND: Re-committing parent payload reverts already-continued sibling back to Unexplored!"
    );
}

// =========================================================================
// 3. Adversarial Scenario 3: Watermark Monotonicity & Out-of-order Commits
// =========================================================================

#[tokio::test]
async fn test_adv_watermark_out_of_order_commit_and_advance() {
    let _guard = TEST_MUTEX.lock().await;
    let harness = TestHarness::new("watermark-order");
    let service = harness.create_service().await;

    let project_slug = "watermark-test";
    let session_id = "session-wm-order";

    // 1. First commit turn 3
    let node_turn_3 = MemoryTreeNode {
        id: "wm#T3".to_string(),
        parent_id: None,
        topic_id: "wm-topic".to_string(),
        summary: "Turn 3 work".to_string(),
        status: NodeStatus::Completed,
        source_session_id: session_id.to_string(),
        turn_index: 3,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };

    let payload3 = TopicEvolutionCommitPayload {
        project_slug: project_slug.to_string(),
        topic_id: "wm-topic".to_string(),
        topic_name: Some("Watermark Topic".to_string()),
        parent_node_id: None,
        parent_new_status: None,
        new_node: node_turn_3,
        unexplored_siblings: vec![],
    };

    service
        .commit_memory_topic_evolution(payload3)
        .await
        .unwrap();

    let wm3 = service
        .check_memory_session_watermark(project_slug, session_id)
        .await
        .unwrap();
    assert_eq!(wm3, Some(3), "Watermark must be 3 after turn 3");

    // 2. Out-of-order commit: now commit turn 2 for the same session!
    let node_turn_2 = MemoryTreeNode {
        id: "wm#T2".to_string(),
        parent_id: None,
        topic_id: "wm-topic".to_string(),
        summary: "Turn 2 late work".to_string(),
        status: NodeStatus::Completed,
        source_session_id: session_id.to_string(),
        turn_index: 2,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };

    let payload2 = TopicEvolutionCommitPayload {
        project_slug: project_slug.to_string(),
        topic_id: "wm-topic".to_string(),
        topic_name: None,
        parent_node_id: None,
        parent_new_status: None,
        new_node: node_turn_2,
        unexplored_siblings: vec![],
    };

    service
        .commit_memory_topic_evolution(payload2)
        .await
        .unwrap();

    let wm_after_turn_2 = service
        .check_memory_session_watermark(project_slug, session_id)
        .await
        .unwrap();

    println!(
        "Watermark after committing turn 3 then turn 2: {:?}",
        wm_after_turn_2
    );

    // CRITICAL ADVERSARIAL ASSERTION:
    // Watermark MUST preserve highest (3), not regress to 2!
    assert_eq!(
        wm_after_turn_2,
        Some(3),
        "BUG FOUND: Watermark regressed from 3 to 2 after out-of-order turn commit!"
    );
}

#[tokio::test]
async fn test_adv_advance_watermark_monotonicity() {
    let _guard = TEST_MUTEX.lock().await;
    let harness = TestHarness::new("advance-monotonic");
    let service = harness.create_service().await;

    let project_slug = "wm-monotonic";
    let session_id = "session-advance";

    // Set watermark to turn 10
    service
        .advance_memory_session_watermark(project_slug, session_id, 10)
        .await
        .unwrap();

    let wm = service
        .check_memory_session_watermark(project_slug, session_id)
        .await
        .unwrap();
    assert_eq!(wm, Some(10));

    // Try to "advance" to turn 5 (regressive)
    service
        .advance_memory_session_watermark(project_slug, session_id, 5)
        .await
        .unwrap();

    let wm_after = service
        .check_memory_session_watermark(project_slug, session_id)
        .await
        .unwrap();

    println!("Watermark after advancing 10 then 5: {:?}", wm_after);

    // CRITICAL ADVERSARIAL ASSERTION:
    // advance_memory_session_watermark should not regress!
    assert_eq!(
        wm_after,
        Some(10),
        "BUG FOUND: advance_memory_session_watermark regressed from 10 to 5!"
    );
}

// =========================================================================
// 4. Adversarial Scenario 4: Path Traversal, Special Chars, Chinese & Panics
// =========================================================================

#[tokio::test]
async fn test_adv_project_slug_with_path_separators() {
    let _guard = TEST_MUTEX.lock().await;
    let harness = TestHarness::new("slug-separator");
    let service = harness.create_service().await;

    // A slug containing path separators like "frontend/components" or "a/b"
    let project_slug = "sub/dir/project";
    let topic_id = "test-topic";

    let node = MemoryTreeNode {
        id: "s1#T1".to_string(),
        parent_id: None,
        topic_id: topic_id.to_string(),
        summary: "Testing slug with path separator".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "session-path".to_string(),
        turn_index: 1,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };

    let payload = TopicEvolutionCommitPayload {
        project_slug: project_slug.to_string(),
        topic_id: topic_id.to_string(),
        topic_name: Some("Topic".to_string()),
        parent_node_id: None,
        parent_new_status: None,
        new_node: node,
        unexplored_siblings: vec![],
    };

    let res = service.commit_memory_topic_evolution(payload).await;
    println!("Commit with slug 'sub/dir/project' returned: {:?}", res);
    assert!(
        res.is_ok(),
        "Commit with slug containing path separators must succeed: {:?}",
        res
    );
}

#[tokio::test]
async fn test_adv_chinese_characters_and_markdown_special_chars() {
    let _guard = TEST_MUTEX.lock().await;
    let harness = TestHarness::new("chinese-chars");
    let service = harness.create_service().await;
    let tenant = service.tenant_id().to_string();
    let storage = harness.storage(&tenant);

    let project_slug = "智能资产编织-项目";
    let topic_id = "记忆体系重构#1";
    let topic_name = "多叉树状态继承 [ADR-0019]";

    let node = MemoryTreeNode {
        id: "zh1#T1".to_string(),
        parent_id: None,
        topic_id: topic_id.to_string(),
        summary: "实现 ACP 纯文本推理与 `.state/` 继承树模型 (带特殊字符: <script>&*#)".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "session-zh-001".to_string(),
        turn_index: 1,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };

    let payload = TopicEvolutionCommitPayload {
        project_slug: project_slug.to_string(),
        topic_id: topic_id.to_string(),
        topic_name: Some(topic_name.to_string()),
        parent_node_id: None,
        parent_new_status: None,
        new_node: node,
        unexplored_siblings: vec![],
    };

    let res = service.commit_memory_topic_evolution(payload).await;
    assert!(
        res.is_ok(),
        "Chinese slug and topic must commit successfully: {:?}",
        res
    );

    let state = service
        .get_project_memory_state(project_slug)
        .await
        .unwrap()
        .expect("state exists");
    assert_eq!(state.project_slug, project_slug);
    assert!(state.topic_trees.contains_key(topic_id));

    let md = storage
        .read_markdown(format!("projects/{}.md", project_slug))
        .unwrap()
        .expect("md exists");
    assert!(md.contains("智能资产编织-项目"));
    assert!(
        md.contains("多叉树状态继承"),
        "Project markdown must contain rendered topic name"
    );
    assert!(
        md.contains("### 🏷️ Topic: 多叉树状态继承 [ADR-0019]"),
        "Project markdown must render formatted topic header"
    );
}

#[test]
fn test_adv_short_id_utf8_char_boundary_panic() {
    // If a node ID doesn't parse via parse_id (e.g. no #T delimiter)
    // and source_session_id has multi-byte UTF-8 characters where byte index 8 is not a boundary:
    let node = MemoryTreeNode {
        id: "nonstandard_node_id".to_string(),
        parent_id: None,
        topic_id: "test".to_string(),
        summary: "Test node".to_string(),
        status: NodeStatus::Completed,
        // "会话测试-123":
        // '会' = 3 bytes (0..3)
        // '话' = 3 bytes (3..6)
        // '测' = 3 bytes (6..9) -> byte 8 is inside '测'!
        source_session_id: "会话测试-123".to_string(),
        turn_index: 1,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };

    // Calling short_id(): will it panic or safely extract?
    let result = std::panic::catch_unwind(|| node.short_id());

    println!("node.short_id() result: {:?}", result);
    // If it panics, that's an empirical bug reproduction!
    assert!(
        result.is_ok(),
        "BUG FOUND: MemoryTreeNode::short_id() panics on non-ASCII source_session_id slicing [..8]!"
    );
}

// =========================================================================
// 5. Adversarial Scenario 5: Malformed JSON in State Files
// =========================================================================

#[tokio::test]
async fn test_adv_malformed_project_state_json_returns_error_not_panic() {
    let _guard = TEST_MUTEX.lock().await;
    let harness = TestHarness::new("malformed-proj");
    let service = harness.create_service().await;
    let tenant = service.tenant_id().to_string();
    let storage = harness.storage(&tenant);

    storage.init_dirs().unwrap();
    let project_file = storage.state_project_file("corrupted-slug");

    // Write malformed JSON
    fs::write(&project_file, "{ this is not valid json !!!").unwrap();

    // 1. Reading state must return Err, not panic
    let read_res = storage.read_project_state("corrupted-slug");
    assert!(read_res.is_err(), "Must return error for malformed JSON");

    // 2. AppService get_project_memory_state must return Err, not panic
    let srv_res = service.get_project_memory_state("corrupted-slug").await;
    assert!(
        srv_res.is_err(),
        "AppService must return Err for malformed JSON"
    );

    // 3. AppService commit_memory_topic_evolution when state is malformed
    let node = MemoryTreeNode {
        id: "s1#T1".to_string(),
        parent_id: None,
        topic_id: "t1".to_string(),
        summary: "test".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "sess1".to_string(),
        turn_index: 1,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };
    let payload = TopicEvolutionCommitPayload {
        project_slug: "corrupted-slug".to_string(),
        topic_id: "t1".to_string(),
        topic_name: None,
        parent_node_id: None,
        parent_new_status: None,
        new_node: node,
        unexplored_siblings: vec![],
    };

    let commit_res = service.commit_memory_topic_evolution(payload).await;
    assert!(
        commit_res.is_err(),
        "AppService commit must return Err when state file is corrupted, not panic or overwrite blindly"
    );
}

#[tokio::test]
async fn test_adv_corrupted_recent_json_returns_error_not_panic() {
    let _guard = TEST_MUTEX.lock().await;
    let harness = TestHarness::new("malformed-recent");
    let service = harness.create_service().await;
    let tenant = service.tenant_id().to_string();
    let storage = harness.storage(&tenant);

    storage.init_dirs().unwrap();
    let recent_file = storage.state_recent_file();

    // Write truncated JSON
    fs::write(&recent_file, r#"{"projects": {"test": {"project_slug": "#).unwrap();

    let read_res = storage.read_recent_state();
    assert!(
        read_res.is_err(),
        "Must return error for malformed recent JSON"
    );

    let srv_res = service.get_recent_memory_state().await;
    assert!(
        srv_res.is_err(),
        "AppService must return Err for malformed recent JSON"
    );
}

#[tokio::test]
async fn test_adv_empty_file_project_state_returns_error_not_panic() {
    let _guard = TEST_MUTEX.lock().await;
    let harness = TestHarness::new("empty-proj");
    let service = harness.create_service().await;
    let tenant = service.tenant_id().to_string();
    let storage = harness.storage(&tenant);

    storage.init_dirs().unwrap();
    let project_file = storage.state_project_file("empty-slug");

    // 0-byte file
    fs::write(&project_file, "").unwrap();

    let read_res = storage.read_project_state("empty-slug");
    assert!(
        read_res.is_err(),
        "Empty file must return error (EOF while parsing)"
    );
}
