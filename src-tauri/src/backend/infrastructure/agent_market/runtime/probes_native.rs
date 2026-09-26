use std::{path::Path, time::SystemTime};

use crate::backend::{
    domain::agents::{
        AgentInstallation, AgentMarketProtocol, InstallationStatus, Ownership, ProtocolStatus,
        RuntimeStatus,
    },
    infrastructure::{
        agent_execution::{
            backends::native::NativeExecutionBackend, AgentConnectionResult, AgentModelsResult,
            AiExecutionError,
        },
        agent_market::error::AgentMarketError,
    },
};

use super::{definition::definition_from_installation, AgentRuntimeManager, STAGING_RETENTION};

impl AgentRuntimeManager {
    pub(crate) async fn refresh_native_health(
        &self,
        agent_id: &str,
    ) -> Result<AgentConnectionResult, AgentMarketError> {
        let result = self.probe_native_health(agent_id).await?;
        self.reload_registry().await?;
        Ok(result)
    }

    pub(crate) async fn refresh_native_models(
        &self,
        agent_id: &str,
    ) -> Result<AgentModelsResult, AgentMarketError> {
        let health = self.refresh_native_health(agent_id).await?;
        if !health.available {
            return Ok(unavailable_models(
                agent_id,
                health
                    .error_code
                    .as_deref()
                    .unwrap_or("native_connection_failed"),
                health
                    .error
                    .as_deref()
                    .unwrap_or("The native Agent is unavailable."),
            ));
        }

        let mutation_gate = self.mutation_gate(agent_id);
        let _mutation_lease = mutation_gate.write().await;
        let mut installation = self.repository.get(agent_id).await?.ok_or_else(|| {
            AgentMarketError::InstallationNotFound {
                agent_id: agent_id.to_string(),
            }
        })?;
        let now = chrono::Utc::now().to_rfc3339();
        let definition = definition_from_installation(&installation)?;
        let discovery = NativeExecutionBackend::new(self.workspace_root.clone())
            .discover_models(&definition)
            .await;
        let result = match discovery {
            Ok((models, current_model_id)) => {
                installation.model_status = Some("ready".to_string());
                installation.model_error_code = None;
                installation.protocol_status = ProtocolStatus::Ready;
                installation.protocol_error_code = None;
                installation.protocol_error_message = None;
                AgentModelsResult {
                    agent_id: agent_id.to_string(),
                    available: true,
                    current_model_id: current_model_id
                        .or_else(|| models.first().map(|model| model.id.clone())),
                    models,
                    error_code: None,
                    error: None,
                }
            }
            Err(error) => {
                let message = error.to_view().message;
                installation.model_status = Some("failed".to_string());
                installation.model_error_code = Some("model_discovery_failed".to_string());
                unavailable_models(agent_id, "model_discovery_failed", &message)
            }
        };
        installation.model_checked_at = Some(now.clone());
        installation.protocol_checked_at = Some(now.clone());
        installation.updated_at = now;
        self.repository.update_health(&installation).await?;
        self.reload_registry().await?;
        Ok(result)
    }

    pub(crate) async fn probe_native_health(
        &self,
        agent_id: &str,
    ) -> Result<AgentConnectionResult, AgentMarketError> {
        let mutation_gate = self.mutation_gate(agent_id);
        let _mutation_lease = mutation_gate.write().await;
        let mut installation = self.repository.get(agent_id).await?.ok_or_else(|| {
            AgentMarketError::InstallationNotFound {
                agent_id: agent_id.to_string(),
            }
        })?;
        if installation.protocol != AgentMarketProtocol::Native {
            return Err(AgentMarketError::new(
                "protocol_mismatch",
                "The installed Agent does not use the native runtime.",
                false,
            ));
        }

        let now = chrono::Utc::now().to_rfc3339();
        if !installation.enabled {
            return Ok(unavailable_native_connection(
                agent_id,
                "agent_disabled",
                "The native Agent is disabled.",
            ));
        }
        if !installation.resolved_program.is_file() {
            let runtime_status = if installation.ownership == Ownership::Managed {
                RuntimeStatus::EntryMissing
            } else {
                RuntimeStatus::RuntimeMissing
            };
            mark_native_health_failed(
                &mut installation,
                &now,
                runtime_status,
                "agent_entry_missing",
                "The resolved Agent entry is missing.",
            );
            self.repository.update_health(&installation).await?;
            return Ok(unavailable_native_connection(
                agent_id,
                "agent_entry_missing",
                "The resolved Agent entry is missing.",
            ));
        }

        let definition = match definition_from_installation(&installation) {
            Ok(definition) => definition,
            Err(error) => {
                mark_native_health_failed(
                    &mut installation,
                    &now,
                    RuntimeStatus::Failed,
                    "definition_invalid",
                    &error.to_string(),
                );
                self.repository.update_health(&installation).await?;
                return Ok(unavailable_native_connection(
                    agent_id,
                    "definition_invalid",
                    "The persisted native Agent definition is invalid.",
                ));
            }
        };
        installation.installation_status = InstallationStatus::Ready;
        installation.runtime_status = RuntimeStatus::Ready;
        installation.runtime_error_code = None;
        installation.runtime_error_message = None;
        installation.runtime_checked_at = Some(now.clone());
        let result = match NativeExecutionBackend::new(self.workspace_root.clone())
            .check_connection(&definition)
            .await
        {
            Ok(()) => {
                installation.protocol_status = ProtocolStatus::Ready;
                installation.protocol_error_code = None;
                installation.protocol_error_message = None;
                installation.model_status = Some("ready".to_string());
                installation.model_error_code = None;
                AgentConnectionResult {
                    agent_id: agent_id.to_string(),
                    available: true,
                    installed: true,
                    connected: true,
                    version: Some(installation.agent_version.clone()),
                    connection_method: Some("native".to_string()),
                    error_code: None,
                    error: None,
                    installation_status: None,
                    runtime_status: None,
                    protocol_status: None,
                    execution_ready: false,
                    health_stale: false,
                }
            }
            Err(error) => {
                let message = error.to_view().message;
                installation.protocol_status = ProtocolStatus::Failed;
                installation.protocol_error_code = Some("native_connection_failed".to_string());
                installation.protocol_error_message = Some(message.clone());
                installation.model_status = Some("failed".to_string());
                installation.model_error_code = Some("native_connection_failed".to_string());
                unavailable_native_connection(agent_id, "native_connection_failed", &message)
            }
        };
        installation.protocol_checked_at = Some(now.clone());
        installation.model_checked_at = Some(now.clone());
        installation.updated_at = now;
        self.repository.update_health(&installation).await?;
        Ok(result)
    }
}

