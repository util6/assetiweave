use super::*;
use crate::backend::domain::memory::{MemoryTreeNode, NodeStatus, TopicTree};
use uuid::Uuid;

fn make_test_node(
    id: &str,
    status: NodeStatus,
    ts: Option<String>,
    summary: &str,
) -> MemoryTreeNode {
    let now = ts
        .clone()
        .unwrap_or_else(|| "2026-11-02T10:00:00Z".to_string());
    MemoryTreeNode {
        id: id.to_string(),
        parent_id: None,
        topic_id: "t1".to_string(),
        summary: summary.to_string(),
        status,
        source_session_id: "s1".to_string(),
        turn_index: 1,
        turn_started_at: ts,
        created_at: now.clone(),
        updated_at: now,
    }
}

#[tokio::test]
async fn test_state_service_commit_evolution_and_watermarks() {
    let tmp = std::env::temp_dir().join(format!("assetiweave-state-srv-{}", Uuid::new_v4()));
    let db_path = tmp.join("db").join("app.db");
    let mem_dir = tmp.join("memories");
    std::fs::create_dir_all(tmp.join("db")).unwrap();
    std::fs::create_dir_all(&mem_dir).unwrap();

    std::env::set_var("ASSETIWEAVE_MEMORY_ROOT", &mem_dir);

    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let tenant_id = service.tenant_id();
    let storage = MemoryStorage::new_with_root(mem_dir.clone(), tenant_id);

    // 1. Watermark initially None
    assert_eq!(
        service
            .check_memory_session_watermark("test-proj", "session-100")
            .await
            .unwrap(),
        None
    );

    // 2. Advance watermark independently
    service
        .advance_memory_session_watermark("test-proj", "session-100", 3)
        .await
        .unwrap();
    assert_eq!(
        service
            .check_memory_session_watermark("test-proj", "session-100")
            .await
            .unwrap(),
        Some(3)
    );

    // 3. Commit topic evolution
    let root_node = MemoryTreeNode {
        id: "s1#T1".to_string(),
        parent_id: None,
        topic_id: "compiler".to_string(),
        summary: "Build syntax parser".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "session-100".to_string(),
        turn_index: 4,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };
    let unexplored_sib = MemoryTreeNode {
        id: "s1#T1.1".to_string(),
        parent_id: None,
        topic_id: "compiler".to_string(),
        summary: "Explore LLVM JIT backend".to_string(),
        status: NodeStatus::Unexplored,
        source_session_id: "session-100".to_string(),
        turn_index: 4,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };

    let payload = TopicEvolutionCommitPayload {
        project_slug: "test-proj".to_string(),
        topic_id: "compiler".to_string(),
        topic_name: Some("Compiler Core".to_string()),
        parent_node_id: None,
        parent_new_status: None,
        new_node: root_node,
        unexplored_siblings: vec![unexplored_sib],
    };

    let result = service
        .commit_memory_topic_evolution(payload)
        .await
        .unwrap();
    assert_eq!(result.committed_node_id, "s1#T1");
    assert_eq!(result.unexplored_node_ids, vec!["s1#T1.1".to_string()]);

    assert_eq!(
        service
            .check_memory_session_watermark("test-proj", "session-100")
            .await
            .unwrap(),
        Some(4)
    );

    let proj_state = service
        .get_project_memory_state("test-proj")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(proj_state.project_slug, "test-proj");
    assert_eq!(proj_state.get_watermark("session-100"), Some(4));
    let committed_node = proj_state
        .topic_trees
        .get("compiler")
        .unwrap()
        .nodes
        .get("s1#T1")
        .unwrap();
    assert!(committed_node.turn_started_at.is_some());

    let proj_md = storage
        .read_markdown("projects/test-proj.md")
        .unwrap()
        .unwrap();
    assert!(proj_md.contains("# 项目记忆: test-proj"));
    assert!(proj_md.contains("[s1#T1](aiwc://session/s1/turn/4)"));
    assert!(proj_md.contains("[s1#T1.1](aiwc://session/s1/turn/4)"));

    let recent_state = service.get_recent_memory_state().await.unwrap().unwrap();
    assert!(recent_state.projects.contains_key("test-proj"));

    let recent_md = storage.read_markdown("recent/latest.md").unwrap().unwrap();
    assert!(recent_md.contains("Recent Work"));
    assert!(recent_md.contains("## 项目: test-proj"));

    std::fs::remove_dir_all(tmp).ok();
}

