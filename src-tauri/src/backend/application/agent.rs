use super::prelude::*;

use crate::backend::agents::types::{
    AgentConnectionCheckRequest, AgentConnectionResult, AgentId, AgentModelsRequest,
    AgentModelsResult,
};

impl AppService {
    pub(crate) fn list_agent_catalog(
        &self,
    ) -> AppResult<Vec<crate::backend::agents::types::AgentCatalogEntry>> {
        Ok(
            crate::backend::ai_execution::shared_agent_execution_runtime(&self.db_path)
                .map_err(|error| error.to_string())?
                .list_agent_catalog(),
        )
    }

    pub(crate) fn check_agent_connection(
        &self,
        params: AgentConnectionCheckRequest,
    ) -> AppResult<AgentConnectionResult> {
        let agent_id = AgentId::parse(params.agent_id).map_err(|error| error.to_string())?;
        let runtime = crate::backend::ai_execution::shared_agent_execution_runtime(&self.db_path)
            .map_err(|error| error.to_string())?;
        Ok(
            crate::backend::ai_execution::check_agent_connection_blocking(
                runtime,
                agent_id,
                params.mode,
            ),
        )
    }

    pub(crate) fn list_agent_models(
        &self,
        params: AgentModelsRequest,
    ) -> AppResult<AgentModelsResult> {
        let agent_id = AgentId::parse(params.agent_id).map_err(|error| error.to_string())?;
        let runtime = crate::backend::ai_execution::shared_agent_execution_runtime(&self.db_path)
            .map_err(|error| error.to_string())?;
        Ok(crate::backend::ai_execution::discover_agent_models_blocking(runtime, agent_id))
    }
}

impl super::service::AgentAppService {
    pub(crate) fn list_agent_catalog(
        &self,
    ) -> AppResult<Vec<crate::backend::agents::types::AgentCatalogEntry>> {
        Ok(self.agent_runtime.list_agent_catalog())
    }

    pub(crate) fn check_agent_connection(
        &self,
        params: AgentConnectionCheckRequest,
    ) -> AppResult<AgentConnectionResult> {
        let agent_id = AgentId::parse(params.agent_id).map_err(|error| error.to_string())?;
        Ok(
            crate::backend::ai_execution::check_agent_connection_blocking(
                self.agent_runtime.clone(),
                agent_id,
                params.mode,
            ),
        )
    }

    pub(crate) fn list_agent_models(
        &self,
        params: AgentModelsRequest,
    ) -> AppResult<AgentModelsResult> {
        let agent_id = AgentId::parse(params.agent_id).map_err(|error| error.to_string())?;
        Ok(
            crate::backend::ai_execution::discover_agent_models_blocking(
                self.agent_runtime.clone(),
                agent_id,
            ),
        )
    }
}
