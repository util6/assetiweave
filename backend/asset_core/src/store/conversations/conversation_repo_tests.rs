use super::*;
use crate::backend::domain::{
    derive_conversation_part_id, ConversationAdapterPackageOrigin,
    ConversationAdapterPackageRecordKind, ConversationAdapterRuntimeGateStatus,
    ConversationCardKindDefinition, ConversationContentCardDescriptor,
    ConversationPackageUpdatePolicy, ConversationPartKind, ConversationPartRole,
    NormalizedConversationPart, NormalizedConversationTurn,
};
use crate::backend::store::Database;
use uuid::Uuid;

const TEST_TENANT_ID: &str = "default";

#[tokio::test]
async fn sqlx_sync_import_reports_progress_and_rolls_back_cancelled_batch() {
    let db_path =
        std::env::temp_dir().join(format!("assetiweave-sync-cancel-{}.sqlite", Uuid::new_v4()));
    let database = Database::open_async(&db_path).await.unwrap();
    let adapter = test_conversation_adapter(
        "sync-cancel",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    async {
        let pool = database.pool();
        upsert_conversation_adapter_sqlx(pool, TEST_TENANT_ID, &adapter)
            .await
            .unwrap();
        upsert_conversation_source_sqlx(pool, TEST_TENANT_ID, &source)
            .await
            .unwrap();
        let mut session = fixture_session("v1");
        session.source_fingerprint = Some("original".to_string());
        import_conversation_sessions_sqlx(pool, TEST_TENANT_ID, &source, &[session.clone()], false)
            .await
            .unwrap();
        let mut changed = fixture_session("v2");
        changed.source_fingerprint = Some("changed".to_string());
        let mut second = changed.clone();
        second.external_id = "session-2".to_string();
        let sessions = vec![changed, second];
        let discovered = sessions
            .iter()
            .map(|s| s.external_id.clone())
            .collect::<BTreeSet<_>>();
        for presence in [None, Some(&discovered)] {
            let cancellation = tokio_util::sync::CancellationToken::new();
            let mut progress = Vec::new();
            let result = import_conversation_sessions_with_control_sqlx(
                pool,
                TEST_TENANT_ID,
                &source,
                &sessions,
                presence,
                false,
                Some(&cancellation),
                &mut |done, total| {
                    progress.push((done, total));
                    if done == 1 {
                        cancellation.cancel();
                    }
                },
            )
            .await;
            assert!(
                matches!(result, Err(StoreError::Cancelled(_))),
                "{result:?}"
            );
            assert_eq!(progress, vec![(0, 2), (1, 2)]);
            let fingerprints: Vec<Option<String>> = sqlx::query_scalar(
                "SELECT source_fingerprint FROM conversation_sessions WHERE tenant_id = ?1",
            )
            .bind(TEST_TENANT_ID)
            .fetch_all(pool)
            .await
            .unwrap();
            assert_eq!(fingerprints, vec![Some("original".to_string())]);
            let runs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversation_sync_runs")
                .fetch_one(pool)
                .await
                .unwrap();
            assert_eq!(runs, 1, "cancelled sync must not be marked complete");
        }
        let mut progress = Vec::new();
        import_conversation_sessions_with_control_sqlx(
            pool,
            TEST_TENANT_ID,
            &source,
            &sessions,
            Some(&discovered),
            false,
            None,
            &mut |done, total| progress.push((done, total)),
        )
        .await
        .unwrap();
        assert_eq!(progress, vec![(0, 2), (1, 2), (2, 2)]);
    }
    .await;
    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn recent_session_cwd_lookup_uses_session_and_turn_indexes() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-recent-session-query-plan-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let explain = format!("EXPLAIN QUERY PLAN {LIST_RECENT_CONVERSATION_SESSIONS_SQL}");
    #[derive(Debug, FromRow)]
    struct SqliteExplainQueryPlanRow {
        detail: String,
    }

    let details = async {
        let rows = sqlx::query_as::<_, SqliteExplainQueryPlanRow>(AssertSqlSafe(explain))
            .bind(TEST_TENANT_ID)
            .bind("2026-08-29T00:00:00Z")
            .bind("2026-09-01T00:00:00Z")
            .bind("")
            .fetch_all(database.pool())
            .await
            .map_err(StoreError::external)?;
        Ok::<_, StoreError>(rows.into_iter().map(|row| row.detail).collect::<Vec<_>>())
    }
    .await
    .expect("explain recent query");
    let plan = details.join("\n");

    assert!(
        plan.contains("idx_conversation_turns_tenant_session"),
        "{plan}"
    );
    assert!(
        !plan.contains("conversation_parts"),
        "recent query must not scan conversation_parts after last_cwd materialization: {plan}"
    );

    drop(database);
    let _ = std::fs::remove_file(db_path);
}

#[tokio::test]
async fn sqlx_conversation_metadata_round_trips_and_disables_sources() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-metadata-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let builtin_adapter = test_conversation_adapter(
        "metadata-builtin",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::BuiltIn,
    );
    let external_adapter = test_conversation_adapter(
        "metadata-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&external_adapter.id);

    let (
        adapters,
        loaded_adapter,
        sources,
        loaded_source,
        disabled_source,
        disabled_builtin,
        deleted_adapter,
        source_after_adapter_delete,
        missing_adapter,
    ) = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &builtin_adapter).await?;
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &external_adapter)
            .await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;

        let adapters = list_conversation_adapters_sqlx(database.pool(), TEST_TENANT_ID).await?;
        let loaded_adapter =
            load_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &external_adapter.id)
                .await?;
        let sources = list_conversation_sources_sqlx(database.pool(), TEST_TENANT_ID).await?;
        let loaded_source =
            load_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source.id).await?;
        let disabled_source =
            disable_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source.id).await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
        let disabled_builtin =
            delete_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &builtin_adapter.id)
                .await?;
        let deleted_adapter =
            delete_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &external_adapter.id)
                .await?;
        let source_after_adapter_delete =
            load_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source.id)
                .await?
                .expect("source is retained after adapter delete");
        let missing_adapter =
            load_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &external_adapter.id)
                .await?;

        Ok::<_, StoreError>((
            adapters,
            loaded_adapter,
            sources,
            loaded_source,
            disabled_source,
            disabled_builtin,
            deleted_adapter,
            source_after_adapter_delete,
            missing_adapter,
        ))
    }
    .await
    .expect("query SQLx conversation metadata repo");

    assert!(adapters.iter().any(|adapter| adapter == &external_adapter));
    assert_eq!(loaded_adapter.as_ref(), Some(&external_adapter));
    assert!(sources.iter().any(|candidate| candidate == &source));
    assert_eq!(loaded_source.as_ref(), Some(&source));
    assert_eq!(disabled_source.id, source.id);
    assert!(!disabled_source.enabled);
    assert_eq!(disabled_builtin.id, builtin_adapter.id);
    assert!(!disabled_builtin.enabled);
    assert_eq!(deleted_adapter, external_adapter);
    assert_eq!(source_after_adapter_delete.id, source.id);
    assert!(!source_after_adapter_delete.enabled);
    assert!(missing_adapter.is_none());

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
#[cfg(unix)]
async fn sqlx_conversation_adapter_packages_round_trip_by_package_and_adapter() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-package-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let install_dir = crate::backend::infrastructure::host_paths::HostDirectories::current()
        .expect("host directories")
        .config
        .join("assetiweave")
        .join("conversation-adapters")
        .join("packages")
        .join("codex-session")
        .join("current");
    let package = ConversationAdapterPackage {
        package_id: "codex-session".to_string(),
        adapter_id: "codex".to_string(),
        name: "Codex Session Parser".to_string(),
        version: "1.0.0".to_string(),
        record_kind: ConversationAdapterPackageRecordKind::Session,
        install_dir: install_dir.to_string_lossy().to_string(),
        manifest_path: install_dir
            .join("conversation-adapter-package.json")
            .to_string_lossy()
            .to_string(),
        adapter_manifest_path: install_dir
            .join("conversation-adapter.json")
            .to_string_lossy()
            .to_string(),
        runtime_protocol: "stdio-ndjson-v1".to_string(),
        runtime_ready: true,
        origin: ConversationAdapterPackageOrigin::ManagedRelease,
        source_url: Some("https://github.com/util6/assetiweave".to_string()),
        git_ref: Some("refs/tags/conversation-adapter-packages-main".to_string()),
        git_commit: Some("abc123".to_string()),
        catalog_url: Some("https://example.com/index.json".to_string()),
        update_policy: ConversationPackageUpdatePolicy::Manual,
        latest_version: Some("1.1.0".to_string()),
        last_checked_at: Some("2026-07-04T01:00:00Z".to_string()),
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
        runtime_validated_at: Some("2026-07-04T02:00:00Z".to_string()),
        installed_content_hash: Some("package-hash".to_string()),
        trusted_package_hash: Some("package-hash".to_string()),
        error_message: None,
        created_at: "2026-07-04T00:00:00Z".to_string(),
        updated_at: "2026-07-04T00:00:00Z".to_string(),
    };

    let (listed, listed_after_switch, by_package, by_adapter, deleted, missing) = async {
        upsert_conversation_adapter_package_sqlx(database.pool(), &package).await?;
        let listed = list_conversation_adapter_packages_sqlx(database.pool()).await?;
        let listed_after_switch = list_conversation_adapter_packages_sqlx(database.pool()).await?;
        let by_package =
            load_conversation_adapter_package_sqlx(database.pool(), &package.package_id).await?;
        let by_adapter =
            load_conversation_adapter_package_by_adapter_sqlx(database.pool(), &package.adapter_id)
                .await?;
        let deleted =
            delete_conversation_adapter_package_sqlx(database.pool(), &package.package_id).await?;
        let missing =
            load_conversation_adapter_package_sqlx(database.pool(), &package.package_id).await?;
        Ok::<_, StoreError>((
            listed,
            listed_after_switch,
            by_package,
            by_adapter,
            deleted,
            missing,
        ))
    }
    .await
    .expect("round trip package");

    let stored_package = package;
    assert_eq!(listed, vec![stored_package.clone()]);
    assert_eq!(listed_after_switch, vec![stored_package.clone()]);
    assert_eq!(by_package, Some(stored_package.clone()));
    assert_eq!(by_adapter, Some(stored_package.clone()));
    assert_eq!(deleted, Some(stored_package));
    assert!(missing.is_none());

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
#[cfg(unix)]
async fn conversation_adapter_store_preserves_prepared_paths() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-adapter-config-path-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let mut adapter = test_conversation_adapter(
        "portable-adapter",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let adapter_dir = crate::backend::infrastructure::host_paths::HostDirectories::current()
        .expect("host directories")
        .config
        .join("assetiweave")
        .join("conversation-adapters")
        .join("portable-adapter");
    adapter.manifest_path = Some(
        adapter_dir
            .join("conversation-adapter.json")
            .to_string_lossy()
            .to_string(),
    );
    adapter.executable_path = Some(
        adapter_dir
            .join("adapter.mjs")
            .to_string_lossy()
            .to_string(),
    );

    let loaded = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
        load_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter.id).await
    }
    .await
    .expect("round trip adapter")
    .expect("stored adapter");

    assert_eq!(loaded.manifest_path, adapter.manifest_path);
    assert_eq!(loaded.executable_path, adapter.executable_path);
    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn managed_package_uninstall_disables_runtime_but_preserves_package_versions_and_sources() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-package-uninstall-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "uninstall-adapter",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let package = ConversationAdapterPackage {
            package_id: "com.util6.uninstall-adapter".to_string(),
            adapter_id: adapter.id.clone(),
            name: "Uninstall Adapter".to_string(),
            version: "1.0.0".to_string(),
            record_kind: ConversationAdapterPackageRecordKind::Session,
            install_dir: "/tmp/packages/com.util6.uninstall-adapter/versions/1.0.0".to_string(),
            manifest_path: "/tmp/packages/com.util6.uninstall-adapter/versions/1.0.0/conversation-adapter-package.json".to_string(),
            adapter_manifest_path: "/tmp/packages/com.util6.uninstall-adapter/versions/1.0.0/conversation-adapter.json".to_string(),
            runtime_protocol: "stdio-ndjson-v1".to_string(),
            runtime_ready: true,
            origin: ConversationAdapterPackageOrigin::ManagedRelease,
            source_url: None,
            git_ref: None,
            git_commit: None,
            catalog_url: None,
            update_policy: ConversationPackageUpdatePolicy::Manual,
            latest_version: Some("1.0.0".to_string()),
            last_checked_at: None,
            runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
            runtime_validated_at: None,
            installed_content_hash: Some("hash-v1".to_string()),
            trusted_package_hash: Some("hash-v1".to_string()),
            error_message: None,
            created_at: "2026-07-17T00:00:00Z".to_string(),
            updated_at: "2026-07-17T00:00:00Z".to_string(),
        };
    let version = ConversationAdapterPackageVersion {
        package_id: package.package_id.clone(),
        version: package.version.clone(),
        install_dir: package.install_dir.clone(),
        artifact_hash: Some("artifact-v1".to_string()),
        content_hash: "hash-v1".to_string(),
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
        installed_at: package.created_at.clone(),
    };

    let (
        uninstalled,
        remaining_versions,
        projected_adapter_after_switch,
        retained_source,
        retained_source_after_switch,
        missing_adapter,
        missing_adapter_after_switch,
    ) = async {
        crate::backend::store::create_local_tenant_sqlx(
            database.pool(),
            "local",
            "Tenant B",
            Some("tenant-b"),
        )
        .await?;
        activate_conversation_adapter_package_sqlx(database.pool(), &adapter, &package, &version)
            .await?;
        let projected_adapter_after_switch =
            load_conversation_adapter_sqlx(database.pool(), "tenant-b", &adapter.id)
                .await?
                .expect("application package projects into every existing tenant");
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
        upsert_conversation_source_sqlx(database.pool(), "tenant-b", &source).await?;
        let uninstalled = deactivate_conversation_adapter_package_sqlx(
            database.pool(),
            &package.package_id,
            &adapter.id,
        )
        .await?;
        let remaining_versions =
            list_conversation_adapter_package_versions_sqlx(database.pool(), &package.package_id)
                .await?;
        let retained_source =
            load_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source.id)
                .await?
                .expect("source remains after uninstall");
        let retained_source_after_switch =
            load_conversation_source_sqlx(database.pool(), "tenant-b", &source.id)
                .await?
                .expect("other tenant source remains after uninstall");
        let missing_adapter =
            load_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter.id).await?;
        let missing_adapter_after_switch =
            load_conversation_adapter_sqlx(database.pool(), "tenant-b", &adapter.id).await?;
        Ok::<_, StoreError>((
            uninstalled,
            remaining_versions,
            projected_adapter_after_switch,
            retained_source,
            retained_source_after_switch,
            missing_adapter,
            missing_adapter_after_switch,
        ))
    }
    .await
    .expect("uninstall managed package runtime");

    assert!(!uninstalled.runtime_ready);
    assert_eq!(
        uninstalled.runtime_gate_status,
        ConversationAdapterRuntimeGateStatus::RuntimeMissing
    );
    assert_eq!(remaining_versions, vec![version]);
    assert_eq!(projected_adapter_after_switch.id, adapter.id);
    assert!(!retained_source.enabled);
    assert!(!retained_source_after_switch.enabled);
    assert!(missing_adapter.is_none());
    assert!(missing_adapter_after_switch.is_none());

    let (deleted_version, missing_package, versions_after_delete, source_after_delete) = async {
        let deleted_version = delete_conversation_adapter_package_version_sqlx(
            database.pool(),
            &package.package_id,
            &package.version,
            None,
            true,
        )
        .await?;
        let missing_package =
            load_conversation_adapter_package_sqlx(database.pool(), &package.package_id).await?;
        let versions_after_delete =
            list_conversation_adapter_package_versions_sqlx(database.pool(), &package.package_id)
                .await?;
        let source_after_delete =
            load_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source.id)
                .await?
                .expect("source remains after deleting package files");
        Ok::<_, StoreError>((
            deleted_version,
            missing_package,
            versions_after_delete,
            source_after_delete,
        ))
    }
    .await
    .expect("delete the final uninstalled package version");
    assert!(deleted_version);
    assert!(missing_package.is_none());
    assert!(versions_after_delete.is_empty());
    assert!(!source_after_delete.enabled);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn package_activation_rolls_back_adapter_and_active_version_when_version_insert_fails() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-package-activation-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let original_adapter = test_conversation_adapter(
        "activation-adapter",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let package = ConversationAdapterPackage {
        package_id: "com.util6.activation".to_string(),
        adapter_id: original_adapter.id.clone(),
        name: "Activation Adapter".to_string(),
        version: "1.0.0".to_string(),
        record_kind: ConversationAdapterPackageRecordKind::Session,
        install_dir: "/tmp/packages/com.util6.activation/versions/1.0.0".to_string(),
        manifest_path:
            "/tmp/packages/com.util6.activation/versions/1.0.0/conversation-adapter-package.json"
                .to_string(),
        adapter_manifest_path:
            "/tmp/packages/com.util6.activation/versions/1.0.0/conversation-adapter.json"
                .to_string(),
        runtime_protocol: "stdio-ndjson-v1".to_string(),
        runtime_ready: true,
        origin: ConversationAdapterPackageOrigin::ManagedRelease,
        source_url: None,
        git_ref: None,
        git_commit: None,
        catalog_url: None,
        update_policy: ConversationPackageUpdatePolicy::Manual,
        latest_version: Some("1.0.0".to_string()),
        last_checked_at: None,
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
        runtime_validated_at: None,
        installed_content_hash: Some("hash-v1".to_string()),
        trusted_package_hash: Some("hash-v1".to_string()),
        error_message: None,
        created_at: "2026-07-15T00:00:00Z".to_string(),
        updated_at: "2026-07-15T00:00:00Z".to_string(),
    };
    let version = ConversationAdapterPackageVersion {
        package_id: package.package_id.clone(),
        version: package.version.clone(),
        install_dir: package.install_dir.clone(),
        artifact_hash: Some("hash-v1".to_string()),
        content_hash: "hash-v1".to_string(),
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
        installed_at: package.created_at.clone(),
    };
    activate_conversation_adapter_package_sqlx(
        database.pool(),
        &original_adapter,
        &package,
        &version,
    )
    .await
    .expect("activate original package");

    let mut candidate_adapter = original_adapter.clone();
    candidate_adapter.version = "2.0.0".to_string();
    let mut candidate_package = package.clone();
    candidate_package.version = "2.0.0".to_string();
    candidate_package.install_dir = "/tmp/packages/com.util6.activation/versions/2.0.0".to_string();
    let invalid_version = ConversationAdapterPackageVersion {
        package_id: "com.util6.missing-parent".to_string(),
        version: "2.0.0".to_string(),
        install_dir: candidate_package.install_dir.clone(),
        artifact_hash: Some("hash-v2".to_string()),
        content_hash: "hash-v2".to_string(),
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
        installed_at: "2026-07-15T01:00:00Z".to_string(),
    };
    activate_conversation_adapter_package_sqlx(
        database.pool(),
        &candidate_adapter,
        &candidate_package,
        &invalid_version,
    )
    .await
    .expect_err("foreign-key failure should roll back activation");

    let (stored_adapter, stored_package) = async {
        Ok::<_, StoreError>((
            load_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &original_adapter.id)
                .await?,
            load_conversation_adapter_package_sqlx(database.pool(), &package.package_id).await?,
        ))
    }
    .await
    .expect("reload active package");
    assert_eq!(stored_adapter, Some(original_adapter));
    assert_eq!(stored_package, Some(package));

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn seeding_builtin_conversation_adapters_preserves_user_registered_adapter() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-adapter-seed-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let mut market_adapter = test_conversation_adapter(
        "codex",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    market_adapter.manifest_path =
        Some("/tmp/assetiweave-market/codex-session/conversation-adapter.json".to_string());
    market_adapter.executable_path =
        Some("/tmp/assetiweave-market/codex-session/adapter.mjs".to_string());

    let loaded = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &market_adapter).await?;
        seed_prepared_builtin_conversation_adapters_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            vec![test_conversation_adapter(
                "codex",
                ConversationAdapterKind::External,
                ConversationAdapterTrustState::BuiltIn,
            )],
        )
        .await?;
        load_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, "codex").await
    }
    .await
    .expect("seed built-in conversation adapters")
    .expect("codex adapter");

    assert_eq!(loaded.trust_state, ConversationAdapterTrustState::Trusted);
    assert_eq!(loaded.manifest_path, market_adapter.manifest_path);
    assert_eq!(loaded.executable_path, market_adapter.executable_path);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn disabling_builtin_adapter_disables_runtime_and_sources() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-adapter-disable-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");

    let builtin_adapter = test_conversation_adapter(
        "disable-builtin",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::BuiltIn,
    );
    let source = test_conversation_source(&builtin_adapter.id);
    let (adapter, source) = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &builtin_adapter).await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
        disable_builtin_conversation_adapter_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &builtin_adapter.id,
        )
        .await?;
        let adapter =
            load_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &builtin_adapter.id)
                .await?
                .expect("built-in adapter retained");
        let source = load_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source.id)
            .await?
            .expect("source retained");
        Ok::<_, StoreError>((adapter, source))
    }
    .await
    .expect("disable built-in adapter");

    assert!(!adapter.enabled);
    assert!(!source.enabled);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_import_leaves_turn_question_id_unwritten_and_zero_write_dependency() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-no-question-write-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "no-question-write-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let pool = database.pool();

    upsert_conversation_adapter_sqlx(pool, TEST_TENANT_ID, &adapter)
        .await
        .expect("upsert adapter");
    upsert_conversation_source_sqlx(pool, TEST_TENANT_ID, &source)
        .await
        .expect("upsert source");

    let session = fixture_session("v1");
    import_conversation_sessions_sqlx(pool, TEST_TENANT_ID, &source, &[session], false)
        .await
        .expect("import session");

    // Ticket 03: conversation_turns table no longer has question_id column
    let query_err = sqlx::query("SELECT question_id FROM conversation_turns WHERE tenant_id = ?1")
        .bind(TEST_TENANT_ID)
        .fetch_all(pool)
        .await
        .expect_err("question_id column must no longer exist");
    assert!(
        query_err
            .to_string()
            .contains("no such column: question_id"),
        "expected 'no such column: question_id', got: {query_err}"
    );

    // Verify turns were imported
    let turn_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversation_turns WHERE tenant_id = ?1")
            .bind(TEST_TENANT_ID)
            .fetch_one(pool)
            .await
            .expect("count turns");
    assert!(turn_count > 0);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_full_repeat_and_equivalent_incremental_imports_converge() {
    let full_db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-full-convergence-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let incremental_db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-incremental-convergence-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let full_database = Database::open_async(&full_db_path)
        .await
        .expect("open full database");
    let incremental_database = Database::open_async(&incremental_db_path)
        .await
        .expect("open incremental database");
    let adapter = test_conversation_adapter(
        "convergence-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let mut full_session = fixture_session("v1");
    full_session.turns[1].user_text = "继续上一个问题".to_string();
    let mut partial_session = full_session.clone();
    partial_session.turns.remove(1);

    let snapshot = |detail: &ConversationSessionDetail| {
        detail
            .questions
            .iter()
            .map(|question| {
                (
                    question.question.id.clone(),
                    question
                        .turns
                        .iter()
                        .map(|turn| turn.id.clone())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
    };

    let (full_snapshot, repeated_snapshot) = async {
        upsert_conversation_adapter_sqlx(full_database.pool(), TEST_TENANT_ID, &adapter).await?;
        upsert_conversation_source_sqlx(full_database.pool(), TEST_TENANT_ID, &source).await?;
        import_conversation_sessions_sqlx(
            full_database.pool(),
            TEST_TENANT_ID,
            &source,
            &[full_session.clone()],
            false,
        )
        .await?;
        let session_id = stable_id("conversation-session", &[&source.id, "session-1"]);
        let full_detail = load_conversation_session_detail_sqlx(
            full_database.pool(),
            TEST_TENANT_ID,
            &session_id,
        )
        .await?;
        import_conversation_sessions_sqlx(
            full_database.pool(),
            TEST_TENANT_ID,
            &source,
            &[full_session.clone()],
            false,
        )
        .await?;
        let repeated_detail = load_conversation_session_detail_sqlx(
            full_database.pool(),
            TEST_TENANT_ID,
            &session_id,
        )
        .await?;
        Ok::<_, StoreError>((snapshot(&full_detail), snapshot(&repeated_detail)))
    }
    .await
    .expect("full and repeated imports");

    let incremental_snapshot = async {
        upsert_conversation_adapter_sqlx(incremental_database.pool(), TEST_TENANT_ID, &adapter)
            .await?;
        upsert_conversation_source_sqlx(incremental_database.pool(), TEST_TENANT_ID, &source)
            .await?;
        import_incremental_conversation_sessions_sqlx(
            incremental_database.pool(),
            TEST_TENANT_ID,
            &source,
            &[partial_session],
            &BTreeSet::from(["session-1".to_string()]),
            false,
        )
        .await?;
        import_incremental_conversation_sessions_sqlx(
            incremental_database.pool(),
            TEST_TENANT_ID,
            &source,
            &[full_session.clone()],
            &BTreeSet::from(["session-1".to_string()]),
            false,
        )
        .await?;
        let session_id = stable_id("conversation-session", &[&source.id, "session-1"]);
        let detail = load_conversation_session_detail_sqlx(
            incremental_database.pool(),
            TEST_TENANT_ID,
            &session_id,
        )
        .await?;
        Ok::<_, StoreError>(snapshot(&detail))
    }
    .await
    .expect("equivalent incremental import");

    assert_eq!(full_snapshot, repeated_snapshot);
    assert_eq!(full_snapshot, incremental_snapshot);

    drop(full_database);
    drop(incremental_database);
    cleanup_database(&full_db_path);
    cleanup_database(&incremental_db_path);
}

#[tokio::test]
async fn sqlx_question_detail_projects_from_question_turn_membership_and_turn_facts() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-question-projection-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "question-projection-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);

    let detail = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[fixture_session("v1")],
            false,
        )
        .await?;
        let session_id = stable_id("conversation-session", &[&source.id, "session-1"]);
        load_conversation_session_detail_sqlx(database.pool(), TEST_TENANT_ID, &session_id).await
    }
    .await
    .expect("load projected question detail");

    let first_question = &detail.questions[0];
    assert_eq!(
        first_question.question.title.as_deref(),
        Some("How does sync work?")
    );
    assert_eq!(
        first_question
            .turns
            .iter()
            .map(|turn| turn.prompt.as_str())
            .collect::<Vec<_>>(),
        vec!["How does sync work?", "继续"]
    );
    assert!(first_question
        .projected_content_nodes
        .iter()
        .any(|node| node.content.contains("answer for t1")));
    assert_eq!(first_question.question_turns.len(), 2);
    assert_eq!(
        first_question
            .question_turns
            .iter()
            .map(|membership| membership.turn_id.as_str())
            .collect::<Vec<_>>(),
        vec![
            first_question.turns[0].id.as_str(),
            first_question.turns[1].id.as_str()
        ]
    );
    assert_eq!(
        first_question.question_turns[0].assignment_origin,
        ConversationGroupingOrigin::AutoMerged
    );
    assert_eq!(first_question.question_turns[0].turn_order, 0);
    assert!(!first_question.question_turns[0].assigned_at.is_empty());
    assert!(!first_question.question_turns[0].updated_at.is_empty());

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_import_skips_unchanged_fingerprinted_sessions() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-import-skip-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "import-skip-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let mut session = fixture_session("v1");
    session.source_fingerprint = Some("unchanged".to_string());

    let imported_at = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[session.clone()],
            false,
        )
        .await?;
        sqlx::query(
            "UPDATE conversation_sessions SET imported_at = 'preserved' WHERE source_id = ?1",
        )
        .bind(&source.id)
        .execute(database.pool())
        .await
        .map_err(StoreError::external)?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[session],
            false,
        )
        .await?;
        sqlx::query_scalar::<_, String>(
            "SELECT imported_at FROM conversation_sessions WHERE source_id = ?1",
        )
        .bind(&source.id)
        .fetch_one(database.pool())
        .await
        .map_err(StoreError::external)
    }
    .await
    .expect("import unchanged fingerprinted session through SQLx");

    assert_eq!(imported_at, "preserved");

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_import_rewrites_session_when_normalized_parts_change() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-import-refresh-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "import-refresh-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let mut old_session = fixture_session("v1");
    old_session.source_fingerprint = Some("same-source".to_string());
    old_session.turns[0].parts[0].text = Some("old answer".to_string());
    let mut refreshed_session = fixture_session("v1");
    refreshed_session.source_fingerprint = Some("same-source".to_string());
    refreshed_session.turns[0].parts[0].text = Some("refreshed answer".to_string());

    let (result, imported_at, refreshed_response, part_ids_before, part_ids_after) = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[old_session],
            false,
        )
        .await?;
        let part_ids_before: Vec<(String, i64)> = sqlx::query_as::<_, (String, i64)>(
            r#"
                    SELECT p.turn_id, p.part_index
                    FROM conversation_parts p
                    JOIN conversation_turns t ON t.id = p.turn_id
                    JOIN conversation_sessions s ON s.id = t.session_id
                    WHERE s.source_id = ?1
                    ORDER BY t.turn_index ASC, p.part_index ASC
                    "#,
        )
        .bind(&source.id)
        .fetch_all(database.pool())
        .await
        .map_err(StoreError::external)?
        .into_iter()
        .map(|(turn_id, part_index)| {
            (
                derive_conversation_part_id(&turn_id, part_index),
                part_index,
            )
        })
        .collect();
        sqlx::query(
            "UPDATE conversation_sessions SET imported_at = 'preserved' WHERE source_id = ?1",
        )
        .bind(&source.id)
        .execute(database.pool())
        .await
        .map_err(StoreError::external)?;
        let result = import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[refreshed_session],
            false,
        )
        .await?;
        let imported_at = sqlx::query_scalar::<_, String>(
            "SELECT imported_at FROM conversation_sessions WHERE source_id = ?1",
        )
        .bind(&source.id)
        .fetch_one(database.pool())
        .await
        .map_err(StoreError::external)?;
        let refreshed_response = sqlx::query_scalar::<_, Option<String>>(
            r#"
                    SELECT t.response
                    FROM conversation_turns t
                    JOIN conversation_sessions s ON s.id = t.session_id
                    WHERE s.source_id = ?1
                    ORDER BY t.turn_index ASC
                    LIMIT 1
                    "#,
        )
        .bind(&source.id)
        .fetch_one(database.pool())
        .await
        .map_err(StoreError::external)?;
        let part_ids_after: Vec<(String, i64)> = sqlx::query_as::<_, (String, i64)>(
            r#"
                    SELECT p.turn_id, p.part_index
                    FROM conversation_parts p
                    JOIN conversation_turns t ON t.id = p.turn_id
                    JOIN conversation_sessions s ON s.id = t.session_id
                    WHERE s.source_id = ?1
                    ORDER BY t.turn_index ASC, p.part_index ASC
                    "#,
        )
        .bind(&source.id)
        .fetch_all(database.pool())
        .await
        .map_err(StoreError::external)?
        .into_iter()
        .map(|(turn_id, part_index)| {
            (
                derive_conversation_part_id(&turn_id, part_index),
                part_index,
            )
        })
        .collect();
        Ok::<_, StoreError>((
            result,
            imported_at,
            refreshed_response,
            part_ids_before,
            part_ids_after,
        ))
    }
    .await
    .expect("refresh normalized parts through SQLx");

    assert_eq!(result.skipped_session_count, 0);
    assert_ne!(imported_at, "preserved");
    assert_eq!(part_ids_after, part_ids_before);
    assert_eq!(refreshed_response.as_deref(), Some("refreshed answer"));

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_import_prunes_turns_removed_by_external_adapter() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-import-prune-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "import-prune-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let mut pruned_session = fixture_session("v2");
    pruned_session.turns.truncate(1);
    pruned_session.source_fingerprint = Some("pruned-source".to_string());

    let (detail, stale_parts) = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[fixture_session("v2")],
            false,
        )
        .await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[pruned_session],
            false,
        )
        .await?;
        let session_id = stable_id("conversation-session", &[&source.id, "session-1"]);
        let detail =
            load_conversation_session_detail_sqlx(database.pool(), TEST_TENANT_ID, &session_id)
                .await?;
        let stale_parts = sqlx::query_scalar::<_, i64>(
            r#"
                    SELECT COUNT(*)
                    FROM conversation_parts p
                    JOIN conversation_turns t ON t.id = p.turn_id
                    WHERE t.session_id = ?1
                      AND t.external_id IN ('t2', 't3')
                    "#,
        )
        .bind(&session_id)
        .fetch_one(database.pool())
        .await
        .map_err(StoreError::external)?;
        Ok::<_, StoreError>((detail, stale_parts))
    }
    .await
    .expect("prune stale turns through SQLx");

    assert_eq!(detail.questions.len(), 1);
    assert_eq!(detail.questions[0].turns.len(), 1);
    assert_eq!(detail.questions[0].turns[0].external_id, "t1");
    assert_eq!(detail.questions[0].parts.len(), 2);
    assert_eq!(stale_parts, 0);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_import_marks_sessions_missing_when_external_adapter_omits_them() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-import-missing-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "import-missing-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let current_session = fixture_session("v1");
    let mut removed_session = fixture_session("v1");
    removed_session.external_id = "removed-session".to_string();
    removed_session.title = Some("Removed fixture".to_string());

    let (listed, missing_count) = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[current_session.clone(), removed_session],
            false,
        )
        .await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[current_session],
            false,
        )
        .await?;
        let listed = list_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            None,
            Some(&source.id),
            None,
            20,
            0,
        )
        .await?;
        let missing_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM conversation_sessions WHERE source_id = ?1 AND missing = 1",
        )
        .bind(&source.id)
        .fetch_one(database.pool())
        .await
        .map_err(StoreError::external)?;
        Ok::<_, StoreError>((listed, missing_count))
    }
    .await
    .expect("mark omitted sessions missing through SQLx");

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].session.external_id, "session-1");
    assert_eq!(missing_count, 1);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_incremental_import_preserves_and_recovers_discovered_sessions() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-import-incremental-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "import-incremental-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let current_session = fixture_session("v1");
    let mut retained_session = fixture_session("v1");
    retained_session.external_id = "retained-session".to_string();
    retained_session.title = Some("Retained fixture".to_string());
    let discovered_external_ids = BTreeSet::from([
        current_session.external_id.clone(),
        retained_session.external_id.clone(),
    ]);

    let listed = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[current_session, retained_session],
            false,
        )
        .await?;

        // Reproduce the damaged state created by the old incremental path.
        import_conversation_sessions_sqlx(database.pool(), TEST_TENANT_ID, &source, &[], false)
            .await?;

        import_incremental_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[],
            &discovered_external_ids,
            false,
        )
        .await?;
        list_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            None,
            Some(&source.id),
            None,
            20,
            0,
        )
        .await
    }
    .await
    .expect("preserve discovered sessions during incremental import");

    assert_eq!(listed.len(), 2);
    assert!(listed.iter().all(|item| !item.session.missing));

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_conversation_reads_and_filters_questions() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-read-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "read-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let mut session = fixture_session("v2");
    session.turns[2].parts.push(NormalizedConversationPart {
        role: ConversationPartRole::Tool,
        kind: ConversationPartKind::Command,
        text: None,
        language: None,
        command: Some("assetiweave-cli conversation session export".to_string()),
        cwd: Some("/tmp/project".to_string()),
        status: Some("completed".to_string()),
        exit_code: Some(0),
        command_label: None,
        source_execution_id: Some("call-export".to_string()),
        content_card: None,
        metadata_json: content_card_metadata("command"),
    });
    session.turns[2].parts.push(NormalizedConversationPart {
        role: ConversationPartRole::Tool,
        kind: ConversationPartKind::Tool,
        text: Some("tests passed".to_string()),
        language: None,
        command: None,
        cwd: None,
        status: Some("completed".to_string()),
        exit_code: Some(0),
        command_label: None,
        source_execution_id: Some("call-export".to_string()),
        content_card: None,
        metadata_json: content_card_metadata("result"),
    });

    let (sessions, detail, filtered_questions, question) = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[session],
            false,
        )
        .await?;
        let sessions = list_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            None,
            Some(&source.id),
            Some("answer for t3"),
            20,
            0,
        )
        .await?;
        let detail = load_conversation_session_detail_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &sessions[0].session.id,
        )
        .await?;
        let filtered_questions = list_conversation_question_details_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &sessions[0].session.id,
            Some("answer for t3"),
            20,
            0,
        )
        .await?;
        let question = load_conversation_question_detail_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &filtered_questions[0].question.id,
        )
        .await?;
        Ok::<_, StoreError>((sessions, detail, filtered_questions, question))
    }
    .await
    .expect("read conversations through SQLx");

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].question_count, 3);
    assert_eq!(sessions[0].turn_count, 3);
    assert_eq!(detail.questions.len(), 2);
    assert_eq!(filtered_questions.len(), 1);
    assert_eq!(
        filtered_questions[0].question.title.as_deref(),
        Some("Export it")
    );
    assert_eq!(question.turns.len(), 1);
    assert_eq!(question.parts.len(), 3);
    let expected_exec_id = format!("exec-{}-1", question.turns[0].id);
    assert_eq!(
        question.parts[1].source_execution_id.as_deref(),
        Some(expected_exec_id.as_str())
    );
    assert_eq!(
        question.parts[2].source_execution_id.as_deref(),
        Some(expected_exec_id.as_str())
    );
    let serialized = serde_json::to_value(&question).expect("serialize question detail");
    let projected_nodes = serialized["projected_content_nodes"]
        .as_array()
        .expect("projected content nodes");
    assert_eq!(projected_nodes.len(), 3);
    assert_eq!(projected_nodes[0]["question_id"], question.question.id);
    assert_eq!(projected_nodes[0]["turn_id"], question.turns[0].id);
    assert_eq!(projected_nodes[0]["part_id"], question.parts[0].id);
    assert_eq!(projected_nodes[0]["node_order"], 0);
    assert_eq!(
        projected_nodes[0]["locator"]["part_id"],
        question.parts[0].id
    );
    assert_eq!(
        projected_nodes[1]["source_execution_id"],
        expected_exec_id.as_str()
    );
    assert_eq!(
        projected_nodes[2]["source_execution_id"],
        expected_exec_id.as_str()
    );

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_conversation_lists_sessions_by_display_id_fragment() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-id-fragment-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "id-fragment-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);

    let (fragment_matches, direct_fragment_matches, full_matches) = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[fixture_session("v1")],
            false,
        )
        .await?;
        let session_id = stable_id("conversation-session", &[&source.id, "session-1"]);
        let fragment = crate::backend::domain::conversation_id_fragment(&session_id);
        let collision_id = format!("conversation-session-{fragment}{}", "0".repeat(56));
        sqlx::query(
            r#"
                    INSERT INTO conversation_sessions (
                        tenant_id, id, source_id, adapter_id, external_id, title, project_path,
                        started_at, updated_at, source_locator, source_fingerprint, missing,
                        created_at, imported_at
                    )
                    SELECT tenant_id, ?1, source_id, adapter_id, 'fragment-collision',
                           'Fragment collision', project_path, started_at, updated_at,
                           source_locator, source_fingerprint, missing, created_at, imported_at
                    FROM conversation_sessions
                    WHERE tenant_id = ?2 AND id = ?3
                    "#,
        )
        .bind(&collision_id)
        .bind(TEST_TENANT_ID)
        .bind(&session_id)
        .execute(database.pool())
        .await
        .map_err(StoreError::external)?;
        let fragment_matches = list_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            None,
            Some(&source.id),
            Some(&fragment),
            20,
            0,
        )
        .await?;
        let direct_fragment_matches = list_conversation_sessions_by_id_fragment_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            ConversationRecordKind::Session,
            None,
            Some(&source.id),
            &fragment,
            20,
            0,
        )
        .await?;
        let full_matches = list_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            None,
            Some(&source.id),
            Some(&session_id),
            20,
            0,
        )
        .await?;
        Ok::<_, StoreError>((fragment_matches, direct_fragment_matches, full_matches))
    }
    .await
    .expect("list conversation sessions by display id fragment");

    assert_eq!(fragment_matches.len(), 2);
    assert_eq!(direct_fragment_matches.len(), 2);
    assert_eq!(full_matches.len(), 1);
    assert_eq!(full_matches[0].session.external_id, "session-1");

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_dynamic_projection_groups_follow_ups_and_counts_turns_consistently() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-dynamic-proj-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "dynamic-proj-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let pool = database.pool();

    upsert_conversation_adapter_sqlx(pool, TEST_TENANT_ID, &adapter)
        .await
        .expect("upsert adapter");
    upsert_conversation_source_sqlx(pool, TEST_TENANT_ID, &source)
        .await
        .expect("upsert source");

    let mut session = fixture_session("v1");
    session.turns[1].user_text = "继续".to_string();
    session.turns[2].user_text = "请问 Rust 的生命周期怎么理解？".to_string();

    import_conversation_sessions_sqlx(pool, TEST_TENANT_ID, &source, &[session.clone()], false)
        .await
        .expect("import session");

    // 1. Session list query returns question_count == turn_count (3 turns)
    let sessions =
        list_conversation_sessions_sqlx(pool, TEST_TENANT_ID, None, Some(&source.id), None, 10, 0)
            .await
            .expect("list sessions");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].turn_count, 3);
    assert_eq!(sessions[0].question_count, 3);

    // 2. Session detail groups dynamically in-memory
    let detail =
        load_conversation_session_detail_sqlx(pool, TEST_TENANT_ID, &sessions[0].session.id)
            .await
            .expect("load detail");
    assert_eq!(detail.questions.len(), 2);
    assert_eq!(detail.questions[0].turns.len(), 2);
    assert_eq!(detail.questions[1].turns.len(), 1);

    // 3. Question IDs are derived deterministically from first turn id
    let q0_id = &detail.questions[0].question.id;
    let t0_id = &detail.questions[0].turns[0].id;
    assert_eq!(
        q0_id,
        &crate::backend::domain::conversations::derive_question_id_from_turn_id(
            "conversation-question",
            t0_id
        )
    );

    // 4. Loading question detail works with derived question_id AND with turn_id
    let q0_detail_by_qid = load_conversation_question_detail_sqlx(pool, TEST_TENANT_ID, q0_id)
        .await
        .expect("load by question_id");
    assert_eq!(q0_detail_by_qid.question.id, *q0_id);
    assert_eq!(q0_detail_by_qid.turns.len(), 2);

    let q0_detail_by_t0 = load_conversation_question_detail_sqlx(pool, TEST_TENANT_ID, t0_id)
        .await
        .expect("load by turn_id");
    assert_eq!(q0_detail_by_t0.question.id, *q0_id);

    let t1_id = &detail.questions[0].turns[1].id;
    let q0_detail_by_t1 = load_conversation_question_detail_sqlx(pool, TEST_TENANT_ID, t1_id)
        .await
        .expect("load by turn 1 id");
    assert_eq!(q0_detail_by_t1.question.id, *q0_id);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_searches_session_and_web_conversation_cards() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-search-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let session_adapter = test_conversation_adapter(
        "search-session-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let web_adapter = test_conversation_adapter(
        "search-web-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let session_source = test_conversation_source(&session_adapter.id);
    let mut web_source = test_conversation_source(&web_adapter.id);
    web_source.id = "search-web-source".to_string();
    let mut session = fixture_session("v1");
    session.started_at = Some("2026-03-02T10:00:00Z".to_string());
    let mut web_session = fixture_session("v1");
    web_session.external_id = "web-session".to_string();
    web_session.started_at = Some("2026-04-02T10:00:00Z".to_string());

    let (session_page, web_page, fragment_page, fragment_session_ids) = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &session_adapter).await?;
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &web_adapter).await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &session_source).await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &web_source).await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &session_source,
            &[session],
            false,
        )
        .await?;
        super::super::web_record_repo::import_web_record_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &web_source,
            &[web_session],
            false,
        )
        .await?;
        let session_page = search_conversation_cards_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            ConversationRecordKind::Session,
            Some(&session_adapter.id),
            Some(&session_source.id),
            Some("/tmp/project"),
            "answer for t1",
            &[ConversationSearchCardType::answer()],
            &[],
            false,
            true,
            Some("2026-03-01"),
            Some("2026-03-31"),
            true,
            20,
            0,
            None,
        )
        .await?;
        let web_page = search_conversation_cards_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            ConversationRecordKind::Web,
            Some(&web_adapter.id),
            Some(&web_source.id),
            None,
            "answer for t3",
            &[ConversationSearchCardType::answer()],
            &[],
            false,
            true,
            None,
            None,
            false,
            20,
            0,
            None,
        )
        .await?;
        let session_id = stable_id("conversation-session", &[&session_source.id, "session-1"]);
        let fragment = crate::backend::domain::conversation_id_fragment(&session_id);
        let fragment_page = search_conversation_cards_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            ConversationRecordKind::Session,
            Some(&session_adapter.id),
            Some(&session_source.id),
            Some("/tmp/project"),
            &fragment,
            &[],
            &[],
            false,
            true,
            None,
            None,
            false,
            20,
            0,
            None,
        )
        .await?;
        let fragment_session_ids = load_search_session_ids_by_id_fragment_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            ConversationRecordKind::Session.tables(),
            &fragment,
        )
        .await?;
        Ok::<_, StoreError>((session_page, web_page, fragment_page, fragment_session_ids))
    }
    .await
    .expect("search session and web records through SQLx");

    assert_eq!(session_page.total_count, 1);
    assert_eq!(
        session_page.hits[0].session.session.source_id,
        session_source.id
    );
    assert_eq!(
        session_page.hits[0].card_type,
        ConversationSearchCardType::answer()
    );
    assert_eq!(web_page.total_count, 1);
    assert_eq!(web_page.hits[0].session.session.source_id, web_source.id);
    assert_eq!(
        web_page.hits[0].card_type,
        ConversationSearchCardType::new("search-web-external.answer")
    );
    assert!(fragment_page.total_count > 0);
    assert_eq!(fragment_session_ids.len(), 1);
    assert!(fragment_page.hits.iter().all(|hit| hit.session.session.id
        == stable_id("conversation-session", &[&session_source.id, "session-1"])));
    assert!(fragment_page
        .hits
        .iter()
        .all(|hit| hit.highlight_segments.is_none()));

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_search_and_aggregates_only_declared_content_cards() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-declared-cards-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "declared-card-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let mut undeclared_turn = fixture_turn("undeclared", 0, "First question");
    undeclared_turn.parts[0].text = Some("undeclared answer needle".to_string());
    undeclared_turn.parts[0].metadata_json = None;
    let mut declared_turn = fixture_turn("declared", 1, "Second question");
    declared_turn.parts[0].text = Some("declared answer needle".to_string());
    declared_turn.parts[0].metadata_json = content_card_metadata("answer");
    let session = NormalizedConversationSession {
        external_id: "declared-card-session".to_string(),
        title: Some("Declared card fixture".to_string()),
        project_path: Some("/tmp/project".to_string()),
        started_at: None,
        updated_at: None,
        source_locator: None,
        source_fingerprint: None,
        turns: vec![undeclared_turn, declared_turn],
        ..Default::default()
    };

    let (detail, undeclared_list, undeclared_page, declared_page) = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[session],
            false,
        )
        .await?;
        let sessions = list_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            None,
            Some(&source.id),
            None,
            20,
            0,
        )
        .await?;
        let detail = load_conversation_session_detail_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &sessions[0].session.id,
        )
        .await?;
        let undeclared_list = list_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            None,
            Some(&source.id),
            Some("undeclared answer"),
            20,
            0,
        )
        .await?;
        let undeclared_page = search_conversation_cards_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            ConversationRecordKind::Session,
            Some(&adapter.id),
            Some(&source.id),
            None,
            "undeclared answer",
            &[ConversationSearchCardType::answer()],
            &[],
            false,
            true,
            None,
            None,
            false,
            20,
            0,
            None,
        )
        .await?;
        let declared_page = search_conversation_cards_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            ConversationRecordKind::Session,
            Some(&adapter.id),
            Some(&source.id),
            None,
            "declared answer",
            &[ConversationSearchCardType::answer()],
            &[],
            false,
            true,
            None,
            None,
            false,
            20,
            0,
            None,
        )
        .await?;
        Ok::<_, StoreError>((detail, undeclared_list, undeclared_page, declared_page))
    }
    .await
    .expect("search declared content cards through SQLx");

    assert!(!detail.questions[0]
        .projected_content_nodes
        .iter()
        .any(|node| node.semantic_role.as_deref() == Some("answer")));
    assert_eq!(
        detail.questions[1]
            .projected_content_nodes
            .iter()
            .find(|node| node.semantic_role.as_deref() == Some("answer"))
            .map(|node| node.content.as_str()),
        Some("declared answer needle")
    );
    assert!(undeclared_list.is_empty());
    assert_eq!(undeclared_page.total_count, 0);
    assert_eq!(declared_page.total_count, 1);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_conversation_records_are_isolated_by_tenant() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-tenant-isolation-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let tenant_alpha = "tenant-alpha";
    let tenant_beta = "tenant-beta";
    let adapter = test_conversation_adapter(
        "tenant-isolation-adapter",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let mut alpha_session = fixture_session("v1");
    alpha_session.turns[0].parts[0].text = Some("alpha tenant answer".to_string());
    let mut beta_session = fixture_session("v1");
    beta_session.turns[0].parts[0].text = Some("beta tenant answer".to_string());

    let (session_id, alpha_detail, beta_detail, alpha_page, beta_page) = async {
        for tenant_id in [tenant_alpha, tenant_beta] {
            upsert_conversation_adapter_sqlx(database.pool(), tenant_id, &adapter).await?;
            upsert_conversation_source_sqlx(database.pool(), tenant_id, &source).await?;
        }
        import_conversation_sessions_sqlx(
            database.pool(),
            tenant_alpha,
            &source,
            &[alpha_session],
            false,
        )
        .await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            tenant_beta,
            &source,
            &[beta_session],
            false,
        )
        .await?;

        let alpha_sessions = list_conversation_sessions_sqlx(
            database.pool(),
            tenant_alpha,
            None,
            Some(&source.id),
            Some("alpha tenant"),
            20,
            0,
        )
        .await?;
        let beta_sessions = list_conversation_sessions_sqlx(
            database.pool(),
            tenant_beta,
            None,
            Some(&source.id),
            Some("beta tenant"),
            20,
            0,
        )
        .await?;
        let session_id = alpha_sessions[0].session.id.clone();
        assert_eq!(beta_sessions[0].session.id, session_id);
        let alpha_detail =
            load_conversation_session_detail_sqlx(database.pool(), tenant_alpha, &session_id)
                .await?;
        let beta_detail =
            load_conversation_session_detail_sqlx(database.pool(), tenant_beta, &session_id)
                .await?;
        let alpha_page = search_conversation_cards_sqlx(
            database.pool(),
            tenant_alpha,
            ConversationRecordKind::Session,
            Some(&adapter.id),
            Some(&source.id),
            None,
            "beta tenant answer",
            &[ConversationSearchCardType::answer()],
            &[],
            false,
            true,
            None,
            None,
            false,
            20,
            0,
            None,
        )
        .await?;
        let beta_page = search_conversation_cards_sqlx(
            database.pool(),
            tenant_beta,
            ConversationRecordKind::Session,
            Some(&adapter.id),
            Some(&source.id),
            None,
            "alpha tenant answer",
            &[ConversationSearchCardType::answer()],
            &[],
            false,
            true,
            None,
            None,
            false,
            20,
            0,
            None,
        )
        .await?;
        Ok::<_, StoreError>((session_id, alpha_detail, beta_detail, alpha_page, beta_page))
    }
    .await
    .expect("isolate conversation records by tenant");

    assert_eq!(alpha_detail.session.id, session_id);
    assert_eq!(beta_detail.session.id, session_id);
    assert!(alpha_detail.questions[0]
        .projected_content_nodes
        .iter()
        .any(|node| node.content.contains("alpha tenant answer")));
    assert!(!alpha_detail.questions[0]
        .projected_content_nodes
        .iter()
        .any(|node| node.content.contains("beta tenant answer")));
    assert!(beta_detail.questions[0]
        .projected_content_nodes
        .iter()
        .any(|node| node.content.contains("beta tenant answer")));
    assert_eq!(alpha_page.total_count, 0);
    assert_eq!(beta_page.total_count, 0);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_round_trips_structured_cards_and_preserves_translation_on_reclassification() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-card-persistence-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let mut adapter = test_conversation_adapter(
        "fixture-cards",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    adapter.card_contract_version = Some(1);
    adapter.card_kinds = vec![
        ConversationCardKindDefinition {
            id: "fixture-cards.reasoning".to_string(),
            semantic_role: Some("reasoning".to_string()),
            label: "Reasoning".to_string(),
            default_renderer: "markdown".to_string(),
            allowed_renderers: vec!["markdown".to_string()],
            icon_hint: Some("brain".to_string()),
        },
        ConversationCardKindDefinition {
            id: "fixture-cards.analysis".to_string(),
            semantic_role: Some("reasoning".to_string()),
            label: "Analysis".to_string(),
            default_renderer: "markdown".to_string(),
            allowed_renderers: vec!["markdown".to_string()],
            icon_hint: None,
        },
    ];
    let source = test_conversation_source(&adapter.id);
    let mut first = fixture_session("v1");
    first.turns[0].parts[0].content_card = Some(ConversationContentCardDescriptor {
        schema_version: 1,
        kind: "fixture-cards.reasoning".to_string(),
        semantic_role: None,
        renderer: Some("markdown".to_string()),
    });
    first.turns[0].parts[0].command_label = Some("DEBUG".to_string());
    let mut second = first.clone();
    second.turns[0].parts[0].content_card.as_mut().unwrap().kind =
        "fixture-cards.analysis".to_string();

    let (stored_adapter, part_id, detail) = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[first],
            false,
        )
        .await?;
        let session_id = stable_id("conversation-session", &[&source.id, "session-1"]);
        let initial =
            load_conversation_session_detail_sqlx(database.pool(), TEST_TENANT_ID, &session_id)
                .await?;
        let part_id = initial.questions[0].parts[0].id.clone();
        update_conversation_part_translation_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &part_id,
            "译文",
        )
        .await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[second],
            false,
        )
        .await?;
        let detail =
            load_conversation_session_detail_sqlx(database.pool(), TEST_TENANT_ID, &session_id)
                .await?;
        let stored_adapter =
            load_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter.id)
                .await?
                .expect("stored adapter");
        Ok::<_, StoreError>((stored_adapter, part_id, detail))
    }
    .await
    .expect("round trip structured card");

    let part = &detail.questions[0].parts[0];
    assert_eq!(part.id, part_id);
    assert_eq!(part.translated_text.as_deref(), Some("译文"));
    assert_eq!(part.command_label.as_deref(), Some("DEBUG"));
    assert_eq!(
        part.content_card.as_ref().map(|card| card.kind.as_str()),
        Some("fixture-cards.analysis")
    );
    let projected = detail.questions[0]
        .projected_content_nodes
        .iter()
        .find(|node| node.part_id == part_id)
        .expect("projected structured Content Node");
    assert_eq!(projected.semantic_role.as_deref(), Some("reasoning"));
    assert_eq!(projected.command_label.as_deref(), Some("DEBUG"));
    assert_eq!(stored_adapter.card_contract_version, Some(1));
    assert_eq!(stored_adapter.card_kinds, adapter.card_kinds);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_adapter_or_card_contract_change_invalidates_incremental_hydration_versions() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-card-hydration-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let descriptors = vec![
        crate::backend::domain::conversations::ConversationSessionObservation {
            external_id: "session-1".to_string(),
            updated_at: None,
            source_locator: None,
            version_token: "source-v1".to_string(),
        },
    ];
    let hydrated = BTreeSet::from(["session-1".to_string()]);

    let (same, adapter_changed, contract_changed, payload_policy_changed, projection_changed) =
        async {
            persist_conversation_session_observations_sqlx(
                database.pool(),
                TEST_TENANT_ID,
                "source-1",
                ConversationRecordKind::Session,
                &descriptors,
                &hydrated,
                Some("adapter-hash-v1"),
                Some(1),
                crate::backend::infrastructure::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
                Some(1),
            )
            .await?;
            let same = load_conversation_session_versions_sqlx(
                database.pool(),
                TEST_TENANT_ID,
                "source-1",
                ConversationRecordKind::Session,
                Some("adapter-hash-v1"),
                Some(1),
                crate::backend::infrastructure::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
                Some(1),
            )
            .await?;
            let adapter_changed = load_conversation_session_versions_sqlx(
                database.pool(),
                TEST_TENANT_ID,
                "source-1",
                ConversationRecordKind::Session,
                Some("adapter-hash-v2"),
                Some(1),
                crate::backend::infrastructure::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
                Some(1),
            )
            .await?;
            let contract_changed = load_conversation_session_versions_sqlx(
                database.pool(),
                TEST_TENANT_ID,
                "source-1",
                ConversationRecordKind::Session,
                Some("adapter-hash-v1"),
                Some(2),
                crate::backend::infrastructure::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
                Some(1),
            )
            .await?;
            let payload_policy_changed = load_conversation_session_versions_sqlx(
                database.pool(),
                TEST_TENANT_ID,
                "source-1",
                ConversationRecordKind::Session,
                Some("adapter-hash-v1"),
                Some(1),
                crate::backend::infrastructure::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION
                    + 1,
                Some(1),
            )
            .await?;
            let projection_changed = load_conversation_session_versions_sqlx(
                database.pool(),
                TEST_TENANT_ID,
                "source-1",
                ConversationRecordKind::Session,
                Some("adapter-hash-v1"),
                Some(1),
                crate::backend::infrastructure::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
                Some(2),
            )
            .await?;
            Ok::<_, StoreError>((
                same,
                adapter_changed,
                contract_changed,
                payload_policy_changed,
                projection_changed,
            ))
        }
        .await
        .expect("compare hydration identity");

    assert_eq!(same.get("session-1").map(String::as_str), Some("source-v1"));
    assert!(adapter_changed.is_empty());
    assert!(contract_changed.is_empty());
    assert!(payload_policy_changed.is_empty());
    assert!(projection_changed.is_empty());

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_observation_reconciles_drift_and_triggers_self_healing() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-observation-drift-self-healing-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "drift-healing-adapter",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);

    upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter)
        .await
        .expect("upsert adapter");
    upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source)
        .await
        .expect("upsert source");

    // 1. 真实会话表里保存的是旧指纹 v1
    let mut session_v1 = fixture_session("v1");
    session_v1.external_id = "session-drift".to_string();
    session_v1.source_fingerprint = Some("v1".to_string());
    import_conversation_sessions_sqlx(
        database.pool(),
        TEST_TENANT_ID,
        &source,
        &[session_v1],
        false,
    )
    .await
    .expect("import session v1");

    // 2. 观察表里发生了漂移：hydrated_version 被错误地写成了 v2
    let descriptors = vec![
        crate::backend::domain::conversations::ConversationSessionObservation {
            external_id: "session-drift".to_string(),
            updated_at: None,
            source_locator: None,
            version_token: "v2".to_string(),
        },
    ];
    let hydrated = BTreeSet::from(["session-drift".to_string()]);
    persist_conversation_session_observations_sqlx(
        database.pool(),
        TEST_TENANT_ID,
        &source.id,
        ConversationRecordKind::Session,
        &descriptors,
        &hydrated,
        None,
        None,
        crate::backend::infrastructure::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
        None,
    )
    .await
    .expect("persist observations");

    // 3. 此时加载已知版本：因为 underlying fingerprint (v1) != hydrated_version (v2)，触发自愈过滤，不返回该版本
    let versions = load_conversation_session_versions_sqlx(
        database.pool(),
        TEST_TENANT_ID,
        &source.id,
        ConversationRecordKind::Session,
        None,
        None,
        crate::backend::infrastructure::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
        None,
    )
    .await
    .expect("load versions");
    assert!(
        versions.get("session-drift").is_none(),
        "Drifted session should be excluded from known versions to trigger self-healing"
    );

    // 4. 当底层真实会话被正确更新为 v2 后，指纹对齐，能够正常返回
    let mut session_v2 = fixture_session("v2");
    session_v2.external_id = "session-drift".to_string();
    session_v2.source_fingerprint = Some("v2".to_string());
    import_conversation_sessions_sqlx(
        database.pool(),
        TEST_TENANT_ID,
        &source,
        &[session_v2],
        false,
    )
    .await
    .expect("import session v2");

    let reconciled_versions = load_conversation_session_versions_sqlx(
        database.pool(),
        TEST_TENANT_ID,
        &source.id,
        ConversationRecordKind::Session,
        None,
        None,
        crate::backend::infrastructure::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
        None,
    )
    .await
    .expect("load reconciled versions");
    assert_eq!(
        reconciled_versions.get("session-drift").map(String::as_str),
        Some("v2"),
        "Reconciled session should be present in known versions"
    );

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_dirty_failed_session_is_retried_without_affecting_unchanged_neighbours() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-observation-dirty-retry-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "dirty-retry-adapter",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);

    upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter)
        .await
        .expect("upsert adapter");
    upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source)
        .await
        .expect("upsert source");

    let descriptors = ["session-1", "session-2"]
        .into_iter()
        .map(
            |external_id| crate::backend::domain::conversations::ConversationSessionObservation {
                external_id: external_id.to_string(),
                updated_at: None,
                source_locator: None,
                version_token: "v1".to_string(),
            },
        )
        .collect::<Vec<_>>();
    let hydrated = BTreeSet::from(["session-1".to_string(), "session-2".to_string()]);

    let load_versions = || async {
        load_conversation_session_versions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source.id,
            ConversationRecordKind::Session,
            None,
            None,
            crate::backend::infrastructure::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
            None,
        )
        .await
    };

    persist_conversation_session_observations_sqlx(
        database.pool(),
        TEST_TENANT_ID,
        &source.id,
        ConversationRecordKind::Session,
        &descriptors,
        &hydrated,
        None,
        None,
        crate::backend::infrastructure::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
        None,
    )
    .await
    .expect("persist clean observations");

    let before_failure = load_versions().await.expect("load versions before failure");
    assert_eq!(
        before_failure.keys().cloned().collect::<Vec<_>>(),
        vec!["session-1".to_string(), "session-2".to_string()]
    );

    record_conversation_session_failure_sqlx(
        database.pool(),
        TEST_TENANT_ID,
        &source.id,
        "session",
        "session-2",
        Some("v1"),
    )
    .await
    .expect("record session-2 failure");

    let after_failure = load_versions().await.expect("load versions after failure");
    assert_eq!(
        after_failure.keys().cloned().collect::<Vec<_>>(),
        vec!["session-1".to_string()],
        "only the failed session must leave known versions so it is re-read next round"
    );
    assert_eq!(
        after_failure.get("session-1").map(String::as_str),
        Some("v1"),
        "unchanged neighbour must stay skippable"
    );

    persist_conversation_session_observations_sqlx(
        database.pool(),
        TEST_TENANT_ID,
        &source.id,
        ConversationRecordKind::Session,
        &descriptors[1..],
        &BTreeSet::from(["session-2".to_string()]),
        None,
        None,
        crate::backend::infrastructure::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
        None,
    )
    .await
    .expect("persist recovered observation");

    let after_recovery = load_versions().await.expect("load versions after recovery");
    assert_eq!(
        after_recovery.keys().cloned().collect::<Vec<_>>(),
        vec!["session-1".to_string(), "session-2".to_string()],
        "successful re-parse must clear dirty and restore incremental skipping"
    );

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_payload_policy_state_requests_one_reparse_for_existing_records() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-payload-policy-state-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "payload-policy-state-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);

    let (required_before, required_after) = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[fixture_session("v1")],
            false,
        )
        .await?;
        let required_before = conversation_payload_policy_reparse_required_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            crate::backend::infrastructure::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
        )
        .await?;
        mark_conversation_payload_policy_applied_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            crate::backend::infrastructure::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
        )
        .await?;
        let required_after = conversation_payload_policy_reparse_required_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            crate::backend::infrastructure::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
        )
        .await?;
        Ok::<_, StoreError>((required_before, required_after))
    }
    .await
    .expect("track payload policy reparse state");

    assert!(required_before);
    assert!(!required_after);

    drop(database);
    cleanup_database(&db_path);
}