#[tokio::test]
async fn test_auto_fill_turn_started_at_from_sqlite() {
    let tmp = std::env::temp_dir().join(format!("assetiweave-autofill-{}", Uuid::new_v4()));
    let db_path = tmp.join("db").join("app.db");
    let mem_dir = tmp.join("memories");
    std::fs::create_dir_all(tmp.join("db")).unwrap();
    std::fs::create_dir_all(&mem_dir).unwrap();

    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let tenant_id = service.tenant_id();
    let storage = MemoryStorage::new_with_root(mem_dir.clone(), tenant_id);

    sqlx::query(
        "INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, title, started_at, updated_at, source_locator, source_fingerprint, missing, created_at, imported_at) \
         VALUES (?1, 'sess-autofill-1', 'src-1', 'claude-desktop', 'ext-1', 'Autofill Test', '2026-10-24T10:00:00Z', '2026-10-24T10:00:00Z', '', '', 0, '2026-10-24T10:00:00Z', '2026-10-24T10:00:00Z')",
    )
    .bind(tenant_id)
    .execute(service.db.pool())
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO conversation_turns (tenant_id, id, session_id, external_id, turn_index, started_at, fingerprint, missing, imported_at) \
         VALUES (?1, 'turn-autofill-1', 'sess-autofill-1', 'turn-ext-1', 1, '2026-10-24T10:05:00Z', '', 0, '2026-10-24T10:05:00Z')",
    )
    .bind(tenant_id)
    .execute(service.db.pool())
    .await
    .unwrap();

    let mut node1 = make_test_node("sess1#T1", NodeStatus::Completed, None, "Turn 1 summary");
    node1.source_session_id = "sess-autofill-1".to_string();
    node1.turn_index = 1;

    let mut sib1 = make_test_node(
        "sess1#T1.1",
        NodeStatus::Unexplored,
        None,
        "Sibling 1 summary",
    );
    sib1.source_session_id = "sess-autofill-1".to_string();
    sib1.turn_index = 2; // Not in turns, falls back to session

    let payload = TopicEvolutionCommitPayload {
        project_slug: "proj-autofill".to_string(),
        topic_id: "t1".to_string(),
        topic_name: Some("Autofill".to_string()),
        parent_node_id: None,
        parent_new_status: None,
        new_node: node1,
        unexplored_siblings: vec![sib1],
    };

    service
        .commit_memory_topic_evolution_to(payload, &storage)
        .await
        .unwrap();

    let state = service
        .get_project_memory_state_in("proj-autofill", &storage)
        .await
        .unwrap()
        .unwrap();
    let tree = state.topic_trees.get("t1").unwrap();
    let n1 = tree.nodes.get("sess1#T1").unwrap();
    let s1 = tree.nodes.get("sess1#T1.1").unwrap();

    assert_eq!(n1.turn_started_at.as_deref(), Some("2026-10-24T10:05:00Z"));
    assert_eq!(s1.turn_started_at.as_deref(), Some("2026-10-24T10:00:00Z"));

    std::fs::remove_dir_all(tmp).ok();
}

