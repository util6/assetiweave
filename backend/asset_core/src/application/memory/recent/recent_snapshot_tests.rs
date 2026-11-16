use super::*;
use std::fs;
use uuid::Uuid;

struct TestHarness {
    tmp_dir: std::path::PathBuf,
    db_path: std::path::PathBuf,
    mem_dir: std::path::PathBuf,
}

impl TestHarness {
    fn new(prefix: &str) -> Self {
        let tmp =
            std::env::temp_dir().join(format!("aiw-snapshot-test-{prefix}-{}", Uuid::new_v4()));
        let db_dir = tmp.join("db");
        // 记忆根目录由运行时锚定在数据库同级目录，测试不再写全局环境变量。
        let mem_dir = db_dir.join("memories");
        fs::create_dir_all(&db_dir).unwrap();
        fs::create_dir_all(&mem_dir).unwrap();
        let db_path = db_dir.join("app.db");
        Self {
            tmp_dir: tmp,
            db_path,
            mem_dir,
        }
    }

    async fn create_service(&self) -> AppService {
        AppService::open_with_db_path(self.db_path.clone())
            .await
            .unwrap()
    }

    fn state_recent_file(&self, tenant_id: &str) -> std::path::PathBuf {
        let path = self
            .mem_dir
            .join(tenant_id)
            .join(".state")
            .join("recent.json");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        path
    }
    fn storage(
        &self,
        tenant_id: &str,
    ) -> crate::backend::infrastructure::memory::storage::MemoryStorage {
        crate::backend::infrastructure::memory::storage::MemoryStorage::new_with_root(
            self.mem_dir.clone(),
            tenant_id,
        )
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.tmp_dir).ok();
    }
}

#[tokio::test]
async fn test_get_recent_memory_snapshot_missing_file() {
    let harness = TestHarness::new("missing");
    let service = harness.create_service().await;
    let storage = harness.storage(service.tenant_id());

    let res = service
        .get_recent_memory_snapshot_with_storage(&storage)
        .await
        .unwrap();
    assert_eq!(res.status, RecentMemoryStatus::Empty);
    assert!(res.snapshot.is_none());
}

#[tokio::test]
async fn test_get_recent_memory_snapshot_empty_file() {
    let harness = TestHarness::new("empty");
    let service = harness.create_service().await;
    let path = harness.state_recent_file(service.tenant_id());
    let storage = harness.storage(service.tenant_id());

    fs::write(&path, "   \n\t  ").unwrap();

    let res = service
        .get_recent_memory_snapshot_with_storage(&storage)
        .await
        .unwrap();
    assert_eq!(res.status, RecentMemoryStatus::Empty);
    assert!(res.snapshot.is_none());
}

#[tokio::test]
async fn test_get_recent_memory_snapshot_malformed_json() {
    let harness = TestHarness::new("malformed");
    let service = harness.create_service().await;
    let path = harness.state_recent_file(service.tenant_id());
    let storage = harness.storage(service.tenant_id());

    fs::write(&path, "{\"invalid\": [not json").unwrap();

    let res = service
        .get_recent_memory_snapshot_with_storage(&storage)
        .await
        .unwrap();
    assert_eq!(res.status, RecentMemoryStatus::Empty);
    assert!(res.snapshot.is_none());
}