fn fixture_session(version: &str) -> NormalizedConversationSession {
    let mut turns = vec![
        fixture_turn("t1", 0, "How does sync work?"),
        fixture_turn("t2", 1, "继续"),
        fixture_turn("t3", 2, "Export it"),
    ];
    if version == "v2" {
        turns[0].parts.push(NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::CodeBlock,
            text: Some("cargo test".to_string()),
            language: Some("sh".to_string()),
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: None,
            metadata_json: content_card_metadata("code"),
        });
    }
    NormalizedConversationSession {
        external_id: "session-1".to_string(),
        title: Some("Fixture".to_string()),
        project_path: Some("/tmp/project".to_string()),
        started_at: None,
        updated_at: None,
        source_locator: None,
        source_fingerprint: None,
        turns,
        ..Default::default()
    }
}

fn fixture_turn(id: &str, index: i64, user_text: &str) -> NormalizedConversationTurn {
    NormalizedConversationTurn {
        external_id: id.to_string(),
        turn_index: index,
        user_text: user_text.to_string(),
        title: None,
        started_at: None,
        ended_at: None,
        model: None,
        parts: vec![NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some(format!("answer for {id}")),
            language: None,
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: None,
            metadata_json: content_card_metadata("answer"),
        }],
    }
}

fn content_card_metadata(card_type: &str) -> Option<String> {
    Some(format!(
        r#"{{"content_card":{{"type":"{card_type}","format":"markdown"}}}}"#
    ))
}