#[tokio::test]
async fn test_windowing_recent_slice_and_cutoff_behavior() {
    let tmp = std::env::temp_dir().join(format!("assetiweave-cutoff-{}", Uuid::new_v4()));
    let db_path = tmp.join("db").join("app.db");
    let mem_dir = tmp.join("memories");
    std::fs::create_dir_all(tmp.join("db")).unwrap();
    std::fs::create_dir_all(&mem_dir).unwrap();

    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let tenant_id = service.tenant_id();
    let storage = MemoryStorage::new_with_root(mem_dir.clone(), tenant_id);

    let now = chrono::Utc::now();
    let ts_100h_ago = (now - chrono::Duration::hours(100)).to_rfc3339();
    let ts_36h_ago = (now - chrono::Duration::hours(36)).to_rfc3339();
    let ts_10h_ago = (now - chrono::Duration::hours(10)).to_rfc3339();
    let ts_now = now.to_rfc3339();

    // Project 1: has all 4 types of nodes
    let mut proj1 = ProjectMemoryState::new("proj-main");
    let mut tree1 = TopicTree::new("t1", "Topic Main");

    let unex_100h = make_test_node(
        "node#unex-100h",
        NodeStatus::Unexplored,
        Some(ts_100h_ago),
        "100h unex",
    );
    let comp_36h = make_test_node(
        "node#comp-36h",
        NodeStatus::Completed,
        Some(ts_36h_ago.clone()),
        "36h comp",
    );
    let comp_10h = make_test_node(
        "node#comp-10h",
        NodeStatus::Completed,
        Some(ts_10h_ago),
        "10h comp",
    );
    let dismissed_node = make_test_node(
        "node#dismissed",
        NodeStatus::Dismissed,
        Some(ts_now),
        "Dismissed",
    );

    tree1.nodes.insert(unex_100h.id.clone(), unex_100h);
    tree1.nodes.insert(comp_36h.id.clone(), comp_36h);
    tree1.nodes.insert(comp_10h.id.clone(), comp_10h);
    tree1
        .nodes
        .insert(dismissed_node.id.clone(), dismissed_node);
    proj1.topic_trees.insert("t1".to_string(), tree1);

    storage
        .write_project_state_atomic("proj-main", &proj1)
        .unwrap();
    sync_project_markdown(&storage, &proj1).unwrap();

    // Project 2: ONLY has a 36h Completed node (for project-level pruning verification)
    let mut proj2 = ProjectMemoryState::new("proj-old-only");
    let mut tree2 = TopicTree::new("t2", "Topic Old Only");
    let comp_old_only = make_test_node(
        "node#old-only",
        NodeStatus::Completed,
        Some(ts_36h_ago),
        "Only 36h",
    );
    tree2.nodes.insert(comp_old_only.id.clone(), comp_old_only);
    proj2.topic_trees.insert("t2".to_string(), tree2);

    storage
        .write_project_state_atomic("proj-old-only", &proj2)
        .unwrap();
    sync_project_markdown(&storage, &proj2).unwrap();

    // Verify 1: Project Markdown for proj-main excludes Dismissed but keeps Unexplored and Completed
    let main_md = storage
        .read_markdown("projects/proj-main.md")
        .unwrap()
        .unwrap();
    assert!(main_md.contains("node#unex-100h"));
    assert!(main_md.contains("node#comp-36h"));
    assert!(main_md.contains("node#comp-10h"));
    assert!(!main_md.contains("node#dismissed"));

    // Verify 2: .state/projects/proj-main.json STILL contains dismissed node
    let proj_json = storage.read_project_state("proj-main").unwrap().unwrap();
    assert!(proj_json
        .topic_trees
        .get("t1")
        .unwrap()
        .nodes
        .contains_key("node#dismissed"));

    // Verify 3: 24h window
    let recent_24h = service
        .refresh_recent_memory_state_in(Some("24h"), &storage)
        .await
        .unwrap();
    let recent_md_24h = storage.read_markdown("recent/latest.md").unwrap().unwrap();

    assert!(recent_24h.projects.contains_key("proj-main"));
    let nodes_24 = &recent_24h
        .projects
        .get("proj-main")
        .unwrap()
        .topic_trees
        .get("t1")
        .unwrap()
        .nodes;
    assert!(nodes_24.contains_key("node#unex-100h"));
    assert!(nodes_24.contains_key("node#comp-10h"));
    assert!(!nodes_24.contains_key("node#comp-36h"));
    assert!(!nodes_24.contains_key("node#dismissed"));

    assert!(recent_md_24h.contains("node#unex-100h"));
    assert!(recent_md_24h.contains("node#comp-10h"));
    assert!(!recent_md_24h.contains("node#comp-36h"));
    assert!(!recent_md_24h.contains("node#dismissed"));

    // Project-level pruning: proj-old-only has 0 nodes within 24h -> completely pruned!
    assert!(!recent_24h.projects.contains_key("proj-old-only"));
    assert!(!recent_md_24h.contains("proj-old-only"));

    // Verify 4: 72h window
    let recent_72h = service
        .refresh_recent_memory_state_in(Some("72h"), &storage)
        .await
        .unwrap();
    let recent_md_72h = storage.read_markdown("recent/latest.md").unwrap().unwrap();

    let nodes_72 = &recent_72h
        .projects
        .get("proj-main")
        .unwrap()
        .topic_trees
        .get("t1")
        .unwrap()
        .nodes;
    assert!(nodes_72.contains_key("node#unex-100h"));
    assert!(nodes_72.contains_key("node#comp-10h"));
    assert!(nodes_72.contains_key("node#comp-36h"));
    assert!(!nodes_72.contains_key("node#dismissed"));

    assert!(recent_md_72h.contains("node#unex-100h"));
    assert!(recent_md_72h.contains("node#comp-10h"));
    assert!(recent_md_72h.contains("node#comp-36h"));
    assert!(!recent_md_72h.contains("node#dismissed"));

    assert!(recent_72h.projects.contains_key("proj-old-only"));
    assert!(recent_md_72h.contains("proj-old-only"));

    std::fs::remove_dir_all(tmp).ok();
}

