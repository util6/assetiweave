use super::*;
use crate::backend::{
    domain::{
        AppKind, AssetKind, ConversationAdapter, ConversationAdapterKind,
        ConversationAdapterPackage, ConversationAdapterPackageOrigin,
        ConversationAdapterPackageRecordKind, ConversationAdapterPackageVersion,
        ConversationAdapterRuntimeGateStatus, ConversationAdapterTrustState,
        ConversationPackageUpdatePolicy, DeploymentStrategy, ProfileSafety, RuleSet, TargetProfile,
    },
    store::Database,
};
use uuid::Uuid;

#[tokio::test]
async fn normalize_existing_profiles_migrates_home_paths_before_store_upsert() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-bootstrap-profile-normalization-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let absolute_path = dirs::home_dir()
        .expect("home directory")
        .join(".codex")
        .join("skills")
        .to_string_lossy()
        .to_string();
    let profile = TargetProfile {
        id: "profile-home".to_string(),
        name: "Home profile".to_string(),
        app_kind: Some(AppKind::Codex),
        target_provider_id: "codex".to_string(),
        target_paths: vec![absolute_path],
        supported_kinds: vec![AssetKind::Skill],
        deployment_strategy: DeploymentStrategy::SymlinkToSource,
        enabled: true,
        include: empty_rules(),
        exclude: empty_rules(),
        safety: ProfileSafety {
            allow_remove: false,
            allow_overwrite: false,
        },
    };
    crate::backend::store::upsert_profile_sqlx(database.pool(), "default", &profile)
        .await
        .expect("seed profile with an absolute path");

    normalize_existing_profiles_sqlx(database.pool(), "default")
        .await
        .expect("normalize existing profile paths");
    let persisted =
        crate::backend::store::load_profile_sqlx(database.pool(), "default", &profile.id)
            .await
            .expect("load normalized profile")
            .expect("profile exists");

    assert_eq!(persisted.target_paths, vec!["~/.codex/skills"]);
    drop(database);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}

#[tokio::test]
async fn normalize_conversation_paths_migrates_legacy_paths_outside_store() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-bootstrap-conversation-paths-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let install_dir = crate::backend::infrastructure::host_paths::HostDirectories::current()
        .expect("host directories")
        .config
        .join("assetiweave")
        .join("conversation-adapters")
        .join("path-normalization-fixture");
    let install_dir = install_dir.to_string_lossy().to_string();
    let adapter = ConversationAdapter {
        id: "path-normalization-fixture".to_string(),
        name: "Path normalization fixture".to_string(),
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
        projection_version: Some(1),
        created_at: "2026-09-23T00:00:00Z".to_string(),
        updated_at: "2026-09-23T00:00:00Z".to_string(),
    };
    let package = ConversationAdapterPackage {
        package_id: "path-normalization-fixture".to_string(),
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
    crate::backend::store::activate_conversation_adapter_package_sqlx(
        database.pool(),
        &adapter,
        &package,
        &version,
    )
    .await
    .expect("seed legacy absolute paths");

    crate::backend::application::system::conversation_adapters::normalize_conversation_paths(
        database.pool(),
        "default",
    )
    .await
    .expect("normalize conversation paths during bootstrap");

    let expected_install_dir =
        "@config/assetiweave/conversation-adapters/path-normalization-fixture";
    let stored_adapter = crate::backend::store::load_conversation_adapter_sqlx(
        database.pool(),
        "default",
        &adapter.id,
    )
    .await
    .expect("load normalized adapter")
    .expect("adapter exists");
    let stored_package = crate::backend::store::load_conversation_adapter_package_sqlx(
        database.pool(),
        &package.package_id,
    )
    .await
    .expect("load normalized package")
    .expect("package exists");
    let stored_version = crate::backend::store::list_conversation_adapter_package_versions_sqlx(
        database.pool(),
        &package.package_id,
    )
    .await
    .expect("load normalized package version")
    .remove(0);
    assert_eq!(
        stored_adapter.manifest_path.as_deref(),
        Some("@config/assetiweave/conversation-adapters/path-normalization-fixture/conversation-adapter.json")
    );
    assert_eq!(stored_package.install_dir, expected_install_dir);
    assert_eq!(stored_version.install_dir, expected_install_dir);

    drop(database);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}

fn empty_rules() -> RuleSet {
    RuleSet {
        kinds: Vec::new(),
        tags: Vec::new(),
        groups: Vec::new(),
        sources: Vec::new(),
        path_patterns: Vec::new(),
    }
}
