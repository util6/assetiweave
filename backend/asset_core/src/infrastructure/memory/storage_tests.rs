use super::*;
use crate::backend::domain::memory::{MemoryTreeNode, NodeStatus, TopicTree};
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
fn test_memory_storage_init_dirs_keeps_cumulative_state_and_no_sessions_dir() {
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

    // init_dirs is idempotent and must never delete accumulated artifacts:
    // 项目记忆是累计真源，存储层不再提供任何全量清空能力。
    storage.init_dirs().expect("init_dirs again");
    assert!(storage.state_project_file("proj-a").exists());
    assert!(storage.global_file().exists());
    assert!(storage.root_dir().join("projects/project-a.md").exists());
    assert!(storage.root_dir().join("recent/latest.md").exists());

    std::fs::remove_dir_all(tmp).ok();
}

#[test]
fn test_read_project_state_merged_absorbs_legacy_unfolded_slug() {
    let tmp = std::env::temp_dir().join(format!("assetiweave-merge-test-{}", Uuid::new_v4()));
    // 生产路径下前缀由宿主机主目录确定性推导，这里显式注入等价前缀。
    let storage = MemoryStorage::new_with_root(tmp.clone(), "default")
        .with_legacy_slug_prefix(Some("users-fixture".to_string()));
    let canonical = "code-space-foo";
    let legacy = "users-fixture-code-space-foo";

    // 1. 历史未折叠 slug：含一个 unexplored 悬空分支与一条会话水位
    let mut legacy_state = ProjectMemoryState::new(legacy);
    legacy_state.set_watermark("sess-legacy", 4);
    legacy_state.topic_trees.insert(
        "topic-a".to_string(),
        TopicTree {
            topic_id: "topic-a".to_string(),
            topic_name: "Topic A".to_string(),
            root_node_ids: vec!["aaaaaaaa#T1".to_string()],
            nodes: std::collections::HashMap::from([(
                "aaaaaaaa#T1".to_string(),
                MemoryTreeNode {
                    id: "aaaaaaaa#T1".to_string(),
                    parent_id: None,
                    topic_id: "topic-a".to_string(),
                    summary: "历史悬空分支".to_string(),
                    status: NodeStatus::Unexplored,
                    source_session_id: "sess-legacy".to_string(),
                    turn_index: 1,
                    turn_started_at: Some("2026-10-01T10:00:00Z".to_string()),
                    created_at: "2026-10-01T10:00:00Z".to_string(),
                    updated_at: "2026-10-01T10:00:00Z".to_string(),
                },
            )]),
        },
    );
    storage
        .write_project_state_atomic(legacy, &legacy_state)
        .expect("write legacy state");

    // 2. 规范 slug：已有一个新主题与同会话的较低水位
    let mut canonical_state = ProjectMemoryState::new(canonical);
    canonical_state.set_watermark("sess-legacy", 2);
    canonical_state
        .topic_trees
        .insert("topic-b".to_string(), TopicTree::new("topic-b", "Topic B"));
    storage
        .write_project_state_atomic(canonical, &canonical_state)
        .expect("write canonical state");

    // 3. 读取即收敛：两个主题并存，水位取最大，旧文件被清理
    let (merged, absorbed) = storage
        .read_project_state_merged(canonical)
        .expect("merged read");
    let merged = merged.expect("merged state exists");
    assert_eq!(absorbed, vec![legacy.to_string()]);
    assert!(merged.topic_trees.contains_key("topic-a"));
    assert!(merged.topic_trees.contains_key("topic-b"));
    assert_eq!(merged.get_watermark("sess-legacy"), Some(4));

    assert!(!storage.state_project_file(legacy).exists());
    assert!(storage.state_project_file(canonical).exists());

    // 4. 幂等：再次读取不再产生吸收，内容保持一致
    let (again, absorbed_again) = storage
        .read_project_state_merged(canonical)
        .expect("merged read again");
    assert!(absorbed_again.is_empty());
    assert_eq!(
        again.expect("state exists").topic_trees.len(),
        merged.topic_trees.len()
    );

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

#[test]
fn test_read_project_state_merged_ignores_unrelated_prefixed_state() {
    let tmp = std::env::temp_dir().join(format!("assetiweave-merge-scope-{}", Uuid::new_v4()));
    let storage = MemoryStorage::new_with_root(tmp.clone(), "default")
        .with_legacy_slug_prefix(Some("users-fixture".to_string()));

    // 一个恰好以 `-code-space-foo` 结尾、但不是由 home 前缀推导出来的其它项目
    let unrelated = "teammate-code-space-foo";
    let mut unrelated_state = ProjectMemoryState::new(unrelated);
    unrelated_state.set_watermark("sess-other", 9);
    storage
        .write_project_state_atomic(unrelated, &unrelated_state)
        .expect("write unrelated state");

    let (merged, absorbed) = storage
        .read_project_state_merged("code-space-foo")
        .expect("merged read");
    assert!(
        absorbed.is_empty(),
        "只允许吸收由 home 前缀确定性推导出的历史 slug"
    );
    assert!(merged.is_none());
    assert!(
        storage.state_project_file(unrelated).exists(),
        "其它项目的状态文件绝不能被当作历史文件删除"
    );

    std::fs::remove_dir_all(tmp).ok();
}
