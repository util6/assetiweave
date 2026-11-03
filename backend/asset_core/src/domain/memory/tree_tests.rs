use super::*;

#[test]
fn test_node_status_conversions() {
    assert_eq!(NodeStatus::Completed.as_str(), "completed");
    assert_eq!(NodeStatus::Continued.as_str(), "continued");
    assert_eq!(NodeStatus::Unexplored.as_str(), "unexplored");

    assert_eq!(NodeStatus::Completed.to_string(), "completed");
    assert_eq!(
        "completed".parse::<NodeStatus>().unwrap(),
        NodeStatus::Completed
    );
    assert_eq!(
        "continued".parse::<NodeStatus>().unwrap(),
        NodeStatus::Continued
    );
    assert_eq!(
        "unexplored".parse::<NodeStatus>().unwrap(),
        NodeStatus::Unexplored
    );
    assert!("invalid".parse::<NodeStatus>().is_err());
}

#[test]
fn test_node_id_format_and_parse() {
    let id1 = MemoryTreeNode::format_id("098b0c4b", 13, None);
    assert_eq!(id1, "098b0c4b#T13");

    let parsed1 = MemoryTreeNode::parse_id(&id1).expect("parse id1");
    assert_eq!(parsed1, ("098b0c4b".to_string(), 13, None));

    let id2 = MemoryTreeNode::format_id("098b0c4b", 13, Some(2));
    assert_eq!(id2, "098b0c4b#T13.2");

    let parsed2 = MemoryTreeNode::parse_id(&id2).expect("parse id2");
    assert_eq!(parsed2, ("098b0c4b".to_string(), 13, Some(2)));

    assert!(MemoryTreeNode::parse_id("invalid_id").is_none());
    assert!(MemoryTreeNode::parse_id("short#T").is_none());
    assert!(MemoryTreeNode::parse_id("#T13").is_none());
}

#[test]
fn test_json_serde_roundtrip() {
    let node = MemoryTreeNode {
        id: "73c20273#T1".to_string(),
        parent_id: None,
        topic_id: "acp-migration".to_string(),
        summary: "Refactor ACP executor".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "session-uuid-12345678".to_string(),
        turn_index: 1,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };

    let json = serde_json::to_string(&node).expect("serialize node");
    assert!(json.contains("\"status\":\"completed\""));
    let deserialized: MemoryTreeNode = serde_json::from_str(&json).expect("deserialize node");
    assert_eq!(deserialized, node);
    assert_eq!(deserialized.short_id(), "73c20273");
}

#[test]
fn test_project_memory_state_apply_evolution_root_and_child() {
    let mut state = ProjectMemoryState::new("assetiweave");
    assert_eq!(state.project_slug, "assetiweave");
    assert!(state.topic_trees.is_empty());
    assert!(state.session_watermarks.is_empty());

    // 1. Commit root evolution turn
    let root_node = MemoryTreeNode {
        id: "s1#T1".to_string(),
        parent_id: None,
        topic_id: "auth-rewrite".to_string(),
        summary: "Design new auth pipeline".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "session-001".to_string(),
        turn_index: 1,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };

    let unexplored_sib = MemoryTreeNode {
        id: "s1#T1.1".to_string(),
        parent_id: None,
        topic_id: "auth-rewrite".to_string(),
        summary: "Investigate OAuth provider compatibility".to_string(),
        status: NodeStatus::Unexplored,
        source_session_id: "session-001".to_string(),
        turn_index: 1,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };

    let payload1 = TopicEvolutionCommitPayload {
        project_slug: "assetiweave".to_string(),
        topic_id: "auth-rewrite".to_string(),
        topic_name: Some("Authentication System Rewrite".to_string()),
        parent_node_id: None,
        parent_new_status: None,
        new_node: root_node,
        unexplored_siblings: vec![unexplored_sib],
    };

    let res1 = state.apply_evolution(payload1).expect("commit root");
    assert_eq!(res1.committed_node_id, "s1#T1");
    assert_eq!(res1.unexplored_node_ids, vec!["s1#T1.1".to_string()]);
    assert_eq!(state.get_watermark("session-001"), Some(1));

    let tree = state.topic_trees.get("auth-rewrite").expect("tree exists");
    assert_eq!(tree.topic_name, "Authentication System Rewrite");
    assert!(tree.root_node_ids.contains(&"s1#T1".to_string()));
    assert_eq!(tree.nodes.len(), 2);
    assert_eq!(
        tree.nodes.get("s1#T1").unwrap().status,
        NodeStatus::Completed
    );
    assert_eq!(
        tree.nodes.get("s1#T1.1").unwrap().status,
        NodeStatus::Unexplored
    );

    // 2. Commit child evolution extending the unexplored branch
    let child_node = MemoryTreeNode {
        id: "s2#T2".to_string(),
        parent_id: Some("s1#T1.1".to_string()),
        topic_id: "auth-rewrite".to_string(),
        summary: "Implemented OAuth Google provider".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "session-002".to_string(),
        turn_index: 2,
        created_at: "2026-11-02T11:00:00Z".to_string(),
        updated_at: "2026-11-02T11:00:00Z".to_string(),
    };

    let payload2 = TopicEvolutionCommitPayload {
        project_slug: "assetiweave".to_string(),
        topic_id: "auth-rewrite".to_string(),
        topic_name: None,
        parent_node_id: Some("s1#T1.1".to_string()),
        parent_new_status: Some(NodeStatus::Continued),
        new_node: child_node,
        unexplored_siblings: vec![],
    };

    let res2 = state.apply_evolution(payload2).expect("commit child");
    assert_eq!(res2.committed_node_id, "s2#T2");
    assert_eq!(res2.parent_node_id, Some("s1#T1.1".to_string()));
    assert_eq!(state.get_watermark("session-002"), Some(2));

    let tree = state.topic_trees.get("auth-rewrite").expect("tree exists");
    // Parent s1#T1.1 should now be Continued
    assert_eq!(
        tree.nodes.get("s1#T1.1").unwrap().status,
        NodeStatus::Continued
    );
    assert_eq!(
        tree.nodes.get("s2#T2").unwrap().status,
        NodeStatus::Completed
    );
    assert_eq!(
        tree.nodes.get("s2#T2").unwrap().parent_id,
        Some("s1#T1.1".to_string())
    );
}

