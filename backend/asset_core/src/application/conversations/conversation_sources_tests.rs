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