fn test_conversation_adapter(
    id: &str,
    kind: ConversationAdapterKind,
    trust_state: ConversationAdapterTrustState,
) -> ConversationAdapter {
    ConversationAdapter {
        id: id.to_string(),
        name: id.to_string(),
        kind,
        version: "1.0.0".to_string(),
        enabled: true,
        manifest_path: Some(format!("/tmp/{id}/manifest.json")),
        executable_path: Some(format!("/tmp/{id}/adapter")),
        content_hash: Some(format!("{id}-hash")),
        trusted_hash: Some(format!("{id}-hash")),
        trust_state,
        protocol_version: Some(1),
        capabilities: vec!["read".to_string()],
        input_kinds: vec![ConversationSourceKind::Directory],
        card_contract_version: None,
        card_kinds: Vec::new(),
        projection_version: Some(1),
        created_at: "2026-06-19T00:00:00Z".to_string(),
        updated_at: "2026-06-19T00:00:00Z".to_string(),
    }
}

fn test_conversation_source(adapter_id: &str) -> ConversationSource {
    ConversationSource {
        id: format!("{adapter_id}-source"),
        adapter_id: adapter_id.to_string(),
        name: format!("{adapter_id} source"),
        kind: ConversationSourceKind::Directory,
        location: format!("/tmp/{adapter_id}/sessions"),
        config_json: Some("{\"mode\":\"test\"}".to_string()),
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: "2026-06-19T00:00:00Z".to_string(),
        updated_at: "2026-06-19T00:00:00Z".to_string(),
    }
}

