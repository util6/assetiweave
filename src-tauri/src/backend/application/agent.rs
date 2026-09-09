use super::prelude::*;
use crate::backend::runtime::{AppError, AppResult};

use crate::backend::agents::types::{
    AgentConnectionCheckMode, AgentConnectionCheckRequest, AgentConnectionResult, AgentId,
    AgentModelsRequest, AgentModelsResult,
};

impl AppService {
    pub(crate) fn list_agent_catalog(
        &self,
    ) -> AppResult<Vec<crate::backend::agents::types::AgentCatalogEntry>> {
        Ok(self.agent_runtime.list_agent_catalog())
    }

    pub(crate) async fn check_agent_connection(
        &self,
        params: AgentConnectionCheckRequest,
    ) -> AppResult<AgentConnectionResult> {
        let agent_id = AgentId::parse(params.agent_id)
            .map_err(|error| AppError::Validation(error.to_string()))?;
        let mode = params.mode;
        let existing_installation = self
            .list_agent_installations()
            .await?
            .into_iter()
            .find(|item| item.agent_id == agent_id.to_string());
        if let Some(installation) = existing_installation.as_ref().filter(|installation| {
            installation.installation_status
                == crate::backend::agent_market::types::InstallationStatus::Incompatible
        }) {
            return Ok(AgentConnectionResult {
                agent_id: agent_id.to_string(),
                available: false,
                installed: true,
                connected: false,
                version: Some(installation.agent_version.clone()),
                connection_method: Some(installation.protocol.as_str().to_string()),
                error_code: Some("agent_reinstall_required".to_string()),
                error: Some(
                    "The installed Agent is incompatible with the active catalog definition; reinstall it before checking the connection."
                        .to_string(),
                ),
                installation_status: Some(installation.installation_status.as_str().to_string()),
                runtime_status: Some(installation.runtime_status.as_str().to_string()),
                protocol_status: Some(installation.protocol_status.as_str().to_string()),
                execution_ready: false,
                health_stale: false,
            });
        }
        let mut result = if matches!(mode, AgentConnectionCheckMode::Connection)
            && existing_installation.as_ref().is_some_and(|installation| {
                installation.protocol
                    == crate::backend::agent_market::types::AgentMarketProtocol::Acp
            }) {
            self.agent_runtime_manager
                .refresh_acp_connection(agent_id.as_str())
                .await
                .map_err(AppError::external)?
        } else if matches!(mode, AgentConnectionCheckMode::Connection)
            && existing_installation.as_ref().is_some_and(|installation| {
                installation.protocol
                    == crate::backend::agent_market::types::AgentMarketProtocol::Native
            })
        {
            self.agent_runtime_manager
                .refresh_native_health(agent_id.as_str())
                .await
                .map_err(AppError::external)?
        } else {
            crate::backend::ai_execution::check_agent_connection(
                self.agent_runtime.clone(),
                agent_id.clone(),
                mode,
            )
            .await
        };
        if let Some(installation) = self
            .list_agent_installations()
            .await?
            .into_iter()
            .find(|item| item.agent_id == agent_id.to_string())
        {
            result.installed = true;
            result.installation_status = Some(if installation.enabled {
                installation.installation_status.as_str().to_string()
            } else {
                "disabled".to_string()
            });
            result.runtime_status = Some(installation.runtime_status.as_str().to_string());
            result.protocol_status = Some(if result.connected {
                "ready".to_string()
            } else {
                installation.protocol_status.as_str().to_string()
            });
            result.execution_ready = result.connected
                && installation.enabled
                && installation.installation_status
                    == crate::backend::agent_market::types::InstallationStatus::Ready
                && installation.runtime_status
                    == crate::backend::agent_market::types::RuntimeStatus::Ready;
            result.health_stale = installation
                .protocol_checked_at
                .as_deref()
                .is_none_or(|value| {
                    chrono::DateTime::parse_from_rfc3339(value)
                        .map(|checked| {
                            chrono::Utc::now() - checked.with_timezone(&chrono::Utc)
                                > chrono::Duration::minutes(30)
                        })
                        .unwrap_or(true)
                });
        } else if matches!(mode, AgentConnectionCheckMode::Installation) {
            result.execution_ready = false;
        }
        Ok(result)
    }

