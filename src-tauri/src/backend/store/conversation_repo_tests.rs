use super::*;
use crate::backend::models::{
    ConversationAdapterPackageOrigin, ConversationAdapterPackageRecordKind,
    ConversationAdapterRuntimeGateStatus, ConversationCardKindDefinition,
    ConversationContentCardDescriptor, ConversationPackageUpdatePolicy, ConversationPartKind,
    ConversationPartRole, NormalizedConversationPart, NormalizedConversationTurn,
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
            assert!(matches!(result, Err(AppError::Cancelled(_))), "{result:?}");
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
            .map_err(AppError::external)?;
        Ok::<_, AppError>(rows.into_iter().map(|row| row.detail).collect::<Vec<_>>())
    }
    .await
    .expect("explain recent query");
    let plan = details.join("\n");

    assert!(
        plan.contains("idx_conversation_turns_tenant_session"),
        "{plan}"
    );
    assert!(
        plan.contains("idx_conversation_parts_tenant_turn"),
        "{plan}"
    );

    drop(database);
    let _ = std::fs::remove_file(db_path);
}

#[test]
fn question_aggregate_excludes_tool_result_and_diff_bodies() {
    let base = |kind: ConversationPartKind, card_kind: &str, text: &str| ConversationPart {
        id: format!("part-{card_kind}"),
        turn_id: "turn-1".to_string(),
        part_index: 0,
        role: crate::backend::models::ConversationPartRole::Tool,
        kind,
        text: Some(text.to_string()),
        language: None,
        command: None,
        cwd: None,
        status: None,
        exit_code: None,
        command_label: None,
        source_execution_id: None,
        content_card: Some(ConversationContentCardDescriptor {
            schema_version: 1,
            kind: format!("fixture.{card_kind}"),
            renderer: Some(if card_kind == "diff" { "diff" } else { "plain" }.to_string()),
        }),
        metadata_json: None,
        translated_text: None,
    };
    let answer = base(ConversationPartKind::Text, "answer", "answer");
    let result = base(ConversationPartKind::Tool, "result", "duplicated result");
    let diff = base(
        ConversationPartKind::FileChange,
        "result",
        "duplicated diff",
    );
    let mut answer_text = Vec::new();
    let mut code_text = Vec::new();
    let mut command_text = Vec::new();

    append_declared_card_to_question_aggregate(
        &answer,
        &mut answer_text,
        &mut code_text,
        &mut command_text,
    );
    append_declared_card_to_question_aggregate(
        &result,
        &mut answer_text,
        &mut code_text,
        &mut command_text,
    );
    append_declared_card_to_question_aggregate(
        &diff,
        &mut answer_text,
        &mut code_text,
        &mut command_text,
    );

    assert_eq!(answer_text, vec!["answer"]);
    assert!(code_text.is_empty());
    assert!(command_text.is_empty());
}

