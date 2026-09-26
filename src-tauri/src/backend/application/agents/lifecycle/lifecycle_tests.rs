use super::*;
use crate::backend::{
    domain::agents::{
        market::{
            Catalog, CatalogCapabilities, CatalogItem, CatalogSource, CoreCompatibility,
            Distribution, Target, UpstreamSource, Verification, VerificationStatus,
        },
        AgentId, AgentMarketProtocol, InstallationStatus, ProtocolStatus, RuntimeStatus,
    },
    infrastructure::agent_market::{
        materialize::register_test_artifact, types::AgentInstallStartRequest,
    },
    store::Database,
};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Arc};

const AGENT_ID: &str = "fixture-agent";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_market_lifecycle_e2e_install_update_failure_recovery_and_cancel() {
    let database_path = std::env::temp_dir().join(format!(
        "assetiweave-agent-market-e2e-{}.db",
        uuid::Uuid::new_v4()
    ));
    let runtime_root = std::env::temp_dir().join(format!(
        "assetiweave-agent-market-runtime-{}",
        uuid::Uuid::new_v4()
    ));
    let workspace_root = std::env::temp_dir().join(format!(
        "assetiweave-agent-market-workspace-{}",
        uuid::Uuid::new_v4()
    ));
    let pool = Database::open_initialized_async(&database_path)
        .await
        .expect("database")
        .pool()
        .clone();
    let fixture_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/fake-acp-agent.mjs");

    let good_v1 = fixture_agent_script(&fixture_path, "happy");
    let good_v2 = fixture_agent_script(&fixture_path, "happy");
    let failed_v3 = fixture_agent_script(&fixture_path, "initialize_error");
    let url_v1 = format!("https://fixture.invalid/{}/v1", uuid::Uuid::new_v4());
    let url_v2 = format!("https://fixture.invalid/{}/v2", uuid::Uuid::new_v4());
    let url_v3 = format!("https://fixture.invalid/{}/v3", uuid::Uuid::new_v4());
    register_test_artifact(&url_v1, good_v1.clone());
    register_test_artifact(&url_v2, good_v2.clone());
    register_test_artifact(&url_v3, failed_v3.clone());

    let manager = Arc::new(AgentRuntimeManager::new(
        pool.clone(),
        workspace_root.clone(),
    ));
    let service_v1 = AgentLifecycleCoordinator::new_with_catalog(
        pool.clone(),
        manager.clone(),
        runtime_root.clone(),
        CatalogService::from_catalog(fixture_catalog("1.0.0", &url_v1, &good_v1)),
    );
    let mut install_request = request_for(&service_v1, "install", "1.0.0");
    install_request.catalog_version = "observed-old-catalog".to_string();
    install_request.agent_version = "0.0.1".to_string();
    let installed = service_v1
        .install(install_request)
        .await
        .expect("observational request versions must not block install");
    assert_eq!(
        installed.installation.protocol_status,
        ProtocolStatus::Ready
    );
    assert_eq!(installed.installation.catalog_item_version, "1.0.0");
    assert_eq!(
        installed.installation.definition_json["capabilities"]["liveEvents"],
        serde_json::json!(true)
    );
    assert_eq!(
        installed.installation.definition_json["capabilities"]["richHistoryReplay"],
        serde_json::json!(true)
    );
    let first_install_dir = installed
        .installation
        .install_dir
        .clone()
        .expect("managed install directory");
    assert!(first_install_dir.is_dir());

    let service_v2 = AgentLifecycleCoordinator::new_with_catalog(
        pool.clone(),
        manager.clone(),
        runtime_root.clone(),
        CatalogService::from_catalog(fixture_catalog("1.1.0", &url_v2, &good_v2)),
    );
    let updated = service_v2
        .install(request_for(&service_v2, "update", "1.1.0"))
        .await
        .expect("fixture update");
    assert_eq!(updated.installation.catalog_item_version, "1.1.0");
    let second_install_dir = updated
        .installation
        .install_dir
        .clone()
        .expect("updated managed install directory");
    assert_ne!(first_install_dir, second_install_dir);
    assert!(!first_install_dir.exists());
    assert!(second_install_dir.is_dir());

    let service_v3 = AgentLifecycleCoordinator::new_with_catalog(
        pool.clone(),
        manager.clone(),
        runtime_root.clone(),
        CatalogService::from_catalog(fixture_catalog("1.2.0", &url_v3, &failed_v3)),
    );
    let failed = service_v3
        .install(request_for(&service_v3, "update", "1.2.0"))
        .await
        .expect_err("failed fixture update");
    assert_eq!(failed.code(), "acp_connection_failed");
    let current = service_v3
        .repository
        .get(AGENT_ID)
        .await
        .expect("current installation")
        .expect("previous installation remains active");
    assert_eq!(current.catalog_item_version, "1.1.0");
    assert_eq!(current.install_dir, Some(second_install_dir.clone()));
    assert!(second_install_dir.is_dir());
    assert_eq!(count_directories(&runtime_root.join("active")), 1);
    assert_eq!(count_directories(&runtime_root.join(".staging")), 0);

    let recovered_manager = Arc::new(AgentRuntimeManager::new(
        pool.clone(),
        workspace_root.clone(),
    ));
    let coordinator = AgentLifecycleCoordinator::new_with_catalog(
        pool.clone(),
        recovered_manager.clone(),
        runtime_root.clone(),
        service_v2.catalog.clone(),
    );
    let warnings = coordinator
        .recover_startup()
        .await
        .expect("restart recovery");
    assert!(
        warnings.is_empty(),
        "unexpected recovery warnings: {warnings:?}"
    );
    let recovered_definition = recovered_manager
        .registry()
        .get(&AgentId::parse(AGENT_ID).expect("agent id"))
        .expect("reloaded Agent definition");
    assert!(recovered_definition.declared_capabilities.live_events);
    assert!(
        recovered_definition
            .declared_capabilities
            .rich_history_replay
    );

    let active_program = second_install_dir.join(fixture_executable_name());
    std::fs::write(
        &active_program,
        fixture_agent_script(&fixture_path, "initialize_error"),
    )
    .expect("replace fixture with an ACP agent that fails to initialize");
    let scheduled = recovered_manager
        .prepare_startup_health_refresh()
        .await
        .expect("mark persisted ACP health pending");
    assert_eq!(scheduled, 1);
    let pending = service_v3
        .repository
        .get(AGENT_ID)
        .await
        .expect("pending installation")
        .expect("pending installation remains downloaded");
    assert_eq!(pending.protocol_status, ProtocolStatus::Unchecked);
    assert_eq!(pending.model_status.as_deref(), Some("unchecked"));
    assert!(recovered_manager
        .registry()
        .get(&AgentId::parse(AGENT_ID).expect("agent id"))
        .is_none());

    let failed_refresh = recovered_manager
        .refresh_installed_agent_health()
        .await
        .expect("startup Agent health refresh");
    assert_eq!(failed_refresh.checked, 1);
    assert_eq!(failed_refresh.available, 0);
    assert_eq!(failed_refresh.unavailable, 1);
    let unavailable = service_v3
        .repository
        .get(AGENT_ID)
        .await
        .expect("unavailable installation")
        .expect("unavailable installation remains downloaded");
    assert_eq!(unavailable.installation_status, InstallationStatus::Ready);
    assert_eq!(unavailable.runtime_status, RuntimeStatus::Ready);
    assert_eq!(unavailable.protocol_status, ProtocolStatus::Failed);
    assert_eq!(unavailable.model_status.as_deref(), Some("failed"));

    std::fs::write(
        &active_program,
        fixture_agent_script(&fixture_path, "happy"),
    )
    .expect("restore usable ACP fixture");
    let recovered_refresh = recovered_manager
        .refresh_installed_agent_health()
        .await
        .expect("repeat Agent health refresh after recovery");
    assert_eq!(recovered_refresh.available, 1);
    let blocking_refresh = recovered_manager
        .clone()
        .refresh_acp_health(AGENT_ID)
        .await
        .expect("async ACP refresh uses a process-capable runtime");
    assert!(blocking_refresh.available);
    assert!(!blocking_refresh.models.is_empty());
    let available = service_v3
        .repository
        .get(AGENT_ID)
        .await
        .expect("available installation")
        .expect("available installation remains downloaded");
    assert_eq!(available.protocol_status, ProtocolStatus::Ready);
    assert_eq!(available.model_status.as_deref(), Some("ready"));
    assert!(recovered_manager
        .registry()
        .get(&AgentId::parse(AGENT_ID).expect("agent id"))
        .is_some());

    let cancellation = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let cancelled = service_v3
        .install_with_cancellation_and_progress(
            request_for(&service_v3, "reinstall", "1.2.0"),
            Some(cancellation),
            None,
        )
        .await
        .expect_err("cancelled reinstall");
    assert_eq!(cancelled.code(), "cancelled");
    let after_cancel = service_v3
        .repository
        .get(AGENT_ID)
        .await
        .expect("installation after cancellation")
        .expect("installation preserved after cancellation");
    assert_eq!(after_cancel.install_dir, Some(second_install_dir));

    drop(pool);
    let _ = std::fs::remove_file(database_path);
    let _ = std::fs::remove_dir_all(runtime_root);
    let _ = std::fs::remove_dir_all(workspace_root);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn managed_acp_install_uses_protocol_handshake_instead_of_version_probe() {
    let database_path = std::env::temp_dir().join(format!(
        "assetiweave-agent-market-no-version-{}.db",
        uuid::Uuid::new_v4()
    ));
    let runtime_root = std::env::temp_dir().join(format!(
        "assetiweave-agent-market-no-version-runtime-{}",
        uuid::Uuid::new_v4()
    ));
    let workspace_root = std::env::temp_dir().join(format!(
        "assetiweave-agent-market-no-version-workspace-{}",
        uuid::Uuid::new_v4()
    ));
    let pool = Database::open_initialized_async(&database_path)
        .await
        .expect("database")
        .pool()
        .clone();
    let fixture_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/fake-acp-agent.mjs");
    let artifact = fixture_agent_script(&fixture_path, "reject_version");
    let url = format!(
        "https://fixture.invalid/{}/no-version",
        uuid::Uuid::new_v4()
    );
    register_test_artifact(&url, artifact.clone());

    let manager = Arc::new(AgentRuntimeManager::new(
        pool.clone(),
        workspace_root.clone(),
    ));
    let service = AgentLifecycleCoordinator::new_with_catalog(
        pool.clone(),
        manager,
        runtime_root.clone(),
        CatalogService::from_catalog(fixture_catalog("1.0.0", &url, &artifact)),
    );

    let installed = service
        .install(request_for(&service, "install", "1.0.0"))
        .await
        .expect("an ACP server need not implement a one-shot --version command");

    assert_eq!(installed.installation.protocol, AgentMarketProtocol::Acp);
    assert_eq!(
        installed.installation.protocol_status,
        ProtocolStatus::Ready
    );
    assert!(installed
        .installation
        .install_dir
        .as_ref()
        .is_some_and(|path| path.is_dir()));

    drop(pool);
    let _ = std::fs::remove_file(database_path);
    let _ = std::fs::remove_dir_all(runtime_root);
    let _ = std::fs::remove_dir_all(workspace_root);
}

fn request_for(
    service: &AgentLifecycleCoordinator,
    action: &str,
    agent_version: &str,
) -> AgentInstallStartRequest {
    let item = service
        .catalog
        .item(AGENT_ID)
        .expect("fixture catalog item");
    let distribution_id = item.distributions[0].id().to_string();
    AgentInstallStartRequest {
        agent_id: AGENT_ID.to_string(),
        action: action.to_string(),
        catalog_version: service.catalog.catalog().catalog_version.clone(),
        agent_version: agent_version.to_string(),
        distribution_id: distribution_id.clone(),
        preview_token: service
            .catalog
            .preview_token(item, &distribution_id, action),
    }
}

fn fixture_catalog(version: &str, url: &str, bytes: &[u8]) -> Catalog {
    Catalog {
        schema: "assetiweave.agent-market/v1".to_string(),
        catalog_version: format!("2099.01.01.{}", version.replace('.', "")),
        generated_at: "2026-08-20T00:00:00Z".to_string(),
        source: CatalogSource {
            kind: "test".to_string(),
            upstream: "local fixture".to_string(),
            upstream_revision: version.to_string(),
        },
        items: vec![CatalogItem {
            id: AGENT_ID.to_string(),
            display_name: "Fixture ACP Agent".to_string(),
            description: "Local ACP lifecycle fixture".to_string(),
            protocol: AgentMarketProtocol::Acp,
            version: version.to_string(),
            core_compatibility: CoreCompatibility {
                min: "0.0.0".to_string(),
                max_exclusive: "99.0.0".to_string(),
            },
            capabilities: CatalogCapabilities {
                purposes: vec!["text_prompt".to_string()],
                text_prompt: true,
                model_discovery: false,
                resume: true,
                history_replay: true,
                live_events: true,
                rich_history_replay: true,
                ..CatalogCapabilities::default()
            },
            verification: Verification {
                status: VerificationStatus::Tested,
                tested_at: "2026-08-20T00:00:00Z".to_string(),
                evidence_id: Some("fixture-evidence".to_string()),
            },
            upstream: UpstreamSource {
                registry_id: "fixture".to_string(),
                homepage: "https://fixture.invalid/agent".to_string(),
                license: "MIT".to_string(),
            },
            distributions: vec![Distribution::Binary {
                id: format!("fixture-{version}"),
                priority: 1,
                target: Target {
                    os: std::env::consts::OS.to_string(),
                    arch: std::env::consts::ARCH.to_string(),
                },
                archive: "none".to_string(),
                url: url.to_string(),
                sha256: sha256(bytes),
                size: Some(bytes.len() as u64),
                executable: fixture_executable_name().to_string(),
                launch_args: Vec::new(),
                model_discovery_args: None,
                session_cleanup_args: None,
                session_cleanup_not_found_markers: Vec::new(),
            }],
        }],
    }
}

fn fixture_executable_name() -> &'static str {
    if cfg!(windows) {
        "bin/agent.cmd"
    } else {
        "bin/agent"
    }
}

