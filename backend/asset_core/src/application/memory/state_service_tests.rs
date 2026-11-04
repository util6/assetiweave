use super::*;
use crate::backend::domain::memory::{MemoryTreeNode, NodeStatus};
use uuid::Uuid;

#[tokio::test]
async fn test_state_service_commit_evolution_and_watermarks() {
    let tmp = std::env::temp_dir().join(format!("assetiweave-state-srv-{}", Uuid::new_v4()));
    let db_path = tmp.join("db").join("app.db");
    let mem_dir = tmp.join("memories");
    std::fs::create_dir_all(tmp.join("db")).unwrap();
    std::fs::create_dir_all(&mem_dir).unwrap();

    // Direct memory storage to isolated temp dir
    std::env::set_var("ASSETIWEAVE_MEMORY_ROOT", &mem_dir);

    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let tenant_id = service.tenant_id();
    let storage = MemoryStorage::new_with_root(mem_dir.clone(), tenant_id);

    // 1. Watermark initially None
    let wm = service
        .check_memory_session_watermark("test-proj", "session-100")
        .await
        .unwrap();
    assert_eq!(wm, None);

    // 2. Advance watermark independently
    service
        .advance_memory_session_watermark("test-proj", "session-100", 3)
        .await
        .unwrap();
    let wm2 = service
        .check_memory_session_watermark("test-proj", "session-100")
        .await
        .unwrap();
    assert_eq!(wm2, Some(3));

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

    // Watermark should now be advanced to 4
    let wm3 = service
        .check_memory_session_watermark("test-proj", "session-100")
        .await
        .unwrap();
    assert_eq!(wm3, Some(4));

    // Project state JSON must exist and match
    let proj_state = service
        .get_project_memory_state("test-proj")
        .await
        .unwrap()
        .expect("state exists");
    assert_eq!(proj_state.project_slug, "test-proj");
    assert_eq!(proj_state.get_watermark("session-100"), Some(4));

    // Markdown file must exist on disk
    let proj_md = storage
        .read_markdown("projects/test-proj.md")
        .unwrap()
        .unwrap();
    assert!(proj_md.contains("# 项目记忆: test-proj"));
    assert!(proj_md.contains("[s1#T1](aiwc://session/s1/turn/4)"));
    assert!(proj_md.contains("[s1#T1.1](aiwc://session/s1/turn/4)"));

    // Recent state JSON and recent markdown must exist on disk
    let recent_state = service
        .get_recent_memory_state()
        .await
        .unwrap()
        .expect("recent state exists");
    assert!(recent_state.projects.contains_key("test-proj"));

    let recent_md = storage.read_markdown("recent/latest.md").unwrap().unwrap();
    assert!(recent_md.contains("Recent Work"));
    assert!(recent_md.contains("## 项目: test-proj"));

    // Clean up
    std::fs::remove_dir_all(tmp).ok();
}
