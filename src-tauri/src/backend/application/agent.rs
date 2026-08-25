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

    pub(crate) fn check_agent_connection(
        &self,
        params: AgentConnectionCheckRequest,
    ) -> AppResult<AgentConnectionResult> {
        let agent_id = AgentId::parse(params.agent_id)
            .map_err(|error| AppError::Validation(error.to_string()))?;
        let mode = params.mode;
        let mut result = crate::backend::ai_execution::check_agent_connection_blocking(
            self.agent_runtime.clone(),
            agent_id.clone(),
            mode,
        );
        if let Some(installation) = self
            .list_agent_installations()?
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

    pub(crate) fn list_agent_models(
        &self,
        params: AgentModelsRequest,
    ) -> AppResult<AgentModelsResult> {
        let agent_id = AgentId::parse(params.agent_id)
            .map_err(|error| AppError::Validation(error.to_string()))?;
        Ok(
            crate::backend::ai_execution::discover_agent_models_blocking(
                self.agent_runtime.clone(),
                agent_id,
            ),
        )
    }
}
