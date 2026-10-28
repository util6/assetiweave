use super::*;
use uuid::Uuid;

#[tokio::test]
async fn test_get_recent_and_project_markdown_views() {
    let base = std::env::temp_dir().join(format!("assetiweave-md-view-{}", Uuid::new_v4()));
    let db_dir = base.join("db");
    let mem_dir = base.join("memories");
    std::fs::create_dir_all(&db_dir).unwrap();
    std::fs::create_dir_all(&mem_dir).unwrap();

    let service = AppService::open_with_db_path(db_dir.join("app.db"))
        .await
        .unwrap();
    let tenant_id = service.tenant_id();
    let storage = MemoryStorage::new_with_root(mem_dir.clone(), tenant_id);

    // Write a sample recent/latest.md and projects/sample-proj.md
    storage
        .write_markdown_atomic("recent/latest.md", "# Recent Work\n- task 1")
        .unwrap();
    storage
        .write_markdown_atomic("projects/sample-proj.md", "# Project Sample\n- leaf A")
        .unwrap();

    // Since service uses default tenant storage directory in its root,
    // let's test view structure directly and against storage
    let empty_recent = service.get_recent_memory_markdown_view(None).await.unwrap();
    assert_eq!(empty_recent.file_path, "recent/latest.md");

    let empty_proj = service
        .get_project_memory_markdown_view("/code/nonexistent")
        .await
        .unwrap();
    assert_eq!(empty_proj.file_path, "projects/code-nonexistent.md");
    assert!(!empty_proj.exists);
}
