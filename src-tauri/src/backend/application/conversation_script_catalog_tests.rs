use super::*;
use std::io::Cursor;

fn extract_install_artifact_bytes(
    spec: &ConversationAdapterPackageInstallSpec,
    bytes: Vec<u8>,
    staging_dir: &Path,
) -> AppResult<PathBuf> {
    super::super::conversation_adapter_installer::extract_install_artifact_reader(
        spec,
        Cursor::new(bytes),
        staging_dir,
    )
}

fn catalog_item(id: &str, adapter_id: Option<&str>) -> ConversationScriptCatalogItem {
    ConversationScriptCatalogItem {
        id: id.to_string(),
        name: "Codex Session Parser".to_string(),
        version: "1.0.0".to_string(),
        record_kind: ConversationScriptRecordKind::Session,
        provider: Some("codex".to_string()),
        adapter_id: adapter_id.map(str::to_string),
        description: None,
        homepage_url: None,
        repository_url: None,
        tags: Vec::new(),
        manifest_file: None,
        package_manifest_file: None,
        expected_content_hash: None,
        expected_package_hash: None,
        expected_artifact_hash: None,
        artifact_size: None,
        source: ConversationScriptCatalogSource {
            kind: ConversationScriptCatalogSourceKind::Github,
            url: "https://github.com/util6/assetiweave/tree/main/builtin-assets/adapters/codex"
                .to_string(),
            branch: None,
            path: None,
        },
    }
}

#[test]
fn legacy_catalog_item_reverse_maps_to_native_install_spec() {
    let mut item = catalog_item("io.github.util6.codex-session", Some("codex"));
    item.expected_package_hash = Some("package-hash".to_string());
    item.expected_artifact_hash = Some("artifact-hash".to_string());
    item.artifact_size = Some(42);

    let spec = item.to_install_spec();

    assert_eq!(spec.id, item.id);
    assert_eq!(spec.adapter_id, item.adapter_id);
    assert_eq!(
        spec.record_kind,
        ConversationAdapterPackageRecordKind::Session
    );
    assert_eq!(spec.expected_package_hash, item.expected_package_hash);
    assert_eq!(spec.expected_artifact_hash, item.expected_artifact_hash);
    assert_eq!(spec.artifact_size, item.artifact_size);
    assert_eq!(
        spec.source.kind,
        ConversationAdapterPackageInstallSourceKind::Github
    );
    assert_eq!(spec.source.url, item.source.url);
}

#[test]
fn native_install_spec_github_source_preserves_tree_location() {
    let item = catalog_item("io.github.util6.codex-session", Some("codex"));
    let spec = item.to_install_spec();

    let location =
        super::super::conversation_adapter_installer::parse_github_install_source(&spec.source)
            .expect("parse install spec");

    assert_eq!(
        location.repo_url,
        "https://github.com/util6/assetiweave.git"
    );
    assert_eq!(location.branch.as_deref(), Some("main"));
    assert_eq!(
        location.path.as_deref(),
        Some("builtin-assets/adapters/codex")
    );
}