#[test]
fn question_aggregate_indexes_the_raw_shell_part_for_fts_compatibility() {
    let part = ConversationPart {
        id: "conversation-part-shell".to_string(),
        turn_id: "conversation-turn-shell".to_string(),
        part_index: 0,
        role: ConversationPartRole::Tool,
        kind: ConversationPartKind::Command,
        text: None,
        language: None,
        command: Some("raw shell command".to_string()),
        cwd: None,
        status: None,
        exit_code: None,
        command_label: None,
        source_execution_id: Some("execution-shell".to_string()),
        content_card: Some(ConversationContentCardDescriptor {
            schema_version: 1,
            kind: "fixture.command".to_string(),
            renderer: Some("command".to_string()),
        }),
        metadata_json: Some(
            serde_json::json!({
                "shell_execution_projection": {
                    "schema_version": 1,
                    "nodes": [
                        {"command": "printf first", "command_label": "first"},
                        {"command": "printf second", "command_label": "second"}
                    ]
                }
            })
            .to_string(),
        ),
        translated_text: None,
    };
    let mut answer_text = Vec::new();
    let mut code_text = Vec::new();
    let mut command_text = Vec::new();
    append_projected_cards_to_question_aggregate(
        &part,
        "fixture",
        &[],
        &mut answer_text,
        &mut code_text,
        &mut command_text,
    )
    .expect("project raw shell Part into FTS fields");

    assert_eq!(command_text, vec!["raw shell command"]);
    assert!(answer_text.is_empty());
    assert!(code_text.is_empty());
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

        Ok::<_, AppError>((
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
    let install_dir = crate::backend::host_paths::HostDirectories::current()
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
        Ok::<_, AppError>((
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

    let mut stored_package = package;
    stored_package.install_dir =
        "@config/assetiweave/conversation-adapters/packages/codex-session/current".to_string();
    stored_package.manifest_path = format!(
        "{}/conversation-adapter-package.json",
        stored_package.install_dir
    );
    stored_package.adapter_manifest_path =
        format!("{}/conversation-adapter.json", stored_package.install_dir);
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
async fn conversation_adapter_runtime_paths_normalize_to_config_anchor() {
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
    let adapter_dir = crate::backend::host_paths::HostDirectories::current()
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

    assert_eq!(
        loaded.manifest_path.as_deref(),
        Some(
            "@config/assetiweave/conversation-adapters/portable-adapter/conversation-adapter.json"
        )
    );
    assert_eq!(
        loaded.executable_path.as_deref(),
        Some("@config/assetiweave/conversation-adapters/portable-adapter/adapter.mjs")
    );
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
        Ok::<_, AppError>((
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
        Ok::<_, AppError>((
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
        Ok::<_, AppError>((
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
        Ok::<_, AppError>((adapter, source))
    }
    .await
    .expect("disable built-in adapter");

    assert!(!adapter.enabled);
    assert!(!source.enabled);

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_import_preserves_manual_grouping_across_resync() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-import-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "import-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);

    let (initial_question_count, initial_first_question_turn_count, detail) = async {
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
        let initial_question_count = detail.questions.len();
        let initial_first_question_turn_count = detail.questions[0].turns.len();
        let question_ids = detail
            .questions
            .iter()
            .map(|question| question.question.id.clone())
            .collect::<Vec<_>>();
        merge_conversation_questions_sqlx(database.pool(), TEST_TENANT_ID, &question_ids, false)
            .await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[fixture_session("v2")],
            false,
        )
        .await?;
        let detail = load_conversation_session_detail_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &sessions[0].session.id,
        )
        .await?;
        Ok::<_, AppError>((
            initial_question_count,
            initial_first_question_turn_count,
            detail,
        ))
    }
    .await
    .expect("import and merge through SQLx");

    assert_eq!(initial_question_count, 2);
    assert_eq!(initial_first_question_turn_count, 2);
    assert_eq!(detail.questions.len(), 1);
    assert_eq!(detail.questions[0].turns.len(), 3);
    assert!(detail.questions[0]
        .question_turns
        .iter()
        .all(|membership| membership.assignment_origin == ConversationGroupingOrigin::Manual));

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_reconciliation_rebuilds_changed_automatic_grouping_deterministically() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-reconciliation-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "reconciliation-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let (detail, membership_rows) = async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
                let mut initial = fixture_session("v1");
                initial.turns[1].user_text = "Export it".to_string();
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[initial],
                    false,
                )
                .await?;

                let mut reconciled = fixture_session("v1");
                reconciled.turns[1].user_text = "继续".to_string();
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[reconciled],
                    false,
                )
                .await?;
                let session_id = stable_id("conversation-session", &[&source.id, "session-1"]);
                let detail = load_conversation_session_detail_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &session_id,
                )
                .await?;
                let membership_rows = sqlx::query_as::<_, (String, String, String)>(
                    "SELECT question_id, turn_id, assignment_origin FROM conversation_question_turns WHERE tenant_id = ?1 ORDER BY turn_id",
                )
                .bind(TEST_TENANT_ID)
                .fetch_all(database.pool())
                .await
                .map_err(AppError::external)?;
                Ok::<_, AppError>((detail, membership_rows))
            }.await
            .expect("reconcile changed grouping");

    assert_eq!(detail.questions.len(), 2);
    assert_eq!(detail.questions[0].turns.len(), 2);
    assert_eq!(detail.questions[0].turns[0].external_id, "t1");
    assert_eq!(detail.questions[0].turns[1].external_id, "t2");
    assert_eq!(detail.questions[1].turns[0].external_id, "t3");
    assert!(membership_rows
        .iter()
        .all(|(_, _, origin)| origin == "imported" || origin == "auto_merged"));

    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_manual_membership_is_a_reconciliation_fence() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-manual-fence-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "manual-fence-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let detail = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
        let mut initial = fixture_session("v1");
        initial.turns.truncate(2);
        initial.turns[1].user_text = "Export it".to_string();
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[initial],
            false,
        )
        .await?;
        let session_id = stable_id("conversation-session", &[&source.id, "session-1"]);
        let initial_detail =
            load_conversation_session_detail_sqlx(database.pool(), TEST_TENANT_ID, &session_id)
                .await?;
        let question_ids = initial_detail
            .questions
            .iter()
            .map(|question| question.question.id.clone())
            .collect::<Vec<_>>();
        merge_conversation_questions_sqlx(database.pool(), TEST_TENANT_ID, &question_ids, false)
            .await?;

        let mut updated = fixture_session("v1");
        updated.turns[1].user_text = "Export it".to_string();
        updated.turns[2].user_text = "继续".to_string();
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[updated],
            false,
        )
        .await?;
        load_conversation_session_detail_sqlx(database.pool(), TEST_TENANT_ID, &session_id).await
    }
    .await
    .expect("preserve manual fence");

    assert_eq!(detail.questions.len(), 2);
    assert_eq!(detail.questions[0].turns.len(), 2);
    assert!(detail.questions[0]
        .question_turns
        .iter()
        .all(|membership| membership.assignment_origin == ConversationGroupingOrigin::Manual));
    assert_eq!(detail.questions[1].turns.len(), 1);
    assert_eq!(detail.questions[1].turns[0].external_id, "t3");

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
                    question
                        .question_turns
                        .iter()
                        .map(|membership| {
                            (membership.turn_id.clone(), membership.assignment_origin)
                        })
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
        Ok::<_, AppError>((snapshot(&full_detail), snapshot(&repeated_detail)))
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
        Ok::<_, AppError>(snapshot(&detail))
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
            .map(|turn| turn.user_text.as_str())
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
async fn sqlx_rejects_cross_session_question_turn_membership_before_new_write() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-question-membership-scope-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "question-membership-scope-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);
    let mut second_session = fixture_session("v1");
    second_session.external_id = "session-2".to_string();

    let error = async {
        upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
        upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
        import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[fixture_session("v1"), second_session],
            false,
        )
        .await?;

        let first_session_id = stable_id("conversation-session", &[&source.id, "session-1"]);
        let second_session_id = stable_id("conversation-session", &[&source.id, "session-2"]);
        let first_turn_id = stable_id("conversation-turn", &[&first_session_id, "t1"]);
        let second_turn_id = stable_id("conversation-turn", &[&second_session_id, "t1"]);
        let first_question_id = stable_id(
            "conversation-question",
            &[&first_session_id, &first_turn_id],
        );
        sqlx::query(
            "DELETE FROM conversation_question_turns WHERE tenant_id = ?1 AND turn_id = ?2",
        )
        .bind(TEST_TENANT_ID)
        .bind(&second_turn_id)
        .execute(database.pool())
        .await
        .map_err(AppError::external)?;
        sqlx::query(
            r#"
                    INSERT INTO conversation_question_turns (
                        tenant_id, question_id, turn_id, turn_order,
                        assignment_origin, assigned_at, updated_at
                    )
                    VALUES (?1, ?2, ?3, 0, 'imported', ?4, ?4)
                    "#,
        )
        .bind(TEST_TENANT_ID)
        .bind(&first_question_id)
        .bind(&second_turn_id)
        .bind("2026-08-25T00:00:00Z")
        .execute(database.pool())
        .await
        .map_err(AppError::external)?;

        let result = import_conversation_sessions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &source,
            &[fixture_session("v1")],
            false,
        )
        .await;
        Ok::<_, AppError>(result.expect_err("cross-session membership must block the write"))
    }
    .await
    .expect("validate cross-session membership");

    assert!(error.to_string().contains("cross_session"));
    let audit_count = async {
                sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM conversation_question_turn_audits WHERE tenant_id = ?1 AND record_kind = 'session' AND reason = 'cross_session'",
                )
                .bind(TEST_TENANT_ID)
                .fetch_one(database.pool())
                .await
                .map_err(AppError::external)
            }.await
            .expect("persist cross-session audit");
    assert_eq!(audit_count, 1);
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
        .map_err(AppError::external)?;
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
        .map_err(AppError::external)
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
    old_session.turns[0].parts[0].metadata_json = None;
    let mut refreshed_session = fixture_session("v1");
    refreshed_session.source_fingerprint = Some("same-source".to_string());
    refreshed_session.turns[0].parts[0].metadata_json =
        Some(r#"{"content_card":{"type":"answer","format":"markdown"}}"#.to_string());

    let (result, imported_at, metadata_json, part_ids_before, part_ids_after) = async {
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
        let part_ids_before = sqlx::query_as::<_, (String, i64)>(
            r#"
                    SELECT p.id, p.part_index
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
        .map_err(AppError::external)?;
        sqlx::query(
            "UPDATE conversation_sessions SET imported_at = 'preserved' WHERE source_id = ?1",
        )
        .bind(&source.id)
        .execute(database.pool())
        .await
        .map_err(AppError::external)?;
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
        .map_err(AppError::external)?;
        let metadata_json = sqlx::query_scalar::<_, Option<String>>(
            r#"
                    SELECT p.metadata_json
                    FROM conversation_parts p
                    JOIN conversation_turns t ON t.id = p.turn_id
                    JOIN conversation_sessions s ON s.id = t.session_id
                    WHERE s.source_id = ?1
                    ORDER BY p.part_index ASC
                    LIMIT 1
                    "#,
        )
        .bind(&source.id)
        .fetch_one(database.pool())
        .await
        .map_err(AppError::external)?;
        let part_ids_after = sqlx::query_as::<_, (String, i64)>(
            r#"
                    SELECT p.id, p.part_index
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
        .map_err(AppError::external)?;
        Ok::<_, AppError>((
            result,
            imported_at,
            metadata_json,
            part_ids_before,
            part_ids_after,
        ))
    }
    .await
    .expect("refresh normalized parts through SQLx");

    assert_eq!(result.skipped_session_count, 0);
    assert_ne!(imported_at, "preserved");
    assert_eq!(part_ids_after, part_ids_before);
    assert!(!part_ids_after.is_empty());
    assert!(metadata_json
        .as_deref()
        .unwrap_or("")
        .contains(r#""content_card""#));

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
        .map_err(AppError::external)?;
        Ok::<_, AppError>((detail, stale_parts))
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
        .map_err(AppError::external)?;
        Ok::<_, AppError>((listed, missing_count))
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
        Ok::<_, AppError>((sessions, detail, filtered_questions, question))
    }
    .await
    .expect("read conversations through SQLx");

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].question_count, 2);
    assert_eq!(sessions[0].turn_count, 3);
    assert_eq!(detail.questions.len(), 2);
    assert_eq!(filtered_questions.len(), 1);
    assert_eq!(
        filtered_questions[0].question.title.as_deref(),
        Some("Export it")
    );
    assert_eq!(question.turns.len(), 1);
    assert_eq!(question.parts.len(), 3);
    assert_eq!(
        question.parts[1].source_execution_id.as_deref(),
        Some("call-export")
    );
    assert_eq!(
        question.parts[2].source_execution_id.as_deref(),
        Some("call-export")
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
    assert_eq!(projected_nodes[1]["source_execution_id"], "call-export");
    assert_eq!(projected_nodes[2]["source_execution_id"], "call-export");

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
        let fragment = crate::backend::models::conversation_id_fragment(&session_id);
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
        .map_err(AppError::external)?;
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
        Ok::<_, AppError>((fragment_matches, direct_fragment_matches, full_matches))
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
async fn sqlx_merge_and_split_conversation_questions_preserve_grouping() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-conversation-mutation-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let adapter = test_conversation_adapter(
        "mutation-external",
        ConversationAdapterKind::External,
        ConversationAdapterTrustState::Trusted,
    );
    let source = test_conversation_source(&adapter.id);

    let (detail, original_part_ids) = async {
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
        let detail = load_conversation_session_detail_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &stable_id("conversation-session", &[&source.id, "session-1"]),
        )
        .await?;
        let question_ids = detail
            .questions
            .iter()
            .map(|question| question.question.id.clone())
            .collect::<Vec<_>>();
        let original_part_ids = detail
            .questions
            .iter()
            .flat_map(|question| question.parts.iter().map(|part| part.id.clone()))
            .collect::<BTreeSet<_>>();
        let dry_run =
            merge_conversation_questions_sqlx(database.pool(), TEST_TENANT_ID, &question_ids, true)
                .await?;
        assert!(dry_run.dry_run);
        assert_eq!(
            load_conversation_session_detail_sqlx(
                database.pool(),
                TEST_TENANT_ID,
                &detail.session.id,
            )
            .await?
            .questions
            .len(),
            2
        );
        let merged = merge_conversation_questions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &question_ids,
            false,
        )
        .await?;
        let repeated_merge = merge_conversation_questions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &question_ids,
            false,
        )
        .await?;
        assert_eq!(repeated_merge.questions.len(), 1);
        assert_eq!(repeated_merge.questions[0].turns.len(), 3);
        let first_turn_error = split_conversation_question_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &merged.questions[0].question.id,
            &merged.questions[0].turns[0].id,
            false,
        )
        .await
        .expect_err("split at first turn should fail");
        assert!(first_turn_error.contains("must not be the first turn"));
        let split_turn_id = merged.questions[0].turns[2].id.clone();
        let split = split_conversation_question_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &merged.questions[0].question.id,
            &split_turn_id,
            false,
        )
        .await?;
        let repeated_split = split_conversation_question_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &merged.questions[0].question.id,
            &split_turn_id,
            false,
        )
        .await?;
        assert_eq!(
            split.affected_question_ids,
            repeated_split.affected_question_ids
        );
        let final_detail = load_conversation_session_detail_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            &detail.session.id,
        )
        .await?;
        Ok::<_, AppError>((final_detail, original_part_ids))
    }
    .await
    .expect("merge and split through SQLx");

    assert_eq!(detail.questions.len(), 2);
    assert_eq!(detail.questions[0].turns.len(), 2);
    assert_eq!(detail.questions[1].turns.len(), 1);
    assert!(detail.questions.iter().all(|question| question
        .question_turns
        .iter()
        .all(|membership| membership.assignment_origin == ConversationGroupingOrigin::Manual)));
    let final_part_ids = detail
        .questions
        .iter()
        .flat_map(|question| question.parts.iter().map(|part| part.id.clone()))
        .collect::<BTreeSet<_>>();
    assert_eq!(final_part_ids, original_part_ids);
    let final_turn_ids = detail
        .questions
        .iter()
        .flat_map(|question| question.turns.iter().map(|turn| turn.id.clone()))
        .collect::<BTreeSet<_>>();
    assert_eq!(final_turn_ids.len(), 3);
    assert!(detail.questions.iter().all(|question| question
        .question_turns
        .iter()
        .all(|membership| membership.assignment_origin == ConversationGroupingOrigin::Manual)));
    assert_eq!(
        detail.questions[0].question.title.as_deref(),
        Some("How does sync work?")
    );
    assert_eq!(
        detail.questions[1].question.title.as_deref(),
        Some("Export it")
    );

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
        let fragment = crate::backend::models::conversation_id_fragment(&session_id);
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
        Ok::<_, AppError>((session_page, web_page, fragment_page, fragment_session_ids))
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
        ConversationSearchCardType::answer()
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
        Ok::<_, AppError>((detail, undeclared_list, undeclared_page, declared_page))
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
        Ok::<_, AppError>((session_id, alpha_detail, beta_detail, alpha_page, beta_page))
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
        Ok::<_, AppError>((stored_adapter, part_id, detail))
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
        crate::backend::conversations::ConversationSessionDescriptor {
            external_id: "session-1".to_string(),
            updated_at: None,
            source_locator: None,
            version_token: "source-v1".to_string(),
        },
    ];
    let hydrated = BTreeSet::from(["session-1".to_string()]);

    let (same, adapter_changed, contract_changed, payload_policy_changed) = async {
        persist_conversation_session_observations_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            "source-1",
            ConversationRecordKind::Session,
            &descriptors,
            &hydrated,
            Some("adapter-hash-v1"),
            Some(1),
            crate::backend::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
        )
        .await?;
        let same = load_conversation_session_versions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            "source-1",
            ConversationRecordKind::Session,
            Some("adapter-hash-v1"),
            Some(1),
            crate::backend::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
        )
        .await?;
        let adapter_changed = load_conversation_session_versions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            "source-1",
            ConversationRecordKind::Session,
            Some("adapter-hash-v2"),
            Some(1),
            crate::backend::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
        )
        .await?;
        let contract_changed = load_conversation_session_versions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            "source-1",
            ConversationRecordKind::Session,
            Some("adapter-hash-v1"),
            Some(2),
            crate::backend::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
        )
        .await?;
        let payload_policy_changed = load_conversation_session_versions_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            "source-1",
            ConversationRecordKind::Session,
            Some("adapter-hash-v1"),
            Some(1),
            crate::backend::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION + 1,
        )
        .await?;
        Ok::<_, AppError>((
            same,
            adapter_changed,
            contract_changed,
            payload_policy_changed,
        ))
    }
    .await
    .expect("compare hydration identity");

    assert_eq!(same.get("session-1").map(String::as_str), Some("source-v1"));
    assert!(adapter_changed.is_empty());
    assert!(contract_changed.is_empty());
    assert!(payload_policy_changed.is_empty());

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
            crate::backend::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
        )
        .await?;
        mark_conversation_payload_policy_applied_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            crate::backend::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
        )
        .await?;
        let required_after = conversation_payload_policy_reparse_required_sqlx(
            database.pool(),
            TEST_TENANT_ID,
            crate::backend::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
        )
        .await?;
        Ok::<_, AppError>((required_before, required_after))
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
async fn conversation_source_locations_normalize_absolute_home_paths() {
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

    assert_eq!(loaded.location, "~/.codex");
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
            SELECT p.id, p.turn_id, p.part_index, p.role, p.kind, p.text, p.language,
                   p.command, p.cwd, p.status, p.exit_code, p.metadata_json,
                   p.content_card_json, p.translated_text, p.source_execution_id, p.command_label
            FROM conversation_turns t INDEXED BY idx_conversation_turns_tenant_session
            JOIN conversation_parts p INDEXED BY idx_conversation_parts_tenant_turn
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

    // 验证 parts 表使用 idx_conversation_parts_tenant_turn 进行 SEARCH
    assert!(
        plan_text.contains("idx_conversation_parts_tenant_turn"),
        "Expected plan to search parts using idx_conversation_parts_tenant_turn, got: {}",
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

pub(crate) async fn delete_conversation_adapter_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: &str,
) -> AppResult<ConversationAdapter> {
    Ok(
        delete_conversation_adapter_registration_sqlx(pool, tenant_id, adapter_id, None)
            .await?
            .ok_or_else(|| {
                AppError::external(format!("conversation adapter not found: {adapter_id}"))
            })?,
    )
}

pub(crate) async fn delete_conversation_adapter_package_sqlx(
    pool: &SqlitePool,
    package_id: &str,
) -> AppResult<Option<ConversationAdapterPackage>> {
    let package = load_conversation_adapter_package_sqlx(pool, package_id).await?;
    sqlx::query(DELETE_CONVERSATION_ADAPTER_PACKAGE_SQL)
        .bind(package_id)
        .execute(pool)
        .await
        .map_err(AppError::external)?;
    Ok(package)
}

pub(crate) async fn import_incremental_conversation_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &ConversationSource,
    sessions: &[NormalizedConversationSession],
    discovered_external_ids: &BTreeSet<String>,
    dry_run: bool,
) -> AppResult<ConversationImportResult> {
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

pub(super) fn append_declared_card_to_question_aggregate(
    part: &ConversationPart,
    answer_text: &mut Vec<String>,
    code_text: &mut Vec<String>,
    command_text: &mut Vec<String>,
) {
    let Some(card) = resolved_content_card_for_part(part) else {
        return;
    };
    let semantic_role = card
        .semantic_role
        .as_deref()
        .or_else(|| card.kind.rsplit_once('.').map(|(_, value)| value))
        .unwrap_or(card.kind.as_str());
    match semantic_role {
        "answer" => answer_text.push(card.body),
        "code" => code_text.push(card.body),
        "command" => command_text.push(card.body),
        // Tool, result, file changes and adapter-specific cards are kept on their
        // original Part/Card only; question aggregates must not duplicate them.
        _ => {}
    }
}

fn resolved_content_card_for_part(
    part: &ConversationPart,
) -> Option<crate::backend::projection::conversation_cards::ResolvedConversationContentCard> {
    crate::backend::projection::conversation_cards::resolve_historical_content_card(
        crate::backend::projection::conversation_cards::ConversationCardProjectionSource {
            content_card: part.content_card.as_ref(),
            metadata_json: part.metadata_json.as_deref(),
            text: part.text.as_deref(),
            language: part.language.as_deref(),
            command: part.command.as_deref(),
            cwd: part.cwd.as_deref(),
            status: part.status.as_deref(),
            exit_code: part.exit_code,
        },
    )
    .ok()?
}
