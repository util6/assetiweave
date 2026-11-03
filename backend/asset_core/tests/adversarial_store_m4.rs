use asset_core::domain::conversations::*;
use asset_core::store::*;
use chrono::Utc;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

struct TempTestDb {
    path: PathBuf,
}

impl TempTestDb {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("aiw-m4-adv-{name}-{}.sqlite", Uuid::new_v4()));
        Self { path }
    }
}

impl Drop for TempTestDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(format!("{}-shm", self.path.display()));
        let _ = std::fs::remove_file(format!("{}-wal", self.path.display()));
    }
}

fn sample_package(package_id: &str, adapter_id: &str, version: &str) -> ConversationAdapterPackage {
    let now = Utc::now().to_rfc3339();
    ConversationAdapterPackage {
        package_id: package_id.to_string(),
        adapter_id: adapter_id.to_string(),
        name: format!("Package {package_id}"),
        version: version.to_string(),
        record_kind: ConversationAdapterPackageRecordKind::Session,
        install_dir: format!("/tmp/packages/{package_id}/versions/{version}"),
        manifest_path: format!("/tmp/packages/{package_id}/versions/{version}/package.json"),
        adapter_manifest_path: format!(
            "/tmp/packages/{package_id}/versions/{version}/adapter.json"
        ),
        runtime_protocol: "acp".to_string(),
        runtime_ready: true,
        origin: ConversationAdapterPackageOrigin::ManagedRelease,
        source_url: Some("https://github.com/util6/assetiweave".to_string()),
        git_ref: Some("main".to_string()),
        git_commit: Some("commit123".to_string()),
        catalog_url: Some("https://catalog.example.com".to_string()),
        update_policy: ConversationPackageUpdatePolicy::Manual,
        latest_version: Some(version.to_string()),
        last_checked_at: Some(now.clone()),
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
        runtime_validated_at: Some(now.clone()),
        installed_content_hash: Some("content-hash-v1".to_string()),
        trusted_package_hash: Some("package-hash-v1".to_string()),
        error_message: None,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn sample_adapter(adapter_id: &str, version: &str) -> ConversationAdapter {
    let now = Utc::now().to_rfc3339();
    ConversationAdapter {
        id: adapter_id.to_string(),
        name: format!("Adapter {adapter_id}"),
        kind: ConversationAdapterKind::External,
        version: version.to_string(),
        enabled: true,
        manifest_path: Some(format!("/tmp/adapters/{adapter_id}/adapter.json")),
        executable_path: Some(format!("/tmp/adapters/{adapter_id}/run.sh")),
        content_hash: Some("content-hash-v1".to_string()),
        trusted_hash: Some("trusted-hash-v1".to_string()),
        trust_state: ConversationAdapterTrustState::Trusted,
        protocol_version: Some(1),
        capabilities: vec!["sessions".to_string()],
        input_kinds: vec![ConversationSourceKind::Directory],
        card_contract_version: Some(1),
        card_kinds: vec![],
        projection_version: Some(1),
        created_at: now.clone(),
        updated_at: now,
    }
}

fn sample_version(
    package_id: &str,
    version: &str,
    artifact_hash: &str,
    content_hash: &str,
) -> ConversationAdapterPackageVersion {
    let now = Utc::now().to_rfc3339();
    ConversationAdapterPackageVersion {
        package_id: package_id.to_string(),
        version: version.to_string(),
        install_dir: format!("/tmp/packages/{package_id}/versions/{version}"),
        artifact_hash: Some(artifact_hash.to_string()),
        content_hash: content_hash.to_string(),
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
        installed_at: now,
    }
}

fn sample_release(
    catalog_url: &str,
    package_id: &str,
    adapter_id: &str,
    version: &str,
) -> ConversationAdapterCatalogRelease {
    let now = Utc::now().to_rfc3339();
    ConversationAdapterCatalogRelease {
        catalog_url: catalog_url.to_string(),
        package_id: package_id.to_string(),
        adapter_id: adapter_id.to_string(),
        name: format!("Release {package_id}"),
        publisher: "AssetIWeave".to_string(),
        version: version.to_string(),
        channel: ConversationAdapterReleaseChannel::Stable,
        released_at: Some(now.clone()),
        core_compatibility: ">=0.7.0".to_string(),
        artifact_url: format!("https://catalog.example.com/{package_id}-{version}.tar.gz"),
        artifact_size: Some(1024 * 1024),
        artifact_sha256: "sha256-artifact-release".to_string(),
        changelog_markdown: "# Changelog\nInitial release".to_string(),
        breaking_change: false,
        runtime_protocol: "acp".to_string(),
        record_kind: ConversationAdapterPackageRecordKind::Session,
        package_manifest_file: "package.json".to_string(),
        adapter_manifest_file: "adapter.json".to_string(),
        adapter_manifest_json: None,
        source_json: None,
        etag: Some("etag-12345".to_string()),
        fetched_at: now,
    }
}

// ============================================================================
// CHALLENGE 1: JSON Deserialization Torture Tests
// ============================================================================

#[tokio::test]
async fn challenge1_1_torture_capability_meta_json_booleans_and_flexible_deserializer() {
    let test_db = TempTestDb::new("torture-bools");
    let pool = open_migrated_pool(&test_db.path).await.expect("open pool");

    let test_cases: Vec<(&str, serde_json::Value, bool)> = vec![
        ("bool-true", serde_json::json!(true), true),
        ("bool-false", serde_json::json!(false), false),
        ("int-1", serde_json::json!(1), true),
        ("int-0", serde_json::json!(0), false),
        ("int-42", serde_json::json!(42), true),
        ("int-neg1", serde_json::json!(-1), true),
        ("str-1", serde_json::json!("1"), true),
        ("str-0", serde_json::json!("0"), false),
        ("str-true", serde_json::json!("true"), true),
        ("str-false", serde_json::json!("false"), false),
    ];

    for (pkg_suffix, runtime_ready_val, expected_bool) in test_cases {
        let pkg_id = format!("pkg-deser-{pkg_suffix}");
        let adapter_id = format!("adapter-deser-{pkg_suffix}");

        let meta = serde_json::json!({
            "record_kind": "session",
            "manifest_path": "/tmp/manifest.json",
            "adapter_manifest_path": "/tmp/adapter.json",
            "runtime_protocol": "acp",
            "runtime_ready": runtime_ready_val,
            "origin": "managed_release",
            "update_policy": "manual",
            "runtime_gate_status": "ready"
        });

        sqlx::query(
            r#"
            INSERT INTO app_extensions (
                extension_id, capability_kind, target_id, display_name, active_version,
                distribution_type, install_dir, enabled, installation_status, runtime_status,
                error_code, error_message, checked_at, installed_at, updated_at, capability_meta_json
            ) VALUES (
                ?1, 'conversation.adapter', ?2, 'Test Adapter', '1.0.0',
                'archive', '/tmp/install', 1, 'ready', 'ready',
                NULL, NULL, '2026-10-31T00:00:00Z', '2026-10-31T00:00:00Z', '2026-10-31T00:00:00Z', ?3
            )
            "#,
        )
        .bind(&pkg_id)
        .bind(&adapter_id)
        .bind(meta.to_string())
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("failed to insert {pkg_id}: {e}"));

        let loaded = load_conversation_adapter_package_sqlx(&pool, &pkg_id)
            .await
            .unwrap_or_else(|e| panic!("failed to load {pkg_id}: {e}"))
            .unwrap_or_else(|| panic!("package {pkg_id} not found"));

        assert_eq!(
            loaded.runtime_ready, expected_bool,
            "failed expectation for {pkg_suffix}"
        );
    }
}

#[tokio::test]
async fn challenge1_2_torture_capability_meta_json_invalid_inputs_fail_closed_without_panic() {
    let test_db = TempTestDb::new("torture-invalid");
    let pool = open_migrated_pool(&test_db.path).await.expect("open pool");

    let invalid_cases: Vec<(&str, serde_json::Value)> = vec![
        ("null-bool", serde_json::json!(null)),
        ("invalid-string", serde_json::json!("not_a_boolean")),
        ("uppercase-true", serde_json::json!("TRUE")),
        ("object-bool", serde_json::json!({"bad": "value"})),
        ("array-bool", serde_json::json!([1, 2, 3])),
        (
            "missing-manifest",
            serde_json::json!({
                "record_kind": "session",
                "runtime_protocol": "acp",
                "runtime_ready": true,
                "origin": "managed_release",
                "update_policy": "manual",
                "runtime_gate_status": "ready"
            }),
        ),
        (
            "invalid-enum",
            serde_json::json!({
                "record_kind": "unsupported_unknown_kind",
                "manifest_path": "/tmp/manifest.json",
                "adapter_manifest_path": "/tmp/adapter.json",
                "runtime_protocol": "acp",
                "runtime_ready": true,
                "origin": "managed_release",
                "update_policy": "manual",
                "runtime_gate_status": "ready"
            }),
        ),
    ];

    for (case_name, meta_payload) in invalid_cases {
        let pkg_id = format!("pkg-invalid-{case_name}");
        let adapter_id = format!("adapter-invalid-{case_name}");

        let meta_str = if meta_payload.is_object() && meta_payload.get("record_kind").is_none() {
            // just an invalid runtime_ready value
            serde_json::json!({
                "record_kind": "session",
                "manifest_path": "/tmp/manifest.json",
                "adapter_manifest_path": "/tmp/adapter.json",
                "runtime_protocol": "acp",
                "runtime_ready": meta_payload,
                "origin": "managed_release",
                "update_policy": "manual",
                "runtime_gate_status": "ready"
            })
            .to_string()
        } else if meta_payload.is_object() {
            meta_payload.to_string()
        } else {
            serde_json::json!({
                "record_kind": "session",
                "manifest_path": "/tmp/manifest.json",
                "adapter_manifest_path": "/tmp/adapter.json",
                "runtime_protocol": "acp",
                "runtime_ready": meta_payload,
                "origin": "managed_release",
                "update_policy": "manual",
                "runtime_gate_status": "ready"
            })
            .to_string()
        };

        sqlx::query(
            r#"
            INSERT INTO app_extensions (
                extension_id, capability_kind, target_id, display_name, active_version,
                distribution_type, install_dir, enabled, installation_status, runtime_status,
                error_code, error_message, checked_at, installed_at, updated_at, capability_meta_json
            ) VALUES (
                ?1, 'conversation.adapter', ?2, 'Test Adapter', '1.0.0',
                'archive', '/tmp/install', 1, 'ready', 'ready',
                NULL, NULL, '2026-10-31T00:00:00Z', '2026-10-31T00:00:00Z', '2026-10-31T00:00:00Z', ?3
            )
            "#,
        )
        .bind(&pkg_id)
        .bind(&adapter_id)
        .bind(&meta_str)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("failed to insert {pkg_id}: {e}"));

        // Must fail closed with StoreError::External, NEVER PANIC!
        let load_res = load_conversation_adapter_package_sqlx(&pool, &pkg_id).await;
        assert!(
            load_res.is_err(),
            "case {case_name} should fail closed but returned: {load_res:?}"
        );
    }

    // Invariant: Non-JSON raw string must be rejected by SQLite CHECK(json_valid(...))
    let check_constraint_res = sqlx::query(
        r#"
        INSERT INTO app_extensions (
            extension_id, capability_kind, target_id, display_name, active_version,
            distribution_type, install_dir, enabled, installation_status, runtime_status,
            error_code, error_message, checked_at, installed_at, updated_at, capability_meta_json
        ) VALUES (
            'pkg-bad-json', 'conversation.adapter', 'adapter-bad-json', 'Test Adapter', '1.0.0',
            'archive', '/tmp/install', 1, 'ready', 'ready',
            NULL, NULL, '2026-10-31T00:00:00Z', '2026-10-31T00:00:00Z', '2026-10-31T00:00:00Z', '{invalid json syntax'
        )
        "#,
    )
    .execute(&pool)
    .await;
    assert!(
        check_constraint_res.is_err(),
        "SQLite CHECK (json_valid) constraint must reject malformed JSON"
    );
}

