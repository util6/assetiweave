use crate::backend::domain::{
    ConversationAdapter, ConversationAdapterKind, ConversationAdapterPackage,
    ConversationAdapterPackageOrigin, ConversationAdapterPackageRecordKind,
    ConversationAdapterPackageVersion, ConversationAdapterRuntimeGateStatus,
    ConversationAdapterTrustState, ConversationPackageUpdatePolicy,
};
use crate::backend::infrastructure::path_utils::{
    normalize_conversation_adapter_package_paths, normalize_conversation_adapter_paths,
    normalize_conversation_adapter_version_paths,
};

#[cfg(unix)]
#[test]
fn conversation_adapter_paths_normalize_to_config_anchor() {
    let install_dir = crate::backend::infrastructure::host_paths::HostDirectories::current()
        .expect("host directories")
        .config
        .join("assetiweave")
        .join("conversation-adapters")
        .join("path-normalization-fixture")
        .to_string_lossy()
        .to_string();
    let mut adapter = ConversationAdapter {
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
        created_at: "2026-09-23T00:00:00Z".to_string(),
        updated_at: "2026-09-23T00:00:00Z".to_string(),
    };
    let mut package = ConversationAdapterPackage {
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
    let mut version = ConversationAdapterPackageVersion {
        package_id: package.package_id.clone(),
        version: package.version.clone(),
        install_dir,
        artifact_hash: None,
        content_hash: "content-hash".to_string(),
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::RuntimeMissing,
        installed_at: package.created_at.clone(),
    };

    normalize_conversation_adapter_paths(&mut adapter).expect("normalize adapter paths");
    normalize_conversation_adapter_package_paths(&mut package).expect("normalize package paths");
    normalize_conversation_adapter_version_paths(&mut version).expect("normalize version paths");

    let normalized_dir = "@config/assetiweave/conversation-adapters/path-normalization-fixture";
    assert_eq!(
        adapter.manifest_path.as_deref(),
        Some("@config/assetiweave/conversation-adapters/path-normalization-fixture/conversation-adapter.json")
    );
    assert_eq!(package.install_dir, normalized_dir);
    assert_eq!(version.install_dir, normalized_dir);
}