#[tokio::test]
async fn conversation_source_repo_preserves_location_without_host_normalization() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-source-home-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .expect("open database");
    let mut source = test_conversation_source("codex");
    source.location = dirs::home_dir()
        .expect("home directory")
        .join(".codex")
        .to_string_lossy()
        .to_string();

    let loaded = async {
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
        load_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source.id).await
    }
    .await
    .expect("round trip conversation source")
    .expect("stored source");

    assert_eq!(loaded.location, source.location);
    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn test_explain_query_plan_for_session_parts_uses_efficient_indexes() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-query-plan-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .expect("open database");
    let pool = database.pool();

    let explain_rows: Vec<(i64, i64, i64, String)> = sqlx::query_as(
        r#"
            EXPLAIN QUERY PLAN
            SELECT p.turn_id, p.part_index, p.kind, p.content,
                   p.attrs_json, p.translated_text
            FROM conversation_turns t INDEXED BY idx_conversation_turns_tenant_session
            JOIN conversation_parts p
              ON p.tenant_id = t.tenant_id AND p.turn_id = t.id
            WHERE t.tenant_id = ?1 AND t.session_id = ?2
            ORDER BY t.turn_index ASC, p.part_index ASC
            "#,
    )
    .bind(TEST_TENANT_ID)
    .bind("test-session")
    .fetch_all(pool)
    .await
    .expect("explain query plan");

    let details: Vec<String> = explain_rows.into_iter().map(|r| r.3).collect();
    let plan_text = details.join(" | ");

    // 验证 turns 表使用 idx_conversation_turns_tenant_session 进行 SEARCH
    assert!(
        plan_text.contains("SEARCH") && plan_text.contains("idx_conversation_turns_tenant_session"),
        "Expected plan to search turns using idx_conversation_turns_tenant_session, got: {}",
        plan_text
    );

    // 验证 parts 表使用 UNIQUE 自动索引进行 SEARCH
    assert!(
        plan_text.contains("SEARCH") && plan_text.contains("conversation_parts"),
        "Expected plan to search parts using unique index, got: {}",
        plan_text
    );

    // 严禁全表 SCAN
    assert!(
        !plan_text.contains("SCAN"),
        "Query plan should not perform SCAN on conversation_parts or turns, got: {}",
        plan_text
    );

    drop(database);
    cleanup_database(&db_path);
}

