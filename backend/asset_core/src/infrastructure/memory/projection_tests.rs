use super::*;
use crate::backend::domain::memory::{MemoryTreeNode, NodeStatus, TopicEvolutionCommitPayload};
use uuid::Uuid;

#[test]
fn test_project_state_to_markdown_empty() {
    let state = ProjectMemoryState {
        project_slug: "test-slug".to_string(),
        topic_trees: std::collections::HashMap::new(),
        session_watermarks: std::collections::HashMap::new(),
        updated_at: "2026-11-02T12:00:00Z".to_string(),
    };

    let md = project_state_to_markdown(&state);
    assert!(md.contains("project_path: test-slug"));
    assert!(md.contains("# 项目记忆: test-slug"));
    assert!(md.contains("## 🍂 待探索的悬空叶子节点 (Unexplored Branches)"));
    assert!(md.contains("- 暂无待探索分支"));
    assert!(md.contains("## ✅ 已完成的闭环任务 (Completed Tasks)"));
    assert!(md.contains("- 暂无已完成任务"));
}

#[test]
fn test_project_state_to_markdown_populated_and_deterministic() {
    let mut state = ProjectMemoryState::new("assetiweave");
    let node1 = MemoryTreeNode {
        id: "s1#T1".to_string(),
        parent_id: None,
        topic_id: "topic-a".to_string(),
        summary: "Initialize topic tree".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "s1".to_string(),
        turn_index: 1,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };
    let node2 = MemoryTreeNode {
        id: "s1#T1.1".to_string(),
        parent_id: Some("s1#T1".to_string()),
        topic_id: "topic-a".to_string(),
        summary: "Investigate performance bottleneck".to_string(),
        status: NodeStatus::Unexplored,
        source_session_id: "s1".to_string(),
        turn_index: 1,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };

    let payload = TopicEvolutionCommitPayload {
        project_slug: "assetiweave".to_string(),
        topic_id: "topic-a".to_string(),
        topic_name: Some("Topic A".to_string()),
        parent_node_id: None,
        parent_new_status: None,
        new_node: node1,
        unexplored_siblings: vec![node2],
    };
    state.apply_evolution(payload).unwrap();

    let md1 = project_state_to_markdown(&state);
    let md2 = project_state_to_markdown(&state);
    assert_eq!(md1, md2, "Projection must be deterministic");

    assert!(
        md1.contains("- [s1#T1.1](aiwc://session/s1/turn/1): Investigate performance bottleneck")
    );
    assert!(md1.contains("- [s1#T1](aiwc://session/s1/turn/1): Initialize topic tree"));
    assert!(md1.contains("### 🏷️ Topic: Topic A"));
}

#[test]
fn test_recent_state_to_markdown_and_one_way_sync() {
    let tmp = std::env::temp_dir().join(format!("assetiweave-proj-test-{}", Uuid::new_v4()));
    let storage = MemoryStorage::new_with_root(tmp.clone(), "default");

    let mut state = ProjectMemoryState::new("proj-x");
    let node = MemoryTreeNode {
        id: "abc#T5".to_string(),
        parent_id: None,
        topic_id: "t1".to_string(),
        summary: "Completed turn 5".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "abc00000".to_string(),
        turn_index: 5,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };
    let payload = TopicEvolutionCommitPayload {
        project_slug: "proj-x".to_string(),
        topic_id: "t1".to_string(),
        topic_name: None,
        parent_node_id: None,
        parent_new_status: None,
        new_node: node,
        unexplored_siblings: vec![],
    };
    state.apply_evolution(payload).unwrap();

    let mut recent = RecentMemoryState::new();
    recent.upsert_project(state.clone());

    let recent_md = recent_state_to_markdown(&recent);
    assert!(recent_md.contains("Recent Work"));
    assert!(recent_md.contains("## 项目: proj-x"));
    assert!(recent_md.contains("### 🏷️ Topic: t1"));
    assert!(recent_md.contains("- [abc#T5](aiwc://session/abc/turn/5): Completed turn 5"));

    // Sync to disk
    let proj_file = sync_project_markdown(&storage, &state).expect("sync project");
    assert!(proj_file.exists());
    assert_eq!(proj_file, storage.projects_dir().join("proj-x.md"));

    let recent_file = sync_recent_markdown(&storage, &recent).expect("sync recent");
    assert!(recent_file.exists());
    assert_eq!(recent_file, storage.recent_dir().join("latest.md"));

    let read_back_proj = storage
        .read_markdown("projects/proj-x.md")
        .unwrap()
        .unwrap();
    assert!(read_back_proj.contains("# 项目记忆: proj-x"));

    let read_back_recent = storage.read_markdown("recent/latest.md").unwrap().unwrap();
    assert!(read_back_recent.contains("Recent Work"));

    std::fs::remove_dir_all(tmp).ok();
}
