//! Application projection of prepared, application-owned built-in adapters.

use crate::backend::{models::ConversationAdapter, runtime::AppResult};
use sqlx::SqlitePool;

/// Seed one tenant from the immutable built-in environment prepared by
/// `AppRuntime`. This boundary performs no filesystem writes.
pub(crate) async fn seed_prepared_builtin_adapters(
    pool: &SqlitePool,
    tenant_id: &str,
    adapters: &[ConversationAdapter],
) -> AppResult<()> {
    crate::backend::bootstrap::seed_prepared_builtin_adapters(pool, tenant_id, adapters).await
}

#[cfg(test)]
mod tests {
    use crate::backend::{
        models::{ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState},
        store,
    };
    use std::{fs, path::Path};

    #[tokio::test]
    async fn seed_receives_prepared_data_and_writes_nothing_to_fs() {
        let fixture_root = std::env::temp_dir().join(format!(
            "assetiweave-prepared-adapter-seed-{}",
            uuid::Uuid::new_v4()
        ));
        let read_only_dir = fixture_root.join("prepared-adapter");
        let database_dir = fixture_root.join("database");
        fs::create_dir_all(&read_only_dir).expect("create read-only adapter directory");
        fs::create_dir_all(&database_dir).expect("create database directory");

        let before = directory_entries(&read_only_dir);
        let mut permissions = fs::metadata(&read_only_dir)
            .expect("read adapter directory metadata")
            .permissions();
        set_read_only_directory(&mut permissions);
        fs::set_permissions(&read_only_dir, permissions).expect("make adapter directory read-only");

        let db_path = database_dir.join("seed.sqlite");
        let pool = store::open_migrated_pool(&db_path)
            .await
            .expect("open test database");
        store::seed_defaults_sqlx(&pool)
            .await
            .expect("seed generic defaults");

        let manifest_path = read_only_dir.join("conversation-adapter.json");
        let executable_path = read_only_dir.join("adapter.mjs");
        let prepared = ConversationAdapter {
            id: "prepared-fixture".to_string(),
            name: "Prepared Fixture".to_string(),
            kind: ConversationAdapterKind::External,
            version: "1.0.0".to_string(),
            enabled: true,
            manifest_path: Some(manifest_path.to_string_lossy().to_string()),
            executable_path: Some(executable_path.to_string_lossy().to_string()),
            content_hash: Some("content-hash".to_string()),
            trusted_hash: Some("content-hash".to_string()),
            trust_state: ConversationAdapterTrustState::Trusted,
            protocol_version: Some(1),
            capabilities: vec!["probe".to_string()],
            input_kinds: Vec::new(),
            card_contract_version: None,
            card_kinds: Vec::new(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };

        let seed_result = super::seed_prepared_builtin_adapters(
            &pool,
            "default",
            std::slice::from_ref(&prepared),
        )
        .await;

        restore_writable_directory(&read_only_dir);
        seed_result.expect("store seed should only persist prepared data");
        assert_eq!(directory_entries(&read_only_dir), before);

        let loaded = store::load_conversation_adapter_sqlx(&pool, "default", &prepared.id)
            .await
            .expect("load prepared adapter")
            .expect("prepared adapter should be persisted");
        assert_eq!(loaded.id, prepared.id);
        assert_eq!(
            loaded.manifest_path.as_deref().map(Path::new),
            prepared.manifest_path.as_deref().map(Path::new)
        );
        assert_eq!(
            loaded.executable_path.as_deref().map(Path::new),
            prepared.executable_path.as_deref().map(Path::new)
        );

        pool.close().await;
        let _ = fs::remove_dir_all(fixture_root);
    }

    fn directory_entries(path: &Path) -> Vec<String> {
        let mut entries = fs::read_dir(path)
            .expect("read fixture directory")
            .map(|entry| {
                entry
                    .expect("read directory entry")
                    .file_name()
                    .to_string_lossy()
                    .into()
            })
            .collect::<Vec<_>>();
        entries.sort();
        entries
    }

    #[cfg(unix)]
    fn set_read_only_directory(permissions: &mut fs::Permissions) {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o555);
    }

    #[cfg(not(unix))]
    fn set_read_only_directory(permissions: &mut fs::Permissions) {
        permissions.set_readonly(true);
    }

    #[cfg(unix)]
    fn restore_writable_directory(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)
            .expect("read directory metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("restore writable directory");
    }

    #[cfg(not(unix))]
    fn restore_writable_directory(path: &Path) {
        let mut permissions = fs::metadata(path)
            .expect("read directory metadata")
            .permissions();
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions).expect("restore writable directory");
    }
}