#[tokio::test]
async fn test_update_memory_node_status_explicit_and_scan() {
    let tmp = std::env::temp_dir().join(format!("assetiweave-status-{}", Uuid::new_v4()));
    let db_path = tmp.join("db").join("app.db");
    let mem_dir = tmp.join("memories");
    std::fs::create_dir_all(tmp.join("db")).unwrap();
    std::fs::create_dir_all(&mem_dir).unwrap();

    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let tenant_id = service.tenant_id();
    let storage = MemoryStorage::new_with_root(mem_dir.clone(), tenant_id);

    let mut proj = ProjectMemoryState::new("proj-target");
    let mut tree = TopicTree::new("topic-1", "Topic One");
    let mut node = make_test_node(
        "target#node-1",
        NodeStatus::Unexplored,
        Some("2026-11-04T00:00:00Z".to_string()),
        "Target node to update",
    );
    node.topic_id = "topic-1".to_string();
    tree.nodes.insert(node.id.clone(), node);
    proj.topic_trees.insert("topic-1".to_string(), tree);
    storage
        .write_project_state_atomic("proj-target", &proj)
        .unwrap();

    // 1. Update with explicit project_slug to Completed
    let updated = service
        .update_memory_node_status_in(
            "target#node-1",
            NodeStatus::Completed,
            Some("proj-target"),
            Some("48h"),
            &storage,
        )
        .await
        .unwrap();
    assert_eq!(updated.status, NodeStatus::Completed);

    let proj_state = service
        .get_project_memory_state_in("proj-target", &storage)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        proj_state
            .topic_trees
            .get("topic-1")
            .unwrap()
            .nodes
            .get("target#node-1")
            .unwrap()
            .status,
        NodeStatus::Completed
    );

    // 2. Update with project_slug = None (auto-scan) to Dismissed
    let updated2 = service
        .update_memory_node_status_in(
            "target#node-1",
            NodeStatus::Dismissed,
            None,
            Some("48h"),
            &storage,
        )
        .await
        .unwrap();
    assert_eq!(updated2.status, NodeStatus::Dismissed);

    // Markdown must have excluded the dismissed node
    let proj_md = storage
        .read_markdown("projects/proj-target.md")
        .unwrap()
        .unwrap();
    assert!(!proj_md.contains("target#node-1"));

    // Recent should be empty because only node was dismissed
    let recent = service
        .get_recent_memory_state_in(&storage)
        .await
        .unwrap()
        .unwrap();
    assert!(!recent.projects.contains_key("proj-target"));

    // 3. Not found returns error
    let err = service
        .update_memory_node_status_in(
            "non-existent-node",
            NodeStatus::Completed,
            None,
            None,
            &storage,
        )
        .await;
    assert!(err.is_err());

    std::fs::remove_dir_all(tmp).ok();
}