#[cfg(windows)]
fn fixture_agent_script(path: &Path, mode: &str) -> Vec<u8> {
    format!(
        "@echo off\r\nset ASSETIWEAVE_FAKE_ACP_MODE={mode}\r\nnode \"{}\" %*\r\n",
        path.display()
    )
    .into_bytes()
}

#[cfg(not(windows))]
fn fixture_agent_script(path: &Path, mode: &str) -> Vec<u8> {
    format!(
        "#!/bin/sh\nexec env ASSETIWEAVE_FAKE_ACP_MODE={mode} node '{}' \"$@\"\n",
        path.display()
    )
    .into_bytes()
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn count_directories(path: &Path) -> usize {
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_dir()))
                .count()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn startup_reconciliation_marks_mismatched_legacy_native_installation_incompatible_and_excludes_from_registry(
) {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-legacy-reconcile-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let db_path = root.join("app.db");
    let db = Database::open_initialized_async(&db_path)
        .await
        .expect("open db");
    let pool = db.pool().clone();
    let repository = AgentInstallationRepository::new(pool.clone());
    let fake_agy = root.join("bin").join("agy");
    std::fs::create_dir_all(fake_agy.parent().unwrap()).unwrap();
    std::fs::write(&fake_agy, b"#!/bin/sh\necho 1.0.0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake_agy, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let now = chrono::Utc::now().to_rfc3339();
    let legacy_installation = AgentInstallation {
        agent_id: "antigravity".to_string(),
        installation_id: uuid::Uuid::new_v4().to_string(),
        display_name: "Google Antigravity".to_string(),
        catalog_item_version: "1.0.0".to_string(),
        agent_version: "1.0.0".to_string(),
        protocol: AgentMarketProtocol::Native,
        distribution_id: "system-antigravity".to_string(),
        distribution_type: crate::backend::domain::agents::DistributionType::System,
        ownership: crate::backend::domain::agents::Ownership::System,
        install_dir: None,
        resolved_program: fake_agy.clone(),
        args: vec![],
        definition_json:
            crate::backend::infrastructure::agent_market::runtime::definition::definition_json(
                "antigravity",
                "Google Antigravity",
                &AgentMarketProtocol::Native,
                &fake_agy,
                &[],
            ),
        integrity_json: None,
        source_registry: "builtin".to_string(),
        catalog_version: "2026.03.01.1".to_string(),
        enabled: true,
        installation_status: InstallationStatus::Ready,
        runtime_status: RuntimeStatus::Ready,
        runtime_error_code: None,
        runtime_error_message: None,
        runtime_checked_at: Some(now.clone()),
        protocol_status: ProtocolStatus::Ready,
        protocol_error_code: None,
        protocol_error_message: None,
        protocol_checked_at: Some(now.clone()),
        model_status: None,
        model_error_code: None,
        model_checked_at: None,
        installed_at: now.clone(),
        updated_at: now.clone(),
    };
    repository
        .upsert_active(&legacy_installation)
        .await
        .unwrap();

    let manager = Arc::new(AgentRuntimeManager::new(
        pool.clone(),
        root.join("workspace"),
    ));
    manager.reload().await.unwrap();
    let active_def = manager
        .registry()
        .get(&AgentId::parse("antigravity").unwrap());
    assert!(active_def.is_some());

    // 构造活动 catalog fixture：protocol 为 ACP，分发仅为 Binary
    let active_catalog = CatalogService::from_catalog(Catalog {
        schema: "assetiweave.agent-market/v1".to_string(),
        catalog_version: "2026.03.07.1".to_string(),
        generated_at: now.clone(),
        source: CatalogSource {
            kind: "official".to_string(),
            upstream: "https://github.com/google/antigravity".to_string(),
            upstream_revision: "v1.1.1".to_string(),
        },
        items: vec![CatalogItem {
            id: "antigravity".to_string(),
            display_name: "Google Antigravity".to_string(),
            description: "Google Antigravity ACP Agent".to_string(),
            protocol: AgentMarketProtocol::Acp,
            version: "1.1.1".to_string(),
            core_compatibility: Default::default(),
            capabilities: CatalogCapabilities::fallback_for_protocol(&AgentMarketProtocol::Acp),
            verification: Verification {
                status: VerificationStatus::Experimental,
                tested_at: now.clone(),
                evidence_id: None,
            },
            upstream: UpstreamSource {
                registry_id: "antigravity-acp".to_string(),
                homepage: "https://github.com/google/antigravity".to_string(),
                license: "Apache-2.0".to_string(),
            },
            distributions: vec![Distribution::Binary {
                id: "binary-antigravity-darwin-arm64".to_string(),
                priority: 100,
                target: Target {
                    os: "darwin".to_string(),
                    arch: "arm64".to_string(),
                },
                archive: "antigravity-darwin-arm64.tar.gz".to_string(),
                url: "https://example.com/antigravity.tar.gz".to_string(),
                sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
                size: Some(1024),
                executable: "antigravity.par".to_string(),
                launch_args: vec![],
                model_discovery_args: None,
                session_cleanup_args: None,
                session_cleanup_not_found_markers: vec![],
            }],
        }],
    });

    let coordinator = AgentLifecycleCoordinator::new_with_catalog(
        pool.clone(),
        manager.clone(),
        root.join("runtime"),
        active_catalog,
    );
    let warnings = coordinator.recover_startup().await.unwrap();
    assert!(!warnings.is_empty());

    let updated = repository
        .get("antigravity")
        .await
        .unwrap()
        .expect("record preserved");
    assert_eq!(
        updated.installation_status,
        InstallationStatus::Incompatible
    );
    assert_eq!(
        updated.runtime_error_code.as_deref(),
        Some("catalog_distribution_incompatible")
    );
    assert!(!updated.connected());
    assert!(!updated.execution_ready());

    assert!(fake_agy.is_file());
    assert_eq!(updated.resolved_program, fake_agy);
    assert_eq!(updated.protocol, AgentMarketProtocol::Native);

    let final_snapshot = manager.registry().snapshot();
    let registered = final_snapshot.get(&AgentId::parse("antigravity").unwrap());
    assert!(registered.is_none());

    let _ = std::fs::remove_dir_all(root);
}
