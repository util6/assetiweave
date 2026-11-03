use super::*;
use crate::backend::{
    domain::{ConversationSource, ConversationSourceKind},
    store::Database,
};
use uuid::Uuid;

#[tokio::test]
async fn application_source_access_normalizes_file_paths_and_preserves_uris() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-app-conversation-source-paths-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let home_path = dirs::home_dir()
        .expect("home directory")
        .join(".codex")
        .to_string_lossy()
        .to_string();
    let mut source = test_source("codex-source", home_path.clone());

    crate::backend::store::upsert_conversation_source_sqlx(database.pool(), "default", &source)
        .await
        .expect("seed an existing source with an absolute path");
    let loaded = load_source(database.pool(), "default", &source.id)
        .await
        .expect("load source through application")
        .expect("source exists");
    assert_eq!(loaded.location, "~/.codex");

    source.location = home_path;
    save_source(database.pool(), "default", &source)
        .await
        .expect("save source through application");
    let persisted = crate::backend::store::load_conversation_source_sqlx(
        database.pool(),
        "default",
        &source.id,
    )
    .await
    .expect("load raw persisted source")
    .expect("source exists");
    assert_eq!(persisted.location, "~/.codex");

    let uri_source = test_source("recall-source", "memory-recall://session/1".to_string());
    save_source(database.pool(), "default", &uri_source)
        .await
        .expect("save URI source through application");
    let persisted_uri = crate::backend::store::load_conversation_source_sqlx(
        database.pool(),
        "default",
        &uri_source.id,
    )
    .await
    .expect("load URI source")
    .expect("URI source exists");
    assert_eq!(persisted_uri.location, "memory-recall://session/1");

    drop(database);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}

#[tokio::test]
async fn persisted_absolute_source_locations_are_repaired_without_losing_watermarks() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-app-conversation-source-repair-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let home_path = dirs::home_dir()
        .expect("home directory")
        .join(".assetiweave")
        .join("harvesters")
        .join("qwen-web")
        .join("output")
        .join("normalized")
        .to_string_lossy()
        .to_string();
    let legacy = test_source("qwen-web-export", home_path);
    let uri = test_source("recall-source", "memory-recall://session/1".to_string());

    crate::backend::store::upsert_conversation_source_sqlx(database.pool(), "default", &legacy)
        .await
        .expect("seed a legacy absolute source");
    crate::backend::store::upsert_conversation_source_sqlx(database.pool(), "default", &uri)
        .await
        .expect("seed a URI source");
    sqlx::query(
        "UPDATE conversation_sources SET synced_adapter_hash = 'hash-1' WHERE tenant_id = 'default' AND id = 'qwen-web-export'",
    )
    .execute(database.pool())
    .await
    .expect("stamp sync watermark");

    let repaired = normalize_persisted_sources(database.pool(), "default")
        .await
        .expect("repair persisted sources");
    assert_eq!(repaired, 1, "only the absolute location needs repair");

    let persisted = crate::backend::store::load_conversation_source_sqlx(
        database.pool(),
        "default",
        "qwen-web-export",
    )
    .await
    .expect("load repaired source")
    .expect("source exists");
    assert_eq!(
        persisted.location,
        "~/.assetiweave/harvesters/qwen-web/output/normalized"
    );

    let watermark: Option<String> = sqlx::query_scalar(
        "SELECT synced_adapter_hash FROM conversation_sources WHERE tenant_id = 'default' AND id = 'qwen-web-export'",
    )
    .fetch_one(database.pool())
    .await
    .expect("read watermark");
    assert_eq!(watermark.as_deref(), Some("hash-1"));

    let persisted_uri = crate::backend::store::load_conversation_source_sqlx(
        database.pool(),
        "default",
        "recall-source",
    )
    .await
    .expect("load URI source")
    .expect("URI source exists");
    assert_eq!(persisted_uri.location, "memory-recall://session/1");

    let second_pass = normalize_persisted_sources(database.pool(), "default")
        .await
        .expect("second repair pass");
    assert_eq!(second_pass, 0, "repair must be idempotent");

    drop(database);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}

fn test_source(id: &str, location: String) -> ConversationSource {
    ConversationSource {
        id: id.to_string(),
        adapter_id: "test-adapter".to_string(),
        name: id.to_string(),
        kind: ConversationSourceKind::Directory,
        location,
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: "2026-09-23T00:00:00Z".to_string(),
        updated_at: "2026-09-23T00:00:00Z".to_string(),
    }
}
