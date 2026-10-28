use super::*;
use crate::backend::application::AppService;
use uuid::Uuid;

async fn setup_test_pool() -> (sqlx::SqlitePool, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("assetiweave-watermark-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create test root");
    let service = AppService::open_with_db_path(root.join("app.db"))
        .await
        .expect("open test service");
    (service.pool().clone(), root)
}

#[tokio::test]
async fn test_watermark_crud_and_idempotent_upsert() {
    let (pool, root) = setup_test_pool().await;
    let tenant_id = "default";
    let session_id = "session-001";

    let watermarks = load_watermarks_sqlx(&pool, tenant_id)
        .await
        .expect("load empty");
    assert!(watermarks.is_empty());

    let record = MemorySessionWatermarkRecord {
        tenant_id: tenant_id.to_string(),
        session_id: session_id.to_string(),
        project_path: Some("/path/to/project".to_string()),
        source_updated_at: "2026-10-21T10:00:00Z".to_string(),
        summary_rel_path: "sessions/2026-10-21_session-001.md".to_string(),
        has_signal: true,
        updated_at: "2026-10-21T10:05:00Z".to_string(),
    };

    upsert_watermark_sqlx(&pool, &record)
        .await
        .expect("insert watermark");

    let loaded = load_watermarks_sqlx(&pool, tenant_id)
        .await
        .expect("load watermarks");
    assert_eq!(loaded.len(), 1);
    let item = loaded.get(session_id).expect("session found");
    assert_eq!(item.source_updated_at, "2026-10-21T10:00:00Z");
    assert_eq!(item.summary_rel_path, "sessions/2026-10-21_session-001.md");
    assert!(item.has_signal);

    // Upsert update
    let mut updated_record = record.clone();
    updated_record.source_updated_at = "2026-10-21T11:00:00Z".to_string();
    updated_record.has_signal = false;
    upsert_watermark_sqlx(&pool, &updated_record)
        .await
        .expect("update watermark");

    let loaded_again = load_watermarks_sqlx(&pool, tenant_id)
        .await
        .expect("load updated");
    assert_eq!(loaded_again.len(), 1);
    let updated_item = loaded_again.get(session_id).expect("session found");
    assert_eq!(updated_item.source_updated_at, "2026-10-21T11:00:00Z");
    assert!(!updated_item.has_signal);

    // Tenant isolation
    let other_loaded = load_watermarks_sqlx(&pool, "other_tenant")
        .await
        .expect("load other");
    assert!(other_loaded.is_empty());

    std::fs::remove_dir_all(root).ok();
}