#[tokio::test]
async fn test_get_recent_memory_snapshot_valid_json_recent_state_file() {
    let harness = TestHarness::new("valid-state-file");
    let service = harness.create_service().await;
    let path = harness.state_recent_file(service.tenant_id());
    let storage = harness.storage(service.tenant_id());

    let state_file_json = r#"{
        "schema_version": 1,
        "updated_at": "2026-11-02T12:00:00Z",
        "projects": [
            {
                "project_key": "assetiweave-core",
                "project_path": "/Users/test/assetiweave",
                "updated_at": "2026-11-02T12:00:00Z",
                "topics": [
                    {
                        "topic_id": "migration-164",
                        "topic": "Memory Subsystem Refactor",
                        "updated_at": "2026-11-02T12:00:00Z",
                        "nodes": [
                            {
                                "id": "098b0c4b#T13",
                                "parent_id": null,
                                "status": "completed",
                                "label": "Implement .state/ schema",
                                "summary": "Direct JSON DTO storage",
                                "recorded_at": "2026-11-02T11:00:00Z"
                            },
                            {
                                "id": "098b0c4b#T14",
                                "parent_id": "098b0c4b#T13",
                                "status": "continued",
                                "label": "Tree UI visualization",
                                "summary": "Multi-way tree rendering",
                                "recorded_at": "2026-11-02T11:30:00Z"
                            },
                            {
                                "id": "098b0c4b#T14.1",
                                "parent_id": "098b0c4b#T13",
                                "status": "unexplored",
                                "label": "Explore SQLite cache",
                                "summary": "Investigate fast SQLite query cache",
                                "recorded_at": "2026-11-02T11:45:00Z"
                            }
                        ]
                    }
                ]
            }
        ]
    }"#;

    fs::write(&path, state_file_json).unwrap();

    let res = service
        .get_recent_memory_snapshot_with_storage(&storage)
        .await
        .unwrap();
    assert_eq!(res.status, RecentMemoryStatus::Ready);

    let snapshot = res.snapshot.expect("snapshot must be present");
    assert_eq!(snapshot.projects.len(), 1);

    let proj = &snapshot.projects[0];
    assert_eq!(proj.project_key, "assetiweave-core");
    assert_eq!(proj.project_title, "/Users/test/assetiweave");
    assert_eq!(proj.topics.len(), 1);

    let topic = &proj.topics[0];
    assert_eq!(topic.topic_id, "migration-164");
    assert_eq!(topic.topic, "Memory Subsystem Refactor");
    assert_eq!(topic.nodes.len(), 3);

    // Verify nodes
    assert_eq!(topic.nodes[0].id, "098b0c4b#T13");
    assert_eq!(topic.nodes[0].status, RecentNodeStatus::Completed);
    assert_eq!(topic.nodes[1].id, "098b0c4b#T14");
    assert_eq!(topic.nodes[1].parent_id, Some("098b0c4b#T13".to_string()));
    assert_eq!(topic.nodes[1].status, RecentNodeStatus::Continued);
    assert_eq!(topic.nodes[2].id, "098b0c4b#T14.1");
    assert_eq!(topic.nodes[2].status, RecentNodeStatus::Unexplored);

    // Verify backward-compatible items fallback
    assert_eq!(proj.items.len(), 3);
    assert_eq!(proj.items[0].item_id, "assetiweave-core-1");
    assert_eq!(proj.items[0].status, "completed");
    assert_eq!(proj.items[1].status, "continued");
    assert_eq!(proj.items[2].status, "unexplored");
}