#[tokio::test]
async fn challenge1_3_torture_extra_keys_and_large_payloads_survive() {
    let test_db = TempTestDb::new("torture-large");
    let pool = open_migrated_pool(&test_db.path).await.expect("open pool");

    // 1. Extra arbitrary nested keys in capability_meta_json (forward compatibility)
    let meta_with_extras = serde_json::json!({
        "record_kind": "session",
        "manifest_path": "/tmp/manifest.json",
        "adapter_manifest_path": "/tmp/adapter.json",
        "runtime_protocol": "acp",
        "runtime_ready": true,
        "origin": "managed_release",
        "update_policy": "manual",
        "runtime_gate_status": "ready",
        "future_capability_flag": true,
        "nested_extra_data": {
            "tags": ["alpha", "beta", "gamma"],
            "score": 99.99,
            "deep": {
                "key": "value"
            }
        }
    });

    sqlx::query(
        r#"
        INSERT INTO app_extensions (
            extension_id, capability_kind, target_id, display_name, active_version,
            distribution_type, install_dir, enabled, installation_status, runtime_status,
            error_code, error_message, checked_at, installed_at, updated_at, capability_meta_json
        ) VALUES (
            'pkg-extra-keys', 'conversation.adapter', 'adapter-extra-keys', 'Extra Keys Adapter', '1.0.0',
            'archive', '/tmp/install', 1, 'ready', 'ready',
            NULL, NULL, '2026-10-31T00:00:00Z', '2026-10-31T00:00:00Z', '2026-10-31T00:00:00Z', ?1
        )
        "#,
    )
    .bind(meta_with_extras.to_string())
    .execute(&pool)
    .await
    .expect("insert extra keys");

    let loaded = load_conversation_adapter_package_sqlx(&pool, "pkg-extra-keys")
        .await
        .expect("load extra keys package")
        .expect("package exists");
    assert_eq!(loaded.package_id, "pkg-extra-keys");
    assert!(loaded.runtime_ready);

    // 2. Large 1MB payload in release_meta_json changelog_markdown
    let large_string = "A".repeat(1024 * 1024); // 1 MB
    let release_meta_large = serde_json::json!({
        "core_compatibility": ">=0.7.0",
        "changelog_markdown": large_string,
        "breaking_change": 1,
        "runtime_protocol": "acp",
        "record_kind": "session",
        "package_manifest_file": "package.json",
        "adapter_manifest_file": "adapter.json"
    });

    sqlx::query(
        r#"
        INSERT INTO app_extension_catalog_releases (
            catalog_source, extension_id, version, capability_kind, target_id,
            display_name, publisher, channel, released_at, artifact_url,
            artifact_size, artifact_sha256, release_meta_json, fetched_at
        ) VALUES (
            'https://catalog.example.com', 'pkg-large-release', '1.0.0', 'conversation.adapter', 'adapter-large',
            'Large Release Adapter', 'Publisher', 'stable', '2026-10-31T00:00:00Z', 'https://catalog.example.com/art.zip',
            1048576, 'sha256-large', ?1, '2026-10-31T00:00:00Z'
        )
        "#,
    )
    .bind(release_meta_large.to_string())
    .execute(&pool)
    .await
    .expect("insert 1MB release meta");

    let releases = list_conversation_adapter_catalog_releases_sqlx(
        &pool,
        "https://catalog.example.com",
        Some("pkg-large-release"),
    )
    .await
    .expect("list catalog releases");
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].changelog_markdown.len(), 1024 * 1024);
    assert!(
        releases[0].breaking_change,
        "breaking_change 1 must deserialize to true"
    );
}