    pub(crate) async fn list_agent_models(
        &self,
        params: AgentModelsRequest,
    ) -> AppResult<AgentModelsResult> {
        let agent_id = AgentId::parse(params.agent_id)
            .map_err(|error| AppError::Validation(error.to_string()))?;
        let existing_installation = self
            .list_agent_installations()
            .await?
            .into_iter()
            .find(|installation| installation.agent_id == agent_id.to_string());
        if existing_installation.as_ref().is_some_and(|installation| {
            installation.installation_status
                == crate::backend::agent_market::types::InstallationStatus::Incompatible
        }) {
            return Ok(AgentModelsResult {
                agent_id: agent_id.to_string(),
                available: false,
                models: Vec::new(),
                current_model_id: None,
                error_code: Some("agent_reinstall_required".to_string()),
                error: Some(
                    "The installed Agent is incompatible with the active catalog definition; reinstall it before loading models."
                        .to_string(),
                ),
            });
        }
        if existing_installation.as_ref().is_some_and(|installation| {
            installation.protocol == crate::backend::agent_market::types::AgentMarketProtocol::Acp
        }) {
            return self
                .agent_runtime_manager
                .get_or_refresh_acp_models(agent_id.as_str())
                .await
                .map_err(AppError::external);
        }
        if existing_installation.as_ref().is_some_and(|installation| {
            installation.protocol
                == crate::backend::agent_market::types::AgentMarketProtocol::Native
        }) {
            return self
                .agent_runtime_manager
                .refresh_native_models(agent_id.as_str())
                .await
                .map_err(AppError::external);
        }
        Ok(crate::backend::ai_execution::discover_agent_models(
            self.agent_runtime.clone(),
            agent_id,
        )
        .await)
    }

