use super::*;
use crate::backend::agent_market::types::{
    DistributionType, InstallationStatus, Ownership, ProtocolStatus, RuntimeStatus,
};
use crate::backend::agents::types::AgentModelOption;

#[test]
fn failed_publish_preserves_the_previous_complete_snapshot() {
    let registry = AgentRuntimeRegistry::default();
    let definition = AgentDefinition {
        id: AgentId::parse("agent").unwrap(),
        installation_id: None,
        display_name: "Agent".to_string(),
        protocol: AgentProtocol::Acp,
        command: "/tmp/agent".to_string(),
        args: Vec::new(),
        env: Vec::new(),
        declared_capabilities: DeclaredAgentCapabilities::acp_text(),
        availability_probe: None,
        model_discovery: None,
        session_cleanup: None,
        session_cleanup_not_found_markers: Vec::new(),
    };
    registry.publish(vec![definition]).unwrap();
    let generation = registry.generation();
    let invalid = AgentDefinition {
        id: AgentId::parse("bad").unwrap(),
        installation_id: None,
        display_name: String::new(),
        protocol: AgentProtocol::Acp,
        command: "/tmp/bad".to_string(),
        args: Vec::new(),
        env: Vec::new(),
        declared_capabilities: DeclaredAgentCapabilities::acp_text(),
        availability_probe: None,
        model_discovery: None,
        session_cleanup: None,
        session_cleanup_not_found_markers: Vec::new(),
    };
    assert!(registry.publish(vec![invalid]).is_err());
    assert_eq!(registry.generation(), generation);
    assert!(registry
        .snapshot()
        .get(&AgentId::parse("agent").unwrap())
        .is_some());
}