// ============================================================================
// CHALLENGE 2: Version Conflict & State Mutation Invariants
// ============================================================================

#[tokio::test]
async fn challenge2_1_version_immutability_and_hash_conflict_rollback() {
    let test_db = TempTestDb::new("version-conflict");
    let pool = open_migrated_pool(&test_db.path).await.expect("open pool");

    let package = sample_package("pkg-immutable", "adapter-immutable", "1.0.0");
    let adapter = sample_adapter("adapter-immutable", "1.0.0");
    let original_version =
        sample_version("pkg-immutable", "1.0.0", "artifact-sha-A", "content-sha-A");

    // 1. Initial activation must succeed
    activate_conversation_adapter_package_sqlx(&pool, &adapter, &package, &original_version)
        .await
        .expect("initial activation succeeds");

    let versions = list_conversation_adapter_package_versions_sqlx(&pool, "pkg-immutable")
        .await
        .expect("list versions");
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].artifact_hash.as_deref(), Some("artifact-sha-A"));
    assert_eq!(versions[0].content_hash, "content-sha-A");

    // 2. Conflicting artifact hash on same version -> must return StoreError::Conflict
    let conflicting_artifact = sample_version(
        "pkg-immutable",
        "1.0.0",
        "artifact-sha-CONFLICT",
        "content-sha-A",
    );
    let mut updated_package = package.clone();
    updated_package.name = "Tampered Package Name".to_string();
    let err = activate_conversation_adapter_package_sqlx(
        &pool,
        &adapter,
        &updated_package,
        &conflicting_artifact,
    )
    .await
    .expect_err("conflicting artifact hash must be rejected");
    match err {
        StoreError::Conflict(msg) => {
            assert!(
                msg.contains("immutable"),
                "expected immutability conflict message: {msg}"
            );
        }
        other => panic!("expected StoreError::Conflict, got: {other:?}"),
    }

    // Verify rollback: package name and version hash must be completely unmutated
    let stored_pkg = load_conversation_adapter_package_sqlx(&pool, "pkg-immutable")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored_pkg.name, "Package pkg-immutable",
        "package name must roll back cleanly"
    );

    let versions_after = list_conversation_adapter_package_versions_sqlx(&pool, "pkg-immutable")
        .await
        .unwrap();
    assert_eq!(
        versions_after[0].artifact_hash.as_deref(),
        Some("artifact-sha-A")
    );

    // 3. Conflicting content hash on same version -> must return StoreError::Conflict
    let conflicting_content = sample_version(
        "pkg-immutable",
        "1.0.0",
        "artifact-sha-A",
        "content-sha-CONFLICT",
    );
    let err2 =
        activate_conversation_adapter_package_sqlx(&pool, &adapter, &package, &conflicting_content)
            .await
            .expect_err("conflicting content hash must be rejected");
    match err2 {
        StoreError::Conflict(msg) => {
            assert!(msg.contains("immutable"));
        }
        other => panic!("expected StoreError::Conflict, got: {other:?}"),
    }

    // 4. Same hashes -> idempotent activation succeeds
    activate_conversation_adapter_package_sqlx(&pool, &adapter, &package, &original_version)
        .await
        .expect("idempotent activation with same hashes succeeds");
}