    pub(crate) async fn cancel_agent_model_probe(&self, agent_id: String) -> AppResult<()> {
        let agent_id =
            AgentId::parse(agent_id).map_err(|error| AppError::Validation(error.to_string()))?;
        self.agent_runtime_manager
            .release_acp_probe_caller(agent_id.as_str())
            .await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{
        agent_market::{
            types::{
                AgentInstallation, AgentMarketProtocol, DistributionType, InstallationStatus,
                Ownership, ProtocolStatus, RuntimeStatus,
            },
            AgentInstallationRepository, AgentRuntimeManager,
        },
        runtime::AppRuntime,
        store::{load_local_request_context_sqlx, Database},
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
        let program = crate::backend::host_process::resolve_host_executable("node")
            .expect("Node runtime for ACP fixture");
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-fixtures/fake-acp-agent.mjs");
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
            distribution_type: DistributionType::System,
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
    async fn native_connection_probe_refreshes_persisted_health_after_it_becomes_stale() {
        let (service, repository, root) = native_service_fixture().await;

        let result = service
            .check_agent_connection(AgentConnectionCheckRequest {
                agent_id: "native-fixture".to_string(),
                mode: AgentConnectionCheckMode::Connection,
            })
            .await
            .expect("native connection probe");
        assert!(result.available);
        assert!(result.connected);
        assert!(result.execution_ready);
        assert!(!result.health_stale);

        let installation = service
            .list_installed_agents()
            .await
            .expect("list refreshed native installation")
            .into_iter()
            .find(|installation| installation.agent_id == "native-fixture")
            .expect("native installation view");
        assert!(!installation.health_stale);

        drop(service);
        drop(repository);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn native_startup_health_refresh_checks_installed_native_agents() {
        let (service, repository, root) = native_service_fixture().await;

        let scheduled = service
            .agent_runtime_manager
            .prepare_startup_health_refresh()
            .await
            .expect("mark native health unchecked");
        assert_eq!(scheduled, 1);
        let summary = service
            .agent_runtime_manager
            .clone()
            .refresh_installed_agent_health()
            .await
            .expect("refresh native startup health");
        assert_eq!(summary.checked, 1);
        assert_eq!(summary.available, 1);
        assert_eq!(summary.unavailable, 0);

        let installation = service
            .list_installed_agents()
            .await
            .expect("list refreshed native installation")
            .into_iter()
            .find(|installation| installation.agent_id == "native-fixture")
            .expect("native installation view");
        assert!(!installation.health_stale);

        drop(service);
        drop(repository);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn native_model_discovery_refreshes_persisted_health() {
        let (service, repository, root) = native_service_fixture().await;

        let result = service
            .list_agent_models(AgentModelsRequest {
                agent_id: "native-fixture".to_string(),
            })
            .await
            .expect("native model discovery");
        assert!(result.available);
        assert_eq!(result.models.len(), 1);
        assert_eq!(result.models[0].id, "fixture-model");

        let installation = service
            .list_installed_agents()
            .await
            .expect("list refreshed native installation")
            .into_iter()
            .find(|installation| installation.agent_id == "native-fixture")
            .expect("native installation view");
        assert!(!installation.health_stale);

        drop(service);
        drop(repository);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn incompatible_native_installation_does_not_run_model_discovery() {
        let (service, repository, root) = native_service_fixture().await;
        let mut installation = repository
            .get("native-fixture")
            .await
            .expect("load native fixture")
            .expect("native fixture exists");
        installation.installation_status = InstallationStatus::Incompatible;
        installation.runtime_error_code = Some("catalog_distribution_incompatible".to_string());
        installation.runtime_error_message = Some(
            "The installed Agent distribution or protocol is incompatible with the active catalog."
                .to_string(),
        );
        repository
            .upsert_active(&installation)
            .await
            .expect("mark native fixture incompatible");

        let result = service
            .list_agent_models(AgentModelsRequest {
                agent_id: "native-fixture".to_string(),
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
            .get("native-fixture")
            .await
            .expect("reload native fixture")
            .expect("native fixture still exists");
        assert_eq!(
            persisted.installation_status,
            InstallationStatus::Incompatible
        );

        drop(service);
        drop(repository);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn incompatible_native_installation_does_not_run_connection_probe() {
        let (service, repository, root) = native_service_fixture().await;
        let mut installation = repository
            .get("native-fixture")
            .await
            .expect("load native fixture")
            .expect("native fixture exists");
        installation.installation_status = InstallationStatus::Incompatible;
        installation.runtime_error_code = Some("catalog_distribution_incompatible".to_string());
        installation.runtime_error_message = Some(
            "The installed Agent distribution or protocol is incompatible with the active catalog."
                .to_string(),
        );
        repository
            .upsert_active(&installation)
            .await
            .expect("mark native fixture incompatible");

        let result = service
            .check_agent_connection(AgentConnectionCheckRequest {
                agent_id: "native-fixture".to_string(),
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
            .get("native-fixture")
            .await
            .expect("reload native fixture")
            .expect("native fixture still exists");
        assert_eq!(
            persisted.installation_status,
            InstallationStatus::Incompatible
        );

        drop(service);
        drop(repository);
        let _ = std::fs::remove_dir_all(root);
    }

    async fn native_service_fixture() -> (
        AppService,
        crate::backend::agent_market::AgentInstallationRepository,
        std::path::PathBuf,
    ) {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-native-agent-health-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create native fixture root");
        let db_path = root.join("app.db");
        let db = Database::open_initialized_async(&db_path)
            .await
            .expect("open database");
        let pool = db.pool().clone();
        let context = load_local_request_context_sqlx(&pool)
            .await
            .expect("load request context");
        let program = crate::backend::host_process::resolve_host_executable("node")
            .expect("node runtime for native fixture");
        let stale = (chrono::Utc::now() - chrono::Duration::minutes(31)).to_rfc3339();
        let installation = AgentInstallation {
            agent_id: "native-fixture".to_string(),
            installation_id: uuid::Uuid::new_v4().to_string(),
            display_name: "Native Fixture".to_string(),
            catalog_item_version: "1.0.0".to_string(),
            agent_version: "1.0.0".to_string(),
            protocol: AgentMarketProtocol::Native,
            distribution_id: "system-fixture".to_string(),
            distribution_type: DistributionType::System,
            ownership: Ownership::System,
            install_dir: None,
            resolved_program: program.clone(),
            args: Vec::new(),
            definition_json: serde_json::json!({
                "id": "native-fixture",
                "display_name": "Native Fixture",
                "protocol": "native",
                "program": program.to_string_lossy(),
                "args": [],
                "env": [],
                "capabilities": { "textPrompt": true, "modelDiscovery": true },
                "modelDiscoveryArgs": ["-e", "process.stdout.write('fixture-model\\tFixture Model\\n')"]
            }),
            integrity_json: None,
            source_registry: "native-fixture".to_string(),
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
            .expect("persist native fixture");
        let manager = Arc::new(AgentRuntimeManager::new(
            db.pool().clone(),
            root.join("agent-executions"),
        ));
        manager
            .reload()
            .await
            .expect("restore native fixture registry");
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
}
