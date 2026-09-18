use super::*;
use crate::backend::store::Database;
use std::time::SystemTime;

#[tokio::test]
async fn repository_is_application_scoped_and_upsert_keeps_one_current_row() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-agent-repo-{}.db",
        uuid::Uuid::new_v4()
    ));
    let database = Database::open_initialized_async(&path)
        .await
        .expect("database");
    let repository = AgentInstallationRepository::new(database.pool().clone());
    let now = format!("{:?}", SystemTime::now());
    let installation = fixture("agent", "installation", &now);
    repository
        .upsert_active(&installation)
        .await
        .expect("upsert");
    repository
        .upsert_active(&AgentInstallation {
            installation_id: "replacement".to_string(),
            ..installation.clone()
        })
        .await
        .expect("replacement");
    let rows = repository.list().await.expect("list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].installation_id, "replacement");
    let _ = std::fs::remove_file(path);
}

fn fixture(agent_id: &str, installation_id: &str, now: &str) -> AgentInstallation {
    AgentInstallation {
        agent_id: agent_id.to_string(),
        installation_id: installation_id.to_string(),
        display_name: "Agent".to_string(),
        catalog_item_version: "1.0.0".to_string(),
        agent_version: "1.0.0".to_string(),
        protocol: AgentMarketProtocol::Acp,
        distribution_id: "system".to_string(),
        distribution_type: DistributionType::System,
        ownership: Ownership::System,
        install_dir: None,
        resolved_program: PathBuf::from("/usr/bin/agent"),
        args: Vec::new(),
        definition_json: serde_json::json!({}),
        integrity_json: None,
        source_registry: "agent".to_string(),
        catalog_version: "2026.08.16.1".to_string(),
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
        installed_at: now.to_string(),
        updated_at: now.to_string(),
    }
}