fn cleanup_database(db_path: &std::path::Path) {
    let _ = std::fs::remove_file(db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}

pub async fn delete_conversation_adapter_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: &str,
) -> StoreResult<ConversationAdapter> {
    Ok(
        delete_conversation_adapter_registration_sqlx(pool, tenant_id, adapter_id, None)
            .await?
            .ok_or_else(|| {
                StoreError::external(format!("conversation adapter not found: {adapter_id}"))
            })?,
    )
}

pub async fn delete_conversation_adapter_package_sqlx(
    pool: &SqlitePool,
    package_id: &str,
) -> StoreResult<Option<ConversationAdapterPackage>> {
    let package = load_conversation_adapter_package_sqlx(pool, package_id).await?;
    sqlx::query(DELETE_CONVERSATION_ADAPTER_PACKAGE_SQL)
        .bind(package_id)
        .execute(pool)
        .await
        .map_err(StoreError::external)?;
    Ok(package)
}

pub async fn import_incremental_conversation_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &ConversationSource,
    sessions: &[NormalizedConversationSession],
    discovered_external_ids: &BTreeSet<String>,
    dry_run: bool,
) -> StoreResult<ConversationImportResult> {
    import_conversation_sessions_with_presence_sqlx(
        pool,
        tenant_id,
        source,
        sessions,
        Some(discovered_external_ids),
        dry_run,
    )
    .await
}