#[tokio::test]
async fn challenge2_2_workspace_mode_clears_versions_completely() {
    let test_db = TempTestDb::new("workspace-clear");
    let pool = open_migrated_pool(&test_db.path).await.expect("open pool");

    let mut package = sample_package("pkg-workspace", "adapter-workspace", "1.0.0");
    let adapter = sample_adapter("adapter-workspace", "1.0.0");
    let v1 = sample_version("pkg-workspace", "1.0.0", "art-1", "cnt-1");
    let v2 = sample_version("pkg-workspace", "1.1.0", "art-2", "cnt-2");

    // Activate v1.0.0 and v1.1.0
    activate_conversation_adapter_package_sqlx(&pool, &adapter, &package, &v1)
        .await
        .expect("activate v1");
    package.version = "1.1.0".to_string();
    activate_conversation_adapter_package_sqlx(&pool, &adapter, &package, &v2)
        .await
        .expect("activate v2");

    let versions = list_conversation_adapter_package_versions_sqlx(&pool, "pkg-workspace")
        .await
        .expect("list versions");
    assert_eq!(
        versions.len(),
        2,
        "must have 2 versions before workspace switch"
    );

    // Switch to workspace mode
    package.origin = ConversationAdapterPackageOrigin::LocalDirectory;
    activate_conversation_adapter_workspace_sqlx(&pool, &adapter, &package)
        .await
        .expect("activate workspace mode");

    // Assert that versions are completely cleared
    let versions_after = list_conversation_adapter_package_versions_sqlx(&pool, "pkg-workspace")
        .await
        .expect("list versions after workspace mode");
    assert_eq!(
        versions_after.len(),
        0,
        "workspace mode must purge all app_extension_versions"
    );

    // Assert that distribution_type in app_extensions is workspace
    let row: (String, String) = sqlx::query_as(
        "SELECT distribution_type, active_version FROM app_extensions WHERE extension_id = 'pkg-workspace'"
    )
    .fetch_one(&pool)
    .await
    .expect("fetch extension");
    assert_eq!(row.0, "workspace", "distribution_type must be workspace");
}