#[tokio::test]
async fn test_get_recent_memory_snapshot_trailing_comma() {
    let harness = TestHarness::new("trailing-comma");
    let service = harness.create_service().await;
    let path = harness.state_recent_file(service.tenant_id());
    let storage = harness.storage(service.tenant_id());

    fs::write(&path, r#"{"schema_version": 1, "projects": [],}"#).unwrap();

    let res = service
        .get_recent_memory_snapshot_with_storage(&storage)
        .await
        .unwrap();
    assert_eq!(res.status, RecentMemoryStatus::Empty);
    assert!(res.snapshot.is_none());
}

#[tokio::test]
async fn test_get_recent_memory_snapshot_truncated_braces() {
    let harness = TestHarness::new("truncated-braces");
    let service = harness.create_service().await;
    let path = harness.state_recent_file(service.tenant_id());
    let storage = harness.storage(service.tenant_id());

    fs::write(
        &path,
        r#"{"schema_version": 1, "projects": [{"project_key": "core""#,
    )
    .unwrap();

    let res = service
        .get_recent_memory_snapshot_with_storage(&storage)
        .await
        .unwrap();
    assert_eq!(res.status, RecentMemoryStatus::Empty);
    assert!(res.snapshot.is_none());
}

#[tokio::test]
async fn test_get_recent_memory_snapshot_non_utf8_bytes() {
    let harness = TestHarness::new("non-utf8");
    let service = harness.create_service().await;
    let path = harness.state_recent_file(service.tenant_id());
    let storage = harness.storage(service.tenant_id());

    // Write raw invalid UTF-8 byte sequence
    let invalid_bytes = [0xFF, 0xFE, 0xFD, 0x80, 0x81, 0xC0, 0xAF];
    fs::write(&path, invalid_bytes).unwrap();

    let res = service
        .get_recent_memory_snapshot_with_storage(&storage)
        .await
        .unwrap();
    assert_eq!(res.status, RecentMemoryStatus::Empty);
    assert!(res.snapshot.is_none());
}

#[tokio::test]
async fn test_get_recent_memory_snapshot_circular_references() {
    let harness = TestHarness::new("circular-refs");
    let service = harness.create_service().await;
    let path = harness.state_recent_file(service.tenant_id());
    let storage = harness.storage(service.tenant_id());

    let state_file_json = r#"{
        "schema_version": 1,
        "updated_at": "2026-11-02T12:00:00Z",
        "projects": [
            {
                "project_key": "assetiweave-circular",
                "project_path": "/Users/test/circular",
                "updated_at": "2026-11-02T12:00:00Z",
                "topics": [
                    {
                        "topic_id": "topic-cycle",
                        "topic": "Cycle Topic",
                        "updated_at": "2026-11-02T12:00:00Z",
                        "nodes": [
                            {
                                "id": "node-A#T1",
                                "parent_id": "node-B#T2",
                                "status": "continued",
                                "label": "Node A",
                                "summary": "Points to B",
                                "recorded_at": "2026-11-02T11:00:00Z"
                            },
                            {
                                "id": "node-B#T2",
                                "parent_id": "node-A#T1",
                                "status": "continued",
                                "label": "Node B",
                                "summary": "Points to A",
                                "recorded_at": "2026-11-02T11:30:00Z"
                            }
                        ]
                    }
                ]
            }
        ]
    }"#;

    fs::write(&path, state_file_json).unwrap();

    let res = service
        .get_recent_memory_snapshot_with_storage(&storage)
        .await
        .unwrap();
    assert_eq!(res.status, RecentMemoryStatus::Ready);
    let snapshot = res.snapshot.unwrap();
    assert_eq!(snapshot.projects.len(), 1);
    assert_eq!(snapshot.projects[0].topics[0].nodes.len(), 2);
    assert_eq!(snapshot.projects[0].items.len(), 2);
}