#[test]
fn installation_definition_is_local_and_does_not_contain_package_manager_invocation() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-agent-runtime-{}",
        uuid::Uuid::new_v4()
    ));
    let program_path = root.join("bin").join("agent");
    std::fs::create_dir_all(program_path.parent().unwrap()).unwrap();
    std::fs::write(&program_path, b"#!/bin/sh\nexit 0\n").unwrap();
    let mut resolved_definition = definition_json(
        "agent",
        "Agent",
        &AgentMarketProtocol::Acp,
        &program_path,
        &["acp".to_string()],
    );
    resolved_definition["capabilities"] = serde_json::json!({
        "textPrompt": true,
        "resume": true,
        "historyReplay": true,
        "liveEvents": true,
        "richHistoryReplay": true,
        "teamTools": true,
    });
    resolved_definition["sessionCleanupArgs"] =
        serde_json::json!(["session", "delete", "{session_id}"]);
    resolved_definition["sessionCleanupNotFoundMarkers"] =
        serde_json::json!(["Session not found:"]);
    let installation = AgentInstallation {
        agent_id: "agent".to_string(),
        installation_id: "id".to_string(),
        display_name: "Agent".to_string(),
        catalog_item_version: "1.0.0".to_string(),
        agent_version: "1.0.0".to_string(),
        protocol: AgentMarketProtocol::Acp,
        distribution_id: "npx".to_string(),
        distribution_type: DistributionType::Npx,
        ownership: Ownership::Managed,
        install_dir: Some(root.clone()),
        resolved_program: program_path.clone(),
        args: vec!["acp".to_string()],
        definition_json: resolved_definition,
        integrity_json: None,
        source_registry: "agent".to_string(),
        catalog_version: "catalog".to_string(),
        enabled: true,
        installation_status: InstallationStatus::Ready,
        runtime_status: RuntimeStatus::Ready,
        runtime_error_code: None,
        runtime_error_message: None,
        runtime_checked_at: None,
        protocol_status: ProtocolStatus::Ready,
        protocol_error_code: None,
        protocol_error_message: None,
        protocol_checked_at: None,
        model_status: None,
        model_error_code: None,
        model_checked_at: None,
        installed_at: "now".to_string(),
        updated_at: "now".to_string(),
    };
    let definition = definition_from_installation(&installation).unwrap();
    assert_eq!(definition.command, program_path.to_string_lossy());
    assert!(definition.declared_capabilities.resume);
    assert!(definition.declared_capabilities.history_replay);
    assert!(definition.declared_capabilities.live_events);
    assert!(definition.declared_capabilities.rich_history_replay);
    assert_eq!(
        definition
            .session_cleanup
            .as_ref()
            .map(|cleanup| &cleanup.args),
        Some(&vec![
            "session".to_string(),
            "delete".to_string(),
            "{session_id}".to_string()
        ])
    );
    assert_eq!(
        definition.session_cleanup_not_found_markers,
        ["Session not found:"]
    );
    assert!(!definition
        .args
        .iter()
        .any(|arg| arg == "-y" || arg == "npx" || arg == "uvx"));
    let package_system = AgentPackageSystem::from_installation(&installation).unwrap();
    let inspected = package_system.inspect(&root).unwrap();
    assert_eq!(inspected.identity, installation.package_identity().unwrap());
    assert_eq!(inspected.invocation.entry, program_path.to_string_lossy());
    assert_eq!(
        inspected.availability_probe.args,
        vec!["--version".to_string()]
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn startup_cleanup_preserves_active_installations_owned_by_another_database() {
    let runtime_root = std::env::temp_dir().join(format!(
        "assetiweave-agent-runtime-shared-{}",
        uuid::Uuid::new_v4()
    ));
    let installation_id = uuid::Uuid::new_v4().to_string();
    let active_install = runtime_root.join("active").join(&installation_id);
    std::fs::create_dir_all(&active_install).expect("create shared active installation");
    std::fs::write(active_install.join("agent"), b"persisted")
        .expect("write shared active installation");

    let warnings = cleanup_runtime_directories(&runtime_root);

    assert!(warnings.is_empty());
    assert!(active_install.join("agent").is_file());
    let _ = std::fs::remove_dir_all(runtime_root);
}

async fn acp_test_fixture_with_id(
    agent_id: &str,
    mode: &str,
) -> (
    AgentRuntimeManager,
    AgentInstallationRepository,
    std::path::PathBuf,
) {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-acp-{}-{}",
        agent_id,
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let db_path = root.join("app.db");
    let db = crate::backend::store::Database::open_initialized_async(&db_path)
        .await
        .expect("open db");
    let pool = db.pool().clone();
    let repository = AgentInstallationRepository::new(pool.clone());
    let program =
        crate::backend::host_process::resolve_host_executable("node").expect("node executable");
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/fake-acp-agent.mjs");
    let record = root.join("record.log");
    let args = vec![
        fixture.to_string_lossy().to_string(),
        format!("--mode={mode}"),
        format!("--record={}", record.to_string_lossy()),
    ];
    let now = chrono::Utc::now().to_rfc3339();
    let installation = AgentInstallation {
        agent_id: agent_id.to_string(),
        installation_id: uuid::Uuid::new_v4().to_string(),
        display_name: format!("{agent_id} Display"),
        catalog_item_version: "1.0.0".to_string(),
        agent_version: "1.0.0".to_string(),
        protocol: AgentMarketProtocol::Acp,
        distribution_id: "test-distribution".to_string(),
        distribution_type: DistributionType::System,
        ownership: Ownership::System,
        install_dir: None,
        resolved_program: program.clone(),
        args: args.clone(),
        definition_json: serde_json::json!({
            "id": agent_id,
            "display_name": format!("{agent_id} Display"),
            "protocol": "acp",
            "program": program.to_string_lossy(),
            "args": args,
            "env": [
                { "name": "ASSETIWEAVE_FAKE_ACP_MODE", "value": mode },
                { "name": "ASSETIWEAVE_FAKE_ACP_RECORD_PATH", "value": record.to_string_lossy() }
            ],
        }),
        integrity_json: None,
        source_registry: "test".to_string(),
        catalog_version: "1.0".to_string(),
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
        updated_at: now,
    };
    repository.upsert_active(&installation).await.unwrap();
    let manager = AgentRuntimeManager::new(pool, root.join("workspaces"));
    (manager, repository, root)
}

async fn acp_test_fixture(
    mode: &str,
) -> (
    AgentRuntimeManager,
    AgentInstallationRepository,
    std::path::PathBuf,
) {
    acp_test_fixture_with_id("test-agent", mode).await
}

fn test_request(
    agent_id: &str,
    model: Option<&str>,
) -> crate::backend::ai_execution::AiExecutionRequest {
    use crate::backend::ai_execution::*;
    AiExecutionRequest {
        execution_id: format!("exec-{}", uuid::Uuid::new_v4()),
        agent_id: AgentId::parse(agent_id).expect("valid agent id"),
        purpose: AiExecutionPurpose::Translation,
        session_mode: AgentSessionMode::OneShot,
        prompt: "Please translate: Hello world".to_string(),
        model: model.map(|m| m.to_string()),
        limits: AiExecutionLimits {
            total_timeout: Duration::from_secs(10),
            spawn_timeout: Duration::from_secs(10),
            initialize_timeout: Duration::from_secs(5),
            config_rpc_timeout: Duration::from_secs(5),
            cancel_grace: Duration::from_secs(2),
            close_timeout: Duration::from_secs(2),
            cleanup_timeout: Duration::from_secs(10),
            text_bytes: 1024 * 1024,
            stderr_bytes: 64 * 1024,
        },
        cancellation: AiExecutionCancellation::default(),
        progress: None,
        tenant_id: None,
        execution_context_key: None,
        binding: None,
        replay: false,
        restore_only: false,
        team_tools: None,
        recall_tools: None,
        memory_generation_tools: None,
    }
}

fn read_record_events(record_file: &Path) -> Vec<serde_json::Value> {
    if !record_file.is_file() {
        return Vec::new();
    }
    let content = std::fs::read_to_string(record_file).unwrap_or_default();
    content
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect()
}

fn assert_clean_workspaces(workspaces_dir: &Path) {
    if workspaces_dir.exists() {
        let entries = std::fs::read_dir(workspaces_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect::<Vec<_>>();
        assert!(
            entries.is_empty(),
            "expected clean workspace directory, found: {:?}",
            entries.iter().map(|e| e.path()).collect::<Vec<_>>()
        );
    }
}

#[tokio::test]
async fn acp_model_requests_share_one_probe_and_force_refresh_bypasses_cache() {
    let (manager, repository, root) = acp_test_fixture("happy").await;

    let (first, second) = tokio::join!(
        manager.get_or_refresh_acp_models("test-agent"),
        manager.get_or_refresh_acp_models("test-agent"),
    );
    assert!(first.unwrap().available);
    assert!(second.unwrap().available);

    let events = read_record_events(&root.join("record.log"));
    assert_eq!(
        events
            .iter()
            .filter(|event| event["event"] == "initialize")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event["event"] == "new")
            .count(),
        1
    );

    let refreshed = manager
        .probe_acp_health("test-agent")
        .await
        .expect("forced ACP health refresh");
    assert!(refreshed.available);
    let events = read_record_events(&root.join("record.log"));
    assert_eq!(
        events
            .iter()
            .filter(|event| event["event"] == "initialize")
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event["event"] == "new")
            .count(),
        2
    );

    repository
        .update_enabled("test-agent", false, &chrono::Utc::now().to_rfc3339())
        .await
        .expect("disable fixture Agent");
    let disabled = manager
        .get_or_refresh_acp_models("test-agent")
        .await
        .expect("disabled Agent model result");
    assert!(!disabled.available);
    assert_eq!(disabled.error_code.as_deref(), Some("agent_disabled"));

    repository
        .delete("test-agent")
        .await
        .expect("uninstall fixture Agent");
    let error = manager
        .get_or_refresh_acp_models("test-agent")
        .await
        .expect_err("uninstalled Agent must not use the model cache");
    assert_eq!(error.code(), "agent_not_installed");

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn cancelling_acp_probe_reaps_the_process_without_marking_health_failed() {
    let (manager, repository, root) = acp_test_fixture("initialize_timeout").await;
    let manager = Arc::new(manager);
    let task_manager = Arc::clone(&manager);
    let task =
        tokio::spawn(async move { task_manager.get_or_refresh_acp_models("test-agent").await });

    for _ in 0..50 {
        if !manager.probe_flights.lock().await.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(manager.release_acp_probe_caller("test-agent").await);
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("cancelled probe should finish")
        .expect("probe task should not panic");
    let error = result.expect_err("cancelled probe should return an error");
    assert_eq!(error.code(), "cancelled");

    let installation = repository
        .get("test-agent")
        .await
        .expect("load installation")
        .expect("installation exists");
    assert_eq!(installation.protocol_status, ProtocolStatus::Ready);
    assert_clean_workspaces(&root.join("workspaces"));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn acp_health_probe_succeeds_when_models_empty_and_leaves_protocol_ready() {
    let (manager, repository, root) = acp_test_fixture("no_models").await;

    let models = manager
        .probe_acp_health("test-agent")
        .await
        .expect("probe ACP health");
    assert!(models.available);
    assert!(models.models.is_empty());
    assert_eq!(models.error_code.as_deref(), Some("model_list_empty"));

    let installation = repository
        .get("test-agent")
        .await
        .unwrap()
        .expect("installation exists");
    assert_eq!(installation.protocol_status, ProtocolStatus::Ready);
    assert_eq!(installation.model_status.as_deref(), Some("unsupported"));
    assert!(installation.connected());
    assert!(installation.execution_ready());

    let candidates = repository.list_registry_candidates().await.unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].agent_id, "test-agent");

    let reloaded = manager.reload().await.expect("reload candidates");
    assert_eq!(reloaded, 1);

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn acp_health_probe_marks_auth_required_when_auth_error() {
    let (manager, repository, root) = acp_test_fixture("auth_error").await;

    let models = manager
        .probe_acp_health("test-agent")
        .await
        .expect("probe ACP health");
    assert!(!models.available);
    assert_eq!(models.error_code.as_deref(), Some("auth_required"));

    let installation = repository
        .get("test-agent")
        .await
        .unwrap()
        .expect("installation exists");
    assert_eq!(installation.installation_status, InstallationStatus::Ready);
    assert_eq!(installation.protocol_status, ProtocolStatus::AuthRequired);
    assert_eq!(
        installation.protocol_error_code.as_deref(),
        Some("auth_required")
    );
    assert!(!installation.connected());
    assert!(!installation.execution_ready());

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn acp_health_probe_marks_failed_when_connection_fails() {
    let (manager, repository, root) = acp_test_fixture("new_error").await;

    let models = manager
        .probe_acp_health("test-agent")
        .await
        .expect("probe ACP health");
    assert!(!models.available);
    assert_eq!(models.error_code.as_deref(), Some("connection_failed"));

    let installation = repository
        .get("test-agent")
        .await
        .unwrap()
        .expect("installation exists");
    assert_eq!(installation.protocol_status, ProtocolStatus::Failed);
    assert_eq!(
        installation.protocol_error_code.as_deref(),
        Some("connection_failed")
    );
    assert_eq!(installation.model_status.as_deref(), Some("failed"));
    assert!(!installation.connected());
    assert!(!installation.execution_ready());

    let _ = std::fs::remove_dir_all(root);
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
    let db = crate::backend::store::Database::open_initialized_async(&db_path)
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
        distribution_type: DistributionType::System,
        ownership: Ownership::System,
        install_dir: None,
        resolved_program: fake_agy.clone(),
        args: vec![],
        definition_json: definition_json(
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

    let manager = AgentRuntimeManager::new(pool.clone(), root.join("workspaces"));

    // 证明旧代码在未按 catalog 协调时仍会把 Native definition 发布到 Registry
    let candidates = repository.list_registry_candidates().await.unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].agent_id, "antigravity");
    assert_eq!(candidates[0].protocol, AgentMarketProtocol::Native);

    let reloaded = manager.reload().await.unwrap();
    assert_eq!(reloaded, 1);
    let snapshot = manager.registry().snapshot();
    let active_def = snapshot.get(&AgentId::parse("antigravity").unwrap());
    assert!(active_def.is_some());
    assert_eq!(active_def.unwrap().protocol, AgentProtocol::Native);

    // 构造活动 catalog fixture：protocol 为 ACP，分发仅为 Binary
    let active_catalog = crate::backend::agent_market::catalog::CatalogService::from_catalog(
        crate::backend::agent_market::types::Catalog {
            schema: "assetiweave.agent-market/v1".to_string(),
            catalog_version: "2026.03.07.1".to_string(),
            generated_at: now.clone(),
            source: crate::backend::agent_market::types::CatalogSource {
                kind: "official".to_string(),
                upstream: "https://github.com/google/antigravity".to_string(),
                upstream_revision: "v1.1.1".to_string(),
            },
            items: vec![crate::backend::agent_market::types::CatalogItem {
                id: "antigravity".to_string(),
                display_name: "Google Antigravity".to_string(),
                description: "Google Antigravity ACP Agent".to_string(),
                protocol: AgentMarketProtocol::Acp,
                version: "1.1.1".to_string(),
                core_compatibility: Default::default(),
                capabilities:
                    crate::backend::agent_market::types::CatalogCapabilities::fallback_for_protocol(
                        &AgentMarketProtocol::Acp,
                    ),
                verification: crate::backend::agent_market::types::Verification {
                    status: crate::backend::agent_market::types::VerificationStatus::Experimental,
                    tested_at: now.clone(),
                    evidence_id: None,
                },
                upstream: crate::backend::agent_market::types::UpstreamSource {
                    registry_id: "antigravity-acp".to_string(),
                    homepage: "https://github.com/google/antigravity".to_string(),
                    license: "Apache-2.0".to_string(),
                },
                distributions: vec![crate::backend::agent_market::types::Distribution::Binary {
                    id: "binary-antigravity-darwin-arm64".to_string(),
                    priority: 100,
                    target: crate::backend::agent_market::types::Target {
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
        },
    );

    // 执行启动协调（传入 active catalog）
    let warnings = manager
        .recover_startup_with_catalog(&root.join("runtime"), Some(&active_catalog))
        .await
        .unwrap();
    assert!(!warnings.is_empty());

    // 验证 SQLite 记录状态
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

    // 外部 agy 未被删除，原记录的 program / protocol 未被静默篡改
    assert!(fake_agy.is_file());
    assert_eq!(updated.resolved_program, fake_agy);
    assert_eq!(updated.protocol, AgentMarketProtocol::Native);

    // Registry 中无该 definition
    let final_snapshot = manager.registry().snapshot();
    let registered = final_snapshot.get(&AgentId::parse("antigravity").unwrap());
    assert!(registered.is_none());

    let candidates_after = repository.list_registry_candidates().await.unwrap();
    assert!(candidates_after.is_empty());

    let refresh = manager
        .refresh_installed_agent_health()
        .await
        .expect("refresh should skip incompatible installation");
    assert_eq!(refresh.checked, 0);
    let after_refresh = repository
        .get("antigravity")
        .await
        .unwrap()
        .expect("incompatible record preserved after refresh");
    assert_eq!(
        after_refresh.installation_status,
        InstallationStatus::Incompatible
    );

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn agacp_05_scene_1_normal_models_and_prompt() {
    let (manager, repository, root) = acp_test_fixture_with_id("test-agent-s1", "happy").await;

    // 1. Health Probe
    let models = manager
        .probe_acp_health("test-agent-s1")
        .await
        .expect("probe ACP health");
    assert!(models.available);
    assert_eq!(models.models.len(), 2);
    assert_eq!(
        models.current_model_id.as_deref(),
        Some("fixture/model-fast")
    );

    // 2. Candidate & Reload to Registry
    let candidates = repository.list_registry_candidates().await.unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].agent_id, "test-agent-s1");

    let reloaded = manager.reload().await.expect("reload candidates");
    assert_eq!(reloaded, 1);
    let snapshot = manager.registry().snapshot();
    let def = snapshot
        .get(&AgentId::parse("test-agent-s1").unwrap())
        .expect("registered");
    assert_eq!(def.protocol, AgentProtocol::Acp);

    // 3. SQLite State Verification
    let installation = repository
        .get("test-agent-s1")
        .await
        .unwrap()
        .expect("installation exists");
    assert_eq!(installation.protocol_status, ProtocolStatus::Ready);
    assert_eq!(installation.model_status.as_deref(), Some("ready"));
    assert!(installation.connected());
    assert!(installation.execution_ready());

    // 4. Execution with specific model
    let req = test_request("test-agent-s1", Some("fixture/model-accurate"));
    let result = manager
        .runtime()
        .execute(req)
        .await
        .expect("execution succeeds");
    assert_eq!(result.text, "translated");
    assert_eq!(result.protocol, AgentProtocol::Acp);
    assert_eq!(
        result.requested_model.as_deref(),
        Some("fixture/model-accurate")
    );

    // 5. Check Record Events: initialize, new, model(accurate), prompt, close, delete
    let events = read_record_events(&root.join("record.log"));
    assert!(events.iter().any(|e| e["event"] == "initialize"));
    assert!(events.iter().any(|e| e["event"] == "new"));
    let model_event = events.iter().find(|e| e["event"] == "model");
    assert!(model_event.is_some());
    assert_eq!(model_event.unwrap()["value"], "fixture/model-accurate");
    assert!(events.iter().any(|e| e["event"] == "prompt"));
    assert!(events.iter().any(|e| e["event"] == "close"));
    assert!(events.iter().any(|e| e["event"] == "delete"));

    // 6. Clean workspace assertion
    assert_clean_workspaces(&root.join("workspaces"));

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn agacp_05_scene_2_empty_models_and_default_model_prompt() {
    let (manager, repository, root) = acp_test_fixture_with_id("test-agent-s2", "no_models").await;

    // 1. Health Probe with empty models
    let models = manager
        .probe_acp_health("test-agent-s2")
        .await
        .expect("probe ACP health");
    assert!(models.available);
    assert!(models.models.is_empty());
    assert_eq!(models.error_code.as_deref(), Some("model_list_empty"));

    // 2. SQLite State: Protocol is Ready, Model is unsupported, but execution_ready is true!
    let installation = repository
        .get("test-agent-s2")
        .await
        .unwrap()
        .expect("installation exists");
    assert_eq!(installation.protocol_status, ProtocolStatus::Ready);
    assert_eq!(installation.model_status.as_deref(), Some("unsupported"));
    assert!(installation.connected());
    assert!(installation.execution_ready());

    // 3. Reload
    let reloaded = manager.reload().await.expect("reload candidates");
    assert_eq!(reloaded, 1);

    // 4. Execute with default model (model: None)
    let req = test_request("test-agent-s2", None);
    let result = manager
        .runtime()
        .execute(req)
        .await
        .expect("execution succeeds with default model");
    assert_eq!(result.text, "translated");
    assert_eq!(result.requested_model, None);

    // 5. Verify record: no "model" set_config_option event was called!
    let events = read_record_events(&root.join("record.log"));
    assert!(!events.iter().any(|e| e["event"] == "model"));
    assert!(events.iter().any(|e| e["event"] == "prompt"));
    assert!(events.iter().any(|e| e["event"] == "close"));
    assert!(events.iter().any(|e| e["event"] == "delete"));

    assert_clean_workspaces(&root.join("workspaces"));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn agacp_05_scene_3_model_discovery_error_leaves_protocol_ready() {
    let (manager, repository, root) =
        acp_test_fixture_with_id("test-agent-s3", "model_discovery_error").await;

    // 1. Probe ACP health: Stage 1 (check_connection) passes, Stage 2 (discover_models) fails
    let models = manager
        .probe_acp_health("test-agent-s3")
        .await
        .expect("probe ACP health succeeds at manager level");
    assert!(models.available);
    assert!(models.models.is_empty());
    assert_eq!(models.error_code.as_deref(), Some("model_catalog_invalid"));

    // 2. SQLite State: Protocol is Ready, Model is failed, connected/execution_ready are true!
    let installation = repository
        .get("test-agent-s3")
        .await
        .unwrap()
        .expect("installation exists");
    assert_eq!(installation.protocol_status, ProtocolStatus::Ready);
    assert_eq!(installation.model_status.as_deref(), Some("failed"));
    assert_eq!(
        installation.model_error_code.as_deref(),
        Some("model_catalog_invalid")
    );
    assert!(installation.connected());
    assert!(installation.execution_ready());

    // 3. Reloads into Registry
    let reloaded = manager.reload().await.expect("reload candidates");
    assert_eq!(reloaded, 1);

    // 4. Default model execution succeeds
    let req_default = test_request("test-agent-s3", None);
    let result = manager
        .runtime()
        .execute(req_default)
        .await
        .expect("default model execution succeeds");
    assert_eq!(result.text, "translated");

    // 5. Explicit model selection fails (rejected by agent)
    let req_model = test_request("test-agent-s3", Some("fixture/model-fast"));
    let err = manager
        .runtime()
        .execute(req_model)
        .await
        .expect_err("explicit model selection should fail");
    assert!(matches!(err, AiExecutionError::ModelSelectionFailed { .. }));

    assert_clean_workspaces(&root.join("workspaces"));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn agacp_05_scene_4_auth_error_cleans_workspace_and_no_fallback() {
    let (manager, repository, root) = acp_test_fixture_with_id("test-agent-s4", "auth_error").await;

    // 1. Probe ACP health fails with auth_required
    let models = manager
        .probe_acp_health("test-agent-s4")
        .await
        .expect("probe completes");
    assert!(!models.available);
    assert_eq!(models.error_code.as_deref(), Some("auth_required"));

    // 2. SQLite State: Protocol is AuthRequired, disconnected
    let installation = repository
        .get("test-agent-s4")
        .await
        .unwrap()
        .expect("installation exists");
    assert_eq!(installation.protocol_status, ProtocolStatus::AuthRequired);
    assert_eq!(
        installation.protocol_error_code.as_deref(),
        Some("auth_required")
    );
    assert!(!installation.connected());
    assert!(!installation.execution_ready());

    // 3. Excluded from Registry candidates
    let candidates = repository.list_registry_candidates().await.unwrap();
    assert!(candidates.is_empty());

    // 4. Directly load definition to verify execution phase error, clean workspace, and no fallback
    let definition = definition_from_installation(&installation).unwrap();
    manager
        .registry_snapshot
        .replace(AgentRegistry::from_definitions(vec![definition]).unwrap());

    let err = manager
        .runtime()
        .execute(test_request("test-agent-s4", None))
        .await
        .expect_err("execution fails on auth error");
    match err {
        AiExecutionError::ProtocolDetail { operation, detail } => {
            assert_eq!(operation, "session_new");
            assert!(detail
                .to_ascii_lowercase()
                .contains("authentication required"));
        }
        AiExecutionError::Protocol { operation } => {
            assert_eq!(operation, "session_new");
        }
        other => panic!("unexpected error on auth_error: {:?}", other),
    }

    // 5. Zero prompt events in fake ACP
    let events = read_record_events(&root.join("record.log"));
    assert!(!events.iter().any(|e| e["event"] == "prompt"));

    // 6. Workspace completely cleaned up
    assert_clean_workspaces(&root.join("workspaces"));

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn agacp_05_scene_5_prompt_error_timeout_cancel_disconnect() {
    // 5a. Prompt Error
    {
        let (manager, _, root) = acp_test_fixture_with_id("test-agent-5a", "prompt_error").await;
        let reloaded = manager.reload().await.unwrap();
        assert_eq!(reloaded, 1);
        let err = manager
            .runtime()
            .execute(test_request("test-agent-5a", None))
            .await
            .expect_err("prompt error expected");
        match err {
            AiExecutionError::ProtocolDetail { operation, detail } => {
                assert_eq!(operation, "prompt");
                assert!(detail.contains("Internal prompt execution error"));
            }
            AiExecutionError::Protocol { operation } => {
                assert_eq!(operation, "prompt");
            }
            other => panic!("unexpected error on prompt_error: {:?}", other),
        }
        assert_clean_workspaces(&root.join("workspaces"));
        let _ = std::fs::remove_dir_all(root);
    }

    // 5b. Timeout
    {
        let (manager, _, root) = acp_test_fixture_with_id("test-agent-5b", "cancel_wait").await;
        let reloaded = manager.reload().await.unwrap();
        assert_eq!(reloaded, 1);
        let mut req = test_request("test-agent-5b", None);
        req.limits.total_timeout = Duration::from_millis(300);
        let err = manager
            .runtime()
            .execute(req)
            .await
            .expect_err("timeout expected");
        assert!(matches!(err, AiExecutionError::Timeout { .. }));
        assert_clean_workspaces(&root.join("workspaces"));
        let _ = std::fs::remove_dir_all(root);
    }

    // 5c. Cancel
    {
        let (manager, _, root) = acp_test_fixture_with_id("test-agent-5c", "cancel_wait").await;
        let reloaded = manager.reload().await.unwrap();
        assert_eq!(reloaded, 1);
        let req = test_request("test-agent-5c", None);
        let cancellation = req.cancellation.clone();
        let runtime = manager.runtime();
        let handle = tokio::spawn(async move { runtime.execute(req).await });

        // Wait until the agent has actually received and started the prompt
        let record_file = root.join("record.log");
        let wait_start = std::time::Instant::now();
        while wait_start.elapsed() < Duration::from_secs(3) {
            let events = read_record_events(&record_file);
            if events.iter().any(|e| e["event"] == "prompt") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        cancellation.cancel();
        let err = handle.await.unwrap().expect_err("cancellation expected");
        assert!(matches!(err, AiExecutionError::Cancelled { .. }));
        let events = read_record_events(&root.join("record.log"));
        assert!(events.iter().any(|e| e["event"] == "cancel"));
        assert_clean_workspaces(&root.join("workspaces"));
        let _ = std::fs::remove_dir_all(root);
    }

    // 5d. Disconnect
    {
        let (manager, _, root) = acp_test_fixture_with_id("test-agent-5d", "disconnect").await;
        let reloaded = manager.reload().await.unwrap();
        assert_eq!(reloaded, 1);
        let err = manager
            .runtime()
            .execute(test_request("test-agent-5d", None))
            .await
            .expect_err("disconnect expected");
        assert!(matches!(
            err,
            AiExecutionError::Protocol { .. }
                | AiExecutionError::ProtocolDetail { .. }
                | AiExecutionError::AgentExited { .. }
        ));
        assert_clean_workspaces(&root.join("workspaces"));
        let _ = std::fs::remove_dir_all(root);
    }
}

#[tokio::test]
async fn agacp_05_scene_6_cleanup_close_and_delete_supported_and_unsupported() {
    // 6a. Supported close and delete
    {
        let (manager, _, root) = acp_test_fixture_with_id("test-agent-6a", "happy").await;
        let reloaded = manager.reload().await.unwrap();
        assert_eq!(reloaded, 1);
        let res = manager
            .runtime()
            .execute(test_request("test-agent-6a", None))
            .await
            .expect("execution succeeds");
        assert_eq!(res.text, "translated");
        let events = read_record_events(&root.join("record.log"));
        assert!(events.iter().any(|e| e["event"] == "close"));
        assert!(events.iter().any(|e| e["event"] == "delete"));
        assert_clean_workspaces(&root.join("workspaces"));
        let _ = std::fs::remove_dir_all(root);
    }

    // 6b. Unsupported close and delete without fallback -> CleanupFailed with delete_unsupported, but process and workspace cleaned cleanly
    {
        let (manager, _, root) =
            acp_test_fixture_with_id("test-agent-6b", "no_close_no_delete").await;
        let reloaded = manager.reload().await.unwrap();
        assert_eq!(reloaded, 1);
        let err = manager
            .runtime()
            .execute(test_request("test-agent-6b", None))
            .await
            .expect_err("without fallback, unsupported delete reports cleanup failure");
        match err {
            AiExecutionError::CleanupFailed { failures } => {
                assert!(
                    failures.iter().any(|f| f == "delete_unsupported"),
                    "failures must include delete_unsupported: {failures:?}"
                );
            }
            other => panic!("expected CleanupFailed error, got: {other:?}"),
        }
        let events = read_record_events(&root.join("record.log"));
        assert!(!events.iter().any(|e| e["event"] == "close"));
        assert!(!events.iter().any(|e| e["event"] == "delete"));
        assert!(events
            .iter()
            .any(|e| e["event"] == "stdin_closed" || e["event"] == "sigterm"));
        assert_clean_workspaces(&root.join("workspaces"));
        let _ = std::fs::remove_dir_all(root);
    }
}

async fn acp_test_fixture_with_extra_args(
    agent_id: &str,
    mode: &str,
    extra_args: &[String],
) -> (
    AgentRuntimeManager,
    AgentInstallationRepository,
    std::path::PathBuf,
) {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-acp-{}-{}",
        agent_id,
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let db_path = root.join("app.db");
    let db = crate::backend::store::Database::open_initialized_async(&db_path)
        .await
        .expect("open db");
    let pool = db.pool().clone();
    let repository = AgentInstallationRepository::new(pool.clone());
    let program =
        crate::backend::host_process::resolve_host_executable("node").expect("node executable");
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/fake-acp-agent.mjs");
    let record = root.join("record.log");
    let mut args = vec![
        fixture.to_string_lossy().to_string(),
        format!("--mode={mode}"),
        format!("--record={}", record.to_string_lossy()),
    ];
    args.extend(extra_args.iter().cloned());
    let now = chrono::Utc::now().to_rfc3339();
    let installation = AgentInstallation {
        agent_id: agent_id.to_string(),
        installation_id: uuid::Uuid::new_v4().to_string(),
        display_name: format!("{agent_id} Display"),
        catalog_item_version: "1.0.0".to_string(),
        agent_version: "1.0.0".to_string(),
        protocol: AgentMarketProtocol::Acp,
        distribution_id: "test-distribution".to_string(),
        distribution_type: DistributionType::System,
        ownership: Ownership::System,
        install_dir: None,
        resolved_program: program.clone(),
        args: args.clone(),
        definition_json: serde_json::json!({
            "id": agent_id,
            "display_name": format!("{agent_id} Display"),
            "protocol": "acp",
            "program": program.to_string_lossy(),
            "args": args,
            "env": [
                { "name": "ASSETIWEAVE_FAKE_ACP_MODE", "value": mode },
                { "name": "ASSETIWEAVE_FAKE_ACP_RECORD_PATH", "value": record.to_string_lossy() }
            ],
        }),
        integrity_json: None,
        source_registry: "test".to_string(),
        catalog_version: "1.0".to_string(),
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
        updated_at: now,
    };
    repository.upsert_active(&installation).await.unwrap();
    let manager = AgentRuntimeManager::new(pool, root.join("workspaces"));
    (manager, repository, root)
}

fn read_spawn_count(path: &std::path::Path) -> usize {
    if !path.exists() {
        return 0;
    }
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count()
}

#[test]
fn test_bounded_model_cache_unit() {
    let mut cache = BoundedModelCache::new(3);
    let make_id = |id: &str| AgentProbeIdentity {
        agent_id: id.to_string(),
        installation_id: format!("inst-{id}"),
        definition_digest: "digest".to_string(),
        enabled: true,
        executable_present: true,
    };
    let make_res = |id: &str| AgentModelsResult {
        agent_id: id.to_string(),
        available: true,
        current_model_id: None,
        models: vec![AgentModelOption {
            id: format!("m-{id}"),
            label: id.to_string(),
            description: None,
        }],
        error_code: None,
        error: None,
    };

    cache.insert("a".to_string(), make_id("a"), make_res("a"));
    cache.insert("b".to_string(), make_id("b"), make_res("b"));
    cache.insert("c".to_string(), make_id("c"), make_res("c"));
    assert_eq!(cache.len(), 3);

    // Access "a" so it becomes most recently used
    assert!(cache.get("a", &make_id("a")).is_some());

    // Insert "d", capacity is 3 -> "b" should be evicted (least recently used)
    cache.insert("d".to_string(), make_id("d"), make_res("d"));
    assert_eq!(cache.len(), 3);
    assert!(cache.get("a", &make_id("a")).is_some());
    assert!(cache.get("b", &make_id("b")).is_none());
    assert!(cache.get("c", &make_id("c")).is_some());
    assert!(cache.get("d", &make_id("d")).is_some());

    // Identity mismatch evicts entry
    let wrong_id = AgentProbeIdentity {
        agent_id: "a".to_string(),
        installation_id: "wrong".to_string(),
        definition_digest: "digest".to_string(),
        enabled: true,
        executable_present: true,
    };
    assert!(cache.get("a", &wrong_id).is_none());
    assert_eq!(cache.len(), 2);

    // Remove and clear
    cache.remove("c");
    assert_eq!(cache.len(), 1);
    cache.clear();
    assert_eq!(cache.len(), 0);
}

#[tokio::test]
async fn test_issue_29_decision_1_and_14_concurrent_success_single_cold_start() {
    let agent_id = "test-agent-c1";
    let root_dir = std::env::temp_dir().join(format!("counter-c1-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root_dir).unwrap();
    let counter_path = root_dir.join("spawn.count");
    let extra_args = vec![
        format!("--counter-path={}", counter_path.display()),
        "--new-delay-ms=50".to_string(),
    ];

    let (manager, _, root) = acp_test_fixture_with_extra_args(agent_id, "happy", &extra_args).await;
    let manager = Arc::new(manager);

    let mut handles = Vec::new();
    for _ in 0..5 {
        let m = manager.clone();
        let aid = agent_id.to_string();
        handles.push(tokio::spawn(async move {
            m.get_or_refresh_acp_models(&aid).await
        }));
    }

    for handle in handles {
        let result = handle.await.unwrap().expect("probe succeeded");
        assert!(result.available);
        assert_eq!(result.models.len(), 2);
    }

    let cold_starts = read_spawn_count(&counter_path);
    assert_eq!(
        cold_starts, 1,
        "concurrent success requests must share exactly 1 cold start"
    );

    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(root_dir);
}

#[tokio::test]
async fn test_issue_29_decision_2_concurrent_failure_coalescing() {
    let agent_id = "test-agent-c2";
    let root_dir = std::env::temp_dir().join(format!("counter-c2-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root_dir).unwrap();
    let counter_path = root_dir.join("spawn.count");
    let extra_args = vec![
        format!("--counter-path={}", counter_path.display()),
        "--start-delay-ms=50".to_string(),
    ];

    let (manager, _, root) =
        acp_test_fixture_with_extra_args(agent_id, "exit_on_init", &extra_args).await;
    let manager = Arc::new(manager);

    let mut handles = Vec::new();
    for _ in 0..5 {
        let m = manager.clone();
        let aid = agent_id.to_string();
        handles.push(tokio::spawn(async move {
            m.get_or_refresh_acp_models(&aid).await
        }));
    }

    for handle in handles {
        let result = handle.await.unwrap().expect("probe returns models result");
        assert!(!result.available);
        assert!(result.models.is_empty());
    }

    let cold_starts = read_spawn_count(&counter_path);
    assert_eq!(
        cold_starts, 1,
        "concurrent failure requests must share exactly 1 cold start"
    );

    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(root_dir);
}

#[tokio::test]
async fn test_issue_29_decision_3_and_5_cache_hit_and_force_refresh() {
    let agent_id = "test-agent-c3";
    let root_dir = std::env::temp_dir().join(format!("counter-c3-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root_dir).unwrap();
    let counter_path = root_dir.join("spawn.count");
    let extra_args = vec![format!("--counter-path={}", counter_path.display())];

    let (manager, _, root) = acp_test_fixture_with_extra_args(agent_id, "happy", &extra_args).await;

    // 1. Initial call: cold start 1, caches models
    let res1 = manager.get_or_refresh_acp_models(agent_id).await.unwrap();
    assert!(res1.available);
    assert_eq!(read_spawn_count(&counter_path), 1);

    // 2. Second call: cache hit, no new process spawned
    let res2 = manager.get_or_refresh_acp_models(agent_id).await.unwrap();
    assert!(res2.available);
    assert_eq!(read_spawn_count(&counter_path), 1);

    // 3. Explicit connection test: force-refresh bypasses model cache and runs probe
    let conn = manager.refresh_acp_connection(agent_id).await.unwrap();
    assert!(conn.available);
    assert!(conn.connected);
    assert_eq!(
        read_spawn_count(&counter_path),
        2,
        "force-refresh must bypass cached models"
    );

    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(root_dir);
}

#[tokio::test]
async fn test_issue_29_decision_6_health_refresh_verifies_connectivity() {
    let (manager_good, _, root_good) = acp_test_fixture_with_id("test-agent-good", "happy").await;
    let (manager_bad, _, root_bad) =
        acp_test_fixture_with_id("test-agent-bad", "exit_nonzero").await;

    let summary_good = manager_good.refresh_installed_agent_health().await.unwrap();
    assert_eq!(summary_good.checked, 1);
    assert_eq!(summary_good.available, 1);
    assert_eq!(summary_good.unavailable, 0);

    let summary_bad = manager_bad.refresh_installed_agent_health().await.unwrap();
    assert_eq!(summary_bad.checked, 1);
    assert_eq!(summary_bad.available, 0);
    assert_eq!(summary_bad.unavailable, 1);

    let _ = std::fs::remove_dir_all(root_good);
    let _ = std::fs::remove_dir_all(root_bad);
}

#[tokio::test]
async fn test_issue_29_decision_7_prerequisite_short_circuit() {
    let (manager, repository, root) = acp_test_fixture_with_id("test-agent-c7", "happy").await;

    // Case A: Agent is disabled
    let now = chrono::Utc::now().to_rfc3339();
    repository
        .update_enabled("test-agent-c7", false, &now)
        .await
        .unwrap();
    let models = manager
        .get_or_refresh_acp_models("test-agent-c7")
        .await
        .unwrap();
    assert!(!models.available);
    assert_eq!(models.error_code.as_deref(), Some("agent_disabled"));

    let conn = manager
        .refresh_acp_connection("test-agent-c7")
        .await
        .unwrap();
    assert!(!conn.available);
    assert_eq!(conn.error_code.as_deref(), Some("agent_disabled"));

    // Case B: Resolved program missing
    repository
        .update_enabled("test-agent-c7", true, &now)
        .await
        .unwrap();
    let mut inst = repository.get("test-agent-c7").await.unwrap().unwrap();
    inst.resolved_program = std::path::PathBuf::from("/non/existent/path/never_spawn");
    repository.upsert_active(&inst).await.unwrap();

    let models_missing = manager
        .get_or_refresh_acp_models("test-agent-c7")
        .await
        .unwrap();
    assert!(!models_missing.available);
    assert_eq!(
        models_missing.error_code.as_deref(),
        Some("agent_entry_missing")
    );

    let conn_missing = manager
        .refresh_acp_connection("test-agent-c7")
        .await
        .unwrap();
    assert!(!conn_missing.available);
    assert_eq!(
        conn_missing.error_code.as_deref(),
        Some("agent_entry_missing")
    );

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn test_issue_29_decision_8_and_10_lifecycle_invalidation_and_race_protection() {
    let agent_id = "test-agent-c8";
    let root_dir = std::env::temp_dir().join(format!("counter-c8-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root_dir).unwrap();
    let counter_path = root_dir.join("spawn.count");
    let extra_args = vec![
        format!("--counter-path={}", counter_path.display()),
        "--new-delay-ms=200".to_string(),
    ];

    let (manager, repository, root) =
        acp_test_fixture_with_extra_args(agent_id, "happy", &extra_args).await;

    // 1. Prime cache
    let (fast_manager, _, fast_root) = acp_test_fixture_with_id("fast-agent", "happy").await;
    let _ = fast_manager
        .get_or_refresh_acp_models("fast-agent")
        .await
        .unwrap();
    assert!(fast_manager.models_cache.read().await.len() > 0);

    // Invalidate state clears model cache
    fast_manager.invalidate_agent_state("fast-agent").await;
    assert_eq!(fast_manager.models_cache.read().await.len(), 0);
    let _ = std::fs::remove_dir_all(fast_root);

    // 2. Race protection: start probe, and before it finishes, change installation in SQLite
    let m = Arc::new(manager);
    let aid = agent_id.to_string();
    let m_clone = m.clone();
    let probe_task = tokio::spawn(async move { m_clone.refresh_acp_connection(&aid).await });

    // Give probe time to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Simulate concurrent lifecycle action (reinstall with new version)
    let mut inst = repository.get(agent_id).await.unwrap().unwrap();
    inst.agent_version = "2.0.0".to_string();
    inst.protocol_status = ProtocolStatus::Unchecked;
    repository.upsert_active(&inst).await.unwrap();
    m.invalidate_agent_state(agent_id).await;

    // Wait for old probe to complete or be cancelled
    let _ = probe_task.await;

    // Verify SQLite was NOT overwritten by the old probe
    let current_inst = repository.get(agent_id).await.unwrap().unwrap();
    assert_eq!(current_inst.agent_version, "2.0.0");
    assert_eq!(current_inst.protocol_status, ProtocolStatus::Unchecked);

    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(root_dir);
}

#[tokio::test]
async fn test_issue_29_decision_9_cancellation_and_caller_release() {
    let agent_id = "test-agent-c9";
    let extra_args = vec!["--new-delay-ms=500".to_string()];
    let (manager, _, root) = acp_test_fixture_with_extra_args(agent_id, "happy", &extra_args).await;

    let m = Arc::new(manager);
    let aid = agent_id.to_string();
    let m_clone = m.clone();
    let handle = tokio::spawn(async move { m_clone.get_or_refresh_acp_models(&aid).await });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Single caller releases -> triggers probe cancellation
    let cancelled = m.release_acp_probe_caller(agent_id).await;
    assert!(cancelled, "releasing only caller must trigger cancellation");

    let res = handle.await.unwrap();
    assert!(res.is_err(), "probe must fail with cancellation");
    assert_eq!(res.unwrap_err().code(), "cancelled");

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn test_issue_29_decision_11_and_12_state_decoupling_empty_corrupt_unsupported() {
    // 1. Empty models
    {
        let (manager, repository, root) =
            acp_test_fixture_with_id("test-decouple-empty", "empty_model_options").await;
        let conn = manager
            .refresh_acp_connection("test-decouple-empty")
            .await
            .unwrap();
        assert!(
            conn.available,
            "empty models must NOT degrade protocol connection available"
        );
        assert!(conn.connected, "protocol connection must remain connected");
        assert_eq!(conn.error_code, None);

        let models = manager
            .get_or_refresh_acp_models("test-decouple-empty")
            .await
            .unwrap();
        assert!(models.available);
        assert!(models.models.is_empty());
        assert_eq!(models.error_code.as_deref(), Some("model_list_empty"));

        let inst = repository
            .get("test-decouple-empty")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(inst.protocol_status, ProtocolStatus::Ready);
        assert_eq!(inst.model_status.as_deref(), Some("unsupported"));
        assert_eq!(inst.model_error_code.as_deref(), Some("model_list_empty"));
        let _ = std::fs::remove_dir_all(root);
    }

    // 2. Corrupt catalog
    {
        let (manager, repository, root) =
            acp_test_fixture_with_id("test-decouple-corrupt", "corrupt_catalog").await;
        let conn = manager
            .refresh_acp_connection("test-decouple-corrupt")
            .await
            .unwrap();
        assert!(
            conn.available,
            "corrupt catalog must NOT degrade protocol connection"
        );
        assert!(conn.connected);
        assert_eq!(conn.error_code, None);

        let models = manager
            .get_or_refresh_acp_models("test-decouple-corrupt")
            .await
            .unwrap();
        assert!(models.available);
        assert!(models.models.is_empty());
        assert_eq!(models.error_code.as_deref(), Some("model_catalog_invalid"));

        let inst = repository
            .get("test-decouple-corrupt")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(inst.protocol_status, ProtocolStatus::Ready);
        assert_eq!(inst.model_status.as_deref(), Some("failed"));
        assert_eq!(
            inst.model_error_code.as_deref(),
            Some("model_catalog_invalid")
        );
        let _ = std::fs::remove_dir_all(root);
    }

    // 3. Unsupported models
    {
        let (manager, repository, root) =
            acp_test_fixture_with_id("test-decouple-unsupported", "unsupported_models").await;
        let conn = manager
            .refresh_acp_connection("test-decouple-unsupported")
            .await
            .unwrap();
        assert!(
            conn.available,
            "unsupported models must NOT degrade protocol connection"
        );
        assert!(conn.connected);
        assert_eq!(conn.error_code, None);

        let models = manager
            .get_or_refresh_acp_models("test-decouple-unsupported")
            .await
            .unwrap();
        assert!(models.available);
        assert!(models.models.is_empty());
        assert_eq!(models.error_code.as_deref(), Some("unsupported"));

        let inst = repository
            .get("test-decouple-unsupported")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(inst.protocol_status, ProtocolStatus::Ready);
        assert_eq!(inst.model_status.as_deref(), Some("unsupported"));
        assert_eq!(inst.model_error_code.as_deref(), Some("unsupported"));
        let _ = std::fs::remove_dir_all(root);
    }
}