fn adapter(id: &str, version: &str) -> ConversationAdapter {
    ConversationAdapter {
        id: id.to_string(),
        name: "Codex".to_string(),
        kind: crate::backend::models::ConversationAdapterKind::External,
        version: version.to_string(),
        enabled: true,
        manifest_path: Some("/tmp/codex/conversation-adapter.json".to_string()),
        executable_path: Some("/tmp/codex/adapter.mjs".to_string()),
        content_hash: Some("hash".to_string()),
        trusted_hash: Some("hash".to_string()),
        trust_state: crate::backend::models::ConversationAdapterTrustState::Trusted,
        protocol_version: Some(1),
        capabilities: vec!["read_session".to_string()],
        input_kinds: Vec::new(),
        card_contract_version: None,
        card_kinds: Vec::new(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

fn package(id: &str, adapter_id: &str, version: &str) -> ConversationAdapterPackage {
    ConversationAdapterPackage {
        package_id: id.to_string(),
        adapter_id: adapter_id.to_string(),
        name: "Codex Session Parser".to_string(),
        version: version.to_string(),
        record_kind: ConversationAdapterPackageRecordKind::Session,
        install_dir: format!("/tmp/{id}/current"),
        manifest_path: format!("/tmp/{id}/current/conversation-adapter-package.json"),
        adapter_manifest_path: format!("/tmp/{id}/current/conversation-adapter.json"),
        runtime_protocol: "stdio-ndjson-v1".to_string(),
        runtime_ready: true,
        origin: ConversationAdapterPackageOrigin::ManagedRelease,
        source_url: None,
        git_ref: None,
        git_commit: None,
        catalog_url: None,
        update_policy: ConversationPackageUpdatePolicy::Manual,
        latest_version: Some(version.to_string()),
        last_checked_at: None,
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
        runtime_validated_at: None,
        installed_content_hash: Some("package-hash".to_string()),
        trusted_package_hash: Some("package-hash".to_string()),
        error_message: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

#[test]
fn resolves_installed_state_from_declared_adapter_id() {
    let entries = resolve_conversation_adapter_package_catalog_entries(
        vec![catalog_item("codex-session", Some("codex"))],
        &[adapter("codex", "1.0.0")],
        &[],
    );

    assert!(entries[0].installed);
    assert_eq!(entries[0].status, "legacy_installed");
    assert!(!entries[0].update_available);
    assert_eq!(entries[0].installed_adapter.as_ref().unwrap().id, "codex");
}

#[test]
fn marks_installed_package_with_different_version_as_update_available() {
    let entries = resolve_conversation_adapter_package_catalog_entries(
        vec![catalog_item("codex-session", Some("codex"))],
        &[adapter("codex", "0.9.0")],
        &[package("codex-session", "codex", "0.9.0")],
    );

    assert!(entries[0].installed);
    assert!(entries[0].update_available);
    assert!(!entries[0].ahead_of_release);
    assert_eq!(entries[0].status, "update_available");
}

#[test]
fn marks_installed_package_newer_than_catalog_as_ahead_of_release() {
    let entries = resolve_conversation_adapter_package_catalog_entries(
        vec![catalog_item("codex-session", Some("codex"))],
        &[adapter("codex", "1.1.0")],
        &[package("codex-session", "codex", "1.1.0")],
    );

    assert!(entries[0].installed);
    assert!(!entries[0].update_available);
    assert!(entries[0].ahead_of_release);
    assert_eq!(entries[0].status, "ahead_of_release");
}

#[test]
fn managed_package_without_registered_runtime_is_reported_as_uninstalled() {
    let mut package = package("codex-session", "codex", "1.0.0");
    package.runtime_ready = false;
    package.runtime_gate_status = ConversationAdapterRuntimeGateStatus::RuntimeMissing;

    let entries = resolve_conversation_adapter_package_catalog_entries(
        vec![catalog_item("codex-session", Some("codex"))],
        &[],
        &[package],
    );

    assert!(entries[0].installed);
    assert_eq!(entries[0].status, "uninstalled");
    assert!(!entries[0].runtime_ready);
}

#[test]
#[cfg(unix)]
fn uninstalled_replacement_uses_content_hash_and_rebases_manifest_paths() {
    let package = package("codex-session", "codex", "2.0.0");
    let replacement = crate::backend::models::ConversationAdapterPackageVersion {
        package_id: package.package_id.clone(),
        version: "1.0.0".to_string(),
        install_dir: "/tmp/codex-session/versions/1.0.0".to_string(),
        artifact_hash: Some("artifact-zip-hash".to_string()),
        content_hash: "unpacked-content-hash".to_string(),
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
        installed_at: "2026-07-17T00:00:00Z".to_string(),
    };

    let replaced = package_for_uninstalled_replacement(&package, &replacement);

    assert_eq!(replaced.version, "1.0.0");
    assert_eq!(
        replaced.trusted_package_hash.as_deref(),
        Some("unpacked-content-hash")
    );
    assert_eq!(
        replaced.manifest_path,
        "/tmp/codex-session/versions/1.0.0/conversation-adapter-package.json"
    );
    assert_eq!(
        replaced.adapter_manifest_path,
        "/tmp/codex-session/versions/1.0.0/conversation-adapter.json"
    );
}

#[test]
fn parses_github_tree_url_into_repo_branch_and_path() {
    let source = ConversationScriptCatalogSource {
        kind: ConversationScriptCatalogSourceKind::Github,
        url: "https://github.com/util6/assetiweave/tree/main/builtin-assets/adapters/codex"
            .to_string(),
        branch: None,
        path: None,
    };

    let location = parse_github_catalog_location(&source).expect("parse GitHub tree URL");

    assert_eq!(
        location.repo_url,
        "https://github.com/util6/assetiweave.git"
    );
    assert_eq!(location.branch.as_deref(), Some("main"));
    assert_eq!(
        location.path.as_deref(),
        Some("builtin-assets/adapters/codex"),
    );
}

#[test]
fn rejects_unsafe_manifest_file_names() {
    let mut item = catalog_item("codex-session", Some("codex"));
    item.manifest_file = Some("../conversation-adapter.json".to_string());

    assert!(validate_conversation_script_catalog_item(&item).is_err());

    let mut item = catalog_item("codex-session", Some("codex"));
    item.package_manifest_file = Some("../conversation-adapter-package.json".to_string());

    assert!(validate_conversation_script_catalog_item(&item).is_err());
}

#[test]
fn package_versions_require_semver_before_becoming_path_segments() {
    assert_eq!(
        validated_package_version("1.2.3-beta.1").unwrap(),
        "1.2.3-beta.1"
    );
    assert!(validated_package_version("latest").is_err());
    assert!(validated_package_version("1/../../external").is_err());
}

#[test]
fn rollback_selects_the_most_recently_installed_inactive_version() {
    let versions = vec![
        package_version("1.2.0", "2026-07-16T02:00:00Z"),
        package_version("1.1.0", "2026-07-16T01:00:00Z"),
        package_version("1.0.0", "2026-07-16T00:00:00Z"),
    ];

    assert_eq!(
        select_rollback_version(&versions, "1.2.0").map(|version| version.version.as_str()),
        Some("1.1.0")
    );
}

fn package_version(
    version: &str,
    installed_at: &str,
) -> crate::backend::models::ConversationAdapterPackageVersion {
    crate::backend::models::ConversationAdapterPackageVersion {
        package_id: "io.github.util6.test".to_string(),
        version: version.to_string(),
        install_dir: format!("/tmp/versions/{version}"),
        artifact_hash: None,
        content_hash: format!("hash-{version}"),
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
        installed_at: installed_at.to_string(),
    }
}

#[test]
fn artifact_zip_rejects_path_traversal() {
    use std::io::Write;

    let root =
        std::env::temp_dir().join(format!("assetiweave-artifact-traversal-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create artifact test root");
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("../escape.txt", zip::write::SimpleFileOptions::default())
        .expect("start unsafe zip entry");
    writer.write_all(b"escape").expect("write unsafe zip entry");
    let bytes = writer.finish().expect("finish zip").into_inner();
    let mut item = catalog_item("io.github.util6.escape-test", Some("escape-test"));
    item.source.kind = ConversationScriptCatalogSourceKind::ArtifactZip;

    let spec = item.to_install_spec();
    let result = extract_install_artifact_bytes(&spec, bytes, &root.join("staging"));

    assert!(result.is_err());
    assert!(!root.join("escape.txt").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn artifact_zip_rejects_windows_reserved_file_names() {
    use std::io::Write;

    let root = std::env::temp_dir().join(format!(
        "assetiweave-artifact-reserved-name-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create artifact test root");
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("package/CON.txt", zip::write::SimpleFileOptions::default())
        .expect("start reserved zip entry");
    writer.write_all(b"reserved").expect("write zip entry");
    let bytes = writer.finish().expect("finish zip").into_inner();
    let mut item = catalog_item("io.github.util6.reserved-test", Some("reserved-test"));
    item.source.kind = ConversationScriptCatalogSourceKind::ArtifactZip;

    let spec = item.to_install_spec();
    let error = extract_install_artifact_bytes(&spec, bytes, &root.join("staging"))
        .expect_err("reserved Windows name must be rejected");

    assert!(error.to_string().contains("reserved on Windows"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn artifact_zip_rejects_case_insensitive_path_collisions() {
    use std::io::Write;

    let root = std::env::temp_dir().join(format!(
        "assetiweave-artifact-case-collision-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create artifact test root");
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for entry_name in ["Package/Adapter.js", "package/adapter.js"] {
        writer
            .start_file(entry_name, zip::write::SimpleFileOptions::default())
            .expect("start colliding zip entry");
        writer.write_all(b"entry").expect("write zip entry");
    }
    let bytes = writer.finish().expect("finish zip").into_inner();
    let mut item = catalog_item("io.github.util6.collision-test", Some("collision-test"));
    item.source.kind = ConversationScriptCatalogSourceKind::ArtifactZip;

    let spec = item.to_install_spec();
    let error = extract_install_artifact_bytes(&spec, bytes, &root.join("staging"))
        .expect_err("case-insensitive collision must be rejected");

    assert!(error.to_string().contains("colliding paths"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn managed_package_delete_target_rejects_external_install_dir() {
    let root = std::env::temp_dir().join(format!("assetiweave-package-delete-{}", Uuid::new_v4()));
    let managed_root = root.join("conversation-adapters");
    let external_dir = root.join("external").join("current");
    fs::create_dir_all(managed_root.join("packages").join("publisher.package"))
        .expect("create managed package root");
    fs::create_dir_all(&external_dir).expect("create external package");

    let result =
        validate_managed_package_delete_target(&managed_root, "publisher.package", &external_dir);

    assert!(result.is_err());
    assert!(external_dir.exists());
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn managed_package_delete_target_rejects_symlink_escape() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!("assetiweave-package-symlink-{}", Uuid::new_v4()));
    let managed_root = root.join("conversation-adapters");
    let package_root = managed_root.join("packages").join("publisher.package");
    let external_dir = root.join("external");
    fs::create_dir_all(&package_root).expect("create managed package root");
    fs::create_dir_all(&external_dir).expect("create external package");
    symlink(&external_dir, package_root.join("current")).expect("create current symlink");

    let result = validate_managed_package_delete_target(
        &managed_root,
        "publisher.package",
        &package_root.join("current"),
    );

    assert!(result.is_err());
    assert!(external_dir.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_gate_errors_have_distinct_repair_states() {
    assert_eq!(
        classify_runtime_gate_error(
            Path::new("/missing/package"),
            "conversation adapter package root is not a directory"
        ),
        ConversationAdapterRuntimeGateStatus::RuntimeMissing
    );
    assert_eq!(
        classify_runtime_gate_error(
            Path::new("/existing/package"),
            "conversation adapter package content hash mismatch"
        ),
        ConversationAdapterRuntimeGateStatus::HashMismatch
    );
    assert_eq!(
        classify_runtime_gate_error(
            Path::new("/existing/package"),
            "conversation adapter package requires AssetIWeave core >= 9.0.0"
        ),
        ConversationAdapterRuntimeGateStatus::CoreIncompatible
    );
    assert_eq!(
        classify_runtime_gate_error(
            Path::new("/existing/package"),
            "conversation adapter package was not valid JSON"
        ),
        ConversationAdapterRuntimeGateStatus::ManifestInvalid
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn unregister_preflight_lists_affected_sources_and_running_sync_conflicts() {
    let root =
        std::env::temp_dir().join(format!("assetiweave-package-preflight-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create preflight test root");
    let service = AppService::open_with_db_path(root.join("app.db"))
        .await
        .expect("open service");
    let adapter = adapter("external-preflight", "1.0.0");
    let source = ConversationSource {
        id: "external-preflight-source".to_string(),
        adapter_id: adapter.id.clone(),
        name: "External preflight source".to_string(),
        kind: crate::backend::models::ConversationSourceKind::Directory,
        location: root.join("sessions").to_string_lossy().to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: "2026-07-15T00:00:00Z".to_string(),
        updated_at: "2026-07-15T00:00:00Z".to_string(),
    };
    let tenant_id = service.tenant_id().to_string();
    crate::backend::store::upsert_conversation_adapter_sqlx(
        service.db.pool(),
        &tenant_id,
        &adapter,
    )
    .await
    .map_err(AppError::external)
    .expect("seed adapter");
    crate::backend::store::upsert_conversation_source_sqlx(service.db.pool(), &tenant_id, &source)
        .await
        .map_err(AppError::external)
        .expect("seed source");
    sqlx::query(
        r#"
            INSERT INTO conversation_sync_runs (
                tenant_id, id, source_id, adapter_id, status, started_at,
                session_count, turn_count, warning_count
            ) VALUES (?1, 'running-sync', ?2, ?3, 'running',
                      '2026-07-15T00:00:00Z', 0, 0, 0)
            "#,
    )
    .bind(&tenant_id)
    .bind(&source.id)
    .bind(&adapter.id)
    .execute(service.db.pool())
    .await
    .map_err(AppError::external)
    .expect("seed preflight records");
    service
        .runtime
        .refresh_conversation_adapter_catalog()
        .await
        .expect("refresh test adapter catalog");

    let preflight = service
        .prepare_conversation_adapter_package_change(ConversationAdapterPackageChangeParams {
            action: ConversationAdapterPackageChangeAction::Unregister,
            package_id: None,
            adapter_id: Some("external-preflight".to_string()),
        })
        .await
        .expect("prepare unregister");

    assert_eq!(
        preflight.origin,
        ConversationAdapterPackageOrigin::LegacyExternal
    );
    assert_eq!(preflight.affected_sources.len(), 1);
    assert_eq!(preflight.task_conflicts, vec!["conversation_sync"]);
    assert!(preflight.preserves_conversation_records);
    assert!(preflight.confirmation_required);

    drop(service);
    let _ = fs::remove_dir_all(root);
}
#[tokio::test(flavor = "multi_thread")]
async fn package_preflight_detects_running_sync_in_another_tenant() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-cross-tenant-package-preflight-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create preflight test root");
    let service = AppService::open_with_db_path(root.join("app.db"))
        .await
        .expect("open service");
    let current_tenant_id = service.tenant_id().to_string();
    let adapter = adapter("cross-tenant-preflight", "1.0.0");
    let source = ConversationSource {
        id: "cross-tenant-preflight-source".to_string(),
        adapter_id: adapter.id.clone(),
        name: "Cross-tenant preflight source".to_string(),
        kind: crate::backend::models::ConversationSourceKind::Directory,
        location: root.join("sessions").to_string_lossy().to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: "2026-07-15T00:00:00Z".to_string(),
        updated_at: "2026-07-15T00:00:00Z".to_string(),
    };
    let pool = service.db.pool();
    let other_tenant = service
        .create_tenant(TenantCreateParams {
            name: "Other tenant".to_string(),
            slug: Some("other-tenant".to_string()),
            set_active: false,
        })
        .await
        .expect("create tenant");
    crate::backend::store::upsert_conversation_adapter_sqlx(pool, &current_tenant_id, &adapter)
        .await
        .expect("seed current tenant adapter");
    crate::backend::store::upsert_conversation_adapter_sqlx(pool, &other_tenant.id, &adapter)
        .await
        .expect("seed other tenant adapter");
    crate::backend::store::upsert_conversation_source_sqlx(pool, &other_tenant.id, &source)
        .await
        .expect("seed other tenant source");
    sqlx::query(
        r#"
            INSERT INTO conversation_sync_runs (
                tenant_id, id, source_id, adapter_id, status, started_at,
                session_count, turn_count, warning_count
            ) VALUES (?1, 'running-sync', ?2, ?3, 'running',
                      '2026-07-15T00:00:00Z', 0, 0, 0)
            "#,
    )
    .bind(&other_tenant.id)
    .bind(&source.id)
    .bind(&source.adapter_id)
    .execute(pool)
    .await
    .map_err(AppError::external)
    .expect("seed sync run");
    service
        .runtime
        .refresh_conversation_adapter_catalog()
        .await
        .expect("refresh test adapter catalog");

    let preflight = service
        .prepare_conversation_adapter_package_change(ConversationAdapterPackageChangeParams {
            action: ConversationAdapterPackageChangeAction::Unregister,
            package_id: None,
            adapter_id: Some("cross-tenant-preflight".to_string()),
        })
        .await
        .expect("prepare unregister");

    assert_eq!(preflight.task_conflicts, vec!["conversation_sync"]);

    drop(service);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "multi_thread")]
async fn builtin_unregister_preflight_allows_disable_and_retains_registration() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-builtin-disable-preflight-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service = AppService::open_with_db_path(root.join("app.db"))
        .await
        .expect("open service");
    let mut builtin = adapter("builtin-preflight", "1.0.0");
    builtin.trust_state = crate::backend::models::ConversationAdapterTrustState::BuiltIn;
    let tenant_id = service.tenant_id().to_string();
    crate::backend::store::upsert_conversation_adapter_sqlx(
        service.db.pool(),
        &tenant_id,
        &builtin,
    )
    .await
    .expect("seed built-in adapter");
    service
        .runtime
        .refresh_conversation_adapter_catalog()
        .await
        .expect("refresh test adapter catalog");

    let preflight = service
        .prepare_conversation_adapter_package_change(ConversationAdapterPackageChangeParams {
            action: ConversationAdapterPackageChangeAction::Unregister,
            package_id: None,
            adapter_id: Some("builtin-preflight".to_string()),
        })
        .await
        .expect("built-in disable preflight");
    assert_eq!(preflight.origin, ConversationAdapterPackageOrigin::BuiltIn);

    service
        .unregister_conversation_adapter(ConversationAdapterUnregisterParams {
            adapter_id: "builtin-preflight".to_string(),
            dry_run: false,
            yes: true,
        })
        .await
        .expect("disable built-in adapter");
    let retained = service
        .list_conversation_adapters()
        .expect("list adapters")
        .into_iter()
        .find(|adapter| adapter.id == "builtin-preflight")
        .expect("built-in registration retained");
    assert!(!retained.enabled);

    drop(service);
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn workspace_upgrade_promotes_only_a_probed_immutable_runtime_copy() {
    use std::os::unix::fs::PermissionsExt;

    let root =
        std::env::temp_dir().join(format!("assetiweave-workspace-upgrade-{}", Uuid::new_v4()));
    let package_dir = root.join("workspace").join("external-test");
    let managed_root = root.join("managed");
    fs::create_dir_all(&package_dir).expect("create workspace package");
    fs::write(
        package_dir.join("conversation-adapter-package.json"),
        r#"{
  "schema_version": 1,
  "package_id": "com.util6.external-test",
  "name": "External Test",
  "version": "1.0.0",
  "min_core_version": "0.1.0",
  "record_kind": "session",
  "adapter_manifest": "conversation-adapter.json",
  "capabilities": ["probe", "read_session"],
  "runtime": { "protocol": "stdio-ndjson-v1" },
  "changelog": []
}"#,
    )
    .expect("write package manifest");
    fs::write(
        package_dir.join("conversation-adapter.json"),
        r#"{
  "schema_version": 1,
  "id": "external-test",
  "name": "External Test",
  "version": "1.0.0",
  "protocol_version": 1,
  "command": ["adapter.sh"],
  "capabilities": ["probe", "read_session"],
  "input_kinds": ["directory"]
}"#,
    )
    .expect("write adapter manifest");
    let executable = package_dir.join("adapter.sh");
    let write_executable = |body: &str| {
        fs::write(&executable, body).expect("write adapter executable");
        let mut permissions = fs::metadata(&executable)
            .expect("adapter metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("make adapter executable");
    };
    write_executable(
            "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"complete\",\"item\":{\"revision\":1}}'\n",
        );

    let service = AppService::open_with_db_path(root.join("app.db"))
        .await
        .expect("open service");
    let first = promote_conversation_adapter_workspace_package(
        &service,
        &package_dir,
        &managed_root,
        false,
    )
    .await
    .expect("promote first workspace revision");
    let first_install = PathBuf::from(
        first["package"]["install_dir"]
            .as_str()
            .expect("first install dir"),
    );
    assert_ne!(first_install, package_dir);
    assert!(first_install.join("adapter.sh").is_file());

    write_executable(
            "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"complete\",\"item\":{\"revision\":2}}'\n",
        );
    let second = promote_conversation_adapter_workspace_package(
        &service,
        &package_dir,
        &managed_root,
        false,
    )
    .await
    .expect("promote second workspace revision");
    let second_install = PathBuf::from(
        second["package"]["install_dir"]
            .as_str()
            .expect("second install dir"),
    );
    assert_ne!(second_install, first_install);
    assert!(!first_install.exists());
    assert!(second_install.is_dir());

    write_executable("#!/bin/sh\ncat >/dev/null\nprintf 'invalid\\n'\n");
    let error = promote_conversation_adapter_workspace_package(
        &service,
        &package_dir,
        &managed_root,
        false,
    )
    .await
    .expect_err("reject invalid workspace revision");
    assert!(error.to_string().contains("probe failed"));
    let retained = service
        .load_conversation_adapter_package("com.util6.external-test")
        .await
        .expect("load retained package")
        .expect("retained package");
    assert_eq!(PathBuf::from(retained.install_dir), second_install);
    assert!(second_install.is_dir());

    fs::write(
        package_dir.join("conversation-adapter-package.json"),
        fs::read_to_string(package_dir.join("conversation-adapter-package.json"))
            .expect("read package manifest")
            .replace("\"version\": \"1.0.0\"", "\"version\": \"0.9.0\""),
    )
    .expect("write older package manifest");
    fs::write(
        package_dir.join("conversation-adapter.json"),
        fs::read_to_string(package_dir.join("conversation-adapter.json"))
            .expect("read adapter manifest")
            .replace("\"version\": \"1.0.0\"", "\"version\": \"0.9.0\""),
    )
    .expect("write older adapter manifest");
    write_executable(
            "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"complete\",\"item\":{\"revision\":3}}'\n",
        );
    let skipped = promote_conversation_adapter_workspace_package(
        &service,
        &package_dir,
        &managed_root,
        false,
    )
    .await
    .expect("skip older workspace revision");
    assert_eq!(skipped["upgraded"], false);
    assert_eq!(skipped["skipped"], true);
    assert_eq!(skipped["reason"], "active_version_newer");
    assert_eq!(skipped["active_version"], "1.0.0");
    let retained_after_skip = service
        .load_conversation_adapter_package("com.util6.external-test")
        .await
        .expect("load package after skipped downgrade")
        .expect("retained package after skipped downgrade");
    assert_eq!(
        PathBuf::from(retained_after_skip.install_dir),
        second_install
    );

    drop(service);
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn local_registration_and_unregistration_never_modify_external_package_files() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!("assetiweave-local-package-{}", Uuid::new_v4()));
    let package_dir = root.join("external-package");
    fs::create_dir_all(&package_dir).expect("create local package");
    fs::write(
        package_dir.join("conversation-adapter-package.json"),
        r#"{
  "schema_version": 1,
  "package_id": "com.util6.external-test",
  "name": "External Test",
  "version": "1.0.0",
  "min_core_version": "0.1.0",
  "record_kind": "session",
  "adapter_manifest": "conversation-adapter.json",
  "capabilities": ["probe", "read_session"],
  "runtime": { "protocol": "stdio-ndjson-v1" },
  "changelog": []
}"#,
    )
    .expect("write package manifest");
    fs::write(
        package_dir.join("conversation-adapter.json"),
        r#"{
  "schema_version": 1,
  "id": "external-test",
  "name": "External Test",
  "version": "1.0.0",
  "protocol_version": 1,
  "command": ["adapter.sh"],
  "capabilities": ["probe", "read_session"],
  "input_kinds": ["directory"]
}"#,
    )
    .expect("write adapter manifest");
    let executable = package_dir.join("adapter.sh");
    fs::write(
            &executable,
            "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"complete\",\"item\":{\"ok\":true}}'\n",
        )
        .expect("write adapter executable");
    let mut permissions = fs::metadata(&executable)
        .expect("adapter metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).expect("make adapter executable");
    let discovered =
        discover_local_conversation_adapter_packages(&root).expect("discover local package");
    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].id, "com.util6.external-test");
    assert_eq!(discovered[0].adapter_id.as_deref(), Some("external-test"));
    assert_eq!(
        discovered[0].source.kind,
        ConversationScriptCatalogSourceKind::LocalDirectory
    );
    assert_eq!(discovered[0].source.url, package_dir.to_string_lossy());
    let content_before =
        crate::backend::conversations::validate_conversation_adapter_package_dir(&package_dir)
            .expect("validate external package")
            .content_hash;

    let service = AppService::open_with_db_path(root.join("app.db"))
        .await
        .expect("open service");
    service
        .register_conversation_adapter_local(ConversationAdapterLocalRegisterParams {
            package_dir: package_dir.to_string_lossy().to_string(),
            origin: ConversationAdapterPackageOrigin::LocalDirectory,
            source_url: None,
            git_ref: None,
            git_commit: None,
            dry_run: false,
            yes: true,
        })
        .await
        .expect("register local package");
    let registered = service
        .load_conversation_adapter_package("com.util6.external-test")
        .await
        .expect("load package")
        .expect("registered package");
    assert_eq!(
        registered.origin,
        ConversationAdapterPackageOrigin::LocalDirectory
    );

    service
        .unregister_conversation_adapter(ConversationAdapterUnregisterParams {
            adapter_id: "external-test".to_string(),
            dry_run: false,
            yes: true,
        })
        .await
        .expect("unregister local package");

    assert!(package_dir.is_dir());
    assert_eq!(
        crate::backend::conversations::validate_conversation_adapter_package_dir(&package_dir)
            .expect("revalidate external package")
            .content_hash,
        content_before
    );
    assert!(service
        .load_conversation_adapter_package("com.util6.external-test")
        .await
        .expect("load unregistered package")
        .is_none());

    drop(service);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn conversation_script_catalog_uses_reqwest_not_ureq() {
    let source = include_str!("conversation_script_catalog.rs");
    assert!(!source.contains(concat!("ur", "eq::")));
}

#[test]
fn conversation_script_catalog_fetch_text_loopback() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 4096];
        let _ = stream.read(&mut request);
        let body = "test catalog text";
        let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
        let _ = stream.write_all(response.as_bytes());
    });
    let result = fetch_catalog_text(&format!("http://{address}/catalog.txt")).unwrap();
    assert_eq!(result, "test catalog text");
    server.join().unwrap();
}
