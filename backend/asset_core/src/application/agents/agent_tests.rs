use super::*;
use crate::backend::{
    domain::agents::{
        AgentInstallation, AgentMarketProtocol, DistributionType, InstallationStatus, Ownership,
        ProtocolStatus, RuntimeStatus,
    },
    infrastructure::{agent_market::AgentRuntimeManager, runtime::AppRuntime},
    store::{load_local_request_context_sqlx, system::AgentInstallationRepository, Database},
};
use std::sync::Arc;

#[tokio::test]
async fn persisted_acp_probe_uses_a_process_capable_runtime_after_restart() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-agent-application-restart-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create test root");
    let db_path = root.join("app.db");
    let program = crate::backend::infrastructure::host_process::resolve_host_executable("node")
        .expect("Node runtime for ACP fixture");
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/fake-acp-agent.mjs");
    let args = vec![fixture.to_string_lossy().to_string()];

    let db = Database::open_initialized_async(&db_path)
        .await
        .expect("open database");
    let pool = db.pool().clone();
    let context = load_local_request_context_sqlx(&pool)
        .await
        .expect("load request context");
    let now = chrono::Utc::now().to_rfc3339();
    let installation = AgentInstallation {
        agent_id: "fixture-agent".to_string(),
        installation_id: uuid::Uuid::new_v4().to_string(),
        display_name: "Fixture Agent".to_string(),
        catalog_item_version: "1.0.0".to_string(),
        agent_version: "1.0.0".to_string(),
        protocol: AgentMarketProtocol::Acp,
        distribution_id: "system-fixture".to_string(),
        distribution_type: DistributionType::Binary,
        ownership: Ownership::System,
        install_dir: None,
        resolved_program: program.clone(),
        args: args.clone(),
        definition_json: serde_json::json!({
            "id": "fixture-agent",
            "display_name": "Fixture Agent",
            "protocol": "acp",
            "program": program.to_string_lossy(),
            "args": args,
            "env": [],
        }),
        integrity_json: None,
        source_registry: "fixture-agent".to_string(),
        catalog_version: "2026.08.28.1".to_string(),
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
        model_status: Some("ready".to_string()),
        model_error_code: None,
        model_checked_at: Some(now.clone()),
        installed_at: now.clone(),
        updated_at: now,
    };
    let repository = AgentInstallationRepository::new(db.pool().clone());
    repository
        .upsert_active(&installation)
        .await
        .expect("persist installed ACP fixture");
    let manager = Arc::new(AgentRuntimeManager::new(
        db.pool().clone(),
        root.join("agent-executions"),
    ));
    manager
        .reload()
        .await
        .expect("restore persisted ACP registry");
    let agent_runtime = manager.runtime();
    let app_runtime = AppRuntime::for_test(
        db_path.clone(),
        db.clone(),
        context.clone(),
        manager.clone(),
        agent_runtime.clone(),
    )
    .await;
    let service = AppService {
        runtime: app_runtime.clone(),
        db,
        db_path,
        context,
        agent_runtime_manager: manager,
        agent_runtime,
        conversation_adapter_catalog: app_runtime.conversation_adapter_catalog(),
    };

    let models = service
        .list_agent_models(AgentModelsRequest {
            agent_id: "fixture-agent".to_string(),
        })
        .await
        .expect("persisted ACP model probe");
    assert!(models.available);
    assert_eq!(models.models.len(), 2);
    let connection = service
        .check_agent_connection(AgentConnectionCheckRequest {
            agent_id: "fixture-agent".to_string(),
            mode: AgentConnectionCheckMode::Connection,
        })
        .await
        .expect("persisted ACP connection probe");
    assert!(connection.connected);
    assert!(connection.execution_ready);

    drop(service);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn acp_connection_probe_refreshes_persisted_health_after_it_becomes_stale() {
    let (service, repository, root) = acp_service_fixture().await;

    let result = service
        .check_agent_connection(AgentConnectionCheckRequest {
            agent_id: "acp-fixture".to_string(),
            mode: AgentConnectionCheckMode::Connection,
        })
        .await
        .expect("acp connection probe");
    assert!(result.available);
    assert!(result.connected);
    assert!(result.execution_ready);
    assert!(!result.health_stale);

    let installation = service
        .list_installed_agents()
        .await
        .expect("list refreshed acp installation")
        .into_iter()
        .find(|installation| installation.agent_id == "acp-fixture")
        .expect("acp installation view");
    assert!(!installation.health_stale);

    drop(service);
    drop(repository);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn acp_startup_health_refresh_checks_installed_agents() {
    let (service, repository, root) = acp_service_fixture().await;

    let scheduled = service
        .agent_runtime_manager
        .prepare_startup_health_refresh()
        .await
        .expect("mark acp health unchecked");
    assert_eq!(scheduled, 1);
    let summary = service
        .agent_runtime_manager
        .clone()
        .refresh_installed_agent_health()
        .await
        .expect("refresh acp startup health");
    assert_eq!(summary.checked, 1);
    assert_eq!(summary.available, 1);
    assert_eq!(summary.unavailable, 0);

    let installation = service
        .list_installed_agents()
        .await
        .expect("list refreshed acp installation")
        .into_iter()
        .find(|installation| installation.agent_id == "acp-fixture")
        .expect("acp installation view");
    assert!(!installation.health_stale);

    drop(service);
    drop(repository);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn acp_model_discovery_lists_available_models() {
    let (service, repository, root) = acp_service_fixture().await;

    let result = service
        .list_agent_models(AgentModelsRequest {
            agent_id: "acp-fixture".to_string(),
        })
        .await
        .expect("acp model discovery");
    assert!(result.available);
    assert_eq!(result.models.len(), 2);
    assert_eq!(result.models[0].id, "fixture/model-fast");

    drop(service);
    drop(repository);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn incompatible_installation_does_not_run_model_discovery() {
    let (service, repository, root) = acp_service_fixture().await;
    let mut installation = repository
        .get("acp-fixture")
        .await
        .expect("load acp fixture")
        .expect("acp fixture exists");
    installation.installation_status = InstallationStatus::Incompatible;
    installation.runtime_error_code = Some("catalog_distribution_incompatible".to_string());
    installation.runtime_error_message = Some(
        "The installed Agent distribution or protocol is incompatible with the active catalog."
            .to_string(),
    );
    repository
        .upsert_active(&installation)
        .await
        .expect("mark acp fixture incompatible");

    let result = service
        .list_agent_models(AgentModelsRequest {
            agent_id: "acp-fixture".to_string(),
        })
        .await
        .expect("incompatible model request should be represented as unavailable");

    assert!(!result.available);
    assert!(result.models.is_empty());
    assert_eq!(
        result.error_code.as_deref(),
        Some("agent_reinstall_required")
    );
    let persisted = repository
        .get("acp-fixture")
        .await
        .expect("reload acp fixture")
        .expect("acp fixture still exists");
    assert_eq!(
        persisted.installation_status,
        InstallationStatus::Incompatible
    );

    drop(service);
    drop(repository);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn incompatible_installation_does_not_run_connection_probe() {
    let (service, repository, root) = acp_service_fixture().await;
    let mut installation = repository
        .get("acp-fixture")
        .await
        .expect("load acp fixture")
        .expect("acp fixture exists");
    installation.installation_status = InstallationStatus::Incompatible;
    installation.runtime_error_code = Some("catalog_distribution_incompatible".to_string());
    installation.runtime_error_message = Some(
        "The installed Agent distribution or protocol is incompatible with the active catalog."
            .to_string(),
    );
    repository
        .upsert_active(&installation)
        .await
        .expect("mark acp fixture incompatible");

    let result = service
        .check_agent_connection(AgentConnectionCheckRequest {
            agent_id: "acp-fixture".to_string(),
            mode: AgentConnectionCheckMode::Connection,
        })
        .await
        .expect("incompatible connection check should be represented as unavailable");

    assert!(!result.available);
    assert!(!result.connected);
    assert!(!result.execution_ready);
    assert_eq!(
        result.error_code.as_deref(),
        Some("agent_reinstall_required")
    );
    let persisted = repository
        .get("acp-fixture")
        .await
        .expect("reload acp fixture")
        .expect("acp fixture still exists");
    assert_eq!(
        persisted.installation_status,
        InstallationStatus::Incompatible
    );

    drop(service);
    drop(repository);
    let _ = std::fs::remove_dir_all(root);
}

async fn acp_service_fixture() -> (
    AppService,
    crate::backend::store::system::AgentInstallationRepository,
    std::path::PathBuf,
) {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-acp-agent-health-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create acp fixture root");
    let db_path = root.join("app.db");
    let db = Database::open_initialized_async(&db_path)
        .await
        .expect("open database");
    let pool = db.pool().clone();
    let context = load_local_request_context_sqlx(&pool)
        .await
        .expect("load request context");
    let program = crate::backend::infrastructure::host_process::resolve_host_executable("node")
        .expect("node runtime for acp fixture");
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/fake-acp-agent.mjs");
    let args = vec![fixture.to_string_lossy().to_string()];
    let stale = (chrono::Utc::now() - chrono::Duration::minutes(31)).to_rfc3339();
    let installation = AgentInstallation {
        agent_id: "acp-fixture".to_string(),
        installation_id: uuid::Uuid::new_v4().to_string(),
        display_name: "ACP Fixture".to_string(),
        catalog_item_version: "1.0.0".to_string(),
        agent_version: "1.0.0".to_string(),
        protocol: AgentMarketProtocol::Acp,
        distribution_id: "system-fixture".to_string(),
        distribution_type: DistributionType::Binary,
        ownership: Ownership::System,
        install_dir: None,
        resolved_program: program.clone(),
        args: args.clone(),
        definition_json: serde_json::json!({
            "id": "acp-fixture",
            "display_name": "ACP Fixture",
            "protocol": "acp",
            "program": program.to_string_lossy(),
            "args": args,
            "env": [],
        }),
        integrity_json: None,
        source_registry: "acp-fixture".to_string(),
        catalog_version: "2026.09.01.1".to_string(),
        enabled: true,
        installation_status: InstallationStatus::Ready,
        runtime_status: RuntimeStatus::Ready,
        runtime_error_code: None,
        runtime_error_message: None,
        runtime_checked_at: Some(stale.clone()),
        protocol_status: ProtocolStatus::Ready,
        protocol_error_code: None,
        protocol_error_message: None,
        protocol_checked_at: Some(stale.clone()),
        model_status: Some("ready".to_string()),
        model_error_code: None,
        model_checked_at: Some(stale),
        installed_at: "2026-08-01T00:00:00Z".to_string(),
        updated_at: "2026-08-01T00:00:00Z".to_string(),
    };
    let repository = AgentInstallationRepository::new(db.pool().clone());
    repository
        .upsert_active(&installation)
        .await
        .expect("persist acp fixture");
    let manager = Arc::new(AgentRuntimeManager::new(
        db.pool().clone(),
        root.join("agent-executions"),
    ));
    manager
        .reload()
        .await
        .expect("restore acp fixture registry");
    let agent_runtime = manager.runtime();
    let app_runtime = AppRuntime::for_test(
        db_path.clone(),
        db.clone(),
        context.clone(),
        manager.clone(),
        agent_runtime.clone(),
    )
    .await;
    let service = AppService {
        runtime: app_runtime.clone(),
        db,
        db_path,
        context,
        agent_runtime_manager: manager,
        agent_runtime,
        conversation_adapter_catalog: app_runtime.conversation_adapter_catalog(),
    };

    (service, repository, root)
}
