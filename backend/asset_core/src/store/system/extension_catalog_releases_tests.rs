use super::*;
use crate::backend::store::Database;

#[tokio::test]
async fn test_extension_catalog_releases_crud_and_queries() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-ext-catalog-{}.db",
        uuid::Uuid::new_v4()
    ));
    let database = Database::open_initialized_async(&path)
        .await
        .expect("open database");
    let repo = ExtensionCatalogReleaseRepository::new(database.pool().clone());

    // Initial count should be zero
    let count = repo.count_by_capability("agent").await.unwrap();
    assert_eq!(count, 0);

    let release1 = ExtensionCatalogRelease {
        catalog_source: "default-curated".to_string(),
        extension_id: "test-agent".to_string(),
        version: "1.0.0".to_string(),
        capability_kind: "agent".to_string(),
        target_id: "test-agent".to_string(),
        display_name: "Test Agent".to_string(),
        publisher: "AssetIWeave".to_string(),
        channel: "stable".to_string(),
        released_at: Some("2026-10-01T00:00:00Z".to_string()),
        artifact_url: "https://example.com/agent-1.0.0.tar.gz".to_string(),
        artifact_size: Some(1024),
        artifact_sha256: Some("sha256-abc".to_string()),
        release_meta_json: serde_json::json!({
            "catalogVersion": "2026.10.1",
            "etag": "\"etag-1\""
        })
        .to_string(),
        fetched_at: "2026-10-01T12:00:00Z".to_string(),
    };

    let release2 = ExtensionCatalogRelease {
        catalog_source: "default-curated".to_string(),
        extension_id: "test-agent".to_string(),
        version: "1.1.0".to_string(),
        capability_kind: "agent".to_string(),
        target_id: "test-agent".to_string(),
        display_name: "Test Agent".to_string(),
        publisher: "AssetIWeave".to_string(),
        channel: "stable".to_string(),
        released_at: Some("2026-10-15T00:00:00Z".to_string()),
        artifact_url: "https://example.com/agent-1.1.0.tar.gz".to_string(),
        artifact_size: Some(2048),
        artifact_sha256: Some("sha256-def".to_string()),
        release_meta_json: serde_json::json!({
            "catalogVersion": "2026.10.2",
            "etag": "\"etag-2\""
        })
        .to_string(),
        fetched_at: "2026-10-15T12:00:00Z".to_string(),
    };

    repo.batch_upsert(&[release1, release2]).await.unwrap();

    let count = repo.count_by_capability("agent").await.unwrap();
    assert_eq!(count, 2);

    let list = repo.list_by_capability("agent").await.unwrap();
    assert_eq!(list.len(), 2);
    // Ordered by released_at DESC, version DESC
    assert_eq!(list[0].version, "1.1.0");
    assert_eq!(list[1].version, "1.0.0");

    let latest = repo
        .get_latest_by_target("agent", "test-agent")
        .await
        .unwrap();
    assert!(latest.is_some());
    assert_eq!(latest.unwrap().version, "1.1.0");

    let etag = repo.get_latest_etag("default-curated").await.unwrap();
    assert_eq!(etag.as_deref(), Some("\"etag-2\""));

    let _ = std::fs::remove_file(path);
}