#[test]
fn test_recent_memory_state_upsert() {
    let mut recent = RecentMemoryState::new();
    let mut p1 = ProjectMemoryState::new("proj-1");
    p1.set_watermark("sess-1", 5);

    recent.upsert_project(p1);
    assert_eq!(recent.projects.len(), 1);
    assert_eq!(
        recent
            .projects
            .get("proj-1")
            .unwrap()
            .get_watermark("sess-1"),
        Some(5)
    );
}

#[test]
fn test_short_id_utf8_multibyte_safety() {
    let node = MemoryTreeNode {
        id: "custom_id".to_string(),
        parent_id: None,
        topic_id: "topic".to_string(),
        summary: "test".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "会话测试-12345".to_string(),
        turn_index: 1,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };
    // Must safely take 8 Unicode characters without slicing mid-codepoint panic
    let short = node.short_id();
    assert_eq!(short, "会话测试-123");
}

#[test]
fn test_watermark_monotonicity() {
    let mut state = ProjectMemoryState::new("proj-wm");
    state.set_watermark("sess-a", 10);
    assert_eq!(state.get_watermark("sess-a"), Some(10));

    // Regressive watermark update must be ignored
    state.set_watermark("sess-a", 5);
    assert_eq!(state.get_watermark("sess-a"), Some(10));

    // Higher watermark must advance
    state.set_watermark("sess-a", 15);
    assert_eq!(state.get_watermark("sess-a"), Some(15));
}

#[test]
fn test_validate_parent_node_must_exist() {
    let mut state = ProjectMemoryState::new("proj-orphan");
    let node = MemoryTreeNode {
        id: "s1#T1".to_string(),
        parent_id: Some("nonexistent_parent".to_string()),
        topic_id: "topic-1".to_string(),
        summary: "Orphan node".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "sess-1".to_string(),
        turn_index: 1,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };
    let payload = TopicEvolutionCommitPayload {
        project_slug: "proj-orphan".to_string(),
        topic_id: "topic-1".to_string(),
        topic_name: None,
        parent_node_id: Some("nonexistent_parent".to_string()),
        parent_new_status: Some(NodeStatus::Continued),
        new_node: node,
        unexplored_siblings: vec![],
    };
    let err = state.apply_evolution(payload).unwrap_err();
    assert!(err.contains("parent node not found in topic tree"));
}

#[test]
fn test_protect_progressed_sibling_nodes() {
    let mut state = ProjectMemoryState::new("proj-sibling");
    let root = MemoryTreeNode {
        id: "r1#T1".to_string(),
        parent_id: None,
        topic_id: "topic-s".to_string(),
        summary: "Root".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "s1".to_string(),
        turn_index: 1,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };
    let sib = MemoryTreeNode {
        id: "r1#T1.1".to_string(),
        parent_id: None,
        topic_id: "topic-s".to_string(),
        summary: "Sibling 1".to_string(),
        status: NodeStatus::Unexplored,
        source_session_id: "s1".to_string(),
        turn_index: 1,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };
    let payload1 = TopicEvolutionCommitPayload {
        project_slug: "proj-sibling".to_string(),
        topic_id: "topic-s".to_string(),
        topic_name: None,
        parent_node_id: None,
        parent_new_status: None,
        new_node: root.clone(),
        unexplored_siblings: vec![sib.clone()],
    };
    state.apply_evolution(payload1.clone()).unwrap();

    // Progress sibling to Continued
    let child = MemoryTreeNode {
        id: "r2#T2".to_string(),
        parent_id: Some("r1#T1.1".to_string()),
        topic_id: "topic-s".to_string(),
        summary: "Child".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "s2".to_string(),
        turn_index: 2,
        created_at: "2026-11-02T11:00:00Z".to_string(),
        updated_at: "2026-11-02T11:00:00Z".to_string(),
    };
    let payload2 = TopicEvolutionCommitPayload {
        project_slug: "proj-sibling".to_string(),
        topic_id: "topic-s".to_string(),
        topic_name: None,
        parent_node_id: Some("r1#T1.1".to_string()),
        parent_new_status: Some(NodeStatus::Continued),
        new_node: child,
        unexplored_siblings: vec![],
    };
    state.apply_evolution(payload2).unwrap();

    let tree = state.topic_trees.get("topic-s").unwrap();
    assert_eq!(
        tree.nodes.get("r1#T1.1").unwrap().status,
        NodeStatus::Continued
    );

    // Re-commit payload1 with unexplored sibling
    state.apply_evolution(payload1).unwrap();

    let tree2 = state.topic_trees.get("topic-s").unwrap();
    assert_eq!(
        tree2.nodes.get("r1#T1.1").unwrap().status,
        NodeStatus::Continued
    );
}