#[tokio::test]
async fn subagent_session_hidden_from_main_list_but_readable_by_id() {
    let db_path =
        std::env::temp_dir().join(format!("assetiweave-subagent-{}.sqlite", Uuid::new_v4()));
    let database = Database::open_async(&db_path).await.unwrap();
    let adapter = test_conversation_adapter(
        "subagent-adapter",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let pool = database.pool();

    upsert_conversation_adapter_sqlx(pool, TEST_TENANT_ID, &adapter)
        .await
        .unwrap();
    upsert_conversation_source_sqlx(pool, TEST_TENANT_ID, &source)
        .await
        .unwrap();

    let mut main_session = fixture_session("main-session");
    main_session.external_id = "main-session".to_string();
    main_session.user_visible = Some(true);
    main_session.execution_origin = Some("user".to_string());
    // Attach a subagent anchor card to the main session turn
    main_session.turns[0]
        .parts
        .push(NormalizedConversationPart {
        role: ConversationPartRole::Assistant,
        kind: ConversationPartKind::Text,
        text: Some(
            r#"{"agent_role":"Researcher","task":"read files","child_session_id":"sub-session-1"}"#
                .to_string(),
        ),
        language: None,
        command: None,
        cwd: None,
        status: Some("completed".to_string()),
        exit_code: None,
        command_label: None,
        source_execution_id: None,
        content_card: Some(ConversationContentCardDescriptor {
            schema_version: 1,
            kind: "subagent-adapter.subagent".to_string(),
            semantic_role: Some("subagent".to_string()),
            renderer: Some("subagent_tree".to_string()),
        }),
        metadata_json: None,
    });

    let mut sub_session = fixture_session("sub-session-1");
    sub_session.external_id = "sub-session-1".to_string();
    sub_session.user_visible = Some(false);
    sub_session.execution_origin = Some("agent".to_string());
    sub_session.execution_purpose = Some("subagent".to_string());

    import_conversation_sessions_sqlx(
        pool,
        TEST_TENANT_ID,
        &source,
        &[main_session.clone(), sub_session.clone()],
        false,
    )
    .await
    .unwrap();

    // 1. list_conversation_sessions_sqlx must only return the user_visible main session
    let list =
        list_conversation_sessions_sqlx(pool, TEST_TENANT_ID, Some(&adapter.id), None, None, 50, 0)
            .await
            .unwrap();

    assert_eq!(list.len(), 1);
    assert_eq!(list[0].session.external_id, "main-session");

    // 2. load_conversation_session_detail_sqlx can load the hidden subagent session directly
    let hidden_session_id: String = sqlx::query_scalar(
        "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND external_id = ?2",
    )
    .bind(TEST_TENANT_ID)
    .bind("sub-session-1")
    .fetch_one(pool)
    .await
    .unwrap();

    let detail = load_conversation_session_detail_sqlx(pool, TEST_TENANT_ID, &hidden_session_id)
        .await
        .expect("load hidden subagent session detail");

    assert_eq!(detail.session.external_id, "sub-session-1");
    assert!(!detail.session.user_visible);
    assert_eq!(detail.session.execution_origin, "agent");
    assert_eq!(
        detail.session.execution_purpose.as_deref(),
        Some("subagent")
    );
}

#[tokio::test]
async fn import_conversation_sessions_drops_internal_agent_execution_workspaces() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-drop-internal-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.unwrap();
    let adapter = test_conversation_adapter(
        "test-adapter",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let pool = database.pool();

    upsert_conversation_adapter_sqlx(pool, TEST_TENANT_ID, &adapter)
        .await
        .unwrap();
    upsert_conversation_source_sqlx(pool, TEST_TENANT_ID, &source)
        .await
        .unwrap();

    let mut normal_session = fixture_session("normal-user-session");
    normal_session.external_id = "normal-user-session".to_string();
    normal_session.project_path = Some("/Users/developer/code-space/real-project".to_string());
    normal_session.user_visible = Some(true);
    normal_session.execution_origin = Some("user".to_string());

    let mut sandbox_session = fixture_session("internal-sandbox-session");
    sandbox_session.external_id = "internal-sandbox-session".to_string();
    sandbox_session.project_path =
        Some("/Users/developer/.assetiweave/agent-runs/translations/execution-123".to_string());
    sandbox_session.user_visible = Some(false);
    sandbox_session.execution_origin = Some("internal_agent".to_string());

    let mut legacy_sandbox_session = fixture_session("internal-legacy-session");
    legacy_sandbox_session.external_id = "internal-legacy-session".to_string();
    legacy_sandbox_session.project_path = Some(
        "/Users/developer/Library/Application Support/AssetIWeave/agent-executions/execution-456"
            .to_string(),
    );
    legacy_sandbox_session.user_visible = Some(false);
    legacy_sandbox_session.execution_origin = Some("internal_agent".to_string());

    let result = import_conversation_sessions_sqlx(
        pool,
        TEST_TENANT_ID,
        &source,
        &[
            normal_session.clone(),
            sandbox_session.clone(),
            legacy_sandbox_session.clone(),
        ],
        false,
    )
    .await
    .unwrap();

    assert_eq!(result.skipped_session_count, 2);
    assert_eq!(result.changed_session_count, 1);

    let all_sessions =
        list_conversation_sessions_sqlx(pool, TEST_TENANT_ID, Some(&adapter.id), None, None, 50, 0)
            .await
            .unwrap();
    assert_eq!(all_sessions.len(), 1);
    assert_eq!(all_sessions[0].session.external_id, "normal-user-session");

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversation_sessions WHERE tenant_id = ?1")
            .bind(TEST_TENANT_ID)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

// ---------------------------------------------------------------------------
// 读路径切换：单问题详情、块定位与短标识全部由轮次列派生
// ---------------------------------------------------------------------------

/// 删除问题表与成员表的数据，模拟「分组只存在于轮次列」的最终数据形态。
/// 列表的问题计数只认轮次上的分组两列：问题表一行都没有时，计数也必须等于分组数。
#[tokio::test]
async fn sqlx_session_list_counts_question_groups_from_turn_columns() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-session-list-question-count-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "session-list-question-count-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let pool = database.pool();
    upsert_conversation_adapter_sqlx(pool, TEST_TENANT_ID, &adapter)
        .await
        .expect("insert adapter");
    upsert_conversation_source_sqlx(pool, TEST_TENANT_ID, &source)
        .await
        .expect("insert source");
    seed_grouped_turns_without_question_rows(pool, &source.id).await;

    let sessions =
        list_conversation_sessions_sqlx(pool, TEST_TENANT_ID, None, Some(&source.id), None, 20, 0)
            .await
            .expect("list sessions");
    let recent = list_recent_conversation_sessions_sqlx(
        pool,
        TEST_TENANT_ID,
        "2000-01-01T00:00:00Z",
        "2100-01-01T00:00:00Z",
        "",
    )
    .await
    .expect("list recent sessions");

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].question_count, 4);
    assert_eq!(sessions[0].turn_count, 4);
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].session.question_count, 4);
    assert_eq!(recent[0].session.turn_count, 4);

    drop(database);
    cleanup_database(&db_path);
}