#[tokio::test]
async fn challenge2_3_delete_package_version_constraints_and_rollback() {
    let test_db = TempTestDb::new("delete-version-rollback");
    let pool = open_migrated_pool(&test_db.path).await.expect("open pool");

    let package = sample_package("pkg-del", "adapter-del", "1.0.0");
    let adapter = sample_adapter("adapter-del", "1.0.0");
    let v1 = sample_version("pkg-del", "1.0.0", "art-1", "cnt-1");

    activate_conversation_adapter_package_sqlx(&pool, &adapter, &package, &v1)
        .await
        .expect("activate v1");

    // Contradictory request: replacement_package AND delete_package=true
    let invalid_call = delete_conversation_adapter_package_version_sqlx(
        &pool,
        "pkg-del",
        "1.0.0",
        Some(&package),
        true,
    )
    .await;
    match invalid_call {
        Err(StoreError::Validation(msg)) => {
            assert!(
                msg.contains("cannot replace and delete"),
                "expected validation error: {msg}"
            );
        }
        other => panic!("expected Validation error, got: {other:?}"),
    }

    // Deleting a non-existent version returns Ok(false) without crashing
    let not_found =
        delete_conversation_adapter_package_version_sqlx(&pool, "pkg-del", "9.9.9", None, false)
            .await
            .expect("delete non-existent version");
    assert!(!not_found, "should return false when rows_affected == 0");

    // Version still exists
    let versions = list_conversation_adapter_package_versions_sqlx(&pool, "pkg-del")
        .await
        .unwrap();
    assert_eq!(versions.len(), 1);
}

// ============================================================================
// CHALLENGE 3: High Concurrency Operations Stress Test
// ============================================================================

