use serde::Serialize;
use uuid::Uuid;

use crate::backend::{
    domain::agents::AgentDefinition,
    infrastructure::agent_execution::{
        error::AiExecutionError,
        types::{
            AgentConnectionResult, AgentModelOption, AgentModelsResult, AgentSessionMode,
            AiExecutionCancellation, AiExecutionLimits, AiExecutionPurpose, AiExecutionRequest,
        },
    },
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub(crate) struct AcpProbeTimings {
    pub(crate) spawn_duration_ms: u64,
    pub(crate) initialize_duration_ms: u64,
    pub(crate) session_new_duration_ms: u64,
    pub(crate) model_discovery_duration_ms: u64,
    pub(crate) cleanup_duration_ms: u64,
    pub(crate) total_duration_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum AcpConnectionStage {
    Spawn,
    Transport,
    Initialize,
    SessionNew,
    Cleanup,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum AcpProtocolConnectionOutcome {
    Connected,
    Failed {
        stage: AcpConnectionStage,
        error_code: String,
        error_message: String,
    },
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum AcpModelDiscoveryOutcome {
    Success {
        models: Vec<AgentModelOption>,
        current_model_id: Option<String>,
    },
    Empty,
    Invalid {
        error_code: String,
        error_message: String,
    },
    Timeout,
    Unsupported,
    Failed {
        error_code: String,
        error_message: String,
    },
    Skipped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct AcpCleanupOutcome {
    pub(crate) process_reaped: bool,
    pub(crate) workspace_removed: bool,
    pub(crate) timed_out: bool,
    pub(crate) failures: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct AcpProbeReport {
    pub(crate) protocol_connection: AcpProtocolConnectionOutcome,
    pub(crate) model_discovery: AcpModelDiscoveryOutcome,
    pub(crate) cleanup: AcpCleanupOutcome,
    pub(crate) timings: AcpProbeTimings,
}

impl AcpProbeReport {
    pub(crate) fn protocol_connected(&self) -> bool {
        matches!(
            self.protocol_connection,
            AcpProtocolConnectionOutcome::Connected
        )
    }

    pub(crate) fn to_connection_result(
        &self,
        agent_id: &str,
        version: Option<&str>,
        installation_status: Option<&str>,
        runtime_status: Option<&str>,
        protocol_status: Option<&str>,
    ) -> AgentConnectionResult {
        match &self.protocol_connection {
            AcpProtocolConnectionOutcome::Connected => AgentConnectionResult {
                agent_id: agent_id.to_string(),
                available: true,
                installed: true,
                connected: true,
                version: version.map(ToString::to_string),
                connection_method: Some("acp".to_string()),
                error_code: None,
                error: None,
                installation_status: installation_status.map(ToString::to_string),
                runtime_status: runtime_status.map(ToString::to_string),
                protocol_status: protocol_status.map(ToString::to_string),
                execution_ready: true,
                health_stale: false,
            },
            AcpProtocolConnectionOutcome::Failed {
                error_code,
                error_message,
                ..
            } => AgentConnectionResult {
                agent_id: agent_id.to_string(),
                available: false,
                installed: true,
                connected: false,
                version: version.map(ToString::to_string),
                connection_method: Some("acp".to_string()),
                error_code: Some(error_code.clone()),
                error: Some(error_message.clone()),
                installation_status: installation_status.map(ToString::to_string),
                runtime_status: runtime_status.map(ToString::to_string),
                protocol_status: protocol_status.map(ToString::to_string),
                execution_ready: false,
                health_stale: false,
            },
            AcpProtocolConnectionOutcome::Cancelled => AgentConnectionResult {
                agent_id: agent_id.to_string(),
                available: false,
                installed: true,
                connected: false,
                version: version.map(ToString::to_string),
                connection_method: Some("acp".to_string()),
                error_code: Some("cancelled".to_string()),
                error: Some("The ACP probe was cancelled.".to_string()),
                installation_status: installation_status.map(ToString::to_string),
                runtime_status: runtime_status.map(ToString::to_string),
                protocol_status: protocol_status.map(ToString::to_string),
                execution_ready: false,
                health_stale: false,
            },
        }
    }

    pub(crate) fn to_models_result(&self, agent_id: &str) -> AgentModelsResult {
        match &self.protocol_connection {
            AcpProtocolConnectionOutcome::Connected => match &self.model_discovery {
                AcpModelDiscoveryOutcome::Success {
                    models,
                    current_model_id,
                } => AgentModelsResult {
                    agent_id: agent_id.to_string(),
                    available: true,
                    current_model_id: current_model_id
                        .clone()
                        .or_else(|| models.first().map(|m| m.id.clone())),
                    models: models.clone(),
                    error_code: None,
                    error: None,
                },
                AcpModelDiscoveryOutcome::Empty => AgentModelsResult {
                    agent_id: agent_id.to_string(),
                    available: true,
                    current_model_id: None,
                    models: Vec::new(),
                    error_code: Some("model_list_empty".to_string()),
                    error: Some("No models advertised by ACP session".to_string()),
                },
                AcpModelDiscoveryOutcome::Invalid {
                    error_code,
                    error_message,
                } => AgentModelsResult {
                    agent_id: agent_id.to_string(),
                    available: true,
                    current_model_id: None,
                    models: Vec::new(),
                    error_code: Some(error_code.clone()),
                    error: Some(error_message.clone()),
                },
                AcpModelDiscoveryOutcome::Timeout => AgentModelsResult {
                    agent_id: agent_id.to_string(),
                    available: true,
                    current_model_id: None,
                    models: Vec::new(),
                    error_code: Some("model_discovery_timeout".to_string()),
                    error: Some("Model discovery timed out".to_string()),
                },
                AcpModelDiscoveryOutcome::Unsupported => AgentModelsResult {
                    agent_id: agent_id.to_string(),
                    available: true,
                    current_model_id: None,
                    models: Vec::new(),
                    error_code: Some("unsupported".to_string()),
                    error: Some("Agent does not support dynamic model selection".to_string()),
                },
                AcpModelDiscoveryOutcome::Failed {
                    error_code,
                    error_message,
                } => AgentModelsResult {
                    agent_id: agent_id.to_string(),
                    available: true,
                    current_model_id: None,
                    models: Vec::new(),
                    error_code: Some(error_code.clone()),
                    error: Some(error_message.clone()),
                },
                AcpModelDiscoveryOutcome::Skipped => AgentModelsResult {
                    agent_id: agent_id.to_string(),
                    available: false,
                    current_model_id: None,
                    models: Vec::new(),
                    error_code: Some("connection_failed".to_string()),
                    error: Some("Connection failed before model discovery".to_string()),
                },
            },
            AcpProtocolConnectionOutcome::Failed {
                error_code,
                error_message,
                ..
            } => AgentModelsResult {
                agent_id: agent_id.to_string(),
                available: false,
                current_model_id: None,
                models: Vec::new(),
                error_code: Some(error_code.clone()),
                error: Some(error_message.clone()),
            },
            AcpProtocolConnectionOutcome::Cancelled => AgentModelsResult {
                agent_id: agent_id.to_string(),
                available: false,
                current_model_id: None,
                models: Vec::new(),
                error_code: Some("cancelled".to_string()),
                error: Some("ACP probe was cancelled".to_string()),
            },
        }
    }
}

pub(crate) fn connection_error_code(error: &AiExecutionError) -> &'static str {
    match error {
        AiExecutionError::Protocol {
            operation: "spawn_timeout",
        } => "spawn_timeout",
        AiExecutionError::Protocol {
            operation: "session_new_timeout",
        } => "session_new_timeout",
        AiExecutionError::Timeout { .. } => "initialize_timeout",
        AiExecutionError::Spawn { .. } | AiExecutionError::RuntimeUnavailable { .. } => {
            "agent_spawn_failed"
        }
        other if is_auth_error(other) => "auth_required",
        _ => "connection_failed",
    }
}

pub(crate) fn is_auth_error(error: &AiExecutionError) -> bool {
    match error {
        AiExecutionError::ProtocolDetail { detail, .. } => is_auth_message(detail),
        AiExecutionError::Output { message } => is_auth_message(message),
        _ => false,
    }
}

pub(crate) fn is_auth_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("auth")
        || lower.contains("login")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("credential")
}

pub(crate) fn connection_probe_request(definition: &AgentDefinition) -> AiExecutionRequest {
    AiExecutionRequest {
        execution_id: format!("agent-probe-{}", Uuid::new_v4()),
        agent_id: definition.id.clone(),
        purpose: AiExecutionPurpose::ConnectionTest,
        session_mode: AgentSessionMode::OneShot,
        prompt: "ACP connection probe".to_string(),
        model: None,
        limits: AiExecutionLimits {
            total_timeout: std::time::Duration::from_secs(65),
            spawn_timeout: std::time::Duration::from_secs(10),
            initialize_timeout: std::time::Duration::from_secs(30),
            config_rpc_timeout: std::time::Duration::from_secs(30),
            cancel_grace: std::time::Duration::from_secs(2),
            close_timeout: std::time::Duration::from_secs(5),
            cleanup_timeout: std::time::Duration::from_secs(10),
            text_bytes: 1024,
            stderr_bytes: 64 * 1024,
        },
        cancellation: AiExecutionCancellation::default(),
        progress: None,
        tenant_id: None,
        execution_context_key: None,
        binding: None,
        replay: false,
        restore_only: false,
        recall_tools: None,
        memory_generation_tools: None,
    }
}

pub(crate) fn model_discovery_request(definition: &AgentDefinition) -> AiExecutionRequest {
    let mut request = connection_probe_request(definition);
    request.execution_id = format!("agent-models-{}", Uuid::new_v4());
    request.purpose = AiExecutionPurpose::ModelDiscovery;
    request.prompt = "ACP model discovery".to_string();
    request
}