#[tokio::test]
async fn test_get_recent_memory_snapshot_deep_tree_hierarchy() {
    let harness = TestHarness::new("deep-tree");
    let service = harness.create_service().await;
    let path = harness.state_recent_file(service.tenant_id());
    let storage = harness.storage(service.tenant_id());

    // Generate a 12-level deep hierarchy chain with branching siblings
    let mut nodes = Vec::new();
    let mut parent_id: Option<String> = None;
    for depth in 0..12 {
        let node_id = format!("sess-{depth:02}#T{depth}");
        nodes.push(RecentTreeNodeView {
            id: node_id.clone(),
            parent_id: parent_id.clone(),
            status: if depth % 3 == 0 {
                RecentNodeStatus::Completed
            } else if depth % 3 == 1 {
                RecentNodeStatus::Continued
            } else {
                RecentNodeStatus::Unexplored
            },
            label: format!("Deep Level {depth}"),
            summary: format!("Summary for depth {depth} under parent {parent_id:?}"),
            recorded_at: Some("2026-11-02T10:00:00Z".to_string()),
            source_session_id: None,
            turn_index: None,
            session_title: None,
            source_agent: None,
            session_available: None,
            parent_summary: None,
        });

        // Add sibling branch
        if depth > 0 && depth % 2 == 0 {
            nodes.push(RecentTreeNodeView {
                id: format!("sess-{depth:02}#T{depth}.sibling"),
                parent_id: parent_id.clone(),
                status: RecentNodeStatus::Unexplored,
                label: format!("Sibling Level {depth}"),
                summary: format!("Sibling summary for depth {depth}"),
                recorded_at: Some("2026-11-02T10:15:00Z".to_string()),
                source_session_id: None,
                turn_index: None,
                session_title: None,
                source_agent: None,
                session_available: None,
                parent_summary: None,
            });
        }

        parent_id = Some(node_id);
    }

    let state_file = RecentStateFile {
        schema_version: 1,
        updated_at: "2026-11-02T12:00:00Z".to_string(),
        projects: vec![RecentStateProject {
            project_key: "deep-project".to_string(),
            project_path: Some("/Users/test/deep".to_string()),
            updated_at: "2026-11-02T12:00:00Z".to_string(),
            topics: vec![RecentTopicView {
                topic_id: "deep-topic".to_string(),
                topic: "Deep Tree Hierarchy Topic".to_string(),
                updated_at: "2026-11-02T12:00:00Z".to_string(),
                nodes,
            }],
        }],
    };

    let serialized = serde_json::to_string(&state_file).unwrap();
    fs::write(&path, serialized).unwrap();

    let res = service
        .get_recent_memory_snapshot_with_storage(&storage)
        .await
        .unwrap();
    assert_eq!(res.status, RecentMemoryStatus::Ready);
    let snapshot = res.snapshot.unwrap();
    let topic = &snapshot.projects[0].topics[0];
    assert_eq!(topic.nodes.len(), 17); // 12 mainline + 5 siblings
    assert_eq!(snapshot.projects[0].items.len(), 17);
    assert_eq!(snapshot.projects[0].source_session_count, 12);
}