pub(crate) fn unavailable_models(agent_id: &str, code: &str, message: &str) -> AgentModelsResult {
    AgentModelsResult {
        agent_id: agent_id.to_string(),
        available: false,
        models: Vec::new(),
        current_model_id: None,
        error_code: Some(code.to_string()),
        error: Some(message.to_string()),
    }
}

pub(crate) fn unavailable_native_connection(
    agent_id: &str,
    code: &str,
    message: &str,
) -> AgentConnectionResult {
    AgentConnectionResult {
        agent_id: agent_id.to_string(),
        available: false,
        installed: true,
        connected: false,
        version: None,
        connection_method: Some("native".to_string()),
        error_code: Some(code.to_string()),
        error: Some(message.to_string()),
        installation_status: None,
        runtime_status: None,
        protocol_status: None,
        execution_ready: false,
        health_stale: false,
    }
}

pub(crate) fn mark_native_health_failed(
    installation: &mut AgentInstallation,
    checked_at: &str,
    runtime_status: RuntimeStatus,
    error_code: &str,
    message: &str,
) {
    installation.installation_status = InstallationStatus::Broken;
    installation.runtime_status = runtime_status;
    installation.runtime_error_code = Some(error_code.to_string());
    installation.runtime_error_message = Some(message.to_string());
    installation.runtime_checked_at = Some(checked_at.to_string());
    installation.protocol_status = ProtocolStatus::Failed;
    installation.protocol_error_code = Some(error_code.to_string());
    installation.protocol_error_message = Some(message.to_string());
    installation.protocol_checked_at = Some(checked_at.to_string());
    installation.model_status = Some("failed".to_string());
    installation.model_error_code = Some(error_code.to_string());
    installation.model_checked_at = Some(checked_at.to_string());
    installation.updated_at = checked_at.to_string();
}

#[allow(dead_code)]
pub(crate) fn model_error_code(error: &AiExecutionError) -> &'static str {
    match error {
        AiExecutionError::Protocol {
            operation: "session_model_catalog_empty",
        } => "model_list_empty",
        AiExecutionError::Protocol {
            operation: "session_model_catalog_invalid",
        } => "model_catalog_invalid",
        AiExecutionError::Timeout { .. }
        | AiExecutionError::Protocol {
            operation: "session_new_timeout" | "spawn_timeout",
        } => "model_discovery_timeout",
        _ => "model_discovery_failed",
    }
}

#[allow(dead_code)]
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
        || lower.contains("sign in")
        || lower.contains("unauthorized")
        || lower.contains("unauthenticated")
        || lower.contains("credential")
}

#[allow(dead_code)]
pub(crate) fn model_discovery_error_message(error: &AiExecutionError) -> String {
    if matches!(
        error,
        AiExecutionError::Protocol {
            operation: "session_model_catalog_empty"
        }
    ) {
        "The ACP Agent did not return a usable model list.".to_string()
    } else {
        error.to_view().message
    }
}

pub(crate) fn cleanup_runtime_directories(runtime_root: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    let now = SystemTime::now();
    let staging = runtime_root.join(".staging");
    if let Ok(entries) = std::fs::read_dir(&staging) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_old = entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age > STAGING_RETENTION);
            if is_old
                && entry
                    .file_type()
                    .map(|file_type| file_type.is_dir() && !file_type.is_symlink())
                    .unwrap_or(false)
            {
                if let Err(error) = std::fs::remove_dir_all(&path) {
                    warnings.push(format!("stale staging cleanup pending: {error}"));
                }
            }
        }
    }
    warnings
}
