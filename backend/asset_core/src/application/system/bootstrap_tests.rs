use crate::backend::{
    domain::{ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState},
    infrastructure::runtime::RuntimeRole,
    store,
};
use std::{fs, path::Path};

fn file_snapshot(path: &Path) -> (Vec<u8>, Option<std::time::SystemTime>) {
    (
        fs::read(path).expect("read adapter file"),
        fs::metadata(path)
            .expect("read adapter metadata")
            .modified()
            .ok(),
    )
}

fn isolated_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("assetiweave-{tag}-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create isolated root");
    root
}

#[tokio::test]
async fn bootstrap_and_reconcile_never_rewrite_managed_adapter_files() {
    let root = isolated_root("bootstrap-managed-adapters");
    let managed_root = root.join("conversation-adapters");
    let adapter_dir = managed_root.join("codex");
    crate::backend::infrastructure::conversations::test_fixtures::copy_builtin_adapter(
        "codex",
        &adapter_dir,
    )
    .expect("stage adapter package");
    let script_path = adapter_dir.join("adapter.mjs");
    let policy_path = adapter_dir.join("payload-policy.mjs");
    let manifest_path = adapter_dir.join("conversation-adapter.json");
    fs::write(&script_path, "// edited by the user\n").expect("edit adapter script");
    fs::write(&policy_path, "// edited payload policy\n").expect("edit payload policy");
    let before = [
        file_snapshot(&script_path),
        file_snapshot(&policy_path),
        file_snapshot(&manifest_path),
    ];

    let db_path = root.join("app.db");
    for pass in 0..2 {
        let runtime = crate::backend::application::AppService::bootstrap_runtime_with_managed_root(
            db_path.clone(),
            RuntimeRole::OneShot,
            managed_root.clone(),
        )
        .await
        .unwrap_or_else(|error| panic!("bootstrap pass {pass} failed: {error}"));
        let tenant_id = runtime.context().tenant.id.clone();
        let adapter = store::list_conversation_adapters_sqlx(runtime.db().pool(), &tenant_id)
            .await
            .expect("list adapters")
            .into_iter()
            .find(|adapter| adapter.id == "codex")
            .expect("managed adapter must be projected from disk");
        assert_eq!(
            adapter.manifest_path.as_deref(),
            Some(manifest_path.to_string_lossy().as_ref()),
            "projection must point at the managed directory copy"
        );
        drop(runtime);
    }

    let pool = store::open_migrated_pool(&db_path)
        .await
        .expect("open database");
    crate::backend::application::system::conversation_adapters::reconcile_app_conversation_adapters(
        &pool, "default",
    )
    .await
    .expect("reconcile adapters");
    pool.close().await;

    let after = [
        file_snapshot(&script_path),
        file_snapshot(&policy_path),
        file_snapshot(&manifest_path),
    ];
    assert_eq!(
        after, before,
        "bootstrap and reconcile must not rewrite user-edited adapter files"
    );
}

#[tokio::test]
async fn bootstrap_projects_no_built_in_adapters_for_an_empty_managed_directory() {
    let root = isolated_root("bootstrap-empty-adapters");
    let managed_root = root.join("conversation-adapters");

    let runtime = crate::backend::application::AppService::bootstrap_runtime_with_managed_root(
        root.join("app.db"),
        RuntimeRole::OneShot,
        managed_root.clone(),
    )
    .await
    .expect("bootstrap fresh installation");

    let tenant_id = runtime.context().tenant.id.clone();
    let adapters = store::list_conversation_adapters_sqlx(runtime.db().pool(), &tenant_id)
        .await
        .expect("list adapters");
    assert!(
        adapters
            .iter()
            .all(|adapter| adapter.trust_state != ConversationAdapterTrustState::BuiltIn),
        "a fresh installation must not fabricate built-in adapter records: {adapters:?}"
    );
    assert!(
        !managed_root.join("codex").exists(),
        "bootstrapping must not release adapter script copies"
    );
    drop(runtime);
}

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
    crate::backend::application::system::test_support::seed_defaults_sqlx(&pool)
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
        projection_version: Some(1),
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    };

    let seed_result =
        super::seed_prepared_builtin_adapters(&pool, "default", std::slice::from_ref(&prepared))
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

#[tokio::test]
async fn seed_rejects_invalid_paths_before_persisting_adapters() {
    let fixture_root = std::env::temp_dir().join(format!(
        "assetiweave-invalid-adapter-seed-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&fixture_root).expect("create database directory");
    let db_path = fixture_root.join("seed.sqlite");
    let pool = store::open_migrated_pool(&db_path)
        .await
        .expect("open test database");
    crate::backend::application::system::test_support::seed_defaults_sqlx(&pool)
        .await
        .expect("seed generic defaults");

    let invalid = ConversationAdapter {
        id: "invalid-prepared-fixture".to_string(),
        name: "Invalid Prepared Fixture".to_string(),
        kind: ConversationAdapterKind::External,
        version: "1.0.0".to_string(),
        enabled: true,
        manifest_path: Some("   ".to_string()),
        executable_path: Some("/tmp/adapter.mjs".to_string()),
        content_hash: None,
        trusted_hash: None,
        trust_state: ConversationAdapterTrustState::Trusted,
        protocol_version: Some(1),
        capabilities: vec!["probe".to_string()],
        input_kinds: Vec::new(),
        card_contract_version: None,
        card_kinds: Vec::new(),
        projection_version: Some(1),
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    };

    let result =
        super::seed_prepared_builtin_adapters(&pool, "default", std::slice::from_ref(&invalid))
            .await;

    assert!(result.is_err(), "invalid adapter paths should be rejected");
    assert!(
        store::load_conversation_adapter_sqlx(&pool, "default", &invalid.id)
            .await
            .expect("load adapter")
            .is_none(),
        "invalid adapter must not be persisted before path preparation"
    );

    pool.close().await;
    let _ = fs::remove_dir_all(fixture_root);
}

#[tokio::test]
async fn open_migrated_pool_drops_legacy_task_journal_table() {
    let fixture_root =
        std::env::temp_dir().join(format!("assetiweave-drop-journal-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&fixture_root).expect("create fixture dir");
    let db_path = fixture_root.join("migrated.sqlite");
    let pool = store::open_migrated_pool(&db_path)
        .await
        .expect("open migrated database");

    let journal_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'task_journal'",
    )
    .fetch_one(&pool)
    .await
    .expect("query sqlite_master");

    assert_eq!(
        journal_exists, 0,
        "task_journal table should be dropped by migration"
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