/// 直接写轮次分组两列，模拟「问题表与成员表已经不存在」的写入结果。
async fn seed_grouped_turns_without_question_rows(pool: &SqlitePool, source_id: &str) {
    sqlx::query(
        r#"
        INSERT INTO conversation_sessions (
            tenant_id, id, source_id, adapter_id, external_id, title, project_path,
            started_at, updated_at, source_locator, source_fingerprint, missing,
            created_at, imported_at
        ) VALUES (
            ?1, 'count-session', ?2, 'count-adapter', 'count-external', 'Count session',
            NULL, '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z', NULL, NULL, 0,
            '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z'
        )
        "#,
    )
    .bind(TEST_TENANT_ID)
    .bind(source_id)
    .execute(pool)
    .await
    .expect("insert session");
    for (turn_id, turn_index) in [
        ("conversation-turn-count-1", 0),
        ("conversation-turn-count-2", 1),
        ("conversation-turn-count-3", 2),
        ("conversation-turn-count-4", 3),
    ] {
        sqlx::query(
            r#"
            INSERT INTO conversation_turns (
                tenant_id, id, session_id, external_id, turn_index, prompt, title,
                started_at, ended_at, fingerprint, missing, imported_at
            ) VALUES (
                ?1, ?2, 'count-session', ?3, ?4, ?5, NULL, NULL, NULL, ?6, 0,
                '2026-10-23T00:00:00Z'
            )
            "#,
        )
        .bind(TEST_TENANT_ID)
        .bind(turn_id)
        .bind(format!("ext-{turn_index}"))
        .bind(turn_index)
        .bind(format!("turn {turn_index}"))
        .bind(format!("fingerprint-{turn_index}"))
        .execute(pool)
        .await
        .expect("insert grouped turn");
    }
}

/// 卡片搜索的分组必须从轮次列派生：问题表与成员表清空后命中不变。
#[tokio::test]
async fn sqlx_card_search_derives_groups_from_turn_columns_after_legacy_rows_are_dropped() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-card-search-turn-columns-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "card-search-turn-columns-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let pool = database.pool();
    let (_, first_question_id, second_question_id) =
        import_read_path_fixture(pool, &adapter, &source).await;

    drop_legacy_grouping_rows(pool).await;

    let page = search_conversation_cards_sqlx(
        pool,
        TEST_TENANT_ID,
        ConversationRecordKind::Session,
        None,
        Some(&source.id),
        None,
        "Export it",
        &[],
        &[],
        true,
        false,
        None,
        None,
        false,
        20,
        0,
        None,
    )
    .await
    .expect("search question bodies through turn columns");

    assert_eq!(page.total_count, 1);
    let hit = &page.hits[0];
    assert_eq!(hit.question_id, second_question_id);
    assert_eq!(hit.question_index, 1);
    assert_eq!(hit.question_title, "Export it");
    assert_ne!(hit.question_id, first_question_id);

    drop(database);
    cleanup_database(&db_path);
}

async fn drop_legacy_grouping_rows(_pool: &SqlitePool) {}

/// 导入 v1 夹具（t1/t2 合并为一组，t3 独立一组），返回会话与两个分组标识。
async fn import_read_path_fixture(
    pool: &SqlitePool,
    adapter: &crate::backend::domain::ConversationAdapter,
    source: &ConversationSource,
) -> (String, String, String) {
    upsert_conversation_adapter_sqlx(pool, TEST_TENANT_ID, adapter)
        .await
        .unwrap();
    upsert_conversation_source_sqlx(pool, TEST_TENANT_ID, source)
        .await
        .unwrap();
    import_conversation_sessions_sqlx(
        pool,
        TEST_TENANT_ID,
        source,
        &[fixture_session("v1")],
        false,
    )
    .await
    .unwrap();
    let session_id = stable_id("conversation-session", &[&source.id, "session-1"]);
    let first_turn_id = stable_id("conversation-turn", &[&session_id, "t1"]);
    let third_turn_id = stable_id("conversation-turn", &[&session_id, "t3"]);
    (
        session_id.clone(),
        crate::backend::domain::conversations::derive_question_id_from_turn_id(
            "conversation-question",
            &first_turn_id,
        ),
        crate::backend::domain::conversations::derive_question_id_from_turn_id(
            "conversation-question",
            &third_turn_id,
        ),
    )
}

#[tokio::test]
async fn sqlx_question_detail_reads_turn_columns_after_legacy_rows_are_dropped() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-question-detail-turn-columns-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "question-detail-turn-columns-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let pool = database.pool();
    let (session_id, question_id, _) = import_read_path_fixture(pool, &adapter, &source).await;
    let first_turn_id = stable_id("conversation-turn", &[&session_id, "t1"]);
    let second_turn_id = stable_id("conversation-turn", &[&session_id, "t2"]);

    drop_legacy_grouping_rows(pool).await;

    let detail = load_conversation_question_detail_sqlx(pool, TEST_TENANT_ID, &question_id)
        .await
        .expect("load question detail from turn columns");

    assert_eq!(detail.question.id, question_id);
    assert_eq!(detail.question.session_id, session_id);
    assert_eq!(
        detail.question.title.as_deref(),
        Some("How does sync work?")
    );
    assert_eq!(
        detail
            .turns
            .iter()
            .map(|turn| turn.id.as_str())
            .collect::<Vec<_>>(),
        vec![first_turn_id.as_str(), second_turn_id.as_str()]
    );
    assert_eq!(
        detail
            .question_turns
            .iter()
            .map(|membership| (membership.turn_id.as_str(), membership.turn_order))
            .collect::<Vec<_>>(),
        vec![(first_turn_id.as_str(), 0), (second_turn_id.as_str(), 1)]
    );
    assert!(detail
        .question_turns
        .iter()
        .all(|membership| membership.question_id == question_id));
    assert_eq!(detail.parts.len(), 2);
    assert!(detail
        .projected_content_nodes
        .iter()
        .all(|node| node.question_id == question_id));

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_block_locators_and_block_detail_read_turn_columns_after_legacy_rows_are_dropped() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-block-read-path-turn-columns-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "block-read-path-turn-columns-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let pool = database.pool();
    let (session_id, question_id, _) = import_read_path_fixture(pool, &adapter, &source).await;
    let first_turn_id = stable_id("conversation-turn", &[&session_id, "t1"]);

    drop_legacy_grouping_rows(pool).await;

    let locators = list_conversation_block_locators_sqlx(
        pool,
        TEST_TENANT_ID,
        ConversationRecordKind::Session,
        &question_id,
    )
    .await
    .expect("list block locators from turn columns");
    assert_eq!(locators.len(), 4);
    assert!(locators
        .iter()
        .all(|locator| locator.question_id == question_id && locator.session_id == session_id));

    let question_locator = locators
        .iter()
        .find(|locator| locator.turn_id == first_turn_id && locator.kind == "question")
        .expect("question block locator");
    assert_eq!(
        question_locator.block_id,
        format!("{first_turn_id}-question")
    );
    assert_eq!(
        question_locator.content_length,
        "How does sync work?".chars().count()
    );
    let answer_locator = locators
        .iter()
        .find(|locator| {
            locator.turn_id == first_turn_id && locator.semantic_role.as_deref() == Some("answer")
        })
        .expect("answer block locator");

    let question_block = load_conversation_block_detail_sqlx(
        pool,
        TEST_TENANT_ID,
        ConversationRecordKind::Session,
        &question_locator.block_id,
    )
    .await
    .expect("load question block from turn columns");
    assert_eq!(question_block.content, "How does sync work?");
    assert_eq!(question_block.locator.question_id, question_id);
    assert_eq!(question_block.locator.turn_id, first_turn_id);

    let answer_block = load_conversation_block_detail_sqlx(
        pool,
        TEST_TENANT_ID,
        ConversationRecordKind::Session,
        &answer_locator.block_id,
    )
    .await
    .expect("load answer block from turn columns");
    assert_eq!(answer_block.content, "answer for t1");
    assert_eq!(answer_block.locator.question_id, question_id);
    assert_eq!(answer_block.locator.part_id, answer_locator.part_id);

    drop(database);
    cleanup_database(&db_path);
}

