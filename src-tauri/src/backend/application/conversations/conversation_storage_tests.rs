use super::*;
use crate::backend::domain::{
    ConversationAdapterKind, ConversationAdapterPackageOrigin,
    ConversationAdapterPackageRecordKind, ConversationAdapterPackageVersion,
    ConversationAdapterRuntimeGateStatus, ConversationAdapterTrustState,
    ConversationPackageUpdatePolicy,
};
use crate::backend::store::Database;
use uuid::Uuid;

#[cfg(unix)]
#[tokio::test]
async fn application_adapter_and_package_storage_normalizes_paths_before_store() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-app-conversation-storage-paths-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let install_dir = crate::backend::infrastructure::host_paths::HostDirectories::current()
        .expect("host directories")
        .config
        .join("assetiweave")
        .join("conversation-adapters")
        .join("application-path-fixture")
        .to_string_lossy()
        .to_string();
    let adapter = ConversationAdapter {
        id: "application-path-fixture".to_string(),
        name: "Application path fixture".to_string(),
        kind: ConversationAdapterKind::External,
        version: "1.0.0".to_string(),
        enabled: true,
        manifest_path: Some(format!("{install_dir}/conversation-adapter.json")),
        executable_path: Some(format!("{install_dir}/adapter.mjs")),
        content_hash: None,
        trusted_hash: None,
        trust_state: ConversationAdapterTrustState::Trusted,
        protocol_version: Some(1),
        capabilities: Vec::new(),
        input_kinds: Vec::new(),
        card_contract_version: None,
        card_kinds: Vec::new(),
        created_at: "2026-09-23T00:00:00Z".to_string(),
        updated_at: "2026-09-23T00:00:00Z".to_string(),
    };
    let package = ConversationAdapterPackage {
        package_id: adapter.id.clone(),
        adapter_id: adapter.id.clone(),
        name: adapter.name.clone(),
        version: adapter.version.clone(),
        record_kind: ConversationAdapterPackageRecordKind::Session,
        install_dir: install_dir.clone(),
        manifest_path: format!("{install_dir}/conversation-adapter-package.json"),
        adapter_manifest_path: format!("{install_dir}/conversation-adapter.json"),
        runtime_protocol: "stdio-ndjson-v1".to_string(),
        runtime_ready: false,
        origin: ConversationAdapterPackageOrigin::ManagedRelease,
        source_url: None,
        git_ref: None,
        git_commit: None,
        catalog_url: None,
        update_policy: ConversationPackageUpdatePolicy::Manual,
        latest_version: None,
        last_checked_at: None,
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::RuntimeMissing,
        runtime_validated_at: None,
        installed_content_hash: None,
        trusted_package_hash: None,
        error_message: None,
        created_at: "2026-09-23T00:00:00Z".to_string(),
        updated_at: "2026-09-23T00:00:00Z".to_string(),
    };
    let version = ConversationAdapterPackageVersion {
        package_id: package.package_id.clone(),
        version: package.version.clone(),
        install_dir: install_dir.clone(),
        artifact_hash: None,
        content_hash: "content-hash".to_string(),
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::RuntimeMissing,
        installed_at: package.created_at.clone(),
    };

    save_adapter(database.pool(), "default", &adapter)
        .await
        .expect("save adapter through application");
    save_package(database.pool(), &package)
        .await
        .expect("save package through application");
    activate_package(database.pool(), &adapter, &package, &version)
        .await
        .expect("activate package through application");
    let persisted_adapter = crate::backend::store::load_conversation_adapter_sqlx(
        database.pool(),
        "default",
        &adapter.id,
    )
    .await
    .expect("load persisted adapter")
    .expect("adapter exists");
    let persisted_package = crate::backend::store::load_conversation_adapter_package_sqlx(
        database.pool(),
        &package.package_id,
    )
    .await
    .expect("load persisted package")
    .expect("package exists");
    let persisted_version = crate::backend::store::list_conversation_adapter_package_versions_sqlx(
        database.pool(),
        &package.package_id,
    )
    .await
    .expect("load persisted package version")
    .remove(0);
    assert_eq!(
        persisted_adapter.manifest_path.as_deref(),
        Some("@config/assetiweave/conversation-adapters/application-path-fixture/conversation-adapter.json")
    );
    assert_eq!(
        persisted_package.install_dir,
        "@config/assetiweave/conversation-adapters/application-path-fixture"
    );
    assert_eq!(
        persisted_version.install_dir,
        "@config/assetiweave/conversation-adapters/application-path-fixture"
    );

    drop(database);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}
