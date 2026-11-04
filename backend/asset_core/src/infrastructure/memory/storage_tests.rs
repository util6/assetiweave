use super::*;
use crate::backend::domain::memory::{MemoryTreeNode, NodeStatus};
use uuid::Uuid;

#[test]
fn test_memory_storage_paths_and_atomic_write_with_redaction() {
    let tmp = std::env::temp_dir().join(format!("assetiweave-storage-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&tmp).expect("create tempdir");
    let storage = MemoryStorage::new_with_root(tmp.clone(), "default");

    assert!(storage.recent_dir().ends_with("recent"));
    assert!(storage.projects_dir().ends_with("projects"));
    assert!(storage.global_file().ends_with("MEMORY.md"));
    assert!(storage.state_dir().ends_with(".state"));
    assert!(storage.state_projects_dir().ends_with(".state/projects"));
    assert!(storage
        .state_project_file("my-proj")
        .ends_with(".state/projects/my-proj.json"));
    assert!(storage.state_recent_file().ends_with(".state/recent.json"));

    // Write a markdown with sensitive API token
    let raw_content = "# Project Title\n\nSecret: Bearer abcdefghijklmnopqrstuvwxyz1234567890\n";
    let rel_path = "projects/my-proj.md";
    let written_path = storage
        .write_markdown_atomic(rel_path, raw_content)
        .expect("write atomic");

    assert!(written_path.exists());
    let read_back = storage
        .read_markdown(rel_path)
        .expect("read markdown")
        .expect("content exists");

    // Must be redacted
    assert!(!read_back.contains("abcdefghijklmnopqrstuvwxyz1234567890"));
    assert!(read_back.contains("[REDACTED"));

    // Reading non-existent file returns None
    let missing = storage
        .read_markdown("projects/non_existent.md")
        .expect("read missing");
    assert!(missing.is_none());

    std::fs::remove_dir_all(tmp).ok();
}

#[test]
fn test_memory_storage_state_atomic_read_write() {
    let tmp = std::env::temp_dir().join(format!("assetiweave-state-test-{}", Uuid::new_v4()));
    let storage = MemoryStorage::new_with_root(tmp.clone(), "default");

    // 1. Initially missing returns None
    assert!(storage.read_project_state("test-slug").unwrap().is_none());
    assert!(storage.read_recent_state().unwrap().is_none());

    // 2. Write project state even if .state/projects does not exist yet (auto-creation)
    let mut proj_state = ProjectMemoryState::new("test-slug");
    proj_state.set_watermark("sess-001", 3);
    let node = MemoryTreeNode {
        id: "sess-001#T3".to_string(),
        parent_id: None,
        topic_id: "core-refactor".to_string(),
        summary: "Sensitive token: Bearer abcdefghijklmnopqrstuvwxyz1234567890".to_string(),
        status: NodeStatus::Completed,
        source_session_id: "sess-001".to_string(),
        turn_index: 3,
        turn_started_at: None,
        created_at: "2026-11-02T10:00:00Z".to_string(),
        updated_at: "2026-11-02T10:00:00Z".to_string(),
    };
    let payload = crate::backend::domain::memory::TopicEvolutionCommitPayload {
        project_slug: "test-slug".to_string(),
        topic_id: "core-refactor".to_string(),
        topic_name: Some("Core Refactoring".to_string()),
        parent_node_id: None,
        parent_new_status: None,
        new_node: node,
        unexplored_siblings: vec![],
    };
    proj_state.apply_evolution(payload).unwrap();

    let path = storage
        .write_project_state_atomic("test-slug", &proj_state)
        .expect("write project state");
    assert!(path.exists());
    assert_eq!(path, storage.state_project_file("test-slug"));

    // Read back and verify redaction
    let loaded = storage
        .read_project_state("test-slug")
        .expect("read state")
        .expect("state exists");
    assert_eq!(loaded.project_slug, "test-slug");
    assert_eq!(loaded.get_watermark("sess-001"), Some(3));
    let tree = loaded.topic_trees.get("core-refactor").unwrap();
    let node_read = tree.nodes.get("sess-001#T3").unwrap();
    assert!(!node_read
        .summary
        .contains("abcdefghijklmnopqrstuvwxyz1234567890"));
    assert!(node_read.summary.contains("[REDACTED"));

    // 3. Write recent memory state
    let mut recent_state = RecentMemoryState::new();
    recent_state.upsert_project(loaded);
    let recent_path = storage
        .write_recent_state_atomic(&recent_state)
        .expect("write recent state");
    assert!(recent_path.exists());
    assert_eq!(recent_path, storage.state_recent_file());

    let loaded_recent = storage
        .read_recent_state()
        .expect("read recent")
        .expect("recent exists");
    assert_eq!(loaded_recent.projects.len(), 1);
    assert!(loaded_recent.projects.contains_key("test-slug"));

    std::fs::remove_dir_all(tmp).ok();
}

#[test]
fn test_memory_storage_clear_all_and_no_sessions_dir() {
    let tmp = std::env::temp_dir().join(format!("assetiweave-clear-test-{}", Uuid::new_v4()));
    let storage = MemoryStorage::new_with_root(tmp.clone(), "default");

    // Initialize dirs
    storage.init_dirs().expect("init_dirs");

    // .state and subdirs must exist, but sessions/ must NOT exist
    assert!(storage.state_dir().exists());
    assert!(storage.state_projects_dir().exists());
    assert!(storage.recent_dir().exists());
    assert!(storage.projects_dir().exists());
    assert!(!storage.sessions_dir().exists());

    // Create junk
    storage
        .write_markdown_atomic("projects/project-a.md", "# Project A")
        .expect("write project");
    storage
        .write_markdown_atomic("recent/latest.md", "# Recent snapshot")
        .expect("write recent");
    storage
        .write_markdown_atomic("MEMORY.md", "# Global memory")
        .expect("write global");
    let state = ProjectMemoryState::new("proj-a");
    storage
        .write_project_state_atomic("proj-a", &state)
        .expect("write state");

    assert!(storage.state_project_file("proj-a").exists());

    // Execute clear_all
    storage.clear_all().expect("clear all");

    // Clean structure reinstated
    assert!(storage.root_dir().exists());
    assert!(storage.state_dir().exists());
    assert!(storage.state_projects_dir().exists());
    assert!(storage.recent_dir().exists());
    assert!(storage.projects_dir().exists());
    assert!(!storage.sessions_dir().exists());

    // All files removed
    assert!(!storage.state_project_file("proj-a").exists());
    assert!(!storage.global_file().exists());

    std::fs::remove_dir_all(tmp).ok();
}

#[test]
fn test_write_project_state_atomic_with_subpath_slug() {
    let tmp = std::env::temp_dir().join(format!("assetiweave-subpath-test-{}", Uuid::new_v4()));
    let storage = MemoryStorage::new_with_root(tmp.clone(), "default");

    let slug = "nested/sub/module-project";
    let state = ProjectMemoryState::new(slug);
    let written = storage
        .write_project_state_atomic(slug, &state)
        .expect("write subpath project state");

    assert!(written.exists());
    assert_eq!(written, storage.state_project_file(slug));

    let loaded = storage
        .read_project_state(slug)
        .expect("read state")
        .expect("state exists");
    assert_eq!(loaded.project_slug, slug);

    std::fs::remove_dir_all(tmp).ok();
}