async fn resolve_question_prefix(pool: &SqlitePool, input: &str) -> StoreResult<String> {
    resolve_conversation_question_id_prefix_sqlx(pool, TEST_TENANT_ID, input).await
}

#[tokio::test]
async fn sqlx_question_short_id_prefix_resolves_prefixed_and_bare_forms_from_turn_columns() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-question-prefix-turn-columns-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "question-prefix-turn-columns-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let pool = database.pool();
    let (_, question_id, second_question_id) =
        import_read_path_fixture(pool, &adapter, &source).await;
    let fragment = crate::backend::domain::conversation_id_fragment(&question_id);
    let second_fragment = crate::backend::domain::conversation_id_fragment(&second_question_id);

    drop_legacy_grouping_rows(pool).await;

    assert_eq!(
        resolve_question_prefix(pool, &fragment)
            .await
            .expect("resolve bare fragment"),
        question_id
    );
    assert_eq!(
        resolve_question_prefix(pool, &format!("conversation-question-{fragment}"))
            .await
            .expect("resolve prefixed fragment"),
        question_id
    );
    assert_eq!(
        resolve_question_prefix(pool, &question_id)
            .await
            .expect("resolve full identifier"),
        question_id
    );
    assert_eq!(
        resolve_question_prefix(pool, &second_fragment)
            .await
            .expect("resolve the second group"),
        second_question_id
    );

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_question_short_id_prefix_reports_ambiguity_instead_of_guessing() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-question-prefix-ambiguous-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "question-prefix-ambiguous-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let pool = database.pool();
    let (session_id, question_id, _) = import_read_path_fixture(pool, &adapter, &source).await;
    let third_turn_id = stable_id("conversation-turn", &[&session_id, "t3"]);
    let fragment = crate::backend::domain::conversation_id_fragment(&question_id);
    let colliding_turn_id = format!("conversation-turn-{fragment}{}", "0".repeat(56));

    let mut conn = pool.acquire().await.expect("acquire connection");
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await
        .expect("disable foreign keys");
    sqlx::query("UPDATE conversation_turns SET id = ?1 WHERE tenant_id = ?2 AND id = ?3")
        .bind(&colliding_turn_id)
        .bind(TEST_TENANT_ID)
        .bind(&third_turn_id)
        .execute(&mut *conn)
        .await
        .expect("force a colliding question prefix");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *conn)
        .await
        .expect("enable foreign keys");
    drop(conn);
    drop_legacy_grouping_rows(pool).await;

    let error = resolve_question_prefix(pool, &fragment)
        .await
        .expect_err("ambiguous prefix must not resolve to one group");
    assert!(
        matches!(error, StoreError::Conflict(_)),
        "ambiguous prefix must be a conflict, got {error:?}"
    );
    let message = error.to_string();
    assert!(message.contains("ambiguous prefix"), "{message}");
    assert!(message.contains("2 questions match"), "{message}");

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_question_short_id_prefix_rejects_turn_prefix_and_resolves_web_records() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-question-prefix-web-and-reject-turn-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let pool = database.pool();

    let session_id = "web-record-session-test-0001";
    let turn_id =
        "web-record-turn-abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    let expected_question_id =
        "web-record-question-abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

    let mut conn = pool.acquire().await.expect("acquire connection");
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await
        .expect("disable fk");
    sqlx::query(
        "INSERT INTO web_record_sessions (tenant_id, id, source_id, adapter_id, external_id, title, missing, created_at, imported_at) \
         VALUES (?1, ?2, 'src-1', 'adp-1', 'ext-s', 'Web Title', 0, '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
    )
    .bind(TEST_TENANT_ID)
    .bind(session_id)
    .execute(&mut *conn)
    .await
    .expect("insert web session");

    sqlx::query(
        "INSERT INTO web_record_turns (tenant_id, id, session_id, external_id, turn_index, prompt, fingerprint, missing, imported_at) \
         VALUES (?1, ?2, ?3, 'ext-t', 0, 'web prompt', 'fp-t', 0, '2026-10-23T00:00:00Z')",
    )
    .bind(TEST_TENANT_ID)
    .bind(turn_id)
    .bind(session_id)
    .execute(&mut *conn)
    .await
    .expect("insert web turn");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *conn)
        .await
        .expect("enable fk");
    drop(conn);

    // 1. Turn- prefixed lookups must return NotFound (no inappropriate cross-entity coercion)
    let err_turn = resolve_question_prefix(pool, "conversation-turn-abcdef01")
        .await
        .expect_err("turn- prefix must not resolve as question");
    assert!(matches!(err_turn, StoreError::NotFound(_)));

    let err_web_turn = resolve_question_prefix(pool, "web-record-turn-abcdef01")
        .await
        .expect_err("web-record-turn- prefix must not resolve as question");
    assert!(matches!(err_web_turn, StoreError::NotFound(_)));

    let err_bare_turn = resolve_question_prefix(pool, "turn-abcdef01")
        .await
        .expect_err("bare turn- prefix must not resolve as question");
    assert!(matches!(err_bare_turn, StoreError::NotFound(_)));

    // 2. Web record questions must resolve from web_record_turns
    let resolved_bare = resolve_question_prefix(pool, "abcdef01")
        .await
        .expect("resolve bare 8-char hex for web question");
    assert_eq!(resolved_bare, expected_question_id);

    let resolved_web_prefixed = resolve_question_prefix(pool, "web-record-question-abcdef01")
        .await
        .expect("resolve web-record-question- prefixed for web question");
    assert_eq!(resolved_web_prefixed, expected_question_id);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_question_fragment_and_any_id_resolve_through_turn_columns() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-question-any-id-turn-columns-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "question-any-id-turn-columns-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let pool = database.pool();
    let (session_id, question_id, _) = import_read_path_fixture(pool, &adapter, &source).await;
    let fragment = crate::backend::domain::conversation_id_fragment(&question_id);

    drop_legacy_grouping_rows(pool).await;

    for identifier in [fragment.as_str(), question_id.as_str()] {
        let (kind, resolved_session_id) =
            resolve_any_id_to_session_id_sqlx(pool, TEST_TENANT_ID, identifier)
                .await
                .unwrap_or_else(|error| panic!("resolve {identifier} to its session: {error}"));
        assert_eq!(kind, ConversationRecordKind::Session);
        assert_eq!(resolved_session_id, session_id);
    }

    let sessions = list_conversation_sessions_by_id_fragment_sqlx(
        pool,
        TEST_TENANT_ID,
        ConversationRecordKind::Session,
        None,
        Some(&source.id),
        &fragment,
        20,
        0,
    )
    .await
    .expect("list sessions by question fragment");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session.id, session_id);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_session_search_hits_tool_output_file_change_and_prompt_bodies() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-part-search-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "part-search-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let mut session = fixture_session("v1");
    session.turns[0].parts.push(normalized_search_part(
        ConversationPartRole::Tool,
        ConversationPartKind::Tool,
        "tool_result",
        "fatal: unable to access remote repository",
    ));
    session.turns[1].parts.push(normalized_search_part(
        ConversationPartRole::Assistant,
        ConversationPartKind::FileChange,
        "file_change",
        "modified src/lib.rs and added a retry loop",
    ));
    session.turns[2].parts.push(normalized_search_part(
        ConversationPartRole::Tool,
        ConversationPartKind::Tool,
        "execution_result",
        "exit status 128 from the test runner",
    ));

    let (tool_hits, file_hits, execution_hits, answer_hits, prompt_hits, missing_hits) = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[session],
            false,
        )
        .await?;
        let mut results = Vec::new();
        for needle in [
            "unable to access remote",
            "retry loop",
            "exit status 128",
            "answer for t1",
            "How does sync work?",
            "nothing-matches-this-needle",
        ] {
            results.push(
                list_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    None,
                    Some(&source.id),
                    Some(needle),
                    20,
                    0,
                )
                .await?,
            );
        }
        let mut results = results.into_iter();
        StoreResult::Ok((
            results.next().unwrap(),
            results.next().unwrap(),
            results.next().unwrap(),
            results.next().unwrap(),
            results.next().unwrap(),
            results.next().unwrap(),
        ))
    }
    .await
    .expect("search sessions by part bodies through SQLx");

    assert_eq!(tool_hits.len(), 1, "工具输出正文必须可搜");
    assert_eq!(file_hits.len(), 1, "文件变更正文必须可搜");
    assert_eq!(execution_hits.len(), 1, "执行结果正文必须可搜");
    assert_eq!(answer_hits.len(), 1, "改造前可搜的答复正文必须仍可搜");
    assert_eq!(prompt_hits.len(), 1, "改造前可搜的提问正文必须仍可搜");
    assert!(missing_hits.is_empty());

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_session_search_degrades_to_title_while_part_index_rebuilds() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-part-search-degraded-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "part-search-degraded-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);

    let (ready_body_hits, rebuilding_body_hits, rebuilding_title_hits) = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[fixture_session("v1")],
            false,
        )
        .await?;
        let search = |needle: &'static str| {
            list_conversation_sessions_sqlx(
                database.pool(),
                TEST_TENANT_ID,
                None,
                Some(&source.id),
                Some(needle),
                20,
                0,
            )
        };
        let ready_body_hits = search("answer for t1").await?;
        crate::backend::store::set_conversation_part_index_status_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            "building",
            0,
            None,
        )
        .await?;
        let rebuilding_body_hits = search("answer for t1").await?;
        let rebuilding_title_hits = search("Fixture").await?;
        StoreResult::Ok((ready_body_hits, rebuilding_body_hits, rebuilding_title_hits))
    }
    .await
    .expect("search sessions while the part index rebuilds");

    assert_eq!(ready_body_hits.len(), 1);
    assert!(
        rebuilding_body_hits.is_empty(),
        "重建期间正文匹配应降级，避免给出误导性的空结果"
    );
    assert_eq!(
        rebuilding_title_hits.len(),
        1,
        "重建期间标题匹配必须仍然可用"
    );

    drop(database);
    cleanup_database(&db_path);
}

fn normalized_search_part(
    role: ConversationPartRole,
    kind: ConversationPartKind,
    card_type: &str,
    text: &str,
) -> NormalizedConversationPart {
    NormalizedConversationPart {
        role,
        kind,
        text: Some(text.to_string()),
        language: None,
        command: None,
        cwd: None,
        status: None,
        exit_code: None,
        command_label: None,
        source_execution_id: None,
        content_card: None,
        metadata_json: content_card_metadata(card_type),
    }
}
