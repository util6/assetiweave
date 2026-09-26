use std::{future::Future, pin::Pin, time::Duration};

use crate::backend::domain::agents::{
    AgentCatalogEntry, AgentDefinition, AgentId, DeclaredAgentCapabilities,
};

use super::{
    backends::{acp::AcpExecutionBackend, native::NativeExecutionBackend},
    registry::{AgentAvailability, AgentProbeError},
    AgentConnectionResult, AgentModelOption, AgentModelsResult, AiExecutionError,
    AiExecutionRequest, AiExecutionResult,
};

pub(crate) type BackendFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AiExecutionResult, AiExecutionError>> + Send + 'a>>;
pub(crate) type AgentConnectionFuture<'a> =
    Pin<Box<dyn Future<Output = AgentConnectionResult> + Send + 'a>>;
pub(crate) type BackendConnectionFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), AiExecutionError>> + Send + 'a>>;
pub(crate) type BackendModelsFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<(Vec<AgentModelOption>, Option<String>), AiExecutionError>>
            + Send
            + 'a,
    >,
>;
pub(crate) type AgentModelsFuture<'a> =
    Pin<Box<dyn Future<Output = AgentModelsResult> + Send + 'a>>;

pub(crate) trait AgentExecutionBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        definition: AgentDefinition,
        request: AiExecutionRequest,
    ) -> BackendFuture<'a>;

    fn check_connection<'a>(&'a self, _definition: AgentDefinition) -> BackendConnectionFuture<'a> {
        Box::pin(async {
            Err(AiExecutionError::Protocol {
                operation: "agent_connection_probe",
            })
        })
    }

    fn discover_models<'a>(&'a self, _definition: AgentDefinition) -> BackendModelsFuture<'a> {
        Box::pin(async {
            Err(AiExecutionError::Protocol {
                operation: "agent_model_discovery",
            })
        })
    }
}

pub(crate) trait AgentExecutionRuntime: Send + Sync {
    fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a>;

    fn list_agent_catalog(&self) -> Vec<AgentCatalogEntry> {
        Vec::new()
    }

    fn agent_capabilities(&self, _agent_id: &AgentId) -> Option<DeclaredAgentCapabilities> {
        None
    }

    fn check_agent_installation(&self, agent_id: &AgentId) -> AgentConnectionResult {
        unavailable_connection_result(
            agent_id,
            "agent_not_found",
            "The selected AI agent is not registered.",
        )
    }

    fn check_agent_connection<'a>(&'a self, agent_id: &'a AgentId) -> AgentConnectionFuture<'a> {
        let result = self.check_agent_installation(agent_id);
        Box::pin(async move { result })
    }

    fn discover_agent_models<'a>(&'a self, agent_id: &'a AgentId) -> AgentModelsFuture<'a> {
        let result = unavailable_models_result(
            agent_id,
            "agent_not_found",
            "The selected AI agent is not registered.",
        );
        Box::pin(async move { result })
    }

    fn check_availability(&self, agent_id: &AgentId) -> AgentAvailability {
        AgentAvailability {
            available: false,
            installed: false,
            version: None,
            error: Some(AgentProbeError::ProbeNotConfigured {
                agent_id: agent_id.clone(),
                kind: "availability",
            }),
        }
    }

    fn discover_models(
        &self,
        agent_id: &AgentId,
        _timeout: Duration,
    ) -> Result<Vec<u8>, AgentProbeError> {
        Err(AgentProbeError::ProbeNotConfigured {
            agent_id: agent_id.clone(),
            kind: "model_discovery",
        })
    }

    fn cancel_all(&self) {}
}

pub(crate) fn unavailable_connection_result(
    agent_id: &AgentId,
    error_code: &str,
    error: &str,
) -> AgentConnectionResult {
    AgentConnectionResult {
        agent_id: agent_id.to_string(),
        available: false,
        installed: false,
        connected: false,
        version: None,
        connection_method: None,
        error_code: Some(error_code.to_string()),
        error: Some(error.to_string()),
        installation_status: None,
        runtime_status: None,
        protocol_status: None,
        execution_ready: false,
        health_stale: false,
    }
}

pub(crate) fn unavailable_models_result(
    agent_id: &AgentId,
    error_code: &str,
    error: &str,
) -> AgentModelsResult {
    AgentModelsResult {
        agent_id: agent_id.to_string(),
        available: false,
        models: Vec::new(),
        current_model_id: None,
        error_code: Some(error_code.to_string()),
        error: Some(error.to_string()),
    }
}

impl AgentExecutionBackend for AcpExecutionBackend {
    fn execute<'a>(
        &'a self,
        definition: AgentDefinition,
        request: AiExecutionRequest,
    ) -> BackendFuture<'a> {
        Box::pin(async move { AcpExecutionBackend::execute(self, &definition, request).await })
    }

    fn check_connection<'a>(&'a self, definition: AgentDefinition) -> BackendConnectionFuture<'a> {
        Box::pin(async move { AcpExecutionBackend::check_connection(self, &definition).await })
    }

    fn discover_models<'a>(&'a self, definition: AgentDefinition) -> BackendModelsFuture<'a> {
        Box::pin(async move { AcpExecutionBackend::discover_models(self, &definition).await })
    }
}

impl AgentExecutionBackend for NativeExecutionBackend {
    fn execute<'a>(
        &'a self,
        definition: AgentDefinition,
        request: AiExecutionRequest,
    ) -> BackendFuture<'a> {
        Box::pin(async move { NativeExecutionBackend::execute(self, &definition, request).await })
    }

    fn check_connection<'a>(&'a self, definition: AgentDefinition) -> BackendConnectionFuture<'a> {
        Box::pin(async move { NativeExecutionBackend::check_connection(self, &definition).await })
    }

    fn discover_models<'a>(&'a self, definition: AgentDefinition) -> BackendModelsFuture<'a> {
        Box::pin(async move { NativeExecutionBackend::discover_models(self, &definition).await })
    }
}