#[tokio::test]
async fn challenge3_1_concurrent_activation_succeeds_cleanly_under_begin_immediate_with_retry() {
    let test_db = TempTestDb::new("concurrency-contention");
    let pool = Arc::new(open_migrated_pool(&test_db.path).await.expect("open pool"));

    let mut handles = Vec::new();
    // 12 concurrent writer tasks calling activate_conversation_adapter_package_sqlx concurrently
    for i in 0..12 {
        let pool_clone = Arc::clone(&pool);
        let handle = tokio::spawn(async move {
            let pkg_id = format!("pkg-raw-{i}");
            let adapter_id = format!("adapter-raw-{i}");
            let pkg = sample_package(&pkg_id, &adapter_id, "1.0.0");
            let adapter = sample_adapter(&adapter_id, "1.0.0");
            let ver = sample_version(&pkg_id, "1.0.0", &format!("art-{i}"), &format!("cnt-{i}"));

            activate_conversation_adapter_package_sqlx(&pool_clone, &adapter, &pkg, &ver).await
        });
        handles.push(handle);
    }

    let mut lock_error_encountered = false;
    let mut success_count = 0;
    for handle in handles {
        match handle.await.unwrap() {
            Ok(_) => success_count += 1,
            Err(err) => {
                let err_str = err.to_string();
                if err_str.contains("database is locked")
                    || err_str.contains("code: 517")
                    || err_str.contains("code: 5")
                {
                    lock_error_encountered = true;
                }
            }
        }
    }

    println!(
        "[CONCURRENCY REMEDIATION] Success: {success_count}/12, Lock error encountered: {lock_error_encountered}"
    );
    assert_eq!(
        success_count, 12,
        "all 12 concurrent activations must succeed under begin_immediate_with_retry"
    );
    assert!(
        !lock_error_encountered,
        "no lock errors (SQLITE_BUSY / SQLITE_BUSY_SNAPSHOT) expected under begin_immediate_with_retry"
    );

    // Verify all 12 extension records are persisted in app_extensions
    let ext_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM app_extensions WHERE capability_kind = 'conversation.adapter' AND extension_id LIKE 'pkg-raw-%'",
    )
    .fetch_one(&*pool)
    .await
    .expect("count raw packages");
    assert_eq!(
        ext_count, 12,
        "all 12 packages must be persisted in app_extensions"
    );

    // Verify all 12 adapter records are persisted in conversation_adapters
    let adapter_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_adapters WHERE id LIKE 'adapter-raw-%'",
    )
    .fetch_one(&*pool)
    .await
    .expect("count raw adapters");
    assert_eq!(
        adapter_count, 12,
        "all 12 adapters must be persisted in conversation_adapters"
    );

    // Verify all 12 extension versions are persisted in app_extension_versions
    let ver_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM app_extension_versions WHERE extension_id LIKE 'pkg-raw-%'",
    )
    .fetch_one(&*pool)
    .await
    .expect("count raw versions");
    assert_eq!(
        ver_count, 12,
        "all 12 versions must be persisted in app_extension_versions"
    );
}

