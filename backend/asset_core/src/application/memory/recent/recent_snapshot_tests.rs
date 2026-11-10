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