#[tokio::test]
async fn test_get_recent_memory_snapshot_enriches_sqlite_parent_summary_and_turn_started_at() {
    use crate::backend::domain::memory::{
        MemoryTreeNode, NodeStatus, ProjectMemoryState, RecentMemoryState, TopicTree,
    };

    let harness = TestHarness::new("enrich");
    let service = harness.create_service().await;
    let storage = harness.storage(service.tenant_id());

    // Insert a conversation source + session into SQLite
    sqlx::query(
        "INSERT INTO conversation_sources (tenant_id, id, adapter_id, name, kind, location, enabled, created_at, updated_at) \
         VALUES ('default', 'src-1', 'claude-code', 'Claude Code', 'local', '/tmp', 1, '2026-11-01T00:00:00Z', '2026-11-01T00:00:00Z')",
    )
    .execute(service.db.pool())
    .await
    .unwrap();

    let full_sess_id =
        "conversation-session-098b0c4b11223344556677889900aabbccddeeff00112233445566778899001122";
    sqlx::query(
        "INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, title, project_path, last_cwd, missing, created_at, imported_at) \
         VALUES ('default', ?1, 'src-1', 'claude-code', 'ext-1', 'Refactor Recent UI', '/Users/test/assetiweave', '/Users/test/assetiweave', 0, '2026-11-01T00:00:00Z', '2026-11-01T00:00:00Z')",
    )
    .bind(full_sess_id)
    .execute(service.db.pool())
    .await
    .unwrap();

    // Write full project state (with a Continued parent node and a Dismissed node)
    let mut full_tree = TopicTree::new("ui-rewrite", "UI 重写");
    full_tree.insert_node(
        MemoryTreeNode {
            id: "098b0c4b#T1".to_string(),
            parent_id: None,
            topic_id: "ui-rewrite".to_string(),
            summary: "旧版前端与 .state 结构脱节".to_string(),
            status: NodeStatus::Continued,
            source_session_id: full_sess_id.to_string(),
            turn_index: 1,
            turn_started_at: Some("2026-11-01T08:00:00Z".to_string()),
            created_at: "2026-11-05T00:00:00Z".to_string(),
            updated_at: "2026-11-05T00:00:00Z".to_string(),
        },
        true,
    );
    full_tree.insert_node(
        MemoryTreeNode {
            id: "098b0c4b#T2".to_string(),
            parent_id: Some("098b0c4b#T1".to_string()),
            topic_id: "ui-rewrite".to_string(),
            summary: "按 .state JSON 重写前端工作区".to_string(),
            status: NodeStatus::Unexplored,
            source_session_id: full_sess_id.to_string(),
            turn_index: 2,
            turn_started_at: Some("2026-11-02T09:30:00Z".to_string()),
            created_at: "2026-11-05T00:00:00Z".to_string(),
            updated_at: "2026-11-05T00:00:00Z".to_string(),
        },
        false,
    );
    full_tree.insert_node(
        MemoryTreeNode {
            id: "098b0c4b#T3".to_string(),
            parent_id: None,
            topic_id: "ui-rewrite".to_string(),
            summary: "被废弃的旧方案".to_string(),
            status: NodeStatus::Dismissed,
            source_session_id: full_sess_id.to_string(),
            turn_index: 3,
            turn_started_at: Some("2026-11-02T10:00:00Z".to_string()),
            created_at: "2026-11-05T00:00:00Z".to_string(),
            updated_at: "2026-11-05T00:00:00Z".to_string(),
        },
        true,
    );

    let mut full_proj = ProjectMemoryState::new("users-test-assetiweave");
    full_proj
        .topic_trees
        .insert("ui-rewrite".to_string(), full_tree);
    full_proj
        .session_watermarks
        .insert(full_sess_id.to_string(), 3);
    storage
        .write_project_state_atomic("users-test-assetiweave", &full_proj)
        .unwrap();

    // Recent slice omits Continued parent & Dismissed node
    let mut recent_tree = TopicTree::new("ui-rewrite", "UI 重写");
    recent_tree.insert_node(
        full_proj.topic_trees["ui-rewrite"].nodes["098b0c4b#T2"].clone(),
        false,
    );
    let mut recent_proj = ProjectMemoryState::new("users-test-assetiweave");
    recent_proj
        .topic_trees
        .insert("ui-rewrite".to_string(), recent_tree);

    let mut recent_state = RecentMemoryState::new();
    recent_state.window_hours = Some(72);
    recent_state.updated_at = "2026-11-02T12:00:00Z".to_string();
    recent_state
        .projects
        .insert("users-test-assetiweave".to_string(), recent_proj);
    storage.write_recent_state_atomic(&recent_state).unwrap();

    let view = service
        .get_recent_memory_snapshot_with_storage(&storage)
        .await
        .unwrap();
    assert_eq!(view.status, RecentMemoryStatus::Ready);
    let snap = view.snapshot.unwrap();
    assert_eq!(snap.window_hours, 72);
    assert_eq!(snap.target_watermark, "2026-11-02T12:00:00Z");

    let proj = &snap.projects[0];
    assert_eq!(proj.project_title, "assetiweave");
    assert_eq!(
        proj.project_path.as_deref(),
        Some("/Users/test/assetiweave")
    );

    let node = &proj.topics[0].nodes[0];
    assert_eq!(node.recorded_at.as_deref(), Some("2026-11-02T09:30:00Z"));
    assert_eq!(node.source_session_id.as_deref(), Some(full_sess_id));
    assert_eq!(node.session_title.as_deref(), Some("Refactor Recent UI"));
    assert_eq!(node.source_agent.as_deref(), Some("claude-code"));
    assert_eq!(node.session_available, Some(true));
    assert_eq!(
        node.parent_summary.as_deref(),
        Some("旧版前端与 .state 结构脱节")
    );

    // Full project state view should include Continued parent and exclude Dismissed
    let full_view = service
        .get_memory_project_state_view_with_storage("users-test-assetiweave", &storage)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(full_view.project_title, "assetiweave");
    assert_eq!(full_view.watermark_count, 1);
    assert_eq!(full_view.topics[0].nodes.len(), 2);
}