#[tokio::test]
async fn challenge3_2_concurrent_reads_and_retried_writes_maintain_consistency_under_wal() {
    let test_db = TempTestDb::new("concurrency-stress-retried");
    let pool = Arc::new(open_migrated_pool(&test_db.path).await.expect("open pool"));

    // Pre-seed catalog releases
    for i in 0..5 {
        let release = sample_release(
            "https://catalog.example.com",
            &format!("pkg-conc-{i}"),
            &format!("adapter-conc-{i}"),
            "1.0.0",
        );
        upsert_conversation_adapter_catalog_release_sqlx(&pool, &release)
            .await
            .expect("upsert release");
    }

    let mut handles = Vec::new();

    // 8 concurrent writer tasks with retry logic (as recommended for begin_immediate_with_retry)
    for i in 0..8 {
        let pool_clone = Arc::clone(&pool);
        let handle = tokio::spawn(async move {
            let pkg_id = format!("pkg-conc-{i}");
            let adapter_id = format!("adapter-conc-{i}");
            let pkg = sample_package(&pkg_id, &adapter_id, "1.0.0");
            let adapter = sample_adapter(&adapter_id, "1.0.0");
            let ver = sample_version(
                &pkg_id,
                "1.0.0",
                &format!("hash-art-{i}"),
                &format!("hash-cnt-{i}"),
            );

            // Retry loop simulating resilient caller
            let mut attempts = 0;
            loop {
                attempts += 1;
                match activate_conversation_adapter_package_sqlx(&pool_clone, &adapter, &pkg, &ver)
                    .await
                {
                    Ok(_) => break,
                    Err(e) if attempts < 15 && e.to_string().contains("database is locked") => {
                        tokio::time::sleep(tokio::time::Duration::from_millis(20 * attempts)).await;
                    }
                    Err(e) => panic!("activate failed after {attempts} attempts: {e}"),
                }
            }

            // Update install dir with retry
            let mut attempts = 0;
            loop {
                attempts += 1;
                match update_conversation_adapter_package_version_install_dir_sqlx(
                    &pool_clone,
                    &pkg_id,
                    "1.0.0",
                    &format!("/opt/custom/install/{pkg_id}"),
                )
                .await
                {
                    Ok(_) => break,
                    Err(e) if attempts < 15 && e.to_string().contains("database is locked") => {
                        tokio::time::sleep(tokio::time::Duration::from_millis(20 * attempts)).await;
                    }
                    Err(e) => panic!("update dir failed after {attempts} attempts: {e}"),
                }
            }
        });
        handles.push(handle);
    }

    // 10 concurrent reader tasks repeatedly querying packages, versions, and releases
    for i in 0..10 {
        let pool_clone = Arc::clone(&pool);
        let handle = tokio::spawn(async move {
            for _ in 0..5 {
                let _ = list_conversation_adapter_packages_sqlx(&pool_clone).await;
                let _ = list_conversation_adapter_catalog_releases_sqlx(
                    &pool_clone,
                    "https://catalog.example.com",
                    None,
                )
                .await;
                let target_pkg = format!("pkg-conc-{}", i % 5);
                let _ = load_conversation_adapter_package_sqlx(&pool_clone, &target_pkg).await;
                let _ =
                    list_conversation_adapter_package_versions_sqlx(&pool_clone, &target_pkg).await;
                tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
            }
        });
        handles.push(handle);
    }

    // Await all 18 concurrent tasks
    for (idx, handle) in handles.into_iter().enumerate() {
        handle
            .await
            .unwrap_or_else(|e| panic!("task {idx} panicked: {e}"));
    }

    // Assert final database state consistency
    let all_packages = list_conversation_adapter_packages_sqlx(&pool)
        .await
        .expect("final package list");
    assert_eq!(
        all_packages.len(),
        8,
        "all 8 writer tasks must have persisted packages"
    );

    for i in 0..8 {
        let pkg_id = format!("pkg-conc-{i}");
        let vers = list_conversation_adapter_package_versions_sqlx(&pool, &pkg_id)
            .await
            .expect("check versions");
        assert_eq!(vers.len(), 1);
        assert_eq!(vers[0].install_dir, format!("/opt/custom/install/{pkg_id}"));
    }
}

// ============================================================================
// CHALLENGE 4: Adapter Deactivation & Reactivation Lifecycle
// ============================================================================

