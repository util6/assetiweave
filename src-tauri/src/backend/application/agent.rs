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
#[path = "agent_tests.rs"]
mod tests;