#[tokio::test]
async fn challenge4_1_full_lifecycle_status_transitions_and_deactivation_cleanup() {
    let test_db = TempTestDb::new("lifecycle-transitions");
    let pool = open_migrated_pool(&test_db.path).await.expect("open pool");

    let pkg_id = "lifecycle-pkg";
    let adapter_id = "lifecycle-adapter";

    // Step 1: Install (upsert)
    let package_v1 = sample_package(pkg_id, adapter_id, "1.0.0");
    upsert_conversation_adapter_package_sqlx(&pool, &package_v1)
        .await
        .expect("step 1: install package");

    let ext_row: (i64, String, String, Option<String>) = sqlx::query_as(
        r#"
        SELECT enabled, installation_status, runtime_status, error_code
        FROM app_extensions WHERE extension_id = ?1
        "#,
    )
    .bind(pkg_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ext_row.0, 1, "enabled must be 1 on install");
    assert_eq!(ext_row.1, "ready", "installation_status must be ready");
    assert_eq!(ext_row.2, "ready", "runtime_status must be ready");
    assert!(ext_row.3.is_none(), "error_code must be None");

    // Step 2: Activate v1.0.0 and insert a conversation_source
    let adapter = sample_adapter(adapter_id, "1.0.0");
    let version_v1 = sample_version(pkg_id, "1.0.0", "hash-art-v1", "hash-cnt-v1");
    activate_conversation_adapter_package_sqlx(&pool, &adapter, &package_v1, &version_v1)
        .await
        .expect("step 2: activate package");

    // Add a source referencing this adapter
    sqlx::query(
        r#"
        INSERT INTO conversation_sources (
            tenant_id, id, adapter_id, name, kind, location, enabled, created_at, updated_at
        ) VALUES ('default', 'src-1', ?1, 'Test Source', 'directory', '/tmp/src', 1, '2026-10-31T00:00:00Z', '2026-10-31T00:00:00Z')
        "#,
    )
    .bind(adapter_id)
    .execute(&pool)
    .await
    .expect("insert test source");

    // Assert adapter and source exist and are enabled
    let adapter_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversation_adapters WHERE id = ?1")
            .bind(adapter_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(adapter_count, 1, "conversation_adapters row must exist");

    // Step 3: Deactivate package
    let deactivated_pkg = deactivate_conversation_adapter_package_sqlx(&pool, pkg_id, adapter_id)
        .await
        .expect("step 3: deactivate package");
    assert!(
        !deactivated_pkg.runtime_ready,
        "runtime_ready must become false"
    );
    assert_eq!(
        deactivated_pkg.runtime_gate_status,
        ConversationAdapterRuntimeGateStatus::RuntimeMissing
    );

    // Assert table states:
    // a) conversation_adapters row was deleted
    let adapter_count_after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversation_adapters WHERE id = ?1")
            .bind(adapter_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        adapter_count_after, 0,
        "conversation_adapters row must be deleted on deactivate"
    );

    // b) conversation_sources enabled was set to 0
    let source_enabled: i64 =
        sqlx::query_scalar("SELECT enabled FROM conversation_sources WHERE id = 'src-1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        source_enabled, 0,
        "conversation_sources must be disabled on deactivate"
    );

    // c) app_extensions updated to broken / failed / runtime_missing
    let ext_after: (i64, String, String, Option<String>, Option<String>) = sqlx::query_as(
        r#"
        SELECT enabled, installation_status, runtime_status, error_code, error_message
        FROM app_extensions WHERE extension_id = ?1
        "#,
    )
    .bind(pkg_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ext_after.0, 0, "enabled must be 0 after deactivate");
    assert_eq!(
        ext_after.1, "broken",
        "installation_status must be broken after deactivate"
    );
    assert_eq!(
        ext_after.2, "failed",
        "runtime_status must be failed after deactivate"
    );
    assert_eq!(
        ext_after.3.as_deref(),
        Some("runtime_missing"),
        "error_code must be runtime_missing"
    );
    assert_eq!(
        ext_after.4.as_deref(),
        Some("conversation adapter package is uninstalled")
    );

    // d) app_extension_versions for v1.0.0 is STILL PRESERVED!
    let versions_preserved = list_conversation_adapter_package_versions_sqlx(&pool, pkg_id)
        .await
        .unwrap();
    assert_eq!(
        versions_preserved.len(),
        1,
        "versions must be preserved after deactivation"
    );

    // Step 4: Reactivate & Upgrade to v1.1.0
    let package_v2 = sample_package(pkg_id, adapter_id, "1.1.0");
    let adapter_v2 = sample_adapter(adapter_id, "1.1.0");
    let version_v2 = sample_version(pkg_id, "1.1.0", "hash-art-v2", "hash-cnt-v2");

    activate_conversation_adapter_package_sqlx(&pool, &adapter_v2, &package_v2, &version_v2)
        .await
        .expect("step 4: upgrade and reactivate package");

    // Assert app_extensions restored to ready and active_version is 1.1.0
    let ext_upgraded: (String, i64, String, String, Option<String>) = sqlx::query_as(
        r#"
        SELECT active_version, enabled, installation_status, runtime_status, error_code
        FROM app_extensions WHERE extension_id = ?1
        "#,
    )
    .bind(pkg_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ext_upgraded.0, "1.1.0", "active_version updated to 1.1.0");
    assert_eq!(ext_upgraded.1, 1, "enabled restored to 1");
    assert_eq!(
        ext_upgraded.2, "ready",
        "installation_status restored to ready"
    );
    assert_eq!(ext_upgraded.3, "ready", "runtime_status restored to ready");
    assert!(ext_upgraded.4.is_none(), "error_code cleared");

    // Adapter row restored
    let adapter_restored: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversation_adapters WHERE id = ?1")
            .bind(adapter_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(adapter_restored, 1, "conversation_adapters row restored");

    // Version history now contains both 1.0.0 and 1.1.0
    let all_versions = list_conversation_adapter_package_versions_sqlx(&pool, pkg_id)
        .await
        .unwrap();
    assert_eq!(all_versions.len(), 2, "must contain both 1.0.0 and 1.1.0");
    assert_eq!(all_versions[0].version, "1.1.0");
    assert_eq!(all_versions[1].version, "1.0.0");

    // Step 5: Full Uninstall (Delete package version with delete_package = true)
    delete_conversation_adapter_package_version_sqlx(&pool, pkg_id, "1.1.0", None, true)
        .await
        .expect("delete package");

    let pkg_final = load_conversation_adapter_package_sqlx(&pool, pkg_id)
        .await
        .unwrap();
    assert!(
        pkg_final.is_none(),
        "package should be completely removed from app_extensions"
    );

    // Cascading delete on app_extension_versions
    let versions_final = list_conversation_adapter_package_versions_sqlx(&pool, pkg_id)
        .await
        .unwrap();
    assert_eq!(
        versions_final.len(),
        0,
        "foreign key cascade must remove all orphan versions"
    );
}
